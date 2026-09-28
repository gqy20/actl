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
use windows::{
    Win32::{Foundation::*, System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

static WATCH_RUNNING: AtomicBool = AtomicBool::new(false);
static INPUT_SEQ: AtomicU64 = AtomicU64::new(0);
#[derive(Default)]
struct Service {
    consent: Consent,
    request: Option<(u32, Request)>,
    deferred: Option<String>,
    active: Option<(u32, String, String)>,
    panel: Option<HWND>,
    failed_call: Option<String>,
    paused_call: Option<String>,
}
thread_local! { static SERVICE: RefCell<Service> = RefCell::new(Service::default()); }

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
        if external_input(event.flags.contains(LLKHF_INJECTED), event.dwExtraInfo) {
            INPUT_SEQ.fetch_add(1, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let event = &*(lp.0 as *const MSLLHOOKSTRUCT);
        if external_input(event.flags & LLMHF_INJECTED != 0, event.dwExtraInfo) {
            INPUT_SEQ.fetch_add(1, Ordering::SeqCst);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

fn read_request(pid: u32) -> Option<Request> {
    let data = std::fs::read(
        SignalPaths::default()
            .dir
            .join(format!("handoff-{pid}.json")),
    )
    .ok()?;
    if data.len() > 8192 {
        return None;
    }
    let r: Request = serde_json::from_slice(&data).ok()?;
    r.fresh(unix_ms()).then_some(r)
}

/// Synchronous query: callback input counters are checked at every injection boundary.
pub fn message(msg: u32, pid: u32) -> usize {
    if !WATCH_RUNNING.load(Ordering::SeqCst) {
        return MONITOR_UNAVAILABLE;
    }
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        let seq = INPUT_SEQ.load(Ordering::SeqCst);
        if msg == CHECK_MESSAGE {
            let Some((owner, call, task)) = s.active.clone() else {
                return YIELDED;
            };
            if owner == pid && s.paused_call.as_deref() == Some(&call) {
                return USER_PAUSED;
            }
            return if owner == pid && s.consent.check(&task, seq, unix_ms()) {
                ALLOWED
            } else {
                YIELDED
            };
        }
        let Some(r) = read_request(pid) else {
            return YIELDED;
        };
        if s.paused_call.as_deref() == Some(&r.call) {
            return USER_PAUSED;
        }
        if s.failed_call.as_deref() == Some(&r.call) {
            return PANEL_UNAVAILABLE;
        }
        if s.deferred.as_deref() == Some(&r.call) {
            return DEFERRED;
        }
        if s.consent.check(&r.task, seq, unix_ms()) {
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
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        if SignalPaths::default().stop_requested() {
            s.consent.revoke();
        }
        if let Some((pid, request)) = &s.request
            && read_request(*pid).is_none_or(|r| r.call != request.call)
        {
            s.request = None;
            close_panel(&mut s);
        }
        if s.request.is_some() && s.panel.is_none() {
            match create_panel(s.request.as_ref().map(|(_, r)| r)) {
                Ok(hwnd) => s.panel = Some(hwnd),
                Err(e) => {
                    eprintln!("NOT_ACTIONABLE: handoff panel unavailable: {e}");
                    s.failed_call = s.request.take().map(|(_, r)| r.call);
                }
            }
        }
    });
}

unsafe fn create_panel(request: Option<&Request>) -> windows::core::Result<HWND> {
    let module = GetModuleHandleW(None)?;
    let class = w!("actl_handoff_window");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(panel_proc),
        hInstance: module.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(6usize as *mut _),
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
        WS_POPUP | WS_BORDER,
        work.left + 24,
        work.top + 70,
        470,
        230,
        None,
        None,
        Some(module.into()),
        None,
    )?;
    let label = format!(
        "准备开始 / 继续桌面操作\n任务：{}\n当前目标：{}\n任务可能切换窗口、使用键鼠和剪贴板。\n点击开始后请暂时放开键鼠；继续操作会让任务停下。",
        request.map_or("", |r| r.task.as_str()),
        request.map_or("", |r| r.target.as_str())
    );
    let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
    let children = (|| -> windows::core::Result<()> {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR(label.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            16,
            14,
            438,
            145,
            Some(hwnd),
            Some(HMENU(10usize as *mut _)),
            Some(module.into()),
            None,
        )?;
        for (id, text, x) in [(1, w!("开始 / 继续"), 16), (2, w!("稍后"), 190)] {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("BUTTON"),
                text,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                x,
                174,
                155,
                36,
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
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    Ok(hwnd)
}
unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| super::paint::pixels(n, dpi);
    let font = CreateFontW(
        -px(16),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        0,
        w!("Segoe UI"),
    );
    let old = SetWindowLongPtrW(hwnd, GWLP_USERDATA, font.0 as isize);
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        px(540),
        px(280),
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    for (id, x, y, width, height) in [
        (10, 16, 14, 508, 190),
        (1, 16, 224, 155, 40),
        (2, 190, 224, 155, 40),
    ] {
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
            let _ = SetWindowPos(
                child,
                None,
                px(x),
                px(y),
                px(width),
                px(height),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            SendMessageW(
                child,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
    if old != 0 {
        let _ = DeleteObject(HGDIOBJ(old as *mut _));
    }
}
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
            let font = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if font != 0 {
                let _ = DeleteObject(HGDIOBJ(font as *mut _));
            }
            LRESULT(0)
        }
        WM_COMMAND if wp.0 >> 16 != BN_CLICKED as usize || !matches!(wp.0 & 0xffff, 1 | 2) => {
            LRESULT(0)
        }
        WM_COMMAND | WM_CLOSE => {
            let start = msg == WM_COMMAND && wp.0 & 0xffff == 1;
            // No outstanding SERVICE borrow when dispatching user messages.
            SERVICE.with(|cell| {
                let mut s = cell.borrow_mut();
                if let Some((pid, r)) = s.request.take() {
                    let fresh = read_request(pid).is_some_and(|now| now.call == r.call);
                    if start && fresh && !SignalPaths::default().stop_requested() {
                        s.consent
                            .allow(&r.task, INPUT_SEQ.load(Ordering::SeqCst), unix_ms());
                    } else {
                        s.deferred = Some(r.call);
                    }
                }
                close_panel(&mut s);
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
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
            assert_eq!(rect.bottom - rect.top, 420);
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
            let font = SetWindowLongPtrW(parent, GWLP_USERDATA, 0);
            DestroyWindow(parent).unwrap();
            let _ = DeleteObject(HGDIOBJ(font as *mut _));
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
pub fn allow_flow(task: &str) -> bool {
    if SignalPaths::default().stop_requested() || !WATCH_RUNNING.load(Ordering::SeqCst) {
        return false;
    }
    SERVICE.with(|cell| {
        cell.borrow_mut()
            .consent
            .allow(task, INPUT_SEQ.load(Ordering::SeqCst), unix_ms())
    });
    true
}
pub fn revoke() {
    SERVICE.with(|cell| cell.borrow_mut().consent.revoke());
}

#[cfg(test)]
mod pause_tests {
    use super::*;
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
