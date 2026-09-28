use super::*;

struct OwnedHost {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    hwnd: usize,
}
impl OwnedHost {
    fn start() -> Self {
        Self::with_choices(false)
    }
    fn with_choices(choices: bool) -> Self {
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
            if choices {
                for (id, x, label) in [(10, 20, w!("Option A")), (11, 210, w!("Option B"))] {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        w!("BUTTON"),
                        label,
                        WS_CHILD | WS_VISIBLE,
                        x,
                        40,
                        140,
                        50,
                        Some(hwnd),
                        Some(HMENU(id as *mut _)),
                        None,
                        None,
                    )
                    .unwrap();
                }
            }
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
    let binary =
        std::env::var("ACTL_PREVIEW_CLI").expect("set ACTL_PREVIEW_CLI to the binary under test");
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

mod lifecycle;
mod local_snapshot;
mod selection;
