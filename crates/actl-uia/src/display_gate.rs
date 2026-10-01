//! Display startup and action acknowledgement. No input can silently bypass this gate.
use actl_core::timing;
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

/// Explicit state repair only; incompatible old displays must be upgraded first.
pub fn recover_execution() -> Result<serde_json::Value, CtlError> {
    let paths = SignalPaths::default();
    if unsafe { FindWindowW(w!("actl_signal_window"), None) }.is_ok()
        && paths
            .display_lease()
            .is_none_or(|l| l.protocol != actl_core::display::DISPLAY_PROTOCOL)
    {
        return Err(unavailable(
            "close or upgrade the incompatible companion before execution-recover",
        ));
    }
    paths.recover_execution()
}

pub fn prepare_action(kind: &str, target: &str) -> Result<crate::feedback::ActionGuard, CtlError> {
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
        .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
        // A popup record from a companion that has exited is stale evidence, not state.
        .filter(|popup| {
            popup["pid"]
                .as_u64()
                .is_some_and(|pid| process_alive(pid as u32))
        });
    serde_json::json!({"popup":popup, "ready":reason == "ready", "reason":reason, "lease":lease,
        "hwnd":hwnd.map(|h|h.0 as isize), "responsive":responsive, "stop_requested":paths.stop_requested()})
}

/// True when `pid` names a running process. A refused handle query counts as gone.
fn process_alive(pid: u32) -> bool {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    if pid == 0 {
        return false;
    }
    let Ok(handle) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }) else {
        return false;
    };
    let mut times = [FILETIME::default(); 4];
    let queried = unsafe {
        GetProcessTimes(
            handle,
            &mut times[0],
            &mut times[1],
            &mut times[2],
            &mut times[3],
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    queried.is_ok() && times[1].dwLowDateTime == 0 && times[1].dwHighDateTime == 0
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
        // the caller's output pipeline open after actl exits.
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
    while start.elapsed() < Duration::from_millis(timing::DISPLAY_READY_MS) {
        if let Some(l) = paths
            .display_lease()
            .filter(|l| l.ready(unix_ms()) && alive(l))
        {
            return Ok(l);
        }
        std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
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
    while start.elapsed() < Duration::from_millis(timing::DISPLAY_ACK_MS) {
        crate::feedback::check_stop()?;
        if paths
            .display_lease()
            .is_some_and(|l| l.ready(unix_ms()) && l.epoch == epoch && alive(&l))
            && paths.acknowledged(&call, id, epoch, since)
        {
            ACTIVE_EPOCH.with(|e| *e.borrow_mut() = Some(epoch.to_owned()));
            // Let the acknowledged preview remain readable before delivery.
            for _ in 0..12 {
                std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
                crate::feedback::check_stop()?;
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
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
            let healthy = |l: &actl_core::display::DisplayLease| {
                l.compatible(unix_ms())
                    && alive(l)
                    && &l.epoch == epoch
                    && matches!(l.state.as_str(), "ready" | "capture")
            };
            // 租约新鲜度窗 1500ms 与渲染 tick(~1s)贴得很近:截图捕获/DWM
            // 打嗝会让单次读取撞上过期租约(2026-10-01 金任务2收尾 click 实证,
            // 伴侣未死、下一 tick 自愈)。有界沉降重读——真死的伴侣在窗口末端
            // 仍读不到健康租约,照样如实报错。
            let start = std::time::Instant::now();
            loop {
                let current = paths.display_lease();
                if current.as_ref().is_some_and(healthy) {
                    return Ok(());
                }
                if start.elapsed() >= std::time::Duration::from_millis(1500) {
                    return Err(unavailable(
                        "display became unavailable; subsequent input stopped",
                    ));
                }
                std::thread::sleep(Duration::from_millis(crate::timing().fast_poll_ms));
            }
        }
        Ok(())
    })
}
