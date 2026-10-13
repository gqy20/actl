//! 浏览器命令执行体:launch/navigate/read/click/close。
//!
//! 语义要点(docs/72):
//! - launch:绝对 user-data-dir + DevToolsActivePort 出现 = 启动成功判定;失败 fail-fast
//!   并回收自启进程(§11 不变量:自启实例必须总能经 CDP 优雅关闭);
//! - 写操作(click)先过停止代次核验,再重核绑定 target 存活,再定位唯一匹配,派发后读回;
//! - cookie 域不在实现面内(构造性不可达,lib.rs 范围铁律)。

use crate::SelectorKind;
use crate::bridge;
use crate::scripts::{locate_all_script, locate_deep_script, locate_script};
use crate::session::{self, BrowserSession, Mode, Paths};
use crate::verify::pre_action_popup_gate;
use actl_core::state::unix_ms;
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LAUNCH_DEFAULT_TIMEOUT_MS: u64 = 15_000;
pub(crate) const OP_DEFAULT_TIMEOUT_MS: u64 = 10_000;
const TEXT_LIMIT: usize = 65_536;

pub struct LaunchSpec {
    pub browser_executable: String,
    pub url: Option<String>,
    pub profile: Option<String>,
    /// 出站代理(HOST:PORT,无 scheme);显式本地代理路径,非任意参数注入
    pub proxy: Option<String>,
    /// UA 覆盖(headless UA 壁垒站点的合规通行道):覆盖的是 UA 字符串本身,
    /// 不伪装指纹面(canvas/WebGL 等)——站点若做指纹一致性校验仍会识破,
    /// 如实边界。持久 profile 复用会话时不重申(进程已在跑)
    pub user_agent: Option<String>,
    pub headless: bool,
    /// 显式禁用扩展(持久 profile 用;ephemeral 恒为禁用——隔离目录本无插件,
    /// 显式化是为确定性与防指纹方差。持久 profile 默认允许扩展:猫抓/沉浸式
    /// 翻译类扩展驱动流程正是自动化 profile 的合法资产,docs/16 §5.5)
    pub disable_extensions: bool,
    pub timeout_ms: Option<u64>,
}

/// 显示层动作卡声明(P2 协同):CDP 写/生命周期命令在信号流可见。与 UIA 轨的
/// display_gate 不同——不经显示租约门(展示是透明性而非权限门,docs/72 §13 治理发现),
/// InputEffects 全空(CDP 写无物理键鼠/焦点/剪贴板效应,如实申报);target 只带会话
/// 标记,不含选择器/URL/文本(动作日志内容省略纪律)。
pub(crate) fn declare_action(
    kind: &str,
    session_tag: &str,
) -> actl_core::signal_session::ActionGuard {
    actl_core::signal_session::ActionGuard::begin(
        kind,
        actl_core::activity::InputEffects::default(),
        actl_core::activity::ActionTarget {
            window: Some(session_tag.to_string()),
            ..Default::default()
        },
    )
}

pub(crate) fn protocol(message: impl Into<String>) -> CtlError {
    CtlError::new(ErrorCode::Protocol, message)
}

/// DevTools HTTP 发现端点的最小 GET(Phase 2 随 cdp.rs 退役挪入 ops)。
/// 端点不保证按 `Connection: close` 断开,必须按头解析、按 Content-Length 读体
/// (docs/72 §10 run-2 实证:`read_to_end` 会吃读超时,Windows 上同为 10060)。
pub(crate) fn http_get_json(port: u16, path: &str, timeout: Duration) -> Result<Value, CtlError> {
    use std::io::{Read, Write};
    let wrap = |message: String| CtlError::new(actl_core::ErrorCode::NotFound, message);
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

pub(crate) fn dur(timeout_ms: Option<u64>, default: u64) -> Duration {
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

pub(crate) fn validate_selector(kind: SelectorKind, value: &str) -> Result<(), CtlError> {
    if value.is_empty() || value.len() > 4096 {
        return Err(protocol(format!(
            "{} selector requires 1-4096 characters",
            kind.as_str()
        )));
    }
    Ok(())
}

/// 代理仅接受裸 HOST:PORT(拒绝 scheme/凭据),防参数注入面
/// UA 覆盖仅接受可打印 ASCII(0x20-0x7E,允许空格),1..=512 字节——
/// 控制字符/非 ASCII 是畸形 UA,拒绝在入口而非让站点 400
fn validate_user_agent(value: &str) -> Result<(), CtlError> {
    if value.is_empty() || value.len() > 512 {
        return Err(protocol("--user-agent must be 1-512 bytes"));
    }
    if !value.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return Err(protocol(
            "--user-agent must be printable ASCII (no control or non-ASCII bytes)",
        ));
    }
    Ok(())
}

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

/// 属性读取白名单(docs/06 §3;--property 的合法值)。text 为缺省行为。
pub(crate) const READ_PROPERTIES: &[&str] =
    &["text", "value", "href", "name", "id", "checked", "tag"];

pub(crate) fn validate_property(property: &str) -> Result<(), CtlError> {
    if READ_PROPERTIES.contains(&property) {
        Ok(())
    } else {
        Err(protocol(format!(
            "unknown --property {property:?}; allowed: {}",
            READ_PROPERTIES.join(", ")
        )))
    }
}

/// --target 形状校验(早于会话 load,坏参数不触达会话文件——netlog 同序)。
pub(crate) fn validate_target_shape(id: &str) -> Result<(), CtlError> {
    if id.is_empty() || id.len() > 128 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(protocol(
            "--target expects a CDP target id (1-128 ascii alphanumeric)",
        ));
    }
    Ok(())
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

pub(crate) fn truncate_text(text: &str) -> (String, bool) {
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
    // UA 校验前置(先于环境探测):畸形 UA 是入口参数错误,与机器是否装有
    // 目标浏览器无关——契约测试无需真实 Edge 即可锁定校验面
    if let Some(ua) = &spec.user_agent {
        validate_user_agent(ua)?;
    }
    // app-registry 别名解析:名字(如 edge)→ 绝对路径;非注册名原样传
    let browser_executable = actl_uia::app_registry::resolve(&spec.browser_executable)
        .ok()
        .flatten()
        .unwrap_or_else(|| spec.browser_executable.clone());
    let exe = PathBuf::from(&browser_executable);
    if !exe.is_absolute() {
        return Err(protocol(format!(
            "browser executable path must be absolute (or a registered alias): {}",
            browser_executable
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

    // 持久 profile 并发治理(§13.4):同 profile 已有活跃会话→复用(幂等 launch);
    // 显示层动作卡(P2):生命周期命令对用户可见
    let mut action = declare_action(
        "browser-launch",
        &match &mode {
            Mode::Persistent { name } => format!("cdp-profile:{name}"),
            Mode::Ephemeral => "cdp:ephemeral".to_string(),
        },
    );
    action.executing();
    // 记录在但端点已死→陈旧接管(注销旧记录后照常新启)。两个 CLI 进程同时竞启
    // 同一 profile 时,后到者的 Edge 会被先到者锁住退出→端口不出现→启动失败,如实报错。
    if let Mode::Persistent { name } = &mode
        && let Some(reused) = reuse_persistent_session(paths, name, timeout)?
    {
        action.delivered();
        return Ok(reused);
    }

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
    if spec.headless {
        edge_args.insert(0, "--headless=new".to_string());
    }
    if let Some(ua) = &spec.user_agent {
        edge_args.push(format!("--user-agent={ua}"));
    }
    // 扩展策略:ephemeral 恒禁用;持久 profile 默认允许(扩展是合法资产),
    // 显式 --disable-extensions 才禁
    if spec.disable_extensions || matches!(mode, Mode::Ephemeral) {
        edge_args.push("--disable-extensions".to_string());
    }
    if let Some(proxy) = &spec.proxy {
        edge_args.push(format!("--proxy-server={proxy}"));
    }
    edge_args.push("about:blank".to_string());

    // GUI 程序不得继承控制台句柄(interact.rs launch 的既有结论):浏览器进程树会攥住
    // 调用方管道的写端,$()/ReadToEnd 永远等不到 EOF。实测 null 三件套对 Edge 多子
    // 进程树仍不够,派生前先清自身 stdio 的 INHERIT 标志(actl-uia::process_io,docs/72)。
    actl_uia::process_io::clear_stdio_inheritance();
    // 持久 profile 会残留上次退出的 DevToolsActivePort(陈旧端口假阳的温床),先清掉
    let _ = std::fs::remove_file(profile_dir.join("DevToolsActivePort"));
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

    let version = http_get_json(devtools.0, "/json/version", timeout).inspect_err(|_| {
        reclaim_on_failure(&mut child, &mode, &profile_dir);
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

    // 骨干接管 launch 尾段(Phase 2):connect + new_page 替代手写
    // createTarget/attachToTarget;浏览器进程治理不变(spawn/端口等待/回收)
    let url_for_page = url.clone();
    let ws_path_for_lane = browser_ws_path.clone();
    let (target_id,) = bridge::block_on(
        async move {
            let (browser, mut handler) = chromiumoxide::Browser::connect(format!(
                "ws://127.0.0.1:{}{}",
                devtools.0, ws_path_for_lane
            ))
            .await
            .map_err(|e| format!("connect: {e}"))?;
            {
                use futures::StreamExt;
                let _pump = tokio::spawn(async move {
                    while let Some(item) = handler.next().await {
                        if item.is_err() {
                            break;
                        }
                    }
                });
            }
            let page = browser
                .new_page(url_for_page.as_str())
                .await
                .map_err(|e| format!("createTarget: {e}"))?;
            Ok::<_, String>((page.target_id().as_ref().to_string(),))
        },
        timeout,
    )
    .map_err(|e| {
        reclaim_on_failure(&mut child, &mode, &profile_dir);
        CtlError::internal(format!("deep-lane launch tail failed: {e}"))
    })?
    .map_err(|msg| {
        reclaim_on_failure(&mut child, &mode, &profile_dir);
        CtlError::internal(format!("deep-lane launch tail failed: {msg}"))
    })?;

    let record = BrowserSession {
        version: session::SESSION_VERSION,
        session_id: session_id.clone(),
        browser_executable: spec.browser_executable.clone(),
        profile_dir: profile_dir.display().to_string(),
        mode: mode.clone(),
        headless: spec.headless,
        pid,
        devtools_port: devtools.0,
        browser_ws_path,
        target_id: target_id.clone(),
        bound_url: url.clone(),
        created_unix: unix_ms() / 1000,
    };
    session::save(paths, &record)?;

    action.delivered();
    Ok(json!({
        "session_id": session_id,
        "target_id": target_id,
        "browser_pid": pid,
        "devtools_port": devtools.0,
        "browser_version": browser_version,
        "url": url,
        "mode": mode,
        "headless": spec.headless,
        "user_agent": spec.user_agent,
        "profile_dir": profile_dir.display().to_string(),
    }))
}

/// 同名持久 profile 的活跃会话查找:活→Some(复用载荷);死→注销陈旧记录后 None。
fn reuse_persistent_session(
    paths: &Paths,
    name: &str,
    _timeout: Duration,
) -> Result<Option<Value>, CtlError> {
    let sessions = list(paths)?;
    let Some(arr) = sessions.get("sessions").and_then(Value::as_array) else {
        return Ok(None);
    };
    for entry in arr {
        // mode 序列化为 {"persistent":{"name":...}}
        let is_same = entry
            .pointer("/mode/persistent/name")
            .and_then(Value::as_str)
            .is_some_and(|n| n == name);
        if !is_same {
            continue;
        }
        let id = entry
            .get("session_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Ok(record) = session::load(paths, id) else {
            continue;
        };
        let alive = (|| {
            let ws = format!(
                "ws://127.0.0.1:{}{}",
                record.devtools_port, record.browser_ws_path
            );
            bridge::connect(&ws, Duration::from_secs(3)).ok()?;
            Some(())
        })()
        .is_some();
        if alive {
            return Ok(Some(json!({
                "session_id": record.session_id,
                "target_id": record.target_id,
                "browser_pid": record.pid,
                "devtools_port": record.devtools_port,
                "url": record.bound_url,
                "mode": record.mode,
                "headless": record.headless,
                "profile_dir": record.profile_dir,
                "reused": true,
            })));
        }
        // 端点已死:陈旧接管,注销后走全新启动
        let _ = session::remove(paths, id);
    }
    Ok(None)
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
                // 文件出现 ≠ 端点就绪:陈旧文件(上次退出残留)与竞态启动都会假阳,
                // 必须以 /json/version 实答为准
                if http_get_json(port, "/json/version", Duration::from_millis(1000)).is_ok() {
                    let ws_path = lines.next().unwrap_or("").trim().to_string();
                    return Some((port, ws_path));
                }
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
    let mut action = declare_action("browser-navigate", &format!("cdp:{session_id}"));
    let lane = bridge::open_lane(&record, None, timeout)?;
    action.executing();
    // 骨干接管(Phase 2):订阅先于派发是事件流语义的不变序
    // (net_error 映射与错误码语义在 bridge::navigate_and_wait 内保持原样)
    let (frame_id,) = bridge::navigate_and_wait(&lane.page, url, timeout)?;
    let info = bridge::eval_json(
        &lane.page,
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
    action.delivered();
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

/// read 的参数束(同 LaunchSpec 惯例——clippy 参数上限)。
pub struct ReadSpec<'a> {
    pub session_id: &'a str,
    pub kind: SelectorKind,
    pub value: &'a str,
    pub property: Option<&'a str>,
    pub iframe: bool,
    pub timeout_ms: Option<u64>,
    pub all: bool,
    pub limit: Option<usize>,
    pub deep: bool,
    /// 沉降谓词(2026-10-11 spec):命中数 ≥ N 才返回——异步渲染页面
    /// (GitHub hpc 列表 CSR 等)的读段自稳,内建 500ms 轮询,超时 TIMEOUT
    /// 带实测命中数;与 --all/唯一读通用。
    pub wait_for: Option<u64>,
    /// --target 寻址覆盖(2026-10-13):None = 会话绑 tab;Some = 指定
    /// CDP target(扩展 popup/其他 tab,发现走 browser-targets)。
    pub target: Option<&'a str>,
}

/// 沉降谓词(2026-10-11 spec,--wait-for N):命中数 < N 时 500ms 轮询
/// 重读直到达标或超时——异步渲染(CSR 列表)的读段自稳;超时 TIMEOUT 带
/// 实测命中数与期望下界(诚实证据:差多少、等多久)。轮询共用药洞超时
/// 预算(与单次读同界,不另开)。
fn wait_for_matches(
    page: &chromiumoxide::Page,
    script: &str,
    context_id: Option<u64>,
    timeout: Duration,
    min: u64,
    first: Value,
) -> Result<Value, CtlError> {
    let matches_of = |v: &Value| v.get("matches").and_then(Value::as_u64).unwrap_or(0);
    if matches_of(&first) >= min {
        return Ok(first);
    }
    let deadline = std::time::Instant::now() + timeout;
    let mut current = first;
    loop {
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::with_evidence(
                ErrorCode::Timeout,
                format!(
                    "browser-read --wait-for {min}: only {} matches within the timeout (async content may not have settled)",
                    matches_of(&current)
                ),
                json!({ "matches": matches_of(&current), "wait_for": min }),
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
        current = bridge::eval_json(page, script, context_id, timeout)?;
        if matches_of(&current) >= min {
            return Ok(current);
        }
    }
}

pub fn read(paths: &Paths, spec: ReadSpec<'_>) -> Result<Value, CtlError> {
    let ReadSpec {
        session_id,
        kind,
        value,
        property,
        iframe,
        timeout_ms,
        all,
        limit,
        deep,
        wait_for,
        target,
    } = spec;
    validate_selector(kind, value)?;
    let property = property.unwrap_or("text");
    validate_property(property)?;
    if let Some(t) = target {
        validate_target_shape(t)?;
    }
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let lane = bridge::open_lane(&record, target, timeout)?;
    let context_id = if iframe {
        Some(bridge::find_iframe_context(&lane.page, timeout)?.1)
    } else {
        None
    };
    if all {
        // --all 枚举形态(2026-10-11 spec,docs/73 round7):零命中如实
        // 空集(不是 NOT_FOUND——枚举语义与 find 一致),limit 只裁返回
        // 面,total 保真;逐元素文本截断与唯一读同规。
        const ALL_LIMIT: usize = 50;
        const ALL_LIMIT_MAX: usize = 500;
        let limit = limit.unwrap_or(ALL_LIMIT);
        if !(1..=ALL_LIMIT_MAX).contains(&limit) {
            return Err(protocol(format!(
                "browser-read --limit must be 1..={ALL_LIMIT_MAX}, got {limit}"
            )));
        }
        let script = if deep {
            locate_deep_script(kind, value, Some(property))
        } else {
            locate_all_script(kind, value, Some(property))
        };
        let mut located = bridge::eval_json(&lane.page, &script, context_id, timeout)?;
        if let Some(min) = wait_for {
            located = wait_for_matches(&lane.page, &script, context_id, timeout, min, located)?;
        }
        let total = located
            .get("matches")
            .and_then(Value::as_u64)
            .ok_or_else(|| CtlError::internal("enumeration script returned no total"))?;
        let elements = located
            .get("elements")
            .and_then(Value::as_array)
            .ok_or_else(|| CtlError::internal("enumeration script returned no elements"))?;
        let mut out = Vec::with_capacity(elements.len().min(limit));
        for element in elements.iter().take(limit) {
            let (text, truncated) = truncate_text(
                element
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let mut item = json!({ "text": text, "truncated": truncated });
            if let Some(tag) = element.get("tag").and_then(Value::as_str) {
                item["tag"] = Value::String(tag.to_string());
            }
            if let Some(id) = element.get("id").and_then(Value::as_str) {
                item["id"] = Value::String(id.to_string());
            }
            if property != "text" {
                item["property"] = Value::String(property.to_string());
                item["value"] = element.get("property").cloned().unwrap_or(Value::Null);
            }
            out.push(item);
        }
        let mut payload = json!({
            "total": total,
            "returned": out.len(),
            "truncated_by_limit": total > out.len() as u64,
            "selector_kind": kind.as_str(),
            "elements": out,
            "session_id": session_id,
            "frame": if iframe { "iframe" } else { "main" },
        });
        if let Some(t) = target {
            payload["target"] = Value::String(t.to_string());
        }
        // --deep 的遍历预算触顶:matches 是下界,如实上报不谎称全量
        if deep {
            payload["deep"] = Value::Bool(true);
            if located.get("traversal_capped").and_then(Value::as_bool) == Some(true) {
                payload["traversal_capped"] = Value::Bool(true);
            }
        }
        return Ok(payload);
    }
    let script = if deep {
        locate_deep_script(kind, value, Some(property))
    } else {
        locate_script(kind, value, Some(property))
    };
    let mut located = bridge::eval_json(&lane.page, &script, context_id, timeout)?;
    if let Some(min) = wait_for {
        located = wait_for_matches(&lane.page, &script, context_id, timeout, min, located)?;
    }
    require_unique_match(&located, kind, value)?;
    // --deep 脚本恒返回枚举形态:唯一命中取 elements[0] 作载荷
    let unique = if deep {
        located
            .get("elements")
            .and_then(Value::as_array)
            .and_then(|elements| elements.first())
            .cloned()
            .ok_or_else(|| CtlError::internal("deep script returned no element"))?
    } else {
        located.clone()
    };
    let mut payload = located_text_payload(&unique);
    if deep {
        payload["deep"] = Value::Bool(true);
    }
    if property != "text" {
        payload["property"] = Value::String(property.to_string());
        payload["value"] = unique.get("property").cloned().unwrap_or(Value::Null);
    }
    payload["session_id"] = Value::String(session_id.to_string());
    payload["frame"] = Value::String(if iframe { "iframe" } else { "main" }.to_string());
    if let Some(t) = target {
        payload["target"] = Value::String(t.to_string());
    }
    Ok(payload)
}

// ---------------------------------------------------------------------------
// click(守卫写:停止核验 → 绑定重核 → 唯一匹配 → 派发 → 读回)
// ---------------------------------------------------------------------------

// clap 变体字段直转链路上的函数,参数即命令面(与 actl-cli 薄适配层同豁免)
#[allow(clippy::too_many_arguments)]
pub fn click(
    paths: &Paths,
    session_id: &str,
    kind: SelectorKind,
    value: &str,
    iframe: bool,
    expect_text: Option<&str>,
    timeout_ms: Option<u64>,
    allow_popups: bool,
    target: Option<&str>,
) -> Result<Value, CtlError> {
    validate_selector(kind, value)?;
    if expect_text.is_some_and(str::is_empty) {
        return Err(protocol("--expect-text must be non-empty"));
    }
    // 停止代次核验:与桌面写操作同纪律(急停压过一切写通道)
    actl_core::stop::check_current()?;
    if let Some(t) = target {
        validate_target_shape(t)?;
    }
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let lane = bridge::open_lane(&record, target, timeout)?;
    let context_id = if iframe {
        Some(bridge::find_iframe_context(&lane.page, timeout)?.1)
    } else {
        None
    };
    let mut action = declare_action("browser-click", &format!("cdp:{session_id}"));
    action.executing();
    if !allow_popups {
        pre_action_popup_gate(&lane.page, timeout)?;
    }

    let located = bridge::eval_json(
        &lane.page,
        &locate_script(kind, value, None),
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
    let clicked = bridge::eval_json(&lane.page, &click_script, context_id, timeout)?;
    if clicked.get("clicked").and_then(Value::as_bool) != Some(true) {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "element resolved but click did not dispatch (DOM changed between locate and click)"
                .to_string(),
            json!({ "clicked": clicked }),
        ));
    }

    // 读回:同选择器重定位取文本
    let after = bridge::eval_json(
        &lane.page,
        &locate_script(kind, value, None),
        context_id,
        timeout,
    )?;
    let (text_after, truncated) = truncate_text(
        after
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let state = bridge::eval_json(
        &lane.page,
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
    action.delivered();
    Ok(json!({
        "session_id": session_id,
        "clicked": true,
        "text_after": text_after,
        "truncated": truncated,
        "url": url,
        "frame": if iframe { "iframe" } else { "main" },
        "target": target,
    }))
}

// ---------------------------------------------------------------------------
// close
// ---------------------------------------------------------------------------

/// 守卫输入写:input/textarea 赋值(native setter + input/change 事件,React 受控组件
/// 兼容),读回 value 必须与请求一致(不一致=ASSERTION_FAILED,受控组件改写了值)。
/// 与 click 同纪律:停止核验→绑定重核→弹层门禁→唯一匹配→派发→读回。
#[allow(clippy::too_many_arguments)]
pub fn type_text(
    paths: &Paths,
    session_id: &str,
    kind: SelectorKind,
    value: &str,
    text: &str,
    iframe: bool,
    timeout_ms: Option<u64>,
    allow_popups: bool,
    target: Option<&str>,
) -> Result<Value, CtlError> {
    validate_selector(kind, value)?;
    if text.is_empty() || text.len() > TEXT_LIMIT {
        return Err(protocol("--text requires 1-65536 characters"));
    }
    if let Some(t) = target {
        validate_target_shape(t)?;
    }
    actl_core::stop::check_current()?;
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let lane = bridge::open_lane(&record, target, timeout)?;
    let context_id = if iframe {
        Some(bridge::find_iframe_context(&lane.page, timeout)?.1)
    } else {
        None
    };
    let mut action = declare_action("browser-type", &format!("cdp:{session_id}"));
    action.executing();
    if !allow_popups {
        pre_action_popup_gate(&lane.page, timeout)?;
    }
    let located = bridge::eval_json(
        &lane.page,
        &locate_script(kind, value, Some("value")),
        context_id,
        timeout,
    )?;
    require_unique_match(&located, kind, value)?;
    if located
        .get("tag")
        .and_then(Value::as_str)
        .is_none_or(|t| !matches!(t.to_ascii_uppercase().as_str(), "INPUT" | "TEXTAREA"))
    {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "browser-type targets input/textarea elements only".to_string(),
            json!({ "tag": located.get("tag") }),
        ));
    }

    let embedded_sel = serde_json::to_string(value).unwrap_or_default();
    let embedded_text = serde_json::to_string(text).unwrap_or_default();
    let locate_expr = match kind {
        SelectorKind::Css => format!("document.querySelectorAll({embedded_sel})[0]"),
        SelectorKind::Xpath => format!(
            "document.evaluate({embedded_sel},document,null,XPathResult.ORDERED_NODE_SNAPSHOT_TYPE,null).snapshotItem(0)"
        ),
    };
    let script = format!(
        "(function(){{var e={locate_expr};\
         if(!e)return JSON.stringify({{typed:false,reason:'gone'}});\
         var tag=(e.tagName||'').toUpperCase();\
         if(tag!=='INPUT'&&tag!=='TEXTAREA')return JSON.stringify({{typed:false,reason:'not_input'}});\
         var proto=tag==='INPUT'?window.HTMLInputElement.prototype:window.HTMLTextAreaElement.prototype;\
         var d=Object.getOwnPropertyDescriptor(proto,'value');\
         if(d&&d.set){{d.set.call(e,{embedded_text});}}else{{e.value={embedded_text};}}\
         e.dispatchEvent(new Event('input',{{bubbles:true}}));\
         e.dispatchEvent(new Event('change',{{bubbles:true}}));\
         return JSON.stringify({{typed:true,value:String(e.value)}});}})()"
    );
    let typed = bridge::eval_json(&lane.page, &script, context_id, timeout)?;
    if typed.get("typed").and_then(Value::as_bool) != Some(true) {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "element resolved but typing did not dispatch (DOM changed between locate and type)"
                .to_string(),
            json!({ "typed": typed }),
        ));
    }
    let value_after = typed
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if value_after != text {
        return Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "post-type value differs from the requested text (controlled input rewrote it)"
                .to_string(),
            json!({ "requested_len": text.len(), "actual_len": value_after.len() }),
        ));
    }
    action.delivered();
    // 不回显输入内容(动作日志不含文本纪律);需要时调用方 browser-read --property value 自取
    Ok(json!({
        "session_id": session_id,
        "typed": true,
        "value_after_len": value_after.len(),
        "frame": if iframe { "iframe" } else { "main" },
        "target": target,
    }))
}

pub fn close(paths: &Paths, session_id: &str, timeout_ms: Option<u64>) -> Result<Value, CtlError> {
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let mut action = declare_action("browser-close", &format!("cdp:{session_id}"));
    action.executing();

    let mut already_gone = false;
    let ws_url = format!(
        "ws://127.0.0.1:{}{}",
        record.devtools_port, record.browser_ws_path
    );
    match bridge::connect(&ws_url, Duration::from_secs(5)) {
        Ok(browser) => {
            bridge::block_on(
                async move {
                    let mut browser = browser;
                    browser.close().await.map_err(|e| format!("Browser.close: {e}"))
                },
                timeout,
            )?
            .map_err(|msg| {
                CtlError::with_evidence(
                    ErrorCode::Internal,
                    format!("Browser.close dispatch failed: {msg}"),
                    json!({ "session_id": session_id, "devtools_port": record.devtools_port }),
                )
            })?;
            // 端口探测兜底:端点仍可达视为未收尾(骨干连接无 is_connected 可查)
            std::thread::sleep(Duration::from_millis(300));
            let port_dead = !TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", record.devtools_port)
                    .parse()
                    .map_err(|_| CtlError::internal("bad devtools port in session"))?,
                Duration::from_millis(500),
            )
            .is_ok();
            if port_dead {
                finish_close(paths, &record)
            } else {
                Err(CtlError::with_evidence(
                    ErrorCode::Internal,
                    "Browser.close acknowledged but the endpoint still answers;                      session kept for retry",
                    json!({ "session_id": session_id, "devtools_port": record.devtools_port }),
                ))
            }
        }
        Err(_) => {
            // 端点本就不可达:幂等关闭(浏览器已死),如实标注
            already_gone = true;
            finish_close(paths, &record)
        }
    }
    .map(|mut value| {
        action.delivered();
        if already_gone {
            value["already_unreachable"] = Value::Bool(true);
        }
        value
    })
}

pub fn list(paths: &Paths) -> Result<Value, CtlError> {
    let dir = paths.sessions_dir();
    let mut sessions: Vec<BrowserSession> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({ "sessions": [], "total": 0 }));
        }
        Err(e) => return Err(CtlError::internal(format!("cannot read sessions dir: {e}"))),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        // load 失败的损坏文件跳过:枚举不因单条坏记录失败
        if let Ok(s) = session::load(paths, id) {
            sessions.push(s);
        }
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.created_unix));
    let list: Vec<Value> = sessions
        .iter()
        .map(|s| {
            json!({
                "session_id": s.session_id,
                "target_id": s.target_id,
                "url": s.bound_url,
                "browser_pid": s.pid,
                "devtools_port": s.devtools_port,
                "mode": s.mode,
                "headless": s.headless,
                "profile_dir": s.profile_dir,
                "created_unix": s.created_unix,
            })
        })
        .collect();
    let total = list.len();
    Ok(json!({ "sessions": list, "total": total }))
}

fn finish_close(paths: &Paths, record: &BrowserSession) -> Result<Value, CtlError> {
    session::remove(paths, &record.session_id)?;
    let (kept, removed) = match &record.mode {
        Mode::Persistent { .. } => (true, None),
        Mode::Ephemeral => (
            false,
            Some(remove_dir_bounded_retry(std::path::Path::new(
                &record.profile_dir,
            ))),
        ),
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

/// 临时 profile 删除(docs/73 实证:7 轮中 2 次失败)——进程树句柄释放
/// 晚于端点死亡,单次 remove_dir_all 会漏。立即试一次(常态:进程已退,
/// 秒成),失败后 200/400/800/1600ms 退避重试(上界 ~3s),穷尽仍败
/// 如实 false(泄漏目录留在 profiles/ephemeral/,envelope 已报告)。
/// 目录缺席(此前轮已清/外部清场)=目标态已达成,不进重试。
fn remove_dir_bounded_retry(dir: &std::path::Path) -> bool {
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(200u64 << (attempt - 1)));
        }
        if std::fs::remove_dir_all(dir).is_ok() {
            return true;
        }
        if !dir.exists() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::remove_dir_bounded_retry;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("actl-rm-retry-{name}-{}", std::process::id()))
    }

    #[test]
    fn removal_succeeds_immediately_when_unlocked() {
        let dir = temp("free");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        assert!(remove_dir_bounded_retry(&dir));
        assert!(!dir.exists());
    }

    #[test]
    fn missing_dir_counts_as_removed() {
        let dir = temp("gone");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(remove_dir_bounded_retry(&dir));
    }

    /// 活体竞争复现:独占句柄(share_mode 0)持锁期间 remove 必败,
    /// 释放后重试路径必须成功——单次实现(旧 finish_close)此形态恒 false。
    #[test]
    #[cfg(windows)]
    fn removal_retries_past_exclusive_handle_release() {
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        let dir = temp("locked");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("locked.txt");
        std::fs::File::create(&file)
            .unwrap()
            .write_all(b"x")
            .unwrap();
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&file)
            .unwrap();
        let dir2 = dir.clone();
        let worker = std::thread::spawn(move || remove_dir_bounded_retry(&dir2));
        // 持锁跨过首次尝试与第一次退避(200ms),释放后由重试收口
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(handle);
        assert!(worker.join().unwrap());
        assert!(!dir.exists());
    }
}
