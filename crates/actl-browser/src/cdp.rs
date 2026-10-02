//! 最小同步 CDP 连接层(docs/72 §5,spike 验证形态的入库版):
//! 单属主事件循环 + mpsc——socket 只被循环线程持有(非阻塞读 + 出站队列),
//! 调用方投递请求并限时阻塞收响应;事件全部外抛供导航竞态判定。

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, channel, sync_channel};
use std::thread;
use std::time::Duration;

/// 连接层错误(内部形态;ops 层统一映射为 CtlError/ErrorCode)。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CdpError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl CdpError {
    fn disconnected() -> Self {
        Self {
            code: "DISCONNECTED",
            message: "CDP connection is closed".into(),
        }
    }
}

/// 外抛事件(导航竞态判定的原始输入)。
pub(crate) struct Event {
    pub session: Option<String>,
    pub method: String,
    pub params: Value,
}

enum Outbound {
    Call {
        method: &'static str,
        params: Value,
        session: Option<String>,
        resp: SyncSender<Result<Value, CdpError>>,
    },
}

type Pending = HashMap<u64, SyncSender<Result<Value, CdpError>>>;

/// 纯路由:按 id 配对响应、无 id 的按事件外抛。自由函数以便离线单测。
fn route_inbound(text: &str, pending: &mut Pending, events: &mut Vec<Event>) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("non-JSON message: {e}"))?;
    if let Some(id) = v.get("id").and_then(Value::as_u64) {
        if let Some(tx) = pending.remove(&id) {
            if let Some(err) = v.get("error") {
                let mut message = err
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("(no message)")
                    .to_string();
                if let Some(data) = err.get("data").and_then(Value::as_str) {
                    message.push_str("; data: ");
                    message.push_str(data);
                }
                let _ = tx.send(Err(CdpError {
                    code: "CDP_ERROR",
                    message,
                }));
            } else {
                let _ = tx.send(Ok(v.get("result").cloned().unwrap_or(Value::Null)));
            }
        }
        return Ok(());
    }
    if let Some(method) = v.get("method").and_then(Value::as_str) {
        events.push(Event {
            session: v
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string),
            method: method.to_string(),
            params: v.get("params").cloned().unwrap_or(Value::Null),
        });
        return Ok(());
    }
    Err(format!("message carries neither id nor method: {text}"))
}

fn run_loop<S: std::io::Read + std::io::Write>(
    mut ws: tungstenite::WebSocket<S>,
    outbound: Receiver<Outbound>,
    events: Sender<Event>,
    connected: Arc<AtomicBool>,
) {
    let mut pending: Pending = HashMap::new();
    let mut next_id: u64 = 0;
    // 非阻塞 socket 上 send 可能 WouldBlock,帧留在这里下一轮重试
    let mut outbox: Vec<String> = Vec::new();

    loop {
        while let Ok(msg) = outbound.try_recv() {
            match msg {
                Outbound::Call {
                    method,
                    params,
                    session,
                    resp,
                } => {
                    next_id += 1;
                    let mut body = json!({ "id": next_id, "method": method, "params": params });
                    if let Some(s) = &session {
                        body["sessionId"] = Value::String(s.clone());
                    }
                    let text = body.to_string();
                    let outcome = match ws.send(tungstenite::Message::text(text.clone())) {
                        Ok(()) => Ok(()),
                        Err(tungstenite::Error::Io(ref e)) if e.kind() == ErrorKind::WouldBlock => {
                            outbox.push(text);
                            Ok(())
                        }
                        Err(e) => Err(format!("send failed: {e}")),
                    };
                    match outcome {
                        Ok(()) => {
                            pending.insert(next_id, resp);
                        }
                        Err(message) => {
                            let _ = resp.send(Err(CdpError {
                                code: "SEND",
                                message,
                            }));
                        }
                    }
                }
            }
        }

        outbox.retain(
            |text| match ws.send(tungstenite::Message::text(text.clone())) {
                Ok(()) => false,
                Err(tungstenite::Error::Io(ref e)) if e.kind() == ErrorKind::WouldBlock => true,
                Err(_) => true,
            },
        );

        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                let mut evbuf = Vec::new();
                if let Err(err) = route_inbound(t.as_str(), &mut pending, &mut evbuf) {
                    // 路由失败不致命(如浏览器通知类消息),留痕在返回的错误里由上层裁决
                    let _ = err;
                }
                for e in evbuf {
                    let _ = events.send(e);
                }
            }
            Ok(tungstenite::Message::Ping(p)) => {
                if ws.send(tungstenite::Message::Pong(p)).is_err() {
                    break;
                }
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e)) if e.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => break,
        }
    }

    connected.store(false, Ordering::SeqCst);
    for (_, tx) in pending.drain() {
        let _ = tx.send(Err(CdpError::disconnected()));
    }
}

/// 从 ws://host:port/... 提取 (host, port)。CDP 端点恒为本机明文 ws,不支持 wss。
fn ws_host_port(url: &str) -> Result<(String, u16), CdpError> {
    let bad = |what: &str| CdpError {
        code: "CONNECT",
        message: format!("bad ws endpoint ({what}): {url}"),
    };
    let rest = url
        .strip_prefix("ws://")
        .ok_or_else(|| bad("only ws:// supported"))?;
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| bad("missing port"))?;
    let port: u16 = port.parse().map_err(|_| bad("unparsable port"))?;
    Ok((host.to_string(), port))
}

pub(crate) struct Connection {
    outbound: Sender<Outbound>,
    events: Receiver<Event>,
    connected: Arc<AtomicBool>,
}

impl Connection {
    /// 握手用阻塞 socket,完成后切非阻塞(tungstenite `connect()` 的
    /// MaybeTlsStream 不暴露该控制,故自建 TcpStream 走 `client()`)。
    pub(crate) fn connect(url: &str) -> Result<Self, CdpError> {
        use tungstenite::client::IntoClientRequest;
        let (host, port) = ws_host_port(url)?;
        let stream = TcpStream::connect((host.as_str(), port)).map_err(|e| CdpError {
            code: "CONNECT",
            message: format!("tcp connect to {host}:{port}: {e}"),
        })?;
        let request = url.into_client_request().map_err(|e| CdpError {
            code: "CONNECT",
            message: format!("bad ws url {url}: {e}"),
        })?;
        let (ws, _response) = tungstenite::client(request, stream).map_err(|e| CdpError {
            code: "CONNECT",
            message: format!("handshake with {url}: {e}"),
        })?;
        ws.get_ref().set_nonblocking(true).map_err(|e| CdpError {
            code: "CONNECT",
            message: format!("cannot switch to non-blocking: {e}"),
        })?;
        let (out_tx, out_rx) = channel::<Outbound>();
        let (ev_tx, ev_rx) = channel::<Event>();
        let connected = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&connected);
        thread::spawn(move || run_loop(ws, out_rx, ev_tx, flag));
        Ok(Self {
            outbound: out_tx,
            events: ev_rx,
            connected,
        })
    }

    pub(crate) fn call(
        &self,
        method: &'static str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value, CdpError> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(CdpError::disconnected());
        }
        let (tx, rx) = sync_channel::<Result<Value, CdpError>>(1);
        self.outbound
            .send(Outbound::Call {
                method,
                params,
                session: session.map(str::to_string),
                resp: tx,
            })
            .map_err(|_| CdpError::disconnected())?;
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(CdpError {
                code: "TIMEOUT",
                message: format!("no response within {}ms: {method}", timeout.as_millis()),
            }),
            Err(RecvTimeoutError::Disconnected) => Err(CdpError::disconnected()),
        }
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub(crate) fn drain_events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            out.push(e);
        }
        out
    }
}

/// Chromium DevTools HTTP 发现端点的最小 GET。
/// 端点不保证按 `Connection: close` 断开,必须按头解析、按 Content-Length 读体
/// (docs/72 §10 run-2 实证:`read_to_end` 会吃读超时,Windows 上同为 10060)。
pub(crate) fn http_get_json(port: u16, path: &str, timeout: Duration) -> Result<Value, CdpError> {
    let wrap = |message: String| CdpError {
        code: "CONNECT",
        message,
    };
    let mut s = TcpStream::connect(("127.0.0.1", port))
        .map_err(|e| wrap(format!("http connect 127.0.0.1:{port}{path}: {e}")))?;
    s.set_read_timeout(Some(timeout))
        .map_err(|e| wrap(e.to_string()))?;
    s.set_write_timeout(Some(timeout))
        .map_err(|e| wrap(e.to_string()))?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes())
        .map_err(|e| wrap(format!("http write: {e}")))?;
    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = s
            .read(&mut chunk)
            .map_err(|e| wrap(format!("http read headers: {e}")))?;
        if n == 0 {
            return Err(wrap(format!(
                "http connection closed before headers: {path}"
            )));
        }
        raw.extend_from_slice(&chunk[..n]);
    };
    let headers = String::from_utf8_lossy(&raw[..header_end]).to_string();
    let content_length = headers
        .lines()
        .find_map(|l| {
            l.split_once(':').and_then(|(k, v)| {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    v.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
        })
        .unwrap_or(0);
    let mut body = raw[header_end + 4..].to_vec();
    while body.len() < content_length {
        let n = s
            .read(&mut chunk)
            .map_err(|e| wrap(format!("http read body: {e}")))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&body).map_err(|e| wrap(format!("bad JSON from {path}: {e}")))
}

use std::io::{Read, Write};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    fn pending_with(id: u64) -> (Pending, Receiver<Result<Value, CdpError>>) {
        let (tx, rx) = sync_channel(1);
        let mut p = HashMap::new();
        p.insert(id, tx);
        (p, rx)
    }

    #[test]
    fn routes_response_result_to_pending_caller() {
        let (mut pending, rx) = pending_with(7);
        let mut events = Vec::new();
        route_inbound(
            r#"{"id":7,"result":{"frameId":"F1"}}"#,
            &mut pending,
            &mut events,
        )
        .unwrap();
        assert!(events.is_empty());
        let got = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(got["frameId"], "F1");
    }

    #[test]
    fn maps_cdp_error_with_message() {
        let (mut pending, rx) = pending_with(3);
        let mut events = Vec::new();
        route_inbound(
            r#"{"id":3,"error":{"code":-32000,"message":"boom","data":"extra"}}"#,
            &mut pending,
            &mut events,
        )
        .unwrap();
        let err = rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap_err();
        assert_eq!(err.code, "CDP_ERROR");
        assert_eq!(err.message, "boom; data: extra");
    }

    #[test]
    fn routes_event_with_session_id() {
        let (mut pending, _rx) = pending_with(1);
        let mut events = Vec::new();
        route_inbound(
            r#"{"method":"Page.frameNavigated","sessionId":"S1","params":{"frame":{"id":"F1"}}}"#,
            &mut pending,
            &mut events,
        )
        .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].method, "Page.frameNavigated");
        assert_eq!(events[0].session.as_deref(), Some("S1"));
        assert_eq!(events[0].params["frame"]["id"], "F1");
    }

    #[test]
    fn rejects_non_object_and_unmatched_messages() {
        let (mut pending, _rx) = pending_with(1);
        let mut events = Vec::new();
        assert!(route_inbound("not json", &mut pending, &mut events).is_err());
        assert!(route_inbound(r#"{"id":99,"result":{}}"#, &mut pending, &mut events).is_ok());
        assert!(route_inbound(r#"{"something":"else"}"#, &mut pending, &mut events).is_err());
    }

    #[test]
    fn parses_ws_endpoint_host_port() {
        assert_eq!(
            ws_host_port("ws://127.0.0.1:9222/devtools/browser/UUID").unwrap(),
            ("127.0.0.1".to_string(), 9222)
        );
        assert!(ws_host_port("wss://example.com/x").is_err());
        assert!(ws_host_port("ws://127.0.0.1/x").is_err());
    }
}
