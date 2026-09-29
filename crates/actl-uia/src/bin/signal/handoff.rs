//! Visible, non-activating consent panel and passive input observation.
use actl_core::{
    handoff::*,
    state::{SignalPaths, unix_ms},
};
use std::{
    cell::RefCell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey,
};
use windows::{
    Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

thread_local! {
    static PANEL_REQUEST: RefCell<Option<Request>> = const { RefCell::new(None) };
    static PANEL_EXPANDED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
static WATCH_RUNNING: AtomicBool = AtomicBool::new(false);
static INPUT_SEQ: AtomicU64 = AtomicU64::new(0);

fn audit(request: &Request, stage: &str, reason: &str) {
    super::handoff_audit::record(
        request,
        stage,
        reason,
        INPUT_SEQ.load(Ordering::SeqCst),
        super::input_watch::ACTIVITY.load(Ordering::SeqCst),
    );
}
fn activity() -> u64 {
    INPUT_SEQ
        .load(Ordering::SeqCst)
        .wrapping_add(super::input_watch::ACTIVITY.load(Ordering::SeqCst))
}

/// 交接授权快捷键:Ctrl+Alt+Y(=开始/继续;Ctrl+Alt+F12 已留作紧急停止)。
/// 与按钮等价的授权通道——免鼠标行程与落点瞄准;授权动作的余韵(键抬起/
/// 手部后摇)由 Consent 宽限窗(GRACE_MS)统一吸收。
const HOTKEY_ID: i32 = 0xB01;
const HOTKEY_KEY: u32 = 'Y' as u32;
#[derive(Default)]
struct Service {
    generation: Option<String>,
    consent: Consent,
    request: Option<(u32, Request)>,
    deferred: Option<String>,
    active: Option<(u32, String, String)>,
    active_request: Option<Request>,
    audited_failure: Option<String>,
    panel: Option<HWND>,
    failed_call: Option<String>,
    paused_call: Option<String>,
}
thread_local! { static SERVICE: RefCell<Service> = RefCell::new(Service::default()); }

impl Service {
    fn audit_failure(&mut self, request: Option<&Request>, reason: &str) {
        let request = request.or(self.active_request.as_ref());
        if let Some(request) = request
            && self.audited_failure.as_deref() != Some(&request.call)
        {
            audit(request, "revoked", reason);
            self.audited_failure = Some(request.call.clone());
        }
    }
    fn invalidate(&mut self) {
        self.consent.revoke();
        self.active = None;
        self.active_request = None;
        self.request = None;
        self.generation = None;
        unsafe {
            close_panel(self);
        }
    }
    fn observe_generation(&mut self, generation: &str) {
        if self.generation.as_deref() != Some(generation) {
            self.invalidate();
            self.generation = Some(generation.into());
        }
    }
}

struct Hooks(HHOOK, HHOOK);
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
            let _ = UnhookWindowsHookEx(self.1);
        }
    }
}
unsafe fn install_hooks() -> windows::core::Result<Hooks> {
    let module = GetModuleHandleW(None)?;
    let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), Some(module.into()), 0)?;
    match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), Some(module.into()), 0) {
        Ok(mouse) => Ok(Hooks(keyboard, mouse)),
        Err(e) => {
            let _ = UnhookWindowsHookEx(keyboard);
            Err(e)
        }
    }
}
// Keep callbacks off the renderer thread: slow paint/IO must not starve hooks.
pub struct Watch(u32, Option<std::thread::JoinHandle<()>>);
impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.0, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.1.take() {
            let _ = thread.join();
        }
    }
}
pub unsafe fn watch() -> windows::core::Result<Watch> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("actl-input-watch".into())
        .spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
            let hooks = match install_hooks() {
                Ok(hooks) => hooks,
                Err(error) => {
                    let _ = tx.send(Err(error));
                    return;
                }
            };
            let _hooks = hooks;
            let _raw = match super::input_watch::RawWatch::start() {
                Ok(raw) => raw,
                Err(error) => {
                    let _ = tx.send(Err(error));
                    return;
                }
            };
            struct Running;
            impl Drop for Running {
                fn drop(&mut self) {
                    WATCH_RUNNING.store(false, Ordering::SeqCst);
                }
            }
            WATCH_RUNNING.store(true, Ordering::SeqCst);
            let _running = Running;
            let id = windows::Win32::System::Threading::GetCurrentThreadId();
            if tx.send(Ok(id)).is_err() {
                return;
            }
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .map_err(|e| {
            windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), e.to_string())
        })?;
    match rx.recv() {
        Ok(Ok(id)) => Ok(Watch(id, Some(thread))),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = thread.join();
            Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "input watcher failed",
            ))
        }
    }
}
unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        super::input_diagnostics::hook(event.flags.contains(LLKHF_INJECTED), event.dwExtraInfo);
        if external_input(event.flags.contains(LLKHF_INJECTED), event.dwExtraInfo) {
            INPUT_SEQ.fetch_add(1, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(lp.0 as *const MSLLHOOKSTRUCT);
        super::input_diagnostics::hook(event.flags & LLMHF_INJECTED != 0, event.dwExtraInfo);
        if external_input(event.flags & LLMHF_INJECTED != 0, event.dwExtraInfo) {
            INPUT_SEQ.fetch_add(1, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

fn read_request(paths: &SignalPaths, pid: u32) -> Option<Request> {
    let data = std::fs::read(paths.dir.join(format!("handoff-{pid}.json"))).ok()?;
    if data.len() > 8192 {
        return None;
    }
    let r: Request = serde_json::from_slice(&data).ok()?;
    r.fresh(unix_ms()).then_some(r)
}

/// Synchronous query: callback input counters are checked at every injection boundary.
pub fn message(msg: u32, pid: u32) -> usize {
    if !WATCH_RUNNING.load(Ordering::SeqCst) || !super::input_watch::healthy() {
        SERVICE.with(|s| {
            let mut s = s.borrow_mut();
            s.consent.revoke_with_reason("input_monitor_unavailable");
            s.audit_failure(
                read_request(&SignalPaths::default(), pid).as_ref(),
                "input_monitor_unavailable",
            );
        });
        return MONITOR_UNAVAILABLE;
    }
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        let paths = SignalPaths::default();
        let Ok(generation) = paths.execution_generation() else {
            s.invalidate();
            return CANCELLED;
        };
        s.observe_generation(&generation);
        let seq = activity();
        if msg == CHECK_MESSAGE {
            if paths.stop_requested() {
                s.consent.revoke();
                return CANCELLED;
            }
            let Some((owner, call, task)) = s.active.clone() else {
                return YIELDED;
            };
            if owner == pid && s.paused_call.as_deref() == Some(&call) {
                return USER_PAUSED;
            }
            return if owner == pid && s.consent.check(&task, seq, unix_ms()) {
                ALLOWED
            } else {
                let reason = s.consent.reason();
                s.audit_failure(None, reason);
                YIELDED
            };
        }
        let Some(r) = read_request(&paths, pid) else {
            return YIELDED;
        };
        if r.generation != generation {
            return CANCELLED;
        }
        if s.paused_call.as_deref() == Some(&r.call) {
            return USER_PAUSED;
        }
        if s.failed_call.as_deref() == Some(&r.call) {
            return PANEL_UNAVAILABLE;
        }
        if s.deferred.as_deref() == Some(&r.call) {
            return DEFERRED;
        }
        if !paths.stop_requested() && s.consent.check_policy(&r.task, seq, unix_ms(), r.approval) {
            audit(&r, "reused", "allowed");
            s.active_request = Some(r.clone());
            s.active = Some((pid, r.call, r.task));
            return ALLOWED;
        }
        // A different caller cannot overwrite a visible prompt.
        if s.request
            .as_ref()
            .is_none_or(|(_, pending)| pending.call == r.call)
        {
            s.request = Some((pid, r));
        }
        PENDING
    })
}

unsafe fn close_panel(s: &mut Service) {
    if let Some(hwnd) = s.panel.take() {
        let _ = DestroyWindow(hwnd);
    }
}
pub unsafe fn tick() {
    tick_with(&SignalPaths::default(), super::input_watch::healthy());
}
unsafe fn tick_with(paths: &SignalPaths, healthy: bool) {
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        // The caller may have exited or stopped polling. Retire its visible prompt
        // independently, including repairs that leave no stop-requested marker.
        let Ok(generation) = paths.execution_generation() else {
            s.invalidate();
            return;
        };
        s.observe_generation(&generation);
        if paths.stop_requested() || !healthy {
            s.consent.revoke_with_reason(if healthy {
                "stopped"
            } else {
                "input_monitor_unavailable"
            });
            let reason = s.consent.reason();
            s.audit_failure(None, reason);
        }
        if let Some((pid, request)) = &s.request
            && read_request(paths, *pid).is_none_or(|r| r.call != request.call)
        {
            s.request = None;
            close_panel(&mut s);
        }
        if s.request.is_some() && s.panel.is_none() {
            match create_panel(s.request.as_ref().map(|(_, r)| r)) {
                Ok(hwnd) => {
                    if let Some((_, request)) = &s.request {
                        audit(request, "shown", s.consent.reason());
                    }
                    s.panel = Some(hwnd);
                }
                Err(e) => {
                    eprintln!("NOT_ACTIONABLE: handoff panel unavailable: {e}");
                    s.failed_call = s.request.take().map(|(_, r)| r.call);
                }
            }
        }
    });
}

unsafe fn create_panel(request: Option<&Request>) -> windows::core::Result<HWND> {
    let hwnd = build_panel(request)?;
    if RegisterHotKey(Some(hwnd), HOTKEY_ID, MOD_CONTROL | MOD_ALT, HOTKEY_KEY).is_err()
        && let Ok(label) = GetDlgItem(Some(hwnd), 105)
    {
        let _ = SetWindowTextW(label, w!("请点击按钮确认"));
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    Ok(hwnd)
}
pub(super) unsafe fn build_panel(request: Option<&Request>) -> windows::core::Result<HWND> {
    build_panel_with(request, &SignalPaths::default())
}
pub(super) unsafe fn build_panel_with(
    request: Option<&Request>,
    paths: &SignalPaths,
) -> windows::core::Result<HWND> {
    let module = GetModuleHandleW(None)?;
    let class = w!("actl_handoff_window");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(panel_proc),
        hInstance: module.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        style: CS_DROPSHADOW,
        hbrBackground: HBRUSH::default(),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassW(&wc);
    let mut work = RECT::default();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )?;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_APPWINDOW | WS_EX_NOACTIVATE,
        class,
        w!("actl · 等待交接"),
        WS_POPUP,
        work.left + 24,
        work.top + 70,
        470,
        230,
        None,
        None,
        Some(module.into()),
        None,
    )?;
    PANEL_REQUEST.with(|r| *r.borrow_mut() = request.cloned());
    PANEL_EXPANDED.with(|v| v.set(false));
    let plan = request.and_then(|r| super::ui_text::plan(paths, &r.task));
    let title = if plan.is_some() {
        super::ui_text::task_title(plan.as_ref())
    } else {
        "桌面操作".into()
    };
    let state = request.and_then(|r| super::flow_ui::read(paths, &r.task));
    let continuing = state
        .as_ref()
        .and_then(|s| s["steps"].as_array())
        .is_some_and(|steps| {
            steps
                .iter()
                .any(|s| s["attempts"].as_u64().unwrap_or(0) > 0 || s["status"] == "completed")
        });
    let context = format!("目标：{}", request.map_or("待确认", |r| r.target.as_str()));
    let policy_text = if request.is_some_and(|r| r.approval == Approval::OncePerRun) {
        "本任务开始时确认一次，不因等待超时。\r\n使用键鼠与剪贴板；人工接管后暂停。"
    } else {
        "可能切换窗口、使用键鼠和剪贴板。\r\n确认后请暂勿操作；操作键鼠会请求暂停。"
    };
    let children = (|| -> windows::core::Result<()> {
        for (id, text) in [
            (102, title.as_str()),
            (15, "关闭"),
            (103, "等待你确认"),
            (10, context.as_str()),
            (104, policy_text),
            (105, "Ctrl+Alt+Y"),
            (14, "详细信息"),
            (
                2,
                if continuing {
                    "暂不继续"
                } else {
                    "暂不开始"
                },
            ),
            (1, if continuing { "继续" } else { "开始" }),
        ] {
            let label = super::paint::wide(text);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                if matches!(id, 1 | 2 | 14 | 15) {
                    w!("BUTTON")
                } else if id == 10 {
                    w!("EDIT")
                } else {
                    w!("STATIC")
                },
                PCWSTR(label.as_ptr()),
                WS_CHILD
                    | WS_VISIBLE
                    | if matches!(id, 1 | 2 | 14 | 15) {
                        WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32)
                    } else if id == 10 {
                        WS_VSCROLL
                            | WINDOW_STYLE(
                                ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32,
                            )
                    } else if matches!(id, 102 | 103) {
                        WINDOW_STYLE(0x4000) // SS_ENDELLIPSIS
                    } else {
                        WINDOW_STYLE(0)
                    },
                0,
                0,
                0,
                0,
                Some(hwnd),
                Some(HMENU(id as *mut _)),
                Some(module.into()),
                None,
            )?;
        }
        Ok(())
    })();
    if let Err(e) = children {
        let _ = DestroyWindow(hwnd);
        return Err(e);
    }
    layout(hwnd, GetDpiForWindow(hwnd));
    Ok(hwnd)
}
unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| super::paint::pixels(n, dpi);
    let compact = if PANEL_EXPANDED.with(|v| v.get()) {
        0
    } else {
        60
    };
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        px(540),
        px(410 - compact),
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    for (id, x, y, width, height) in [
        (102, 24, 22, 428, 32),
        (15, 476, 22, 40, 32),
        (103, 24, 62, 492, 22),
        (10, 24, 104, 492, 138 - compact),
        (104, 24, 260, 492, 48),
        (105, 368, 314, 148, 24),
        (14, 24, 346, 148, 40),
        (2, 252, 346, 104, 40),
        (1, 368, 346, 148, 40),
    ] {
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
            let _ = SetWindowPos(
                child,
                None,
                px(x),
                px(if y >= 260 { y - compact } else { y }),
                px(width),
                px(height),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    super::panel::fonts(hwnd, dpi, &[102, 103, 10, 104, 105, 2, 1, 14, 15]);
    if let Ok(edit) = GetDlgItem(Some(hwnd), 10) {
        let rect = RECT {
            left: px(14),
            top: px(10),
            right: px(464),
            bottom: px(128 - compact),
        };
        SendMessageW(
            edit,
            windows::Win32::UI::Controls::EM_SETRECT,
            None,
            Some(LPARAM((&rect as *const RECT) as isize)),
        );
        super::panel::scroll(edit);
    }
}
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Some(result) = super::panel::message(hwnd, msg, wp, lp) {
        return result;
    }
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_DPICHANGED => {
            let rect = *(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            layout(hwnd, wp.0 as u32 & 0xffff);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
            super::panel::release(hwnd);
            PANEL_REQUEST.with(|r| r.borrow_mut().take());
            LRESULT(0)
        }
        WM_COMMAND if wp.0 >> 16 == BN_CLICKED as usize && wp.0 & 0xffff == 14 => {
            let expanded = PANEL_EXPANDED.with(|v| {
                v.set(!v.get());
                v.get()
            });
            let text = PANEL_REQUEST.with(|r| {
                r.borrow().as_ref().map(|r| {
                    if expanded {
                        format!("目标：{}\r\n\r\n任务编号：{}", r.target, r.task)
                    } else {
                        format!("目标：{}", r.target)
                    }
                })
            });
            if let Some(text) = text
                && let Ok(edit) = GetDlgItem(Some(hwnd), 10)
            {
                let wide = super::paint::wide(&text);
                let _ = SetWindowTextW(edit, PCWSTR(wide.as_ptr()));
                super::panel::scroll(edit);
            }
            if let Ok(button) = GetDlgItem(Some(hwnd), 14) {
                let _ = SetWindowTextW(
                    button,
                    if expanded {
                        w!("收起详情")
                    } else {
                        w!("详细信息")
                    },
                );
            }
            layout(hwnd, GetDpiForWindow(hwnd));
            LRESULT(0)
        }
        WM_COMMAND if wp.0 >> 16 != BN_CLICKED as usize || !matches!(wp.0 & 0xffff, 1 | 2 | 15) => {
            LRESULT(0)
        }
        WM_COMMAND | WM_CLOSE => {
            let start = msg == WM_COMMAND && wp.0 & 0xffff == 1;
            decide(hwnd, start);
            LRESULT(0)
        }
        WM_HOTKEY if wp.0 as i32 == HOTKEY_ID => {
            decide(hwnd, true);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// 授权/稍后的共同裁决:开始(按钮或 Ctrl+Alt+Y)在请求仍新鲜且未停止时
/// 授予 Consent;其余路径(关闭/稍后/过期)记 deferred。任何路径都收面板。
unsafe fn decide(_hwnd: HWND, start: bool) {
    // No outstanding SERVICE borrow when dispatching user messages.
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        if let Some((pid, r)) = s.request.take() {
            let fresh =
                read_request(&SignalPaths::default(), pid).is_some_and(|now| now.call == r.call);
            if start
                && fresh
                && WATCH_RUNNING.load(Ordering::SeqCst)
                && super::input_watch::healthy()
                && SignalPaths::default()
                    .acknowledge_stop(&r.generation)
                    .is_ok()
            {
                s.consent
                    .allow_with_policy(&r.task, activity(), unix_ms(), r.approval);
                audit(&r, "granted", "user_start");
            } else {
                s.deferred = Some(r.call);
            }
        }
        close_panel(&mut s);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handoff_layout_scales_native_bounds_and_font() {
        unsafe {
            let module = GetModuleHandleW(None).unwrap();
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("handoff-test"),
                WS_POPUP,
                0,
                0,
                540,
                280,
                None,
                None,
                Some(module.into()),
                None,
            )
            .unwrap();
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("text"),
                WS_CHILD,
                0,
                0,
                10,
                10,
                Some(parent),
                Some(HMENU(10usize as *mut _)),
                Some(module.into()),
                None,
            )
            .unwrap();
            layout(parent, 144);
            let mut rect = RECT::default();
            GetWindowRect(parent, &mut rect).unwrap();
            assert_eq!(rect.right - rect.left, 810);
            assert_eq!(rect.bottom - rect.top, 525);
            let font = SendMessageW(child, WM_GETFONT, None, None);
            let mut info = LOGFONTW::default();
            assert!(
                GetObjectW(
                    HGDIOBJ(font.0 as *mut _),
                    std::mem::size_of::<LOGFONTW>() as i32,
                    Some((&mut info as *mut LOGFONTW).cast())
                ) > 0
            );
            assert_eq!(info.lfHeight, -24);
            super::super::panel::release(parent);
            DestroyWindow(parent).unwrap();
        }
    }
}

/// Target only the invocation represented by the clicked strip, never a global stop.
pub fn pause_call(call: &str) {
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        let owns = s.active.as_ref().is_some_and(|(_, c, _)| c == call)
            || s.request.as_ref().is_some_and(|(_, r)| r.call == call);
        if owns {
            s.consent.revoke();
        }
        s.paused_call = Some(call.into());
        if s.request.as_ref().is_some_and(|(_, r)| r.call == call) {
            s.request = None;
            unsafe {
                close_panel(&mut s);
            }
        }
    });
}
/// A real user's Continue click grants only the flow that the button is bound to.
pub fn allow_flow(task: &str, generation: &str) -> bool {
    if !WATCH_RUNNING.load(Ordering::SeqCst) || !super::input_watch::healthy() {
        return false;
    }
    let paths = SignalPaths::default();
    let Some(plan) = super::ui_text::plan(&paths, task) else {
        return false;
    };
    let policy = match plan.get("handoff") {
        None | Some(serde_json::Value::Null) => Policy::default(),
        Some(value) => match serde_json::from_value::<Policy>(value.clone()) {
            Ok(policy) => policy,
            Err(_) => return false,
        },
    };
    if paths.acknowledge_stop(generation).is_err() {
        return false;
    }
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        s.observe_generation(generation);
        s.consent
            .allow_with_policy(task, activity(), unix_ms(), policy.approval);
    });
    true
}
pub fn revoke() {
    SERVICE.with(|cell| cell.borrow_mut().consent.revoke());
}

#[cfg(test)]
#[path = "handoff_lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
mod pause_tests {
    use super::*;
    #[test]
    fn changing_generation_revokes_same_task_grant_and_active_caller() {
        let mut s = Service::default();
        s.observe_generation("old");
        s.consent.allow("task", 0, 100);
        s.active = Some((42, "call".into(), "task".into()));
        s.observe_generation("new");
        assert!(s.active.is_none());
        assert!(!s.consent.check("task", 0, 101));
        s.consent.allow("task", 0, 102);
        s.observe_generation("new");
        assert!(s.consent.check("task", 0, 103));
    }
    #[test]
    fn revoke_requires_new_consent_without_resuming_old_task() {
        SERVICE.with(|cell| {
            let mut s = cell.borrow_mut();
            *s = Service::default();
            s.consent.allow("task-a", 0, 100);
        });
        revoke();
        SERVICE.with(|cell| assert!(!cell.borrow_mut().consent.check("task-a", 0, 101)));
    }
    #[test]
    fn pause_is_call_bound_and_does_not_revoke_another_task() {
        SERVICE.with(|cell| {
            let mut s = cell.borrow_mut();
            *s = Service::default();
            s.active = Some((10, "call-a".into(), "task-a".into()));
            s.consent.allow("task-a", 0, 100);
        });
        pause_call("call-b");
        SERVICE.with(|cell| assert!(cell.borrow_mut().consent.check("task-a", 0, 101)));
        pause_call("call-a");
        SERVICE.with(|cell| {
            let mut s = cell.borrow_mut();
            assert!(!s.consent.check("task-a", 0, 102));
            assert_eq!(s.paused_call.as_deref(), Some("call-a"));
        });
    }
}
