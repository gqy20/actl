//! 浏览器 A1 spike 探针:隔离 headless Edge + 自研最小 CDP 客户端全链验证。
//! 验证路径对齐 Python 原型(docs/34):精确 target 绑定、同 URL 干扰页隔离、
//! 导航事件经读线程可达、一次写后核验(CSS/XPath)、断连显式报错、
//! 显式重连只读核验、CDP Browser.close 优雅关闭自有浏览器。
//!
//! stdout 保持空白,诊断走 stderr,报告写 <output>/report.json(实验产物,非 actl 协议)。
//! 退出码:成功 0;已有急停 77;其他失败 1。不删除急停标记,失败时保留浏览器与 profile 并记录清理状态。

mod cdp;
mod write_state;

use cdp::{CdpError, Connection, Event};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpStream, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const NAV_TIMEOUT: Duration = Duration::from_secs(30);
const EDGE_STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const EDGE_EXIT_TIMEOUT: Duration = Duration::from_secs(10);

struct Probe {
    calls: Vec<Value>,
    checks: Vec<Value>,
    core_failures: Vec<String>,
}

impl Probe {
    fn new() -> Self {
        Self { calls: Vec::new(), checks: Vec::new(), core_failures: Vec::new() }
    }

    fn step<T>(&mut self, name: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let t0 = Instant::now();
        let result = f();
        self.calls.push(json!({
            "step": name,
            "ok": result.is_ok(),
            "elapsed_ms": t0.elapsed().as_millis() as u64,
            "error": match &result { Ok(_) => Value::Null, Err(e) => Value::String(e.clone()) },
        }));
        if let Err(e) = &result {
            eprintln!("[probe] step {name} FAILED: {e}");
        } else {
            eprintln!("[probe] step {name} ok ({}ms)", t0.elapsed().as_millis());
        }
        result
    }

    fn check(&mut self, name: &str, ok: bool, detail: impl Into<String>, core: bool) {
        let detail = detail.into();
        eprintln!("[probe] check {name}: {} {}", if ok { "PASS" } else { "FAIL" }, detail);
        self.checks.push(json!({ "name": name, "ok": ok, "core": core, "detail": detail }));
        if core && !ok {
            self.core_failures.push(name.to_string());
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn spawn_http_server(html: String) -> std::io::Result<(u16, thread::JoinHandle<()>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = match stream { Ok(s) => s, Err(_) => continue };
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    Ok((port, handle))
}

fn wait_devtools_port(profile: &Path) -> Option<u16> {
    let deadline = Instant::now() + EDGE_STARTUP_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(profile.join("DevToolsActivePort")) {
            if let Some(port) = text.lines().next().and_then(|l| l.trim().parse::<u16>().ok()) {
                return Some(port);
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    None
}

fn http_get_json(port: u16, path: &str) -> Result<Value, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("tcp connect: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    // Chromium DevTools HTTP 端点不保证按 Connection:close 断开,必须按头解析、按长度读体
    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        let n = s.read(&mut chunk).map_err(|e| format!("read headers from {path}: {e}"))?;
        if n == 0 {
            return Err(format!("connection closed before headers complete: {path}"));
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
        let n = s.read(&mut chunk).map_err(|e| format!("read body from {path}: {e}"))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&body).map_err(|e| format!("bad JSON from {path}: {e}"))
}

fn eval_js(conn: &Connection, session: &str, expr: &str) -> Result<String, CdpError> {
    eval_js_in_context(conn, session, expr, None)
}

/// contextId 定向执行:frame 各有独立执行上下文,跨源 iframe 无需独立 target
/// (headless 未启用站点隔离时 getTargets 不暴露 iframe,这是唯一正路)
fn eval_js_in_context(conn: &Connection, session: &str, expr: &str, context_id: Option<u64>) -> Result<String, CdpError> {
    let mut params = json!({ "expression": expr, "returnByValue": true, "awaitPromise": true });
    if let Some(ctx) = context_id {
        params["contextId"] = json!(ctx);
    }
    let res = conn.call("Runtime.evaluate", params, Some(session), CALL_TIMEOUT)?;
    if let Some(exc) = res.get("exceptionDetails") {
        let text = exc.get("text").and_then(Value::as_str).unwrap_or("(no text)");
        return Err(CdpError { code: "JS_EXCEPTION".into(), message: format!("evaluate raised: {text}") });
    }
    match res.get("result").and_then(|r| r.get("value")) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(v) => Ok(v.to_string()),
        None => Ok(String::new()),
    }
}

fn attach_session(conn: &Connection, target_id: &str) -> Result<String, String> {
    let res = conn
        .call("Target.attachToTarget", json!({ "targetId": target_id, "flatten": true }), None, CALL_TIMEOUT)
        .map_err(|e| e.to_string())?;
    res.get("sessionId").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "attachToTarget returned no sessionId".into())
}

fn wait_event<F: Fn(&Event) -> bool>(conn: &Connection, backlog: &mut Vec<Event>, timeout: Duration, pred: F) -> Result<Event, String> {
    let deadline = Instant::now() + timeout;
    loop {
        backlog.extend(conn.drain_events());
        if let Some(pos) = backlog.iter().position(|e| pred(e)) {
            return Ok(backlog.remove(pos));
        }
        if Instant::now() >= deadline {
            return Err(format!("event not observed within {}ms (backlog has {} events)", timeout.as_millis(), backlog.len()));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_child_exit(child: &mut Child) -> Result<Option<i32>, String> {
    let deadline = Instant::now() + EDGE_EXIT_TIMEOUT;
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => return Ok(status.code()),
            None if Instant::now() >= deadline => return Err("browser did not exit within timeout".into()),
            None => thread::sleep(Duration::from_millis(100)),
        }
    }
}

struct Args {
    browser_executable: PathBuf,
    output: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut browser = None;
    let mut output = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--browser-executable" => browser = it.next().map(PathBuf::from),
            "--output" => output = it.next().map(PathBuf::from),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        browser_executable: browser.ok_or("--browser-executable is required")?,
        output: output.ok_or("--output is required")?,
    })
}

fn stop_requested() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let p = Path::new(&local).join("actl").join("stop-requested");
    p.exists().then_some(p)
}

fn main() {
    if let Some(stop) = stop_requested() {
        eprintln!("[probe] active stop request found, refusing to run: {}", stop.display());
        std::process::exit(77);
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[probe] {e}");
            std::process::exit(1);
        }
    };
    let code = run(args);
    std::process::exit(code);
}

fn run(args: Args) -> i32 {
    let mut probe = Probe::new();
    let t_start = Instant::now();
    // 启动前必须绝对化:相对 user-data-dir 会被 Edge 静默弃用并落入默认目录,
    // 136+ 规则随即禁用调试端口,Browser.close 永久不可达(2026-10-02 真机实证)
    let output = std::env::current_dir()
        .map(|d| d.join(&args.output))
        .unwrap_or_else(|_| args.output.clone());
    let output = &output;
    let profile = output.join("profile");
    let mut cleanup = "not_started".to_string();
    let mut child: Option<Child> = None;
    let mut conn: Option<Connection> = None;
    let mut browser_ws_url = String::new();
    let mut env_extras: Vec<(String, Value)> = Vec::new();

    let outcome = (|| -> Result<(), String> {
        std::fs::create_dir_all(output).map_err(|e| format!("create output dir: {e}"))?;
        std::fs::create_dir_all(&profile).map_err(|e| format!("create profile dir: {e}"))?;

        let browser_bytes = std::fs::read(&args.browser_executable).map_err(|e| format!("read browser exe: {e}"))?;
        env_extras.push(("browser_exe_sha256".into(), Value::String(sha256_hex(&browser_bytes))));

        // 双端口本地 HTTP:主页与 iframe 各自独立 origin(端口不同即跨源)
        let (frame_port, _frame_srv) = spawn_http_server(FRAME_HTML.to_string()).map_err(|e| format!("frame server: {e}"))?;
        let main_html = MAIN_HTML_TEMPLATE.replace("FRAME_PORT_MARKER", &frame_port.to_string());
        let (main_port, _main_srv) = spawn_http_server(main_html).map_err(|e| format!("main server: {e}"))?;
        let page_url = format!("http://127.0.0.1:{main_port}/probe.html");
        env_extras.push(("page_url".into(), Value::String(page_url.clone())));

        let stdout_log = std::fs::File::create(output.join("edge-stdout.log")).map_err(|e| e.to_string())?;
        let stderr_log = std::fs::File::create(output.join("edge-stderr.log")).map_err(|e| e.to_string())?;
        let edge_args = [
            "--headless=new".to_string(),
            "--remote-debugging-port=0".to_string(),
            format!("--user-data-dir={}", profile.display()),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            "about:blank".to_string(),
        ];
        let launched = Command::new(&args.browser_executable)
            .args(&edge_args)
            .stdout(Stdio::from(stdout_log))
            .stderr(Stdio::from(stderr_log))
            .spawn()
            .map_err(|e| format!("launch browser: {e}"))?;
        child = Some(launched);
        eprintln!("[probe] browser launched, waiting for DevToolsActivePort");

        let port = wait_devtools_port(&profile).ok_or("DevToolsActivePort not written within timeout")?;
        env_extras.push(("devtools_port".into(), json!(port)));
        let version = http_get_json(port, "/json/version")?;
        let browser_version = version.get("Browser").and_then(Value::as_str).unwrap_or("(unknown)").to_string();
        browser_ws_url = version
            .get("webSocketDebuggerUrl")
            .and_then(Value::as_str)
            .ok_or("no webSocketDebuggerUrl in /json/version")?
            .to_string();
        env_extras.push(("browser_version".into(), Value::String(browser_version)));
        eprintln!("[probe] devtools endpoint ready on port {port}");

        let c = Connection::connect(&browser_ws_url).map_err(|e| format!("ws connect: {e}"))?;
        conn = Some(c);

        // —— 会话绑定:两个同 URL target,操作只落在精确绑定的那个 ——
        let target_main = probe.step("create main target", || {
            conn.as_ref().unwrap()
                .call("Target.createTarget", json!({ "url": page_url }), None, CALL_TIMEOUT)
                .map_err(|e| e.to_string())?
                .get("targetId").and_then(Value::as_str).map(str::to_string)
                .ok_or_else(|| "createTarget returned no targetId".to_string())
        })?;
        let target_decoy = probe.step("create decoy target (same url)", || {
            conn.as_ref().unwrap()
                .call("Target.createTarget", json!({ "url": page_url }), None, CALL_TIMEOUT)
                .map_err(|e| e.to_string())?
                .get("targetId").and_then(Value::as_str).map(str::to_string)
                .ok_or_else(|| "createTarget returned no targetId".to_string())
        })?;
        env_extras.push(("target_main".into(), Value::String(target_main.clone())));
        env_extras.push(("target_decoy".into(), Value::String(target_decoy.clone())));

        let session_main = probe.step("attach main session", || attach_session(conn.as_ref().unwrap(), &target_main))?;
        let session_decoy = probe.step("attach decoy session", || attach_session(conn.as_ref().unwrap(), &target_decoy))?;
        env_extras.push(("session_main".into(), Value::String(session_main.clone())));

        let c = conn.as_ref().unwrap();
        probe.step("enable Page+Runtime on main", || {
            c.call("Page.enable", json!({}), Some(&session_main), CALL_TIMEOUT).map_err(|e| e.to_string())?;
            c.call("Runtime.enable", json!({}), Some(&session_main), CALL_TIMEOUT).map_err(|e| e.to_string())?;
            Ok(())
        })?;

        // —— 导航竞态前提:读线程必须能收到 frameNavigated ——
        let nav = probe.step("navigate main target", || {
            conn.as_ref().unwrap()
                .call("Page.navigate", json!({ "url": page_url }), Some(&session_main), CALL_TIMEOUT)
                .map_err(|e| e.to_string())
        })?;
        let frame_id = nav.get("frameId").and_then(Value::as_str).unwrap_or("").to_string();
        let mut backlog: Vec<Event> = Vec::new();
        let ev = probe.step("observe Page.frameNavigated via reader thread", || {
            wait_event(
                conn.as_ref().unwrap(),
                &mut backlog,
                NAV_TIMEOUT,
                |e| {
                    e.method == "Page.frameNavigated"
                        && e.session.as_deref() == Some(session_main.as_str())
                        && e.params["frame"]["id"].as_str() == Some(frame_id.as_str())
                        && e.params["frame"].get("parentId").is_none()
                },
            )
        });
        probe.check("nav_event_via_reader_thread", ev.is_ok(), format!("frame {frame_id}"), true);

        // 等主文档就绪,再等 iframe 文档就绪(探测不到就继续,iframe 检查自会裁决)
        probe.step("wait document readyState=complete", || {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                if eval_js(conn.as_ref().unwrap(), &session_main, "document.readyState").map_err(|e| e.to_string())? == "complete" {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err("main document never reached complete".into());
                }
                thread::sleep(Duration::from_millis(100));
            }
        })?;

        // —— 一次写后核验:CSS 点击 + 计数核验 + XPath 读回 ——
        let before = probe.step("read counter before write", || {
            eval_js(conn.as_ref().unwrap(), &session_main, "String(window.__clicks)").map_err(|e| e.to_string())
        })?;
        probe.check("initial_counter_zero", before == "0", format!("before={before}"), true);

        let mut gate = write_state::WriteGate::new();
        let attempt = probe.step("begin write (enter uncertain)", || gate.begin())?;
        let clicked = probe.step("dispatch single click via JS", || {
            eval_js(
                conn.as_ref().unwrap(),
                &session_main,
                "(function(){document.getElementById('actl-btn').click();return String(window.__clicks);})()",
            )
            .map_err(|e| e.to_string())
        });
        let xpath_read = probe.step("read back via XPath", || {
            eval_js(
                conn.as_ref().unwrap(),
                &session_main,
                "(function(){var r=document.evaluate('//span[@id=\"actl-count\"]/text()',document,null,XPathResult.FIRST_ORDERED_NODE_TYPE,null);return r.singleNodeValue?r.singleNodeValue.textContent:'(none)';})()",
            )
            .map_err(|e| e.to_string())
        });
        let write_ok = matches!((&clicked, &xpath_read), (Ok(a), Ok(x)) if a.as_str() == "1" && x.as_str() == "1");
        probe.check("write_then_verify_css_xpath", write_ok, format!("counter={:?} xpath={:?}", clicked, xpath_read), true);
        if write_ok {
            gate.confirm(attempt);
        }
        probe.check(
            "gate_state_after_verify",
            gate.state() == write_state::WriteState::Idle,
            format!("state={:?}", gate.state()),
            true,
        );

        // —— 探索项:跨源 iframe 定位与操作 ——
        // headless Edge 未启用站点隔离,getTargets 不暴露 iframe target;
        // 改走 frame 执行上下文:Page.getFrameTree 找子 frame,事件流里配对其 contextId
        let oopif = probe.step("locate cross-origin iframe execution context", || {
            let c = conn.as_ref().unwrap();
            backlog.extend(c.drain_events());
            let tree = c.call("Page.getFrameTree", json!({}), Some(&session_main), CALL_TIMEOUT).map_err(|e| e.to_string())?;
            let children = tree.pointer("/frameTree/childFrames").and_then(Value::as_array).cloned().unwrap_or_default();
            let child = children
                .iter()
                .find(|f| f.pointer("/frame/id").and_then(Value::as_str).is_some())
                .ok_or_else(|| format!("no child frames in frame tree: {tree}"))?;
            let child_id = child.pointer("/frame/id").and_then(Value::as_str).unwrap_or("").to_string();
            let ctx = backlog.iter().rev().find_map(|e| {
                if e.method != "Runtime.executionContextCreated" {
                    return None;
                }
                let ctx = e.params.get("context")?;
                let aux = ctx.get("auxData")?;
                if aux.get("frameId").and_then(Value::as_str) == Some(child_id.as_str()) {
                    ctx.get("id").and_then(Value::as_u64)
                } else {
                    None
                }
            });
            match ctx {
                Some(ctx_id) => Ok((child_id, ctx_id)),
                None => Err(format!(
                    "no execution context observed for iframe frame {child_id} (backlog {} events)",
                    backlog.len()
                )),
            }
        });
        match oopif {
            Ok((child_id, ctx_id)) => {
                env_extras.push(("iframe_frame_id".into(), Value::String(child_id.clone())));
                env_extras.push(("iframe_context_id".into(), json!(ctx_id)));
                let frame_clicked = eval_js_in_context(
                    conn.as_ref().unwrap(),
                    &session_main,
                    "(function(){document.getElementById('frame-btn').click();return String(window.__f);})()",
                    Some(ctx_id),
                );
                let frame_read = eval_js_in_context(conn.as_ref().unwrap(), &session_main, "String(window.__f)", Some(ctx_id));
                let ok = frame_clicked.as_deref() == Ok("1") && frame_read.as_deref() == Ok("1");
                probe.check(
                    "cross_origin_iframe_click_and_verify",
                    ok,
                    format!("{:?}/{:?}", frame_clicked, frame_read),
                    false,
                );
            }
            Err(detail) => {
                probe.check("cross_origin_iframe_click_and_verify", false, format!("context not located: {detail}"), false);
            }
        }

        // —— 同 URL 干扰页隔离:decoy 计数必须未被波及 ——
        let decoy_count = probe.step("read decoy counter", || {
            eval_js(conn.as_ref().unwrap(), &session_decoy, "String(window.__clicks)").map_err(|e| e.to_string())
        });
        probe.check("decoy_same_url_untouched", decoy_count.as_deref() == Ok("0"), format!("decoy={decoy_count:?}"), true);

        // —— 显式断连:后续调用必须明确报 DISCONNECTED ——
        probe.step("client-side disconnect", || {
            conn.as_ref().unwrap().shutdown();
            let deadline = Instant::now() + Duration::from_secs(3);
            while conn.as_ref().unwrap().is_connected() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        })?;
        let after_drop = eval_js(conn.as_ref().unwrap(), &session_main, "String(window.__clicks)");
        probe.check(
            "post_disconnect_error_is_explicit",
            matches!(&after_drop, Err(e) if e.code == "DISCONNECTED"),
            format!("{after_drop:?}"),
            true,
        );

        // —— 显式重连:只读核验,不重放点击 ——
        let reconnected = Connection::connect(&browser_ws_url).map_err(|e| format!("reconnect: {e}"));
        match reconnected {
            Ok(c2) => {
                let s2 = attach_session(&c2, &target_main)?;
                let reread = eval_js(&c2, &s2, "String(window.__clicks)");
                probe.check("reconnect_readonly_recheck", reread.as_deref() == Ok("1"), format!("{reread:?}"), true);
                // —— 优雅关闭自有浏览器 ——
                let close = c2.call("Browser.close", json!({}), None, CALL_TIMEOUT);
                let exit = match &mut child {
                    Some(c) => wait_child_exit(c),
                    None => Err("no child handle".into()),
                };
                let closed = close.is_ok() && exit.is_ok();
                probe.check(
                    "owned_browser_closed_gracefully",
                    closed,
                    format!("close={:?} exit={:?}", close.map(|_| "ok").map_err(|e| e.to_string()), exit),
                    true,
                );
                cleanup = if closed { "owned_browser_closed".into() } else { "close_attempted_check_logs".into() };
                let _ = conn.take();
                conn = Some(c2);
            }
            Err(detail) => {
                probe.check("reconnect_readonly_recheck", false, detail, true);
            }
        }

        Ok(())
    })();

    // 收尾判定:探针已尽力清理;失败时保留浏览器与 profile 供检查,绝不强杀
    if outcome.is_err() {
        if let Some(c) = conn.as_ref() {
            if c.is_connected() {
                let _ = c.call("Browser.close", json!({}), None, CALL_TIMEOUT);
            }
        }
        if let Some(ch) = child.as_mut() {
            match wait_child_exit(ch) {
                Ok(_) => cleanup = "closed_after_failure".into(),
                Err(_) => cleanup = "left_running_after_failure_kept_for_inspection".into(),
            }
        } else {
            cleanup = "not_started".into();
        }
        if let Err(e) = &outcome {
            eprintln!("[probe] FAILED: {e}");
        }
    }

    let mut env = json!({
        "os": std::env::consts::OS,
        "browser_executable": args.browser_executable.display().to_string(),
        "profile_dir": profile.display().to_string(),
        "output_dir": output.display().to_string(),
        "generated_unix": SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "wall_ms": t_start.elapsed().as_millis() as u64,
        "source_sha256": {
            "cdp.rs": sha256_hex(include_str!("cdp.rs").as_bytes()),
            "write_state.rs": sha256_hex(include_str!("write_state.rs").as_bytes()),
            "main.rs": sha256_hex(include_str!("main.rs").as_bytes()),
            "Cargo.toml": sha256_hex(include_str!("../Cargo.toml").as_bytes()),
        },
    });
    for (k, v) in env_extras {
        env[k] = v;
    }
    let report = json!({
        "probe": "browser-rs A1 spike",
        "env": env,
        "calls": probe.calls,
        "checks": probe.checks,
        "core_failures": probe.core_failures,
        "cleanup": cleanup,
        "outcome": if outcome.is_ok() { "pass" } else { "fail" },
    });
    match serde_json::to_string_pretty(&report) {
        Ok(text) => {
            let path = output.join("report.json");
            if let Err(e) = std::fs::write(&path, text) {
                eprintln!("[probe] cannot write report: {e}");
            } else {
                eprintln!("[probe] report written: {}", path.display());
            }
        }
        Err(e) => eprintln!("[probe] cannot serialize report: {e}"),
    }

    if outcome.is_ok() && probe.core_failures.is_empty() { 0 } else { 1 }
}

const MAIN_HTML_TEMPLATE: &str = r#"<!doctype html><html><body><button id="actl-btn">probe</button><span id="actl-count">0</span><iframe id="cross" src="http://127.0.0.1:FRAME_PORT_MARKER/frame.html" width="240" height="80"></iframe><script>window.__clicks=0;document.getElementById('actl-btn').addEventListener('click',function(){window.__clicks++;document.getElementById('actl-count').textContent=String(window.__clicks);});</script></body></html>"#;

const FRAME_HTML: &str = r#"<!doctype html><html><body><button id="frame-btn">f</button><span id="frame-count">0</span><script>window.__f=0;document.getElementById('frame-btn').addEventListener('click',function(){window.__f++;document.getElementById('frame-count').textContent=String(window.__f);});</script></body></html>"#;
