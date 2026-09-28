//! Client of the companion's human handoff gate. No unattended bypass switch.
use actl_core::{
    CtlError,
    handoff::*,
    state::{SignalPaths, unix_ms},
};
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};
thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }

fn failure(reason: &str) -> CtlError {
    actl_core::signal_session::handoff_event(reason);
    actl_core::handoff::error(reason)
}

fn query(message: u32) -> Result<usize, CtlError> {
    let hwnd = unsafe { FindWindowW(w!("actl_signal_window"), None) }
        .map_err(|_| failure("companion_missing"))?;
    let mut result = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            message,
            WPARAM(std::process::id() as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            500,
            Some(&mut result),
        )
    };
    if sent.0 == 0 || result == 0 {
        return Err(failure("companion_unresponsive"));
    }
    Ok(result)
}
pub(crate) fn check() -> Result<(), CtlError> {
    if ACTIVE.with(Cell::get) {
        match query(CHECK_MESSAGE)? {
            ALLOWED => {}
            USER_PAUSED => return Err(failure("user_paused")),
            MONITOR_UNAVAILABLE => return Err(failure("input_monitor_unavailable")),
            _ => return Err(failure("human_takeover_or_grant_expired")),
        }
    }
    Ok(())
}
pub(crate) fn prepare(target: &str) -> Result<(), CtlError> {
    if ACTIVE.with(Cell::get) {
        return check();
    }
    request(target).map_err(|mut error| {
        if let Some(evidence) = error.evidence.as_mut()
            && evidence["stage"] == "handoff"
        {
            evidence["action_started"] = false.into();
        }
        error
    })
}
fn request(target: &str) -> Result<(), CtlError> {
    let (call, task) =
        actl_core::signal_session::handoff_identity().ok_or_else(|| failure("missing_session"))?;
    let paths = SignalPaths::default();
    let file = format!("handoff-{}.json", std::process::id());
    struct Remove(std::path::PathBuf);
    impl Drop for Remove {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _remove = Remove(paths.dir.join(&file));
    let mut request = Request {
        call,
        task,
        target: target.chars().take(180).collect(),
        ts_ms: unix_ms(),
    };
    actl_core::signal_session::handoff_event("handoff_pending");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(25) {
        crate::feedback::check_stop()?;
        request.ts_ms = unix_ms();
        paths
            .write_display(&file, &request)
            .map_err(crate::internal)?;
        match query(REQUEST_MESSAGE)? {
            ALLOWED => {
                actl_core::signal_session::handoff_event("allowed");
                ACTIVE.with(|a| a.set(true));
                return Ok(());
            }
            PANEL_UNAVAILABLE => return Err(failure("handoff_panel_unavailable")),
            MONITOR_UNAVAILABLE => return Err(failure("input_monitor_unavailable")),
            USER_PAUSED => return Err(failure("user_paused")),
            DEFERRED => return Err(failure("user_deferred")),
            PENDING => std::thread::sleep(Duration::from_millis(crate::timing().handoff_poll_ms)),
            _ => return Err(failure("request_rejected")),
        }
    }
    Err(failure("handoff_pending"))
}
