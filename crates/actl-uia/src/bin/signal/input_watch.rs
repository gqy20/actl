//! Passive raw input redundancy and message-pump health. Never injects input.
//! Only an activity counter is kept; no keys, coordinates or input payloads are stored.
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use windows::{
    Win32::{
        Foundation::*,
        System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64},
        UI::{Input::*, WindowsAndMessaging::*},
    },
    core::w,
};

static TARGET: AtomicUsize = AtomicUsize::new(0);
static HEARTBEAT: AtomicU64 = AtomicU64::new(0);
pub(super) static ACTIVITY: AtomicU64 = AtomicU64::new(0);
const TIMER: usize = 1;
const MAX_AGE_MS: u64 = 1000;

fn fresh(now: u64, last: u64) -> bool {
    last != 0 && now >= last && now - last < MAX_AGE_MS
}

fn devices(hwnd: HWND, flags: RAWINPUTDEVICE_FLAGS) -> [RAWINPUTDEVICE; 2] {
    [2, 6].map(|usage| RAWINPUTDEVICE {
        usUsagePage: 1,
        usUsage: usage,
        dwFlags: flags,
        hwndTarget: hwnd,
    })
}

fn registered(items: &[RAWINPUTDEVICE], hwnd: HWND) -> bool {
    [2, 6].into_iter().all(|usage| {
        items.iter().any(|d| {
            d.usUsagePage == 1
                && d.usUsage == usage
                && d.hwndTarget == hwnd
                && d.dwFlags.contains(RIDEV_INPUTSINK)
        })
    })
}

pub(super) fn healthy() -> bool {
    let target = TARGET.load(Ordering::SeqCst);
    if target == 0
        || !fresh(
            unsafe { GetTickCount64() },
            HEARTBEAT.load(Ordering::SeqCst),
        )
    {
        return false;
    }
    let mut items = [RAWINPUTDEVICE::default(); 16];
    let mut count = items.len() as u32;
    let read = unsafe {
        GetRegisteredRawInputDevices(
            Some(items.as_mut_ptr()),
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )
    };
    read != u32::MAX
        && read <= items.len() as u32
        && registered(&items[..read as usize], HWND(target as *mut _))
}

/// Owned by the existing dedicated input thread, not by a library consumer.
pub(super) struct RawWatch(HWND);
impl RawWatch {
    pub(super) unsafe fn start() -> windows::core::Result<Self> {
        let module = GetModuleHandleW(None)?;
        let class = w!("actl_input_watch");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module.into(),
            lpszClassName: class,
            ..Default::default()
        };
        // The class can already exist when a test restarts its owned watcher.
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from_thread());
        }
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!(""),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(module.into()),
            None,
        )?;
        let watch = Self(hwnd);
        RegisterRawInputDevices(
            &devices(hwnd, RIDEV_INPUTSINK),
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )?;
        if SetTimer(Some(hwnd), TIMER, 100, None) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        HEARTBEAT.store(GetTickCount64(), Ordering::SeqCst);
        TARGET.store(hwnd.0 as usize, Ordering::SeqCst);
        Ok(watch)
    }
}
impl Drop for RawWatch {
    fn drop(&mut self) {
        TARGET.store(0, Ordering::SeqCst);
        HEARTBEAT.store(0, Ordering::SeqCst);
        unsafe {
            let _ = KillTimer(Some(self.0), TIMER);
            let _ = RegisterRawInputDevices(
                &devices(HWND::default(), RIDEV_REMOVE),
                std::mem::size_of::<RAWINPUTDEVICE>() as u32,
            );
            let _ = DestroyWindow(self.0);
        }
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            // Any raw activity conservatively revokes consent. DefWindowProc also
            // performs the required foreground WM_INPUT resource cleanup.
            ACTIVITY.fetch_add(1, Ordering::SeqCst);
        }
        WM_TIMER if wp.0 == TIMER => {
            HEARTBEAT.store(GetTickCount64(), Ordering::SeqCst);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_or_unknown_pump_is_not_healthy() {
        assert!(!fresh(100, 0));
        assert!(!fresh(100, 101));
        assert!(!fresh(1100, 100));
        assert!(fresh(1099, 100));
    }
    #[test]
    fn both_devices_must_target_our_window() {
        let mut first = 0u8;
        let mut second = 0u8;
        let hwnd = HWND((&mut first as *mut u8).cast());
        let mut items = devices(hwnd, RIDEV_INPUTSINK);
        assert!(registered(&items, hwnd));
        assert!(!registered(&items[..1], hwnd));
        items[1].hwndTarget = HWND((&mut second as *mut u8).cast());
        assert!(!registered(&items, hwnd));
        assert!(!registered(&devices(hwnd, RIDEV_REMOVE), hwnd));
    }
    #[test]
    #[ignore = "live passive registration lifecycle; does not inject input or grant consent"]
    fn raw_registration_lifecycle() {
        unsafe {
            let watch = RawWatch::start().unwrap();
            assert!(healthy());
            drop(watch);
            assert!(!healthy());
        }
    }
}
