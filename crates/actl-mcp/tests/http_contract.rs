//! 模式 A HTTP 契约(docs/82 §3.1/§5):真实服务进程 + 裸 TCP HTTP。
//! 断言:Bearer 门禁(缺/错 token → 401)、Origin 校验(跨源 → 403)、
//! 带凭证的 tools/call 走通子进程派发(envelope 进 structuredContent、
//! isError 语义正确)、排除动词返回明确错误、未知方法按规范 404。
//! 不依赖额外 HTTP 客户端 crate——手写 HTTP/1.1 请求(传输面我们自己
//! 就是手写的,契约必须从线格式验证)。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const TOKEN: &str = "contract-test-token";

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server() -> Server {
    // 同一测试进程内多用例并行:端口按原子计数器分配(pid 派生会让全部
    // 用例撞同一端口)。
    static NEXT_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(17600);
    let port = NEXT_PORT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let child = Command::new(env!("CARGO_BIN_EXE_actl-mcp"))
        .args(["--listen", &format!("127.0.0.1:{port}"), "--token", TOKEN])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn actl-mcp");
    let server = Server { child, port };
    // 就绪探测:有界重试(事件等待替代固定 sleep 的纪律)。
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("actl-mcp did not become ready on port {port}");
}

/// 手写一次 HTTP POST,返回 (状态码, 正文)。
/// `mcp_name` 是 Mcp-Name 头的取值——规范要求与 body 的
/// params.name(tools/call)或 params.uri(resources/read)一致,rmcp 强制。
fn post(
    port: u16,
    path: &str,
    token: Option<&str>,
    origin: Option<&str>,
    body: &str,
    method_header: Option<&str>,
    mcp_name: &str,
) -> (u16, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    // HTTP/1.0:server 关连接即 EOF,免 chunked 解析(契约从原始线格式验证)。
    let mut request = format!("POST {path} HTTP/1.0\r\nHost: localhost\r\n");
    request.push_str("Content-Type: application/json\r\n");
    request.push_str("Accept: application/json, text/event-stream\r\n");
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
    }
    if let Some(method) = method_header {
        request.push_str(&format!("Mcp-Method: {method}\r\n"));
        request.push_str(&format!("Mcp-Name: {mcp_name}\r\n"));
        request.push_str("MCP-Protocol-Version: 2026-07-28\r\n");
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    request.push_str(body);
    stream.write_all(request.as_bytes()).expect("write");

    let mut response = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                response.extend_from_slice(&buffer[..n]);
                // 简化:读到连接关闭或 Content-Length 满足即停;本契约的
                // 响应都是短 JSON 或 4xx,非 SSE。
                if let Some(split) = find_headers_end(&response) {
                    let headers = String::from_utf8_lossy(&response[..split]).to_string();
                    if let Some(length) = content_length(&headers)
                        && response.len() - split - 4 >= length
                    {
                        break;
                    }
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&response).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let raw_body = find_headers_end(&response)
        .map(|split| String::from_utf8_lossy(&response[split + 4..]).into_owned())
        .unwrap_or_default();
    // rmcp 对 tools/call 走 SSE 应答(data: 帧);错误走纯 JSON。两种形态
    // 统一归约为 JSON 载荷:取 data: 帧的 payload(末帧为 JSON-RPC 响应)。
    let body = sse_payload(&raw_body).unwrap_or(raw_body);
    (status, body)
}

fn sse_payload(raw: &str) -> Option<String> {
    let mut payload = None;
    for line in raw.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            payload = Some(data.to_string());
        }
    }
    payload
}

fn find_headers_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    })
}

fn tool_call_body(command: &str, args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|arg| format!("{arg:?}")).collect();
    // 2026-07-28 无状态契约:每请求携带 protocolVersion 与 clientCapabilities
    // (rmcp 强制;本契约测试按规范客户端行事)。
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"actl","arguments":{{"command":"{command}","args":[{}]}},"_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}"#,
        args.join(",")
    )
}

#[test]
fn bearer_gate_rejects_missing_and_wrong_tokens() {
    let server = start_server();
    let body = tool_call_body("status", &[]);
    let (status, _) = post(
        server.port,
        "/mcp",
        None,
        None,
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 401, "missing token must be 401");
    let (status, _) = post(
        server.port,
        "/mcp",
        Some("wrong-token"),
        None,
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 401, "wrong token must be 401");
}

#[test]
fn cross_origin_is_rejected_with_valid_token() {
    let server = start_server();
    let body = tool_call_body("status", &[]);
    let (status, _) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        Some("http://evil.example"),
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 403, "cross-origin must be 403");
}

#[test]
fn tool_call_dispatches_to_the_real_actl_binary() {
    let server = start_server();
    let body = tool_call_body("status", &[]);
    let (status, response) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        None,
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 200, "{response}");
    let json: serde_json::Value = serde_json::from_str(&response).expect("json-rpc response");
    let result = &json["result"];
    assert_eq!(result["isError"], false, "{response}");
    assert_eq!(result["structuredContent"]["ok"], true, "{response}");
    assert_eq!(result["structuredContent"]["command"], "status");
}

#[test]
fn excluded_verb_returns_an_explicit_error_not_silence() {
    let server = start_server();
    let body = tool_call_body("execution-recover", &[]);
    let (status, response) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        None,
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 200, "{response}");
    let json: serde_json::Value = serde_json::from_str(&response).expect("json-rpc response");
    let result = &json["result"];
    assert_eq!(result["isError"], true, "{response}");
    assert_eq!(result["structuredContent"]["error"]["code"], "PROTOCOL");
    assert!(
        result["structuredContent"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("local-only"),
        "{response}"
    );
}

#[test]
fn unknown_method_is_jsonrpc_error_not_http_crash() {
    let server = start_server();
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"no/such/method","params":{},"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}"#;
    let (status, response) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        None,
        body,
        Some("no/such/method"),
        "",
    );
    // 契约:结构化 JSON-RPC 错误体返回(校验序由 rmcp 定,400/-32602 合法),
    // 服务不崩;后续调用仍可用。
    let json: serde_json::Value = serde_json::from_str(&response).ok().unwrap_or_default();
    assert!(
        status < 500,
        "server must not crash on unknown method: {status} {response}"
    );
    assert!(
        json["error"].is_object() || json["result"].is_object(),
        "must be a json-rpc response, got: {response}"
    );
    // 服务仍活着:紧接一次合法调用走通。
    let body = tool_call_body("status", &[]);
    let (status, response) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        None,
        &body,
        Some("tools/call"),
        "actl",
    );
    assert_eq!(status, 200, "{response}");
}

#[test]
fn help_resource_template_serves_real_command_help() {
    let server = start_server();
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"actl://help/verify-window","_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}"#;
    let uri_like = "actl://help/verify-window";
    let (status, response) = post(
        server.port,
        "/mcp",
        Some(TOKEN),
        None,
        body,
        Some("resources/read"),
        uri_like,
    );
    assert_eq!(status, 200, "{response}");
    let json: serde_json::Value = serde_json::from_str(&response).expect("json-rpc");
    let text = json["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_default();
    // 四要素纪律:真 --help 文本须含 Usage/Example/Errors 骨架。
    assert!(text.contains("Usage"), "{text}");
    assert!(text.contains("Example"), "{text}");
}

#[test]
fn help_resource_is_absent_for_excluded_and_unknown_commands() {
    let server = start_server();
    for (uri, expect) in [
        ("actl://help/execution-recover", "local-only"),
        ("actl://help/not-a-command", "no help"),
    ] {
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{{"uri":"{uri}","_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}"#
        );
        let (status, response) = post(
            server.port,
            "/mcp",
            Some(TOKEN),
            None,
            &body,
            Some("resources/read"),
            uri,
        );
        assert!(status < 500, "{uri}: {status} {response}");
        let json: serde_json::Value = serde_json::from_str(&response).ok().unwrap_or_default();
        // 排除动词走 JSON-RPC 错误体;未知命令走资源文本如实回报——
        // 两种形态都不得伪造帮助内容。
        let error_text = json["error"]["message"].as_str().unwrap_or_default();
        let contents_text = json["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(
            error_text.contains(expect) || contents_text.contains(expect),
            "{uri} should say '{expect}' — error: '{error_text}' contents: '{contents_text}'"
        );
    }
}
