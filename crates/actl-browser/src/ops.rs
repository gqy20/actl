//! 浏览器命令执行体:launch/navigate/read/click/close。
//!
//! 语义要点(docs/72):
//! - launch:绝对 user-data-dir + DevToolsActivePort 出现 = 启动成功判定;失败 fail-fast
//!   并回收自启进程(§11 不变量:自启实例必须总能经 CDP 优雅关闭);
//! - 写操作(click)先过停止代次核验,再重核绑定 target 存活,再定位唯一匹配,派发后读回;
//! - cookie 域不在实现面内(构造性不可达,lib.rs 范围铁律)。

use crate::SelectorKind;
use crate::cdp::{self, CdpError, Connection};
use crate::session::{self, BrowserSession, Mode, Paths};
use actl_core::state::unix_ms;
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LAUNCH_DEFAULT_TIMEOUT_MS: u64 = 15_000;
const OP_DEFAULT_TIMEOUT_MS: u64 = 10_000;
const TEXT_LIMIT: usize = 65_536;

pub struct LaunchSpec {
    pub browser_executable: String,
    pub url: Option<String>,
    pub profile: Option<String>,
    /// 出站代理(HOST:PORT,无 scheme);显式本地代理路径,非任意参数注入
    pub proxy: Option<String>,
    pub visible: bool,
    pub timeout_ms: Option<u64>,
}

fn protocol(message: impl Into<String>) -> CtlError {
    CtlError::new(ErrorCode::Protocol, message)
}

/// CDP 连接层错误 → envelope 错误码(docs/72 §9 错误映射)。
fn map_cdp(err: CdpError) -> CtlError {
    match err.code {
        "CONNECT" => CtlError::with_evidence(
            ErrorCode::NotFound,
            format!(
                "browser devtools endpoint unreachable (browser exited or session expired): {}",
                err.message
            ),
            json!({ "cdp_code": err.code }),
        ),
        "TIMEOUT" => CtlError::new(ErrorCode::Timeout, err.message),
        _ => CtlError::with_evidence(
            ErrorCode::Internal,
            format!("CDP layer failure: {err}"),
            json!({ "cdp_code": err.code }),
        ),
    }
}

fn dur(timeout_ms: Option<u64>, default: u64) -> Duration {
    Duration::from_millis(timeout_ms.filter(|t| *t > 0).unwrap_or(default))
}

fn validate_url(url: &str) -> Result<(), CtlError> {
    let ok = url.len() <= 2048
        && ["http://", "https://", "about:blank", "file://"]
            .iter()
            .any(|p| url.starts_with(p));
    if ok {
        Ok(())
    } else {
        Err(protocol(format!(
            "url must start with http:// https:// file:// or be about:blank: {url}"
        )))
    }
}

fn validate_selector(kind: SelectorKind, value: &str) -> Result<(), CtlError> {
    if value.is_empty() || value.len() > 4096 {
        return Err(protocol(format!(
            "{} selector requires 1-4096 characters",
            kind.as_str()
        )));
    }
    Ok(())
}

/// 代理仅接受裸 HOST:PORT(拒绝 scheme/凭据),防参数注入面
fn validate_proxy(value: &str) -> Result<(), CtlError> {
    let Some((host, port)) = value.rsplit_once(':') else {
        return Err(protocol(format!(
            "--proxy requires HOST:PORT, got: {value}"
        )));
    };
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'[' || b == b']')
    {
        return Err(protocol(format!(
            "--proxy host must be alphanumeric/dots/brackets: {value}"
        )));
    }
    let valid_port = port.parse::<u16>().is_ok_and(|p| (1..=65535).contains(&p));
    if !valid_port {
        return Err(protocol(format!("--proxy port must be 1-65535: {value}")));
    }
    Ok(())
}

/// JS 定位脚本:唯一匹配为硬条件,返回 JSON.stringify 的结果对象。
/// 选择器以 JSON 字符串字面量嵌入(serde_json 转义,防注入)。
fn locate_script(kind: SelectorKind, value: &str) -> String {
    let embedded = serde_json::to_string(value).unwrap_or_default();
    match kind {
        SelectorKind::Css => format!(
            "(function(){{var els=document.querySelectorAll({embedded});\
             if(els.length===0)return JSON.stringify({{matches:0}});\
             if(els.length>1)return JSON.stringify({{matches:els.length}});\
             var e=els[0];\
             return JSON.stringify({{matches:1,text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName,id:e.id||null,name:e.getAttribute('name')||null}});}})()"
        ),
        SelectorKind::Xpath => format!(
            "(function(){{var it=document.evaluate({embedded},document,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);\
             if(it.snapshotLength===0)return JSON.stringify({{matches:0}});\
             if(it.snapshotLength>1)return JSON.stringify({{matches:it.snapshotLength}});\
             var e=it.snapshotItem(0);\
             return JSON.stringify({{matches:1,text:e.textContent==null?'':String(e.textContent),\
             tag:e.tagName||null}});}})()"
        ),
    }
}

/// Runtime.evaluate;脚本约定返回 JSON 字符串(内部脚本,非用户表达式)。
fn eval_json(
    conn: &Connection,
    cdp_session: &str,
    script: &str,
    context_id: Option<u64>,
    timeout: Duration,
) -> Result<Value, CtlError> {
    let mut params = json!({ "expression": script, "returnByValue": true, "awaitPromise": true });
    if let Some(ctx) = context_id {
        params["contextId"] = json!(ctx);
    }
    let res = conn
        .call("Runtime.evaluate", params, Some(cdp_session), timeout)
        .map_err(map_cdp)?;
    if let Some(exc) = res.get("exceptionDetails") {
        let text = exc
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("(no text)");
        return Err(CtlError::with_evidence(
            ErrorCode::Internal,
            format!("page script raised: {text}"),
            json!({ "exception": text }),
        ));
    }
    let raw = match res.pointer("/result/value").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return Err(CtlError::internal("page script returned no value")),
    };
    serde_json::from_str(&raw)
        .map_err(|_| CtlError::internal("page script returned non-JSON payload"))
}

fn connect_browser(session: &BrowserSession) -> Result<Connection, CtlError> {
    let url = format!(
        "ws://127.0.0.1:{}{}",
        session.devtools_port, session.browser_ws_path
    );
    Connection::connect(&url).map_err(map_cdp)
}

fn attach_target(
    conn: &Connection,
    target_id: &str,
    timeout: Duration,
) -> Result<String, CtlError> {
    let res = conn
        .call(
            "Target.attachToTarget",
            json!({ "targetId": target_id, "flatten": true }),
            None,
            timeout,
        )
        .map_err(map_cdp)?;
    res.get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CtlError::internal("attachToTarget returned no sessionId"))
}

/// 绑定重核:target 必须仍存在(导航竞态防护的第一道闸,docs/72 §6)。
fn verify_target_alive(
    conn: &Connection,
    target_id: &str,
    timeout: Duration,
) -> Result<(), CtlError> {
    let res = conn.call(
        "Target.getTargetInfo",
        json!({ "targetId": target_id }),
        None,
        timeout,
    );
    match res {
        Ok(_) => Ok(()),
        Err(e) => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!(
                "bound target no longer exists (new tab took over or browser restarted); \
                 re-run browser-navigate or browser-launch: {e}"
            ),
            json!({ "target_id": target_id }),
        )),
    }
}

/// 跨源 iframe 定位:主 frame 的首个子 frame + 其执行上下文。
/// headless 未启用站点隔离时 iframe 无独立 target,上下文路线是唯一正路(docs/72 §10)。
fn iframe_context(
    conn: &Connection,
    cdp_session: &str,
    timeout: Duration,
) -> Result<(String, u64), CtlError> {
    let tree = conn
        .call("Page.getFrameTree", json!({}), Some(cdp_session), timeout)
        .map_err(map_cdp)?;
    let children = tree
        .pointer("/frameTree/childFrames")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let child_id = children
        .iter()
        .find_map(|f| f.pointer("/frame/id").and_then(Value::as_str))
        .ok_or_else(|| {
            CtlError::new(
                ErrorCode::NotFound,
                "page has no child iframe to address (--iframe is not applicable here)",
            )
        })?
        .to_string();
    // Runtime.enable 会重放既有上下文的 executionContextCreated 事件
    conn.call("Runtime.enable", json!({}), Some(cdp_session), timeout)
        .map_err(map_cdp)?;
    let deadline = Instant::now() + timeout;
    loop {
        for e in conn.drain_events() {
            if e.method != "Runtime.executionContextCreated" {
                continue;
            }
            let frame_ok = e
                .params
                .pointer("/context/auxData/frameId")
                .and_then(Value::as_str)
                .is_some_and(|id| id == child_id);
            if frame_ok && let Some(ctx) = e.params.pointer("/context/id").and_then(Value::as_u64) {
                return Ok((child_id.clone(), ctx));
            }
        }
        if Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::NotFound,
                format!("no execution context observed for iframe {child_id} within timeout"),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 唯一匹配硬判定:0 → NOT_FOUND,>1 → AMBIGUOUS(带 matches 证据)。
fn require_unique_match(located: &Value, kind: SelectorKind, value: &str) -> Result<(), CtlError> {
    let matches = located.get("matches").and_then(Value::as_u64).unwrap_or(0);
    match matches {
        0 => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!("no element matches the {} selector", kind.as_str()),
            json!({ "selector_kind": kind.as_str(), "selector": value, "matches": 0 }),
        )),
        1 => Ok(()),
        n => Err(CtlError::with_evidence(
            ErrorCode::Ambiguous,
            format!(
                "{} elements match the {} selector; tighten it",
                n,
                kind.as_str()
            ),
            json!({ "selector_kind": kind.as_str(), "selector": value, "matches": n }),
        )),
    }
}

fn truncate_text(text: &str) -> (String, bool) {
    if text.len() <= TEXT_LIMIT {
        return (text.to_string(), false);
    }
    let mut cut = TEXT_LIMIT;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].to_string(), true)
}

/// 读取定位结果中的文本(带截断标记)。
fn located_text_payload(located: &Value) -> Value {
    let (text, truncated) = truncate_text(
        located
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut payload = json!({ "text": text, "truncated": truncated });
    if let Some(tag) = located.get("tag").and_then(Value::as_str) {
        payload["tag"] = Value::String(tag.to_string());
    }
    if let Some(id) = located.get("id").and_then(Value::as_str) {
        payload["id"] = Value::String(id.to_string());
    }
    payload
}

// ---------------------------------------------------------------------------
// launch
// ---------------------------------------------------------------------------

pub fn launch(paths: &Paths, spec: LaunchSpec) -> Result<Value, CtlError> {
    let exe = PathBuf::from(&spec.browser_executable);
    if !exe.is_absolute() {
        return Err(protocol(format!(
            "browser executable path must be absolute: {}",
            spec.browser_executable
        )));
    }
    if !exe.is_file() {
        return Err(protocol(format!(
            "browser executable not found: {}",
            spec.browser_executable
        )));
    }
    let url = spec.url.unwrap_or_else(|| "about:blank".to_string());
    validate_url(&url)?;
    let mode = match spec.profile {
        Some(name) => {
            session::validate_profile_name(&name)?;
            Mode::Persistent { name }
        }
        None => Mode::Ephemeral,
    };
    if let Some(proxy) = &spec.proxy {
        validate_proxy(proxy)?;
    }
    let timeout = dur(spec.timeout_ms, LAUNCH_DEFAULT_TIMEOUT_MS);

    let session_id = actl_core::new_snapshot_id();
    let profile_dir = match &mode {
        Mode::Persistent { name } => paths.profiles_dir().join(name),
        Mode::Ephemeral => paths.profiles_dir().join("ephemeral").join(&session_id),
    };
    // §11 不变量:传给浏览器的 user-data-dir 必须是绝对路径
    let profile_dir = std::env::current_dir()
        .map(|d| d.join(&profile_dir))
        .unwrap_or(profile_dir);
    std::fs::create_dir_all(&profile_dir)
        .map_err(|e| CtlError::internal(format!("cannot create browser profile dir: {e}")))?;
    std::fs::create_dir_all(paths.sessions_dir())
        .map_err(|e| CtlError::internal(format!("cannot create browser sessions dir: {e}")))?;

    let mut edge_args: Vec<String> = vec![
        "--remote-debugging-port=0".to_string(),
        format!("--user-data-dir={}", profile_dir.display()),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];
    if !spec.visible {
        edge_args.insert(0, "--headless=new".to_string());
    }
    if let Some(proxy) = &spec.proxy {
        edge_args.push(format!("--proxy-server={proxy}"));
    }
    edge_args.push("about:blank".to_string());

    // GUI 程序不得继承控制台句柄(interact.rs launch 的既有结论):浏览器进程树会攥住
    // 调用方管道的写端,$()/ReadToEnd 永远等不到 EOF。实测 null 三件套对 Edge 多子
    // 进程树仍不够,派生前先清自身 stdio 的 INHERIT 标志(actl-uia::process_io,docs/72)。
    actl_uia::process_io::clear_stdio_inheritance();
    let mut child = Command::new(&exe)
        .args(&edge_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| CtlError::internal(format!("cannot launch browser: {e}")))?;
    let pid = child.id();

    let devtools = match wait_devtools_port(&profile_dir, timeout) {
        Some(v) => v,
        None => {
            // 启动失败:回收自启进程(无会话可交接,端口不可达即无法优雅关闭)
            let _ = child.kill();
            let _ = child.wait();
            if matches!(mode, Mode::Ephemeral) {
                let _ = std::fs::remove_dir_all(&profile_dir);
            }
            return Err(CtlError::with_evidence(
                ErrorCode::Internal,
                "browser did not expose a devtools endpoint within timeout; \
                 launch aborted and the owned process was reclaimed \
                 (user-data-dir must be non-default for the debug port, docs/72 §2.1)"
                    .to_string(),
                json!({ "pid": pid, "profile_dir": profile_dir.display().to_string() }),
            ));
        }
    };

    let version = cdp::http_get_json(devtools.0, "/json/version", timeout).map_err(|e| {
        reclaim_on_failure(&mut child, &mode, &profile_dir);
        map_cdp(e)
    })?;
    let browser_version = version
        .get("Browser")
        .and_then(Value::as_str)
        .unwrap_or("(unknown)")
        .to_string();
    let browser_ws_path = version
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .and_then(|u| u.split_once("/devtools"))
        .map(|(_, rest)| format!("/devtools{rest}"))
        .ok_or_else(|| {
            CtlError::internal("devtools /json/version returned no browser websocket url")
        })?;

    let conn = Connection::connect(&format!("ws://127.0.0.1:{}{}", devtools.0, browser_ws_path))
        .map_err(|e| {
            reclaim_on_failure(&mut child, &mode, &profile_dir);
            map_cdp(e)
        })?;
    let created = conn
        .call("Target.createTarget", json!({ "url": url }), None, timeout)
        .map_err(|e| {
            reclaim_on_failure(&mut child, &mode, &profile_dir);
            map_cdp(e)
        })?;
    let target_id = created
        .get("targetId")
        .and_then(Value::as_str)
        .ok_or_else(|| CtlError::internal("createTarget returned no targetId"))?
        .to_string();

    let record = BrowserSession {
        version: session::SESSION_VERSION,
        session_id: session_id.clone(),
        browser_executable: spec.browser_executable.clone(),
        profile_dir: profile_dir.display().to_string(),
        mode: mode.clone(),
        headless: !spec.visible,
        pid,
        devtools_port: devtools.0,
        browser_ws_path,
        target_id: target_id.clone(),
        bound_url: url.clone(),
        created_unix: unix_ms() / 1000,
    };
    session::save(paths, &record)?;

    Ok(json!({
        "session_id": session_id,
        "target_id": target_id,
        "browser_pid": pid,
        "devtools_port": devtools.0,
        "browser_version": browser_version,
        "url": url,
        "mode": mode,
        "headless": !spec.visible,
        "profile_dir": profile_dir.display().to_string(),
    }))
}

fn reclaim_on_failure(child: &mut Child, mode: &Mode, profile_dir: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    if matches!(mode, Mode::Ephemeral) {
        let _ = std::fs::remove_dir_all(profile_dir);
    }
}

fn wait_devtools_port(profile_dir: &Path, timeout: Duration) -> Option<(u16, String)> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(profile_dir.join("DevToolsActivePort")) {
            let mut lines = text.lines();
            if let Some(port) = lines.next().and_then(|l| l.trim().parse::<u16>().ok()) {
                let ws_path = lines.next().unwrap_or("").trim().to_string();
                return Some((port, ws_path));
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

// ---------------------------------------------------------------------------
// navigate
// ---------------------------------------------------------------------------

pub fn navigate(
    paths: &Paths,
    session_id: &str,
    url: &str,
    timeout_ms: Option<u64>,
) -> Result<Value, CtlError> {
    validate_url(url)?;
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let conn = connect_browser(&record)?;
    let cdp_session = attach_target(&conn, &record.target_id, timeout)?;
    verify_target_alive(&conn, &record.target_id, timeout)?;
    conn.call("Page.enable", json!({}), Some(&cdp_session), timeout)
        .map_err(map_cdp)?;
    let nav = conn
        .call(
            "Page.navigate",
            json!({ "url": url }),
            Some(&cdp_session),
            timeout,
        )
        .map_err(map_cdp)?;
    // 网络级加载失败必须显式失败:chrome-error 错误页不是导航成功
    // (2026-10-02 x.com 场景实证:DoH 绕过 TUN fake-ip 致连接重置曾表现为静默 ok)
    if let Some(net_error) = nav.get("errorText").and_then(Value::as_str)
        && !net_error.is_empty()
    {
        return Err(CtlError::with_evidence(
            ErrorCode::Internal,
            format!("page load failed at network level: {net_error}"),
            json!({ "url": url, "net_error": net_error }),
        ));
    }
    let frame_id = nav
        .get("frameId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // 导航事件经读线程可达是竞态判定的前提(spike 实证 10ms 级)
    let deadline = Instant::now() + timeout;
    let mut navigated = false;
    loop {
        for e in conn.drain_events() {
            if e.method == "Page.frameNavigated"
                && e.session.as_deref() == Some(cdp_session.as_str())
                && e.params["frame"]["id"].as_str() == Some(frame_id.as_str())
            {
                navigated = true;
            }
        }
        if navigated || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if !navigated {
        return Err(CtlError::new(
            ErrorCode::Timeout,
            format!(
                "Page.frameNavigated for frame {frame_id} not observed within {}ms",
                timeout.as_millis()
            ),
        ));
    }
    let info = eval_json(
        &conn,
        &cdp_session,
        "(function(){return JSON.stringify({url:location.href,title:document.title})})()",
        None,
        timeout,
    )?;
    let mut record = record;
    record.bound_url = info
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or(url)
        .to_string();
    session::save(paths, &record)?;
    Ok(json!({
        "session_id": session_id,
        "url": record.bound_url,
        "title": info.get("title").and_then(Value::as_str).unwrap_or_default(),
        "frame_id": frame_id,
    }))
}

// ---------------------------------------------------------------------------
// read
// ---------------------------------------------------------------------------

pub fn read(
    paths: &Paths,
    session_id: &str,
    kind: SelectorKind,
    value: &str,
    iframe: bool,
    timeout_ms: Option<u64>,
) -> Result<Value, CtlError> {
    validate_selector(kind, value)?;
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let conn = connect_browser(&record)?;
    let cdp_session = attach_target(&conn, &record.target_id, timeout)?;
    verify_target_alive(&conn, &record.target_id, timeout)?;
    let context_id = if iframe {
        Some(iframe_context(&conn, &cdp_session, timeout)?.1)
    } else {
        None
    };
    let located = eval_json(
        &conn,
        &cdp_session,
        &locate_script(kind, value),
        context_id,
        timeout,
    )?;
    require_unique_match(&located, kind, value)?;
    let mut payload = located_text_payload(&located);
    payload["session_id"] = Value::String(session_id.to_string());
    payload["frame"] = Value::String(if iframe { "iframe" } else { "main" }.to_string());
    Ok(payload)
}

// ---------------------------------------------------------------------------
// click(守卫写:停止核验 → 绑定重核 → 唯一匹配 → 派发 → 读回)
// ---------------------------------------------------------------------------

pub fn click(
    paths: &Paths,
    session_id: &str,
    kind: SelectorKind,
    value: &str,
    iframe: bool,
    expect_text: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<Value, CtlError> {
    validate_selector(kind, value)?;
    if expect_text.is_some_and(str::is_empty) {
        return Err(protocol("--expect-text must be non-empty"));
    }
    // 停止代次核验:与桌面写操作同纪律(急停压过一切写通道)
    actl_core::stop::check_current()?;
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let conn = connect_browser(&record)?;
    let cdp_session = attach_target(&conn, &record.target_id, timeout)?;
    verify_target_alive(&conn, &record.target_id, timeout)?;
    let context_id = if iframe {
        Some(iframe_context(&conn, &cdp_session, timeout)?.1)
    } else {
        None
    };

    let located = eval_json(
        &conn,
        &cdp_session,
        &locate_script(kind, value),
        context_id,
        timeout,
    )?;
    require_unique_match(&located, kind, value)?;

    // 派发点击并读回(写后核验;一次写后必须 recheck,不盲重放)
    let embedded = serde_json::to_string(value).unwrap_or_default();
    let click_script = match kind {
        SelectorKind::Css => format!(
            "(function(){{var els=document.querySelectorAll({embedded});\
             if(els.length!==1)return JSON.stringify({{clicked:false,matches:els.length}});\
             els[0].click();return JSON.stringify({{clicked:true}});}})()"
        ),
        SelectorKind::Xpath => format!(
            "(function(){{var it=document.evaluate({embedded},document,null,\
             XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null);\
             if(it.snapshotLength!==1)return JSON.stringify({{clicked:false,matches:it.snapshotLength}});\
             it.snapshotItem(0).click();return JSON.stringify({{clicked:true}});}})()"
        ),
    };
    let clicked = eval_json(&conn, &cdp_session, &click_script, context_id, timeout)?;
    if clicked.get("clicked").and_then(Value::as_bool) != Some(true) {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "element resolved but click did not dispatch (DOM changed between locate and click)"
                .to_string(),
            json!({ "clicked": clicked }),
        ));
    }

    // 读回:同选择器重定位取文本
    let after = eval_json(
        &conn,
        &cdp_session,
        &locate_script(kind, value),
        context_id,
        timeout,
    )?;
    let (text_after, truncated) = truncate_text(
        after
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let state = eval_json(
        &conn,
        &cdp_session,
        "(function(){return JSON.stringify({url:location.href})})()",
        None,
        timeout,
    )?;
    let url = state
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if let Some(expected) = expect_text
        && !text_after.contains(expected)
    {
        return Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "post-click text does not contain the expected substring".to_string(),
            json!({
                "expected_contains": expected,
                "actual": text_after,
                "truncated": truncated,
            }),
        ));
    }
    Ok(json!({
        "session_id": session_id,
        "clicked": true,
        "text_after": text_after,
        "truncated": truncated,
        "url": url,
        "frame": if iframe { "iframe" } else { "main" },
    }))
}

// ---------------------------------------------------------------------------
// close
// ---------------------------------------------------------------------------

pub fn close(paths: &Paths, session_id: &str, timeout_ms: Option<u64>) -> Result<Value, CtlError> {
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;

    let mut already_gone = false;
    match connect_browser(&record) {
        Ok(conn) => {
            conn.call("Browser.close", json!({}), None, timeout)
                .map_err(map_cdp)?;
            // 等连接断开(浏览器侧关闭会终结 websocket)
            let deadline = Instant::now() + timeout;
            while conn.is_connected() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            // 端口探测兜底:连接断开但端口仍可达视为未收尾
            let port_dead = !TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", record.devtools_port)
                    .parse()
                    .map_err(|_| CtlError::internal("bad devtools port in session"))?,
                Duration::from_millis(500),
            )
            .is_ok();
            if !conn.is_connected() && port_dead {
                finish_close(paths, &record)
            } else {
                Err(CtlError::with_evidence(
                    ErrorCode::Internal,
                    "Browser.close acknowledged but the endpoint still answers; \
                     session kept for retry",
                    json!({ "session_id": session_id, "devtools_port": record.devtools_port }),
                ))
            }
        }
        Err(e) => {
            // 端点本就不可达:幂等关闭(浏览器已死),如实标注
            already_gone = true;
            let _ = e;
            finish_close(paths, &record)
        }
    }
    .map(|mut value| {
        if already_gone {
            value["already_unreachable"] = Value::Bool(true);
        }
        value
    })
}

fn finish_close(paths: &Paths, record: &BrowserSession) -> Result<Value, CtlError> {
    session::remove(paths, &record.session_id)?;
    let (kept, removed) = match &record.mode {
        Mode::Persistent { .. } => (true, None),
        Mode::Ephemeral => {
            let removed = std::fs::remove_dir_all(&record.profile_dir).is_ok();
            (false, Some(removed))
        }
    };
    Ok(json!({
        "session_id": record.session_id,
        "closed": true,
        "mode": record.mode,
        "profile_kept": kept,
        "ephemeral_profile_removed": removed,
        "browser_pid": record.pid,
    }))
}

use std::net::TcpStream;
