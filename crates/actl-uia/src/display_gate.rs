//! Display startup and action acknowledgement. No input can silently bypass this gate.
use actl_core::{
    CtlError, ErrorCode,
    state::{SignalPaths, unix_ms},
};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};
use windows::{
    Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId, IsWindowVisible},
    core::w,
};
thread_local! { static ACTIVE_EPOCH: RefCell<Option<String>> = const { RefCell::new(None) }; }

pub fn prepare_action(
    kind: &str,
    target: &str,
) -> Result<actl_core::signal_session::ActionGuard, CtlError> {
    crate::feedback::prepare(
        kind,
        actl_core::activity::InputEffects {
            focus: true,
            ..Default::default()
        },
        actl_core::activity::ActionTarget {
            window: Some(target.into()),
            ..Default::default()
        },
    )
}

fn unavailable(message: &str) -> CtlError {
    CtlError::new(ErrorCode::NotActionable, message)
}
fn alive(l: &actl_core::display::DisplayLease) -> bool {
    let Ok(hwnd) = (unsafe { FindWindowW(w!("actl_signal_window"), None) }) else {
        return false;
    };
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid == l.pid
        && (l.state == "capture"
            || (unsafe { IsWindowVisible(hwnd).as_bool() }
                && crate::display_support::surface_exposed(hwnd)))
}
/// Read-only diagnostics: lease freshness alone does not prove a visible renderer.
pub fn diagnostics() -> serde_json::Value {
    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::*,
    };
    let paths = SignalPaths::default();
    let lease = paths.display_lease();
    let hwnd = unsafe { FindWindowW(w!("actl_signal_window"), None) }.ok();
    let responsive = hwnd.is_some_and(|h| unsafe {
        SendMessageTimeoutW(
            h,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            150,
            None,
        )
        .0 != 0
    });
    let reason = match (&lease, hwnd) {
        (_, None) => "companion_missing",
        (None, _) => "lease_missing",
        (Some(l), _)
            if l.protocol != actl_core::display::DISPLAY_PROTOCOL
                || l.build != env!("CARGO_PKG_VERSION") =>
        {
            "version_mismatch"
        }
        (_, _) if !responsive => "companion_unresponsive",
        (Some(l), _) if !l.compatible(unix_ms()) => "lease_stale",
        (_, Some(h)) if !unsafe { IsWindowVisible(h).as_bool() } => "hidden",
        (Some(l), _) if l.state == "capture" => "capture_in_progress",
        (_, Some(h)) if !crate::display_support::surface_exposed(h) => "covered_or_offscreen",
        (Some(l), _) if !l.ready(unix_ms()) => "frame_not_ready",
        (Some(l), _) if !alive(l) => "window_identity_mismatch",
        _ => "ready",
    };
    let popup = std::fs::read(paths.dir.join("popup.json"))
        .ok()
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok());
    serde_json::json!({"popup":popup, "ready":reason == "ready", "reason":reason, "lease":lease,
        "hwnd":hwnd.map(|h|h.0 as isize), "responsive":responsive, "stop_requested":paths.stop_requested()})
}
pub fn ensure() -> Result<actl_core::display::DisplayLease, CtlError> {
    let paths = SignalPaths::default();
    if let Some(l) = paths
        .display_lease()
        .filter(|l| l.ready(unix_ms()) && alive(l))
    {
        return Ok(l);
    }
    // Restore only a compatible companion. Replacement is explicit and preserves stop state.
    if let Some(l) = paths
        .display_lease()
        .filter(|l| l.compatible(unix_ms()) && l.state != "capture")
        && let Ok(hwnd) = unsafe { FindWindowW(w!("actl_signal_window"), None) }
    {
        use windows::Win32::{
            Foundation::{LPARAM, WPARAM},
            UI::WindowsAndMessaging::*,
        };
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == l.pid {
            unsafe {
                SendMessageTimeoutW(
                    hwnd,
                    actl_core::handoff::RECOVER_MESSAGE,
                    WPARAM(0),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    500,
                    None,
                );
            }
        }
    }
    if unsafe { FindWindowW(w!("actl_signal_window"), None) }.is_err() {
        let exe = std::env::current_exe()
            .map_err(crate::internal)?
            .with_file_name("actl-signal.exe");
        if !exe.is_file() {
            return Err(unavailable(
                "matching actl-signal.exe is missing beside actl.exe",
            ));
        }
        use std::os::windows::ffi::OsStrExt;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::*;
        use windows::core::PCWSTR;
        let path: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut child = PROCESS_INFORMATION::default();
        // Never inherit the caller's pipe handles: a resident display must not keep
        // PowerShell's output pipeline open after actl exits.
        unsafe {
            CreateProcessW(
                PCWSTR(path.as_ptr()),
                None,
                None,
                None,
                false,
                DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
                None,
                None,
                &startup,
                &mut child,
            )
            .map_err(crate::internal)?;
            let _ = CloseHandle(child.hThread);
            let _ = CloseHandle(child.hProcess);
        }
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(4) {
        if let Some(l) = paths
            .display_lease()
            .filter(|l| l.ready(unix_ms()) && alive(l))
        {
            return Ok(l);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(CtlError::with_evidence(
        ErrorCode::NotActionable,
        "display not ready; inspect evidence.reason; use a matching companion for version_mismatch; stop state was preserved",
        diagnostics(),
    ))
}
pub(crate) fn wait_presented(
    action: &actl_core::signal_session::ActionGuard,
    epoch: &str,
) -> Result<(), CtlError> {
    let Some((call, id, since)) = action.identity() else {
        return Err(unavailable(
            "action has no signal session; cannot verify its preview",
        ));
    };
    let paths = SignalPaths::default();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        crate::feedback::check_stop()?;
        if paths
            .display_lease()
            .is_some_and(|l| l.ready(unix_ms()) && l.epoch == epoch && alive(&l))
            && paths.acknowledged(&call, id, epoch, since)
        {
            ACTIVE_EPOCH.with(|e| *e.borrow_mut() = Some(epoch.to_owned()));
            // Let the acknowledged preview remain readable before delivery.
            for _ in 0..12 {
                std::thread::sleep(Duration::from_millis(25));
                crate::feedback::check_stop()?;
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err(unavailable(
        "display did not acknowledge this action preview; action was not delivered",
    ))
}
pub(crate) fn check_active() -> Result<(), CtlError> {
    ACTIVE_EPOCH.with(|e| {
        if let Some(epoch) = e.borrow().as_ref() {
            let paths = SignalPaths::default();
            // Brief acknowledged screenshot hiding is not a broken display.
            let valid = paths.display_lease().is_some_and(|l| {
                l.compatible(unix_ms())
                    && alive(&l)
                    && &l.epoch == epoch
                    && matches!(l.state.as_str(), "ready" | "capture")
            });
            if !valid {
                return Err(unavailable(
                    "display became unavailable; subsequent input stopped",
                ));
            }
        }
        Ok(())
    })
}
