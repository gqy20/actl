//! Native display isolation and lock observation (the only unsafe platform layer).
use actl_core::{CtlError, ErrorCode};
use windows::Win32::Foundation::{CloseHandle, HWND, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    OpenMutexW, ReleaseMutex, SYNCHRONIZATION_ACCESS_RIGHTS, WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

pub const CAPTURE_HIDE: u32 = WM_APP + 41;
pub const CAPTURE_RESTORE: u32 = WM_APP + 42;

/// Visible style alone remains true behind LockApp or another covering surface.
pub fn surface_exposed(hwnd: HWND) -> bool {
    unsafe {
        let mut rect = windows::Win32::Foundation::RECT::default();
        if !IsWindowVisible(hwnd).as_bool()
            || IsIconic(hwnd).as_bool()
            || GetWindowRect(hwnd, &mut rect).is_err()
        {
            return false;
        }
        let point = windows::Win32::Foundation::POINT {
            x: rect.left + (rect.right - rect.left) / 2,
            y: rect.top + (rect.bottom - rect.top) / 2,
        };
        WindowFromPoint(point) == hwnd
    }
}

pub(crate) fn reject_display(raw: isize) -> Result<(), CtlError> {
    if is_display_window(HWND(raw as *mut _)) {
        Err(CtlError::new(
            ErrorCode::NotActionable,
            "actl display windows are excluded from automation targets",
        ))
    } else {
        Ok(())
    }
}

/// None means permission/observation failure; never pretend this means input released.
pub fn input_busy() -> Option<bool> {
    unsafe {
        match OpenMutexW(
            SYNCHRONIZATION_ACCESS_RIGHTS(0x0010_0001),
            false,
            w!("actl-input-lock"),
        ) {
            Ok(h) => {
                let busy = observe_mutex(h);
                let _ = CloseHandle(h);
                busy
            }
            Err(e) if e.code() == windows::core::HRESULT::from_win32(2) => Some(false),
            Err(_) => None,
        }
    }
}

pub fn observe_mutex(h: windows::Win32::Foundation::HANDLE) -> Option<bool> {
    unsafe {
        match WaitForSingleObject(h, 0) {
            WAIT_OBJECT_0 | WAIT_ABANDONED => {
                let _ = ReleaseMutex(h);
                Some(false)
            }
            WAIT_TIMEOUT => Some(true),
            _ => None,
        }
    }
}

pub fn is_display_window(hwnd: HWND) -> bool {
    let mut name = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut name) };
    let class = String::from_utf16_lossy(&name[..len.max(0) as usize]);
    if matches!(
        class.as_str(),
        "actl_signal_window"
            | "actl_target_preview"
            | "actl_signal_marker"
            | "actl_signal_controls"
            | "actl_signal_perimeter"
    ) {
        return true;
    }
    // User-opened detail dialogs share the display process and must not become targets.
    if let Ok(display) = unsafe { FindWindowW(w!("actl_signal_window"), None) } {
        let (mut owner, mut candidate) = (0, 0);
        unsafe {
            GetWindowThreadProcessId(display, Some(&mut owner));
            GetWindowThreadProcessId(hwnd, Some(&mut candidate));
        }
        return owner != 0 && owner == candidate;
    }
    false
}

/// Synchronous hide acknowledgement + compositor flush, independent of capture API.
pub(crate) struct CaptureGuard(Option<HWND>);
impl CaptureGuard {
    pub(crate) fn enter() -> Result<Self, CtlError> {
        let Ok(hwnd) = (unsafe { FindWindowW(w!("actl_signal_window"), None) }) else {
            return Ok(Self(None));
        };
        let mut ack = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                hwnd,
                CAPTURE_HIDE,
                windows::Win32::Foundation::WPARAM(std::process::id() as usize),
                Default::default(),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                1000,
                Some(&mut ack),
            )
        };
        if sent.0 == 0 || ack != 1 {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "display did not acknowledge screenshot exclusion; retry after closing actl-signal",
            ));
        }
        let guard = Self(Some(hwnd));
        unsafe { windows::Win32::Graphics::Dwm::DwmFlush() }
            .map_err(|e| CtlError::internal(format!("display exclusion flush: {e}")))?;
        Ok(guard)
    }
}
impl Drop for CaptureGuard {
    fn drop(&mut self) {
        if let Some(hwnd) = self.0 {
            unsafe {
                let _ = PostMessageW(
                    Some(hwnd),
                    CAPTURE_RESTORE,
                    windows::Win32::Foundation::WPARAM(std::process::id() as usize),
                    Default::default(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observing_free_mutex_releases_it() {
        use windows::Win32::System::Threading::CreateMutexExW;
        unsafe {
            let h = CreateMutexExW(None, None, 0, 0x0010_0001).unwrap();
            assert_eq!(observe_mutex(h), Some(false));
            let raw = h.0 as usize;
            assert!(
                std::thread::spawn(move || {
                    observe_mutex(windows::Win32::Foundation::HANDLE(raw as *mut _)) == Some(false)
                })
                .join()
                .unwrap()
            );
            let _ = CloseHandle(h);
        }
    }
}
