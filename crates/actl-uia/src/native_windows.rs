//! Native top-level inventory, independent of UIA provider readiness.
use actl_core::CtlError;
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, RECT},
        UI::WindowsAndMessaging::*,
    },
    core::BOOL,
};

use crate::window::{WindowEntry, window_title_of};

// SAFETY: EnumWindows calls synchronously on this thread. LPARAM points to the
// caller's live Vec for the whole call; no reference or HWND ownership escapes.
unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(param.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    BOOL(1)
}

/// Enumerate all native top-level windows, including hidden/untitled windows.
/// A failed enumeration is an error, never a successful empty inventory.
pub fn native_windows() -> Result<Vec<WindowEntry>, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let mut handles = Vec::<HWND>::new();
    unsafe {
        EnumWindows(
            Some(collect),
            LPARAM((&mut handles as *mut Vec<HWND>) as isize),
        )
    }
    .map_err(crate::internal)?;
    let foreground = unsafe { GetForegroundWindow() };
    let mut windows = Vec::new();
    for hwnd in handles {
        if crate::display_support::is_display_window(hwnd) {
            continue;
        }
        if let Some(entry) = read_window(hwnd, foreground)? {
            windows.push(entry);
        }
    }
    Ok(windows)
}

/// Direct lookup also supports native child HWNDs (e.g. Calculator CoreWindow).
pub fn native_window(handle: isize) -> Result<Vec<WindowEntry>, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let foreground = unsafe { GetForegroundWindow() };
    Ok(read_window(HWND(handle as *mut _), foreground)?
        .into_iter()
        .collect())
}

/// Resolve native title candidates before touching any accessibility provider.
/// None preserves the existing UIA fallback for virtual/non-native windows.
pub(crate) fn title_elements(
    auto: &uiautomation::UIAutomation,
    pattern: &str,
) -> Result<Option<Vec<uiautomation::UIElement>>, CtlError> {
    let _trace = actl_core::trace::scope("window.native_title_candidates");
    let candidates: Vec<_> = native_windows()?
        .into_iter()
        .filter(|w| w.visible && w.title.contains(pattern))
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    candidates
        .into_iter()
        .map(|w| {
            let hwnd = w
                .hwnd
                .ok_or_else(|| crate::internal("native HWND missing"))?;
            let element = auto
                .element_from_handle(uiautomation::types::Handle::from(hwnd))
                .map_err(crate::internal)?;
            if element.get_process_id().map_err(crate::internal)? != w.pid
                || !element
                    .get_name()
                    .map_err(crate::internal)?
                    .contains(pattern)
            {
                return Err(actl_core::CtlError::new(
                    actl_core::ErrorCode::StaleRef,
                    "native window identity changed during UIA resolution",
                ));
            }
            Ok(element)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn read_window(hwnd: HWND, foreground: HWND) -> Result<Option<WindowEntry>, CtlError> {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            return Ok(None);
        }
        return Err(crate::internal("native window PID unavailable"));
    }
    let mut class = [0u16; 256];
    let class_len = unsafe { GetClassNameW(hwnd, &mut class) };
    let mut rect = RECT::default();
    let has_rect = unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok();
    let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
    let entry = WindowEntry {
        title: window_title_of(hwnd).unwrap_or_default(),
        class: String::from_utf16_lossy(&class[..class_len.max(0) as usize]),
        pid,
        foreground: hwnd == foreground,
        topmost: (unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32) & WS_EX_TOPMOST.0 != 0,
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        // Enumeration order is not used as a claimed stacking rank.
        z_order: None,
        rect: has_rect.then_some([rect.left, rect.top, rect.right, rect.bottom]),
        owner_title: (!owner.0.is_null())
            .then(|| window_title_of(owner))
            .flatten(),
        hwnd: Some(hwnd.0 as isize),
    };
    // A window destroyed/reused while reading is not a reliable sample.
    let mut current_pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut current_pid)) };
    if current_pid == pid {
        return Ok(Some(entry));
    }
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Ok(None);
    }
    Err(crate::internal("native window changed during observation"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_lookup_reads_a_handle_without_requiring_top_level_enumeration() {
        let desktop = unsafe { GetDesktopWindow() };
        let rows = native_window(desktop.0 as isize).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].hwnd, Some(desktop.0 as isize));
    }
}
