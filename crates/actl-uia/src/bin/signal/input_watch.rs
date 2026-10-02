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

#[cfg(test)]
fn raw_external(input: &RAWINPUT, bytes: u32) -> bool {
    raw_reason(input, bytes) != "own_injection"
}

fn raw_reason(input: &RAWINPUT, bytes: u32) -> &'static str {
    let payload = match input.header.dwType {
        kind if kind == RIM_TYPEKEYBOARD.0 => std::mem::size_of::<RAWKEYBOARD>(),
        kind if kind == RIM_TYPEMOUSE.0 => std::mem::size_of::<RAWMOUSE>(),
        _ => return "unknown_kind",
    };
    let required = (std::mem::size_of::<RAWINPUTHEADER>() + payload) as u32;
    if bytes < required || input.header.dwSize != bytes {
        return "invalid_length";
    }
    // SAFETY: the validated type selects the initialized, complete union member.
    // Only device-less events bearing our injector tag are excluded. Unknown,
    // malformed, other-tool and device-originated input all still revoke consent.
    let tag = unsafe {
        if input.header.dwType == RIM_TYPEKEYBOARD.0 {
            input.data.keyboard.ExtraInformation
        } else {
            input.data.mouse.ulExtraInformation
        }
    };
    if !input.header.hDevice.0.is_null() {
        if tag == actl_core::handoff::INPUT_TAG as u32 {
            "tagged_device_input"
        } else {
            "device_input"
        }
    } else if tag == actl_core::handoff::INPUT_TAG as u32 {
        "own_injection"
    } else {
        "unrecognized_tag"
    }
}

unsafe fn external_message(lp: LPARAM) -> bool {
    let mut input = RAWINPUT::default();
    let mut bytes = std::mem::size_of::<RAWINPUT>() as u32;
    let read = GetRawInputData(
        HRAWINPUT(lp.0 as *mut _),
        RID_INPUT,
        Some((&mut input as *mut RAWINPUT).cast()),
        &mut bytes,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );
    let capacity = std::mem::size_of::<RAWINPUT>() as u32;
    let reason = read_reason(&input, read, capacity);
    super::input_diagnostics::record(
        reason,
        read,
        capacity,
        input.header.dwSize,
        input.header.dwType,
    );
    reason != "own_injection"
}

#[cfg(test)]
fn external_read(input: &RAWINPUT, read: u32, capacity: u32) -> bool {
    read_reason(input, read, capacity) != "own_injection"
}

fn read_reason(input: &RAWINPUT, read: u32, capacity: u32) -> &'static str {
    // The return value is bytes copied; pcbSize can remain the buffer capacity.
    // A keyboard payload is smaller than the RAWINPUT union's mouse payload.
    if read == u32::MAX {
        "read_failed"
    } else if read > capacity {
        "buffer_exceeded"
    } else {
        raw_reason(input, read)
    }
}

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
    health_reason() == "healthy"
}

pub(super) fn diagnostics() -> serde_json::Value {
    let now = unsafe { GetTickCount64() };
    let last = HEARTBEAT.load(Ordering::SeqCst);
    serde_json::json!({"reason":health_reason(),"heartbeat_age_ms":now.checked_sub(last),
        "raw":super::input_diagnostics::snapshot()})
}

pub(super) fn health_reason() -> &'static str {
    let target = TARGET.load(Ordering::SeqCst);
    if target == 0 {
        return "watcher_missing";
    }
    if !fresh(
        unsafe { GetTickCount64() },
        HEARTBEAT.load(Ordering::SeqCst),
    ) {
        return "heartbeat_stale";
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
    if read == u32::MAX || read > items.len() as u32 {
        "registration_read_failed"
    } else if !registered(&items[..read as usize], HWND(target as *mut _)) {
        "registration_missing"
    } else {
        "healthy"
    }
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
            if external_message(lp) {
                ACTIVITY.fetch_add(1, Ordering::SeqCst);
            }
            // DefWindowProc performs the foreground WM_INPUT resource cleanup.
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
    fn keyboard_read_may_be_smaller_than_union_buffer() {
        let mut input = RAWINPUT::default();
        input.header.dwType = RIM_TYPEKEYBOARD.0;
        let read =
            (std::mem::size_of::<RAWINPUTHEADER>() + std::mem::size_of::<RAWKEYBOARD>()) as u32;
        input.header.dwSize = read;
        input.data.keyboard.ExtraInformation = actl_core::handoff::INPUT_TAG as u32;
        let capacity = std::mem::size_of::<RAWINPUT>() as u32;
        assert!(!external_read(&input, read, capacity));
        assert!(external_read(&input, u32::MAX, capacity));
        assert!(external_read(&input, capacity + 1, capacity));
        assert_eq!(read_reason(&input, u32::MAX, capacity), "read_failed");
        assert_eq!(
            read_reason(&input, capacity + 1, capacity),
            "buffer_exceeded"
        );
        assert_eq!(read_reason(&input, read - 1, capacity), "invalid_length");
        input.data.keyboard.ExtraInformation = 0;
        assert_eq!(read_reason(&input, read, capacity), "unrecognized_tag");
        input.header.dwType = u32::MAX;
        assert_eq!(read_reason(&input, read, capacity), "unknown_kind");
    }
    #[test]
    fn raw_own_injection_is_not_human_takeover() {
        for kind in [RIM_TYPEKEYBOARD.0, RIM_TYPEMOUSE.0] {
            let mut input = RAWINPUT::default();
            input.header.dwType = kind;
            let bytes = (std::mem::size_of::<RAWINPUTHEADER>()
                + if kind == RIM_TYPEKEYBOARD.0 {
                    std::mem::size_of::<RAWKEYBOARD>()
                } else {
                    std::mem::size_of::<RAWMOUSE>()
                }) as u32;
            input.header.dwSize = bytes;
            assert!(raw_external(&input, bytes), "unknown input must yield");
            if kind == RIM_TYPEKEYBOARD.0 {
                input.data.keyboard.ExtraInformation = actl_core::handoff::INPUT_TAG as u32;
            } else {
                input.data.mouse.ulExtraInformation = actl_core::handoff::INPUT_TAG as u32;
            }
            assert!(
                !raw_external(&input, bytes),
                "own tagged injection must not revoke"
            );
            assert!(
                raw_external(&input, bytes - 1),
                "truncated input must yield"
            );
            input.header.hDevice = HANDLE(std::ptr::dangling_mut::<u8>().cast());
            assert!(
                raw_external(&input, bytes),
                "device input must yield even with matching tag"
            );
        }
        assert!(raw_external(&RAWINPUT::default(), 0));
    }
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
