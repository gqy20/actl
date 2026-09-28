//! Win32 resources owned only by a synchronous preview invocation.
//! Unsafe: window creation/message dispatch and GDI painting; no input injection.
use actl_core::{CtlError, ErrorCode};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR, w},
};

pub(crate) struct Window {
    frame: HWND,
    notice: HWND,
}

impl Window {
    pub fn check_target(&self, owner: HWND, bounds: [i32; 4]) -> Result<(), CtlError> {
        unsafe {
            if !IsWindow(Some(owner)).as_bool()
                || IsIconic(owner).as_bool()
                || !IsWindowVisible(owner).as_bool()
            {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target window is not visible",
                ));
            }
            let point = POINT {
                x: (i64::from(bounds[0]) + (i64::from(bounds[2]) - i64::from(bounds[0])) / 2)
                    as i32,
                y: (i64::from(bounds[1]) + (i64::from(bounds[3]) - i64::from(bounds[1])) / 2)
                    as i32,
            };
            let hit = GetAncestor(WindowFromPoint(point), GA_ROOT);
            if hit != owner && (self.notice.is_invalid() || hit != self.notice) {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target center is covered or unavailable",
                ));
            }
            Ok(())
        }
    }

    pub fn target_owner(element: &uiautomation::UIElement) -> Result<HWND, CtlError> {
        let auto = uiautomation::UIAutomation::new().map_err(crate::internal)?;
        let walker = auto.get_raw_view_walker().map_err(crate::internal)?;
        let mut node = element.clone();
        for _ in 0..=crate::MAX_DEPTH {
            // Failures must not look like an element without a native handle.
            let handle = node
                .get_native_window_handle()
                .map_err(|e| crate::read_channel::failure("preview.hwnd", e))?;
            if !handle.is_invalid() {
                // UIAutomation Handle and HANDLE are native pointer-sized handles.
                let native: HANDLE = unsafe { std::mem::transmute(handle) };
                let hwnd = HWND(native.0);
                crate::display_support::reject_display(hwnd.0 as isize)?;
                return Ok(unsafe { GetAncestor(hwnd, GA_ROOT) });
            }
            node = walker
                .get_parent(&node)
                .map_err(|e| crate::read_channel::failure("preview.parent", e))?;
        }
        Err(CtlError::new(
            ErrorCode::NotActionable,
            "preview cannot identify target window",
        ))
    }

    pub fn check_initial_target(owner: HWND, bounds: [i32; 4]) -> Result<(), CtlError> {
        let empty = Self {
            frame: HWND::default(),
            notice: HWND::default(),
        };
        empty.check_target(owner, bounds)
    }

    pub fn open(bounds: [i32; 4], caption: &str, message: &str) -> Result<Self, CtlError> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(crate::internal)?;
            let class = w!("actl_target_preview");
            // Class may already exist when preview is used more than once in batch.
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hbrBackground: GetSysColorBrush(COLOR_WINDOW),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let mut window = Self {
                frame: HWND::default(),
                notice: HWND::default(),
            };
            let [l, t, r, b] = bounds;
            window.frame = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                class,
                w!(""),
                WS_POPUP,
                l,
                t,
                r - l,
                b - t,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            SetLayeredWindowAttributes(window.frame, COLORREF(0), 255, LWA_COLORKEY)
                .map_err(crate::internal)?;
            let monitor = MonitorFromRect(
                &RECT {
                    left: l,
                    top: t,
                    right: r,
                    bottom: b,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview monitor is unavailable",
                ));
            }
            let work = info.rcWork;
            if r <= work.left || l >= work.right || b <= work.top || t >= work.bottom {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target is outside the visible work area",
                ));
            }
            let (mut dx, mut dy) = (96, 96);
            let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(
                monitor,
                windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
                &mut dx,
                &mut dy,
            );
            let scale = |v: i32| ((i64::from(v) * i64::from(dx)) / 96).clamp(1, 16000) as i32;
            let width = scale(600).min(work.right - work.left);
            let height = scale(120).min(work.bottom - work.top);
            let x = l.clamp(work.left, work.right - width);
            let y = if b.saturating_add(height + 8) <= work.bottom {
                b + 8
            } else {
                t.saturating_sub(height).saturating_sub(8).max(work.top)
            };
            let title: Vec<u16> = caption.encode_utf16().chain(Some(0)).collect();
            window.notice = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class,
                PCWSTR(title.as_ptr()),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            // A native STATIC child wraps the action description; it accepts no input.
            let text: Vec<u16> =
                format!("{message}\n仅预览，不会执行操作。点击右上角关闭，或等待自动结束。")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                scale(12),
                scale(8),
                (width - scale(24)).max(1),
                (height - scale(38)).max(1),
                Some(window.notice),
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            let _ = ShowWindow(window.frame, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(window.notice, SW_SHOWNOACTIVATE);
            if !UpdateWindow(window.frame).as_bool() || !UpdateWindow(window.notice).as_bool() {
                return Err(CtlError::internal("preview painting failed"));
            }
            windows::Win32::Graphics::Dwm::DwmFlush().map_err(crate::internal)?;
            if !IsWindowVisible(window.frame).as_bool() || !IsWindowVisible(window.notice).as_bool()
            {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview windows are not visible",
                ));
            }
            Ok(window)
        }
    }

    pub fn pump(&self) -> bool {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            IsWindow(Some(self.frame)).as_bool() && IsWindow(Some(self.notice)).as_bool()
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            for hwnd in [self.notice, self.frame] {
                if IsWindow(Some(hwnd)).as_bool() {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_PAINT if GetWindowTextLengthW(hwnd) == 0 => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let pen = CreatePen(PS_SOLID, 3, COLORREF(0x0041_B3E3));
            if !pen.is_invalid() {
                let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
                let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                let _ = Rectangle(dc, 1, 1, rect.right - 1, rect.bottom - 1);
                SelectObject(dc, old_pen);
                SelectObject(dc, old_brush);
                let _ = DeleteObject(HGDIOBJ(pen.0));
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OwnedHost {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: Option<std::thread::JoinHandle<()>>,
        hwnd: usize,
    }
    impl OwnedHost {
        fn start() -> Self {
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let signal = stop.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let thread = std::thread::spawn(move || unsafe {
                let _dpi = crate::capture::Pmv2Guard::enter();
                let hwnd = CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                    w!("STATIC"),
                    w!("ACTL isolated preview target"),
                    WS_POPUP | WS_CAPTION,
                    120,
                    180,
                    400,
                    180,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                tx.send(hwnd.0 as usize).unwrap();
                while !signal.load(std::sync::atomic::Ordering::Relaxed) {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let _ = DestroyWindow(hwnd);
            });
            Self {
                stop,
                thread: Some(thread),
                hwnd: rx.recv().unwrap(),
            }
        }
    }
    impl Drop for OwnedHost {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    #[test]
    #[ignore = "interactive desktop; ACTL_PREVIEW_CLI must point to the built CLI"]
    fn release_cli_previews_owned_target_without_activation() {
        assert!(
            !actl_core::state::SignalPaths::default().stop_requested(),
            "existing stop: preserve and report blocked"
        );
        let binary = std::env::var("ACTL_PREVIEW_CLI")
            .expect("set ACTL_PREVIEW_CLI to the binary under test");
        let foreground = unsafe { GetForegroundWindow() };
        let host = OwnedHost::start();
        let out = std::process::Command::new(binary)
            .args([
                "preview",
                r#"uia:{"match":{"name":"ACTL isolated preview target"}}"#,
                "--app",
                &format!("hwnd:{}", host.hwnd),
                "--message",
                "Preview only test",
                "--duration",
                "500",
            ])
            .output()
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(out.status.success(), "{value}");
        assert_eq!(value["command"], "preview");
        assert_eq!(value["data"]["executed"], false);
        assert_eq!(value["data"]["exit_reason"], "expired");
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        assert!(unsafe { FindWindowW(w!("actl_target_preview"), None) }.is_err());
        let hwnd = HWND(host.hwnd as *mut _);
        drop(host);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    }

    #[test]
    #[ignore = "interactive desktop; owned native target, no injected input; respects existing stop"]
    fn preview_resolves_owned_target_and_expires_without_activation() {
        assert!(
            !actl_core::state::SignalPaths::default().stop_requested(),
            "existing stop: preserve and report blocked"
        );
        let foreground = unsafe { GetForegroundWindow() };
        let host = OwnedHost::start();
        let target =
            actl_core::parse_target(r#"uia:{"match":{"name":"ACTL isolated preview target"}}"#)
                .unwrap();
        let result = crate::preview::show(
            &format!("hwnd:{}", host.hwnd),
            &target,
            "Preview only test",
            500,
        )
        .unwrap();
        assert_eq!(result["previewed"], true);
        assert_eq!(result["executed"], false);
        assert_eq!(result["exit_reason"], "expired");
        assert_eq!(result["resolved_by"], "uia-composite");
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        assert!(unsafe { FindWindowW(w!("actl_target_preview"), None) }.is_err());
        let hwnd = HWND(host.hwnd as *mut _);
        drop(host);
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    }
    #[test]
    #[ignore = "requires an interactive desktop; creates and closes only owned preview windows"]
    fn native_preview_preserves_foreground_and_closes_owned_windows() {
        let _dpi = crate::capture::Pmv2Guard::enter();
        let foreground = unsafe { GetForegroundWindow() };
        let window = Window::open(
            [100, 200, 420, 340],
            "actl preview lifecycle test",
            "Preview test only",
        )
        .unwrap();
        assert!(window.pump());
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
        if let Ok(path) = std::env::var("ACTL_PREVIEW_CAPTURE") {
            let mut rect = RECT::default();
            unsafe { GetWindowRect(window.notice, &mut rect).unwrap() };
            crate::screenshot_rect(
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                Some(&path),
            )
            .unwrap();
        }
        let (frame, notice) = (window.frame, window.notice);
        unsafe { PostMessageW(Some(notice), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap() };
        assert!(!window.pump());
        drop(window);
        assert!(!unsafe { IsWindow(Some(frame)) }.as_bool());
        assert!(!unsafe { IsWindow(Some(notice)) }.as_bool());
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
}
