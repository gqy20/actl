//! 最小同步 CDP 连接层:单属主事件循环 + mpsc,无 async 运行时。
//! 架构形态即 docs/72 为 actl-browser 设想的连接层:socket 只被循环线程持有,
//! 调用方经通道投递请求并限时阻塞等待;事件全部外抛给上层做导航竞态判定。

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, SyncSender, sync_channel};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// 连接层错误码:DISCONNECTED / TIMEOUT / SEND / PROTOCOL / CDP_<协议错误>
#[derive(Debug, Clone, PartialEq)]
pub struct CdpError {
    pub code: String,
    pub message: String,
}

impl CdpError {
    fn disconnected() -> Self {
        Self { code: "DISCONNECTED".into(), message: "CDP connection is closed".into() }
    }
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

pub struct Event {
    pub session: Option<String>,
    pub method: String,
    pub params: Value,
}

enum Outbound {
    Call {
        method: String,
        params: Value,
        session: Option<String>,
        resp: SyncSender<Result<Value, CdpError>>,
    },
    Shutdown,
}

type Pending = HashMap<u64, SyncSender<Result<Value, CdpError>>>;

pub struct Connection {
    outbound: Sender<Outbound>,
    events: Receiver<Event>,
    connected: Arc<AtomicBool>,
}

/// 纯路由:按 id 配对响应、无 id 的按事件外抛。抽出为自由函数以便离线单测。
pub fn route_inbound(text: &str, pending: &mut Pending, events: &mut Vec<Event>) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("non-JSON message: {e}"))?;
    if let Some(id) = v.get("id").and_then(Value::as_u64) {
        if let Some(tx) = pending.remove(&id) {
            if let Some(err) = v.get("error") {
                let code = match err.get("code").and_then(Value::as_i64) {
                    Some(c) => format!("CDP_{c}"),
                    None => "CDP_ERROR".to_string(),
                };
                let mut message = err.get("message").and_then(Value::as_str).unwrap_or("(no message)").to_string();
                if let Some(data) = err.get("data").and_then(Value::as_str) {
                    message.push_str("; data: ");
                    message.push_str(data);
                }
                let _ = tx.send(Err(CdpError { code, message }));
            } else {
                let _ = tx.send(Ok(v.get("result").cloned().unwrap_or(Value::Null)));
            }
        }
        return Ok(());
    }
    if let Some(method) = v.get("method").and_then(Value::as_str) {
        events.push(Event {
            session: v.get("sessionId").and_then(Value::as_str).map(str::to_string),
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
                Outbound::Shutdown => {
                    let _ = ws.close(None);
                    connected.store(false, Ordering::SeqCst);
                    return;
                }
                Outbound::Call { method, params, session, resp } => {
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
                            let _ = resp.send(Err(CdpError { code: "SEND".into(), message }));
                        }
                    }
                }
            }
        }

        outbox.retain(|text| match ws.send(tungstenite::Message::text(text.clone())) {
            Ok(()) => false,
            Err(tungstenite::Error::Io(ref e)) if e.kind() == ErrorKind::WouldBlock => true,
            Err(_) => true,
        });

        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                let mut evbuf = Vec::new();
                if let Err(err) = route_inbound(t.as_str(), &mut pending, &mut evbuf) {
                    eprintln!("[cdp] routing problem: {err}");
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
    let rest = url.strip_prefix("ws://").ok_or_else(|| CdpError {
        code: "CONNECT".into(),
        message: format!("only ws:// endpoints are supported, got: {url}"),
    })?;
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = authority.rsplit_once(':').ok_or_else(|| CdpError {
        code: "CONNECT".into(),
        message: format!("endpoint carries no port: {url}"),
    })?;
    let port: u16 = port.parse().map_err(|_| CdpError {
        code: "CONNECT".into(),
        message: format!("bad port in endpoint: {url}"),
    })?;
    Ok((host.to_string(), port))
}

impl Connection {
    pub fn connect(url: &str) -> Result<Self, CdpError> {
        use tungstenite::client::IntoClientRequest;
        // 自建 TcpStream 走握手,完成后才能切非阻塞(connect() 返回的 MaybeTlsStream 不暴露该控制)
        let (host, port) = ws_host_port(url)?;
        let stream = std::net::TcpStream::connect((host.as_str(), port)).map_err(|e| CdpError {
            code: "CONNECT".into(),
            message: format!("tcp connect to {host}:{port}: {e}"),
        })?;
        let request = url.into_client_request().map_err(|e| CdpError {
            code: "CONNECT".into(),
            message: format!("bad ws url {url}: {e}"),
        })?;
        let (ws, _response) = tungstenite::client(request, stream).map_err(|e| CdpError {
            code: "CONNECT".into(),
            message: format!("handshake with {url}: {e}"),
        })?;
        ws.get_ref()
            .set_nonblocking(true)
            .map_err(|e| CdpError { code: "CONNECT".into(), message: format!("cannot switch to non-blocking: {e}") })?;
        let (out_tx, out_rx) = channel::<Outbound>();
        let (ev_tx, ev_rx) = channel::<Event>();
        let connected = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&connected);
        thread::spawn(move || run_loop(ws, out_rx, ev_tx, flag));
        Ok(Self { outbound: out_tx, events: ev_rx, connected })
    }

    pub fn call(
        &self,
        method: &str,
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
                method: method.to_string(),
                params,
                session: session.map(str::to_string),
                resp: tx,
            })
            .map_err(|_| CdpError::disconnected())?;
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(CdpError {
                code: "TIMEOUT".into(),
                message: format!("no response within {}ms: {method}", timeout.as_millis()),
            }),
            Err(RecvTimeoutError::Disconnected) => Err(CdpError::disconnected()),
        }
    }

    /// 主动断开;后续任何调用都返回 DISCONNECTED。
    pub fn shutdown(&self) {
        let _ = self.outbound.send(Outbound::Shutdown);
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub fn drain_events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(e) = self.events.try_recv() {
            out.push(e);
        }
        out
    }
}

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
        route_inbound(r#"{"id":7,"result":{"frameId":"F1"}}"#, &mut pending, &mut events).unwrap();
        assert!(events.is_empty());
        let got = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(got["frameId"], "F1");
    }

    #[test]
    fn maps_cdp_error_with_code_and_message() {
        let (mut pending, rx) = pending_with(3);
        let mut events = Vec::new();
        route_inbound(
            r#"{"id":3,"error":{"code":-32000,"message":"boom","data":"extra"}}"#,
            &mut pending,
            &mut events,
        )
        .unwrap();
        let err = rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap_err();
        assert_eq!(err.code, "CDP_-32000");
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
}
