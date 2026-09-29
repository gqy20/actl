//! Bounded window activation: request once, then observe; never synthesize Alt keys.
use actl_core::{CtlError, ErrorCode};
use std::time::{Duration, Instant};
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

fn request_activation<T>(
    mut is_foreground: impl FnMut() -> bool,
    request: impl FnOnce() -> T,
) -> Option<T> {
    (!is_foreground()).then(request)
}

fn activation_evidence(
    before: isize,
    requested: Option<bool>,
    pid: u32,
    waited_ms: u64,
) -> serde_json::Value {
    serde_json::json!({"foreground_before":before,"activation_requested":requested.is_some(),
        "activation_returned":requested,"caller_pid":pid,"waited_ms":waited_ms})
}

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
    // Request native foreground once, without invoking a provider's SetFocus on
    // the window root. Child input focus is still checked by the input guard.
    let foreground_before = unsafe { GetForegroundWindow() }.0 as isize;
    let requested = request_activation(
        || unsafe { GetForegroundWindow() } == root,
        || {
            // SAFETY: root was checked above; Windows can reject this request and
            // the bounded foreground observation below remains authoritative.
            unsafe { SetForegroundWindow(root).as_bool() }
        },
    );
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(crate::timing().vis_probe_ms) {
        crate::feedback::check_stop()?;
        if unsafe { GetForegroundWindow() } == root && !unsafe { IsIconic(root).as_bool() } {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
    }
    let mut error = failure("foreground_not_acquired");
    if let Some(serde_json::Value::Object(evidence)) = error.evidence.as_mut()
        && let serde_json::Value::Object(details) = activation_evidence(
            foreground_before,
            requested,
            std::process::id(),
            start.elapsed().as_millis() as u64,
        )
    {
        evidence.extend(details);
    }
    Err(error)
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

#[cfg(test)]
mod tests {
    use super::request_activation;

    #[test]
    fn foreground_window_does_not_receive_another_activation_request() {
        let mut requests = 0;
        request_activation(|| true, || requests += 1);
        assert_eq!(requests, 0);
    }

    #[test]
    fn background_window_gets_only_one_request() {
        let mut requests = 0;
        request_activation(|| false, || requests += 1);
        assert_eq!(requests, 1);
    }

    #[test]
    fn rejected_activation_is_preserved_for_diagnostics() {
        assert_eq!(request_activation(|| false, || false), Some(false));
        assert_eq!(
            request_activation(|| true, || panic!("already foreground")),
            None
        );
    }

    #[test]
    fn foreground_diagnostics_contract() {
        let actual = super::activation_evidence(985468, Some(false), 42, 1000);
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/foreground_activation.json"))
                .unwrap();
        assert_eq!(actual, golden);
        assert!(super::activation_evidence(1, None, 42, 1000)["activation_returned"].is_null());
    }
}
