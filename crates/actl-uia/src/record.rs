//! GUI 操作录制会话——捕获层(docs/06「GUI 操作录制 R1」;设计 docs/76 §11、调研 docs/79)。
//! 分层:本模块只做捕获(低级钩子 + 点击瞬间 UIA 反查 + trace JSONL + 会话状态文件);
//! trace→YAML 的机械编译在 actl-flow,命令面在 actl-cli。
//!
//! 钩子纪律(docs/79 §5.2):WH_*_LL 回调极简——时间戳+事件入队,零 UIA、零系统调用;
//! 一切反查(element_from_point / get_focused_element / 窗口元数据)在会话线程执行。
//! 低级钩子回调超时会被 Windows 回收(Quicker 实证),反查 P50 200ms+ 绝不能进回调。
//! 测试缝:ACTL_RECORD_DISABLE_HOOKS=1 跳过钩子线程(docs/06 测试缝节,契约测试离线)。

use crate::internal;
use actl_core::{
    CtlError,
    state::{SignalPaths, unix_ms},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write as IoWrite},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use uiautomation::UIAutomation;
use windows::{
    Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    },
    Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetThreadDpiAwarenessContext,
    },
    Win32::UI::WindowsAndMessaging::{
        self, CallNextHookEx, DispatchMessageW, FindWindowW, GA_ROOT, GetAncestor, GetMessageW,
        GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, HHOOK, KBDLLHOOKSTRUCT,
        LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP,
        WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
    core::w,
};

/// 会话配置(dir = records/<id>,由 CLI 侧解析传入)。
pub struct SessionConfig {
    pub id: String,
    pub dir: PathBuf,
    pub max_seconds: u64,
}

/// 捕获终态:事件全集(含会话头)交调用方编译落 draft.yml;state.json 的终态写入
/// 也由 CLI worker 完成——本模块负责 recording→finalizing 之间的全部状态转换。
pub struct SessionOutcome {
    pub events: Vec<Value>,
    pub stop_reason: String,
    pub skipped_own: u64,
}

/// records/ 根目录(%LOCALAPPDATA%\actl\records)。
pub fn records_dir() -> PathBuf {
    SignalPaths::default().dir.join("records")
}

/// True when `pid` names a running process(display_gate::process_alive 同判据)。
pub fn pid_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    if pid == 0 {
        return false;
    }
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };
    let mut exit: u32 = 0;
    let code = unsafe { windows::Win32::System::Threading::GetExitCodeProcess(handle, &mut exit) };
    let _ = unsafe { CloseHandle(handle) };
    code.is_ok() && exit == 259 // STILL_ACTIVE
}

/// spawn 分离的录制 worker(signal probe.rs 同款:CREATE_NO_WINDOW + stdio 全 NUL)。
/// worker = `actl record-worker --id … --max-seconds …`,会话有界、非常驻;
/// NUL stdio 保证 worker 的收尾 println 安全(直跑测试时 stdout 有效,envelope 正常)。
pub fn spawn_worker(id: &str, max_seconds: u64) -> Result<u32, CtlError> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    // 长命子进程派生纪律(docs/72 process_io):清自身 stdio 的 INHERIT 标志,
    // 否则 worker 继承调用方管道写端,$()/ReadToEnd/管道读全部阻塞到 worker 退出
    // (实测:record-start 的管道读停满 worker 整个生命周期;浏览器通道同坑先例)。
    crate::process_io::clear_stdio_inheritance();
    let exe = std::env::current_exe().map_err(internal)?;
    let child = Command::new(exe)
        .args([
            "record-worker",
            "--id",
            id,
            "--max-seconds",
            &max_seconds.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| internal(format!("record worker spawn: {e}")))?;
    Ok(child.id())
}

// ── 钩子线程:回调极简,事件入队 ──────────────────────────────────────────────
// 低级钩子回调在安装线程的消息泵上下文中执行(Windows 语义),故共享状态经
// thread_local 提供给钩子过程,worker 经 Arc 并发访问队列。

struct RawEvent {
    t_ms: u64,
    kind: RawKind,
}

enum RawKind {
    Click {
        button: &'static str,
        x: i32,
        y: i32,
        injected: bool,
    },
    /// 键按下。mods = 事件瞬间按住的修饰键名(回调侧 held 集快照,免 keyup 跟踪)。
    Key {
        vk: u32,
        mods: Vec<&'static str>,
        injected: bool,
    },
}

struct HookShared {
    queue: Mutex<VecDeque<RawEvent>>,
    signal: Condvar,
    thread_id: Mutex<Option<u32>>,
    install_error: Mutex<Option<String>>,
}

thread_local! {
    static SHARED: RefCell<Option<Arc<HookShared>>> = const { RefCell::new(None) };
    static HELD: RefCell<HashSet<u32>> = RefCell::new(HashSet::new());
}

fn vk_modifiers(vk: u32) -> Option<&'static str> {
    Some(match vk {
        0x11 | 0xA2 | 0xA3 => "ctrl",
        0x10 | 0xA0 | 0xA1 => "shift",
        0x12 | 0xA4 | 0xA5 => "alt",
        0x5B | 0x5C => "win",
        _ => return None,
    })
}

impl HookShared {
    fn push(&self, event: RawEvent) {
        if let Ok(mut queue) = self.queue.lock() {
            queue.push_back(event);
            drop(queue);
            self.signal.notify_all();
        }
    }

    /// 鼠标按下事件(移动/滚轮不录——语义事件纪律,docs/79 §5.4)。
    fn on_mouse(&self, wparam: WPARAM, lparam: LPARAM) {
        let button = match wparam.0 as u32 {
            WM_LBUTTONDOWN => "left",
            WM_RBUTTONDOWN => "right",
            WM_MBUTTONDOWN => "middle",
            _ => return,
        };
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        self.push(RawEvent {
            t_ms: unix_ms(),
            kind: RawKind::Click {
                button,
                x: info.pt.x,
                y: info.pt.y,
                injected: (info.flags & LLMHF_INJECTED) != 0,
            },
        });
    }

    /// 键事件:只录按下(按住重复去重);held 集合维护修饰键状态供 mods 快照。
    fn on_key(&self, wparam: WPARAM, lparam: LPARAM) {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let vk = info.vkCode;
        // 真机发现 #2:KBDLLHOOKSTRUCT 的 INJECTED 标志是 0x10(LLKHF_INJECTED;
        // 0x1 是 EXTENDED 位——0.62.2 未导出键盘侧常量,鼠标侧 LLMHF_INJECTED=0x1
        // 恰好同位,首版误用了 0x1 导致注入键全部漏标,实测 enter/VK_PACKET 均 false)。
        let injected = (info.flags.0 & 0x10) != 0; // LLKHF_INJECTED
        let message = wparam.0 as u32;
        HELD.with(|held| {
            let mut held = held.borrow_mut();
            match message {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    if !held.insert(vk) {
                        return; // 自动重复的第二次 down
                    }
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    held.remove(&vk);
                    return; // keyup 不入队
                }
                _ => return,
            }
            let mods: Vec<&'static str> = held.iter().filter_map(|vk| vk_modifiers(*vk)).collect();
            self.push(RawEvent {
                t_ms: unix_ms(),
                kind: RawKind::Key { vk, mods, injected },
            });
        });
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        SHARED.with(|cell| {
            if let Ok(shared) = cell.try_borrow()
                && let Some(shared) = shared.as_ref()
            {
                shared.on_mouse(wparam, lparam);
            }
        });
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn kbd_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        SHARED.with(|cell| {
            if let Ok(shared) = cell.try_borrow()
                && let Some(shared) = shared.as_ref()
            {
                shared.on_key(wparam, lparam);
            }
        });
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 钩子线程:安装双钩 + 消息泵;线程退出即卸载(会话有界,不残留系统钩子)。
fn hook_thread(shared: Arc<HookShared>, ready: mpsc::Sender<()>) {
    unsafe {
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0);
        let kbd = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kbd_proc), None, 0);
        let (mouse, kbd) = match (mouse, kbd) {
            (Ok(mouse), Ok(kbd)) => (mouse, kbd),
            (mouse_result, kbd_result) => {
                let error = match (&mouse_result, &kbd_result) {
                    (Err(error), _) => format!("mouse hook: {error}"),
                    (_, Err(error)) => format!("keyboard hook: {error}"),
                    _ => String::new(),
                };
                if let Ok(mut slot) = shared.install_error.lock() {
                    *slot = Some(error);
                }
                drop(mouse_result.ok().map(UnhookGuard));
                drop(kbd_result.ok().map(UnhookGuard));
                let _ = ready.send(());
                return;
            }
        };
        SHARED.with(|cell| {
            *cell.borrow_mut() = Some(shared.clone());
        });
        if let Ok(mut slot) = shared.thread_id.lock() {
            *slot = Some(windows::Win32::System::Threading::GetCurrentThreadId());
        }
        let _ = ready.send(());
        let mut msg = MSG::default();
        // LL 钩子回调在本线程泵内触发;WM_QUIT(GetMessageW==0)或错误(-1)退出。
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 == 0 || result.0 == -1 {
                break;
            }
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
        SHARED.with(|cell| {
            *cell.borrow_mut() = None;
        });
        let _ = UnhookWindowsHookEx(mouse);
        let _ = UnhookWindowsHookEx(kbd);
    }
}

/// 退出即卸载的守卫(安装失败路径用)。
struct UnhookGuard(#[allow(dead_code)] HHOOK);
impl Drop for UnhookGuard {
    fn drop(&mut self) {
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

// ── 反查与窗口元数据(会话线程) ────────────────────────────────────────────

fn window_json(hwnd: HWND) -> Value {
    unsafe {
        let root = if hwnd.0.is_null() {
            hwnd
        } else {
            GetAncestor(hwnd, GA_ROOT)
        };
        let mut title = String::new();
        if !root.0.is_null() {
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(root, &mut buf);
            if len > 0 {
                title = String::from_utf16_lossy(&buf[..len as usize]);
            }
        }
        let mut rect = RECT::default();
        let has_rect = !root.0.is_null() && GetWindowRect(root, &mut rect).is_ok();
        let dpi = if root.0.is_null() {
            96
        } else {
            GetDpiForWindow(root)
        };
        let mut pid = 0u32;
        if !root.0.is_null() {
            let _ = GetWindowThreadProcessId(root, Some(&mut pid));
        }
        json!({
            "hwnd": root.0 as isize,
            "title": title,
            "pid": pid,
            "rect": if has_rect { json!([rect.left, rect.top, rect.right, rect.bottom]) } else { Value::Null },
            "dpi": dpi,
        })
    }
}

fn element_identity(element: &uiautomation::UIElement) -> Value {
    json!({
        "automation_id": element.get_automation_id().unwrap_or_default(),
        "name": element.get_name().unwrap_or_default(),
        "control_type": element.get_control_type().map(|t| format!("{t:?}")).unwrap_or_default(),
        "runtime_id": element.get_runtime_id().unwrap_or_default(),
        "is_password": element.is_password().unwrap_or(false),
    })
}

/// 点击瞬间反查(docs/76 原则 2):坐标→元素身份;失败如实返回状态串。
fn click_element(auto: &UIAutomation, x: i32, y: i32) -> (Value, String) {
    match auto.element_from_point(uiautomation::types::Point::new(x, y)) {
        Ok(element) => (element_identity(&element), "ok".into()),
        Err(error) => {
            let message = error.to_string();
            let brief: String = message.chars().take(80).collect();
            (Value::Null, format!("failed:{brief}"))
        }
    }
}

/// 焦点身份缓存:物理焦点窗(kbd 判据,WinUI 下 UIA focused 会谎报——docs/09)+ UIA 元素身份。
struct FocusCache {
    hwnd: Option<isize>,
    title: Option<String>,
    element: Option<Value>,
    /// 密码框脱敏开关:置位期间键事件只记 masked(true),VK 不落盘。
    password: bool,
}

fn refresh_focus(auto: &UIAutomation) -> FocusCache {
    let root = crate::kbd::focused_root_window();
    let element = auto
        .get_focused_element()
        .ok()
        .map(|el| element_identity(&el));
    let password = element
        .as_ref()
        .and_then(|e| e["is_password"].as_bool())
        .unwrap_or(false);
    FocusCache {
        hwnd: root.as_ref().map(|(h, _)| *h),
        title: root.map(|(_, t)| t),
        element,
        password,
    }
}

/// burst 末值读回(docs/06 R2):提交后的最终文本——IME 中文/自动纠正的真值来源。
/// 真机发现 #4:burst 末/停止瞬间的读回与应用的输入提交管线竞态(实测停止在
/// 最后按键后 231ms,文档尚未提交→读到空,值事件缺失)——有界沉降重试:
/// ≤3 次、150ms 间隔(ACTL_SETTLE_MS 同族思想),耗尽如实返回无事件。
/// 真机发现 #11(rec-ma-5 复验轮):护栏曾按窗口标题精确比对——编辑会改标题
/// (脏标记星号:marker4 → *marker4),读回全程被拒、值事件系统性缺失;
/// 演示轮偶然成功只因标题从录制起就带星号。护栏改比焦点根 HWND(稳定身份)。
fn capture_value(auto: &UIAutomation, expected_hwnd: Option<isize>) -> Option<Value> {
    for attempt in 0..3 {
        if let Some(event) = read_focus_value(auto, expected_hwnd) {
            return Some(event);
        }
        if attempt < 2 {
            thread::sleep(Duration::from_millis(150));
        }
    }
    None
}

/// 单次读回尝试。守卫:焦点根窗须与键入时一致(按 HWND;不一致 = 焦点已换,
/// 读到的会是别的元素);密码焦点不读(脱敏不可关闭)。Value → Text 文档区
/// 回退链(真机发现 #3:TextPattern 型应用无 ValuePattern,与 verify 的
/// value/text 通道同族)。
fn read_focus_value(auto: &UIAutomation, expected_hwnd: Option<isize>) -> Option<Value> {
    let (hwnd, title) = crate::kbd::focused_root_window()?;
    if let Some(expected) = expected_hwnd
        && expected != hwnd
    {
        return None;
    }
    let element = auto.get_focused_element().ok()?;
    let identity = element_identity(&element);
    if identity["is_password"].as_bool() == Some(true) {
        return None;
    }
    let value = if let Ok(pattern) = element.get_pattern::<uiautomation::patterns::UIValuePattern>()
    {
        pattern.get_value().ok()
    } else if let Ok(pattern) = element.get_pattern::<uiautomation::patterns::UITextPattern>() {
        pattern
            .get_document_range()
            .ok()
            .and_then(|range| range.get_text(-1).ok())
    } else {
        None
    };
    let value = value.filter(|text| !text.is_empty())?;
    Some(json!({
        "t": unix_ms(), "kind": "value", "value": value,
        "focus": {"hwnd": hwnd, "title": title},
        "element": identity,
    }))
}

/// 追加事件行到 trace(JSONL)与事件集(编译输入)。
fn append_event(
    trace: &mut BufWriter<File>,
    events: &mut Vec<Value>,
    event: Value,
) -> Result<(), CtlError> {
    serde_json::to_writer(&mut *trace, &event)
        .map_err(|e| internal(format!("trace write: {e}")))?;
    trace
        .write_all(b"\n")
        .map_err(|e| internal(e.to_string()))?;
    events.push(event);
    Ok(())
}

fn write_state(dir: &Path, status: &str, extra: Value) {
    let mut state = json!({
        "id": dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "status": status,
        "pid": unsafe { GetCurrentProcessId() },
        "started_ms": 0,
    });
    if let (Some(base), Some(add)) = (state.as_object_mut(), extra.as_object()) {
        for (key, value) in add {
            base.insert(key.clone(), value.clone());
        }
    }
    let tmp = dir.join("state.json.tmp");
    let path = dir.join("state.json");
    let _ = fs::write(&tmp, serde_json::to_vec(&state).unwrap_or_default());
    let _ = fs::rename(&tmp, &path);
}

/// 会话主循环(由 record-worker 命令进入)。
pub fn run_session(cfg: &SessionConfig) -> Result<SessionOutcome, CtlError> {
    fs::create_dir_all(&cfg.dir).map_err(|e| internal(format!("records dir: {e}")))?;
    let started_ms = unix_ms();
    write_state(
        &cfg.dir,
        "recording",
        json!({"started_ms": started_ms, "max_seconds": cfg.max_seconds, "events": 0, "skipped_own": 0}),
    );

    let auto = UIAutomation::new().map_err(internal)?;
    // 真机发现 #1:钩子事件坐标是物理屏幕坐标,而 WindowFromPoint/ElementFromPoint
    // 按调用线程的 DPI 感知解释坐标——多显示器混合 DPI 下错位(实测点击反查命中
    // 隔壁屏元素、窗口元数据全空)。会话线程设 PER_MONITOR_AWARE_V2 对齐口径。
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let own_pid = unsafe { GetCurrentProcessId() };
    let signal_pid = unsafe {
        FindWindowW(w!("actl_signal_window"), None)
            .ok()
            .map(|hwnd| {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                pid
            })
            .filter(|pid| *pid != 0)
    };

    let mut trace = BufWriter::new(
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(cfg.dir.join("trace.jsonl"))
            .map_err(|e| internal(format!("trace open: {e}")))?,
    );
    let header = json!({
        "kind": "session",
        "id": cfg.id,
        "started_ms": started_ms,
        "max_seconds": cfg.max_seconds,
        "pid": own_pid,
    });
    serde_json::to_writer(&mut trace, &header).map_err(|e| internal(e.to_string()))?;
    trace
        .write_all(b"\n")
        .map_err(|e| internal(e.to_string()))?;

    // 钩子线程(测试缝:禁用钩子也会话协议全真,docs/06)。
    let hooks_disabled = std::env::var_os("ACTL_RECORD_DISABLE_HOOKS").is_some_and(|v| v == "1");
    let shared = Arc::new(HookShared {
        queue: Mutex::new(VecDeque::new()),
        signal: Condvar::new(),
        thread_id: Mutex::new(None),
        install_error: Mutex::new(None),
    });
    let pump = if hooks_disabled {
        None
    } else {
        let (ready, was_ready) = mpsc::channel();
        let hook_shared = shared.clone();
        let handle = thread::spawn(move || hook_thread(hook_shared, ready));
        was_ready
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| internal("hook thread did not signal readiness"))?;
        if let Ok(slot) = shared.install_error.lock()
            && let Some(error) = slot.clone()
        {
            let _ = handle.join();
            write_state(&cfg.dir, "failed", json!({"error": error}));
            return Err(CtlError::internal(format!("hook install: {error}")));
        }
        Some(handle)
    };

    let mut events: Vec<Value> = vec![header];
    let mut focus: Option<FocusCache> = None;
    // 键入 burst 待收口状态:(焦点根 HWND, 最后键事件 t)——值读回的触发时机 =
    // 静默 300ms / 点击事件 / 会话终点。HWND 而非标题:编辑会改标题(发现 #11)。
    let mut pending_keys: Option<(Option<isize>, u64)> = None;
    let mut skipped_own: u64 = 0;
    let started = Instant::now();
    let mut last_state_ms = started_ms;
    let stop_path = cfg.dir.join("stop-requested");
    let stop_reason;

    // 看门狗心跳:事件处理 = 进展(长录制会话不误杀)。
    actl_core::watchdog::reset();
    loop {
        if stop_path.exists() {
            stop_reason = "requested";
            break;
        }
        if started.elapsed().as_secs() >= cfg.max_seconds {
            stop_reason = "duration_cap";
            break;
        }
        // 取事件批次:空则带超时等待(stop 轮询 100ms 粒度)。
        let batch = {
            let mut queue = shared
                .queue
                .lock()
                .map_err(|_| internal("hook queue poisoned"))?;
            if queue.is_empty() {
                let (mut guard, _) = shared
                    .signal
                    .wait_timeout(queue, Duration::from_millis(100))
                    .map_err(|_| internal("hook queue poisoned"))?;
                guard.drain(..).collect::<Vec<_>>()
            } else {
                queue.drain(..).collect::<Vec<_>>()
            }
        };

        for raw in batch {
            let event = match raw.kind {
                RawKind::Click {
                    button,
                    x,
                    y,
                    injected,
                } => {
                    // 点击终结键入 burst:先补值读回(提交后最终文本,IME 中文的
                    // 唯一可靠来源),值事件先于点击事件入 trace(docs/06 R2)。
                    if let Some(expected) = pending_keys.take()
                        && let Some(value) = capture_value(&auto, expected.0)
                    {
                        append_event(&mut trace, &mut events, value)?;
                    }
                    // actl 自家窗口过滤(显示端/录制器自身)——"点录制 UI 自己"不进 trace。
                    let hit = unsafe { WindowsAndMessaging::WindowFromPoint(POINT { x, y }) };
                    let mut pid = 0;
                    if !hit.0.is_null() {
                        let _ = unsafe { GetWindowThreadProcessId(hit, Some(&mut pid)) };
                    }
                    if pid == own_pid || signal_pid == Some(pid) {
                        skipped_own += 1;
                        continue;
                    }
                    // 注入事件同样全量捕获(反查+元数据):trace 是"发生过什么"的完整证据
                    // (机器动作带目标身份,诊断价值更高;只读查询与注入方分属两进程,
                    // 无发现 #4 的同进程并发问题)。编译侧继续排除注入(录的是人的演示)。
                    let window = window_json(hit);
                    let (element, lookup) = click_element(&auto, x, y);
                    json!({
                        "t": raw.t_ms, "kind": "click", "button": button, "x": x, "y": y,
                        "injected": injected,
                        "window": window, "element": element, "lookup": lookup,
                    })
                }
                RawKind::Key { vk, mods, injected } => {
                    if focus.as_ref().is_some_and(|f| f.password) {
                        // 密码脱敏:焦点在密码框期间,VK 一律不落盘(不可关闭,docs/06;
                        // 对注入键同样生效——机器向密码框写入也不留键迹)。
                        json!({"t": raw.t_ms, "kind": "key", "masked": true})
                    } else {
                        // 焦点窗变化才刷新身份(一次 UIA 调用,物理判据先行)。
                        let root_now = crate::kbd::focused_root_window().map(|(h, _)| h);
                        if focus.as_ref().is_none_or(|f| f.hwnd != root_now) {
                            focus = Some(refresh_focus(&auto));
                        }
                        let focus = focus.as_ref().unwrap();
                        // 记录 burst 待收口状态(密码焦点不进——值读回会泄露)。
                        pending_keys = Some((focus.hwnd, raw.t_ms));
                        json!({
                            "t": raw.t_ms, "kind": "key", "vk": vk, "mods": mods,
                            "ime": vk == 0xE5, "injected": injected,
                            "focus": {"hwnd": focus.hwnd, "title": focus.title},
                            "focus_element": focus.element.clone(),
                        })
                    }
                }
            };
            serde_json::to_writer(&mut trace, &event)
                .map_err(|e| internal(format!("trace write: {e}")))?;
            trace
                .write_all(b"\n")
                .map_err(|e| internal(e.to_string()))?;
            events.push(event);
        }
        // 键入静默 ≥300ms = burst 末:读回提交值(用户还没点下一个目标,
        // 焦点最可能仍在输入控件上——值配对成功率最高的时机)。
        if let Some((expected, last_t)) = pending_keys
            && unix_ms().saturating_sub(last_t) >= 300
            && let Some(value) = capture_value(&auto, expected)
        {
            pending_keys = None;
            append_event(&mut trace, &mut events, value)?;
        }
        trace.flush().map_err(|e| internal(e.to_string()))?;

        let now = unix_ms();
        if now - last_state_ms > 1000 {
            last_state_ms = now;
            write_state(
                &cfg.dir,
                "recording",
                json!({
                    "started_ms": started_ms, "max_seconds": cfg.max_seconds,
                    "events": events.len() - 1, "skipped_own": skipped_own,
                }),
            );
        }
    }

    // 卸钩:请求钩子线程退出(WM_QUIT)并回收;禁用钩子路径无泵。
    if let Some(handle) = pump {
        if let Ok(slot) = shared.thread_id.lock()
            && let Some(thread_id) = *slot
        {
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
        let _ = handle.join();
    }
    // 会话终点补最后一次值读回(最后一段键入可能直到停止都没等到静默/点击)。
    if let Some(expected) = pending_keys.take()
        && let Some(value) = capture_value(&auto, expected.0)
    {
        append_event(&mut trace, &mut events, value)?;
    }
    write_state(
        &cfg.dir,
        "finalizing",
        json!({
            "started_ms": started_ms, "max_seconds": cfg.max_seconds,
            "events": events.len() - 1, "skipped_own": skipped_own,
            "stop_reason": stop_reason,
        }),
    );
    trace.flush().map_err(|e| internal(e.to_string()))?;
    Ok(SessionOutcome {
        events,
        stop_reason: stop_reason.into(),
        skipped_own,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn pid_alive_reports_current_process_alive() {
        let pid = unsafe { super::GetCurrentProcessId() };
        assert!(super::pid_alive(pid));
    }

    #[test]
    fn pid_alive_reports_missing_pid_dead() {
        assert!(!super::pid_alive(999999));
    }
}
