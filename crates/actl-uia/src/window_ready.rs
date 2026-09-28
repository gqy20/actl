//! Bounded window activation: request once, then observe; never synthesize Alt keys.
use actl_core::{CtlError, ErrorCode};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

pub(crate) fn activate(win: &uiautomation::UIElement) -> Result<(), CtlError> {
    let hwnd = crate::window::native_hwnd(win).ok_or_else(|| {
        CtlError::new(
            ErrorCode::NotActionable,
            "cannot verify foreground: target has no native window handle",
        )
    })?;
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let failure = |reason: &str| {
        CtlError::with_evidence(
            ErrorCode::NotActionable,
            "window is not ready for input; inspect native window state and visible dialogs before retrying",
            serde_json::json!({"stage":"window_prepare", "reason":reason, "hwnd":root.0 as isize,
            "foreground_hwnd":unsafe { GetForegroundWindow() }.0 as isize,
            "minimized":unsafe { IsIconic(root).as_bool() }, "visible":unsafe { IsWindowVisible(root).as_bool() }}),
        )
    };
    crate::feedback::check_stop()?;
    if root.0.is_null() || !unsafe { IsWindow(Some(root)).as_bool() } {
        return Err(failure("window_gone"));
    }
    if !unsafe { IsWindowEnabled(root).as_bool() } {
        return Err(failure("window_disabled_or_modal"));
    }
    let responsive = unsafe {
        SendMessageTimeoutW(
            root,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            200,
            None,
        )
    }
    .0 != 0;
    if !responsive {
        return Err(failure("window_unresponsive"));
    }
    if unsafe { IsIconic(root).as_bool() } {
        if !unsafe { ShowWindowAsync(root, SW_RESTORE).as_bool() } {
            return Err(failure("restore_rejected"));
        }
    } else if !unsafe { IsWindowVisible(root).as_bool() } {
        return Err(failure("window_hidden"));
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(root, &mut rect) }.map_err(|_| failure("bounds_unavailable"))?;
    if !unsafe { IsIconic(root).as_bool() }
        && unsafe {
            windows::Win32::Graphics::Gdi::MonitorFromRect(
                &rect,
                windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONULL,
            )
        }
        .0
        .is_null()
    {
        return Err(failure("window_offscreen"));
    }
    crate::feedback::check_stop()?;
    // SetFocus may report success while Windows refuses foreground activation.
    let _ = win.set_focus();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(crate::timing().vis_probe_ms) {
        crate::feedback::check_stop()?;
        if unsafe { GetForegroundWindow() } == root && !unsafe { IsIconic(root).as_bool() } {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
    }
    Err(failure("foreground_not_acquired"))
}

/// Match explicitly supplied window title. Process launchers may hand off to an
/// existing process, so returned window PID is reported separately from spawned PID.
pub fn wait_window_ready(pattern: &str, timeout_ms: u64) -> Result<serde_json::Value, CtlError> {
    let start = Instant::now();
    loop {
        crate::feedback::check_stop()?;
        let rows: Vec<_> = crate::native_windows()?
            .into_iter()
            .filter(|w| w.title.contains(pattern) && w.visible)
            .collect();
        if rows.len() > 1 {
            return Err(CtlError::with_evidence(
                ErrorCode::Ambiguous,
                "multiple windows match the requested readiness title; select a specific HWND",
                serde_json::json!({"stage":"window_discovery","candidates":rows.iter().map(|w|serde_json::json!({"hwnd":w.hwnd,"pid":w.pid,"title":w.title})).collect::<Vec<_>>()}),
            ));
        }
        if let Some(row) = rows.first()
            && let Some(hwnd) = row.hwnd
        {
            crate::focus_window_by(crate::WindowRef::Hwnd(hwnd))?;
            return Ok(
                serde_json::json!({"hwnd":hwnd,"pid":row.pid,"title":row.title,"foreground":true,"matched_by":"title"}),
            );
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            return Err(CtlError::with_evidence(
                ErrorCode::Timeout,
                "process started but no visible matching window appeared; inspect native windows before retrying",
                serde_json::json!({"stage":"window_discovery","reason":"window_not_found","timeout_ms":timeout_ms}),
            ));
        }
        std::thread::sleep(Duration::from_millis(crate::timing().ready_poll_ms));
    }
}
