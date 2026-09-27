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
pub fn ensure() -> Result<actl_core::display::DisplayLease, CtlError> {
    let paths = SignalPaths::default();
    if let Some(l) = paths
        .display_lease()
        .filter(|l| l.ready(unix_ms()) && alive(l))
    {
        return Ok(l);
    }
    // A hidden or incompatible companion is a user-visible condition, not permission to replace it.
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
    Err(unavailable(
        "display not ready: show the status strip and use a matching companion; stop state was preserved",
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
