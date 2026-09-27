//! Small non-modal native popup; shares the strip's antialiased surface and typography.
use super::paint;
use std::sync::Mutex;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::w,
};
pub const CHOOSE: u32 = WM_APP + 43;
#[derive(Clone)]
struct Popup {
    hwnd: usize,
    owner: usize,
    lines: Vec<String>,
    menu: bool,
    selected: Option<usize>,
}
static POPUP: Mutex<Option<Popup>> = Mutex::new(None);
pub unsafe fn hide() {
    let old = POPUP.lock().ok().and_then(|mut p| p.take());
    if let Some(p) = old {
        let _ = DestroyWindow(HWND(p.hwnd as *mut _));
    }
}
pub unsafe fn show(owner: HWND, lines: Vec<String>, menu: bool) {
    hide();
    let Ok(instance) = GetModuleHandleW(None) else {
        return;
    };
    let class = w!("actl_signal_details");
    let _ = RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: class,
        ..Default::default()
    });
    let mut point = POINT::default();
    let _ = GetCursorPos(&mut point);
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = GetMonitorInfoW(monitor, &mut info);
    let (mut dx, mut dy) = (96, 96);
    let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
    let width = paint::pixels(if menu { 136 } else { 280 }, dx);
    let height = paint::pixels(16 + lines.len() as i32 * 28, dx);
    let x = point.x.clamp(
        info.rcWork.left,
        (info.rcWork.right - width).max(info.rcWork.left),
    );
    let y = point.y.clamp(
        info.rcWork.top,
        (info.rcWork.bottom - height).max(info.rcWork.top),
    );
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        class,
        w!("actl · 详情"),
        WS_POPUP,
        x,
        y,
        width,
        height,
        Some(owner),
        None,
        Some(instance.into()),
        None,
    ) else {
        return;
    };
    if let Ok(mut p) = POPUP.lock() {
        *p = Some(Popup {
            hwnd: hwnd.0 as usize,
            owner: owner.0 as usize,
            lines,
            menu,
            selected: None,
        });
    }
    if let Some(p) = POPUP.lock().ok().and_then(|p| p.clone()) {
        paint::draw_popup(hwnd, &p.lines, p.menu, p.selected);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
}
unsafe fn choose(p: &Popup) {
    if p.menu
        && let Some(i) = p.selected
    {
        let _ = PostMessageW(
            Some(HWND(p.owner as *mut _)),
            CHOOSE,
            WPARAM(i + 1),
            LPARAM(0),
        );
    }
    hide();
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let state = POPUP.lock().ok().and_then(|p| p.clone());
    match msg {
        WM_PAINT => {
            if let Some(p) = state {
                paint::draw_popup(hwnd, &p.lines, p.menu, p.selected);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            if let Some(p) = state
                && p.menu
            {
                let next = row_at(lp, GetDpiForWindow(hwnd), p.lines.len());
                if next != p.selected {
                    if let Ok(mut s) = POPUP.lock()
                        && let Some(s) = s.as_mut()
                    {
                        s.selected = next;
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(mut p) = state {
                p.selected = row_at(lp, GetDpiForWindow(hwnd), p.lines.len());
                choose(&p);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 == 27 {
                hide();
            } else if let Some(mut p) = state {
                if wp.0 == 13 {
                    choose(&p);
                } else if p.menu && (wp.0 == 38 || wp.0 == 40) {
                    let n = p.lines.len();
                    let i = match p.selected {
                        Some(i) if wp.0 == 38 => (i + n - 1) % n,
                        Some(i) => (i + 1) % n,
                        None => 0,
                    };
                    p.selected = Some(i);
                    if let Ok(mut s) = POPUP.lock() {
                        *s = Some(p);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_ACTIVATE if wp.0 & 0xffff == WA_INACTIVE as usize => {
            hide();
            LRESULT(0)
        }
        WM_CLOSE => {
            hide();
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), 1);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn row_at(lp: LPARAM, dpi: u32, count: usize) -> Option<usize> {
    let x = (lp.0 as u16 as i16) as i32;
    let y = ((lp.0 >> 16) as u16 as i16) as i32;
    let row = (y - paint::pixels(8, dpi)) / paint::pixels(28, dpi);
    (x >= 0
        && x < paint::pixels(136, dpi)
        && y >= paint::pixels(8, dpi)
        && row >= 0
        && (row as usize) < count)
        .then_some(row as usize)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_click_maps_to_visible_row_at_scaled_dpi() {
        for dpi in [96, 120, 144, 192] {
            let point = |x, y| {
                LPARAM(((paint::pixels(y, dpi) as isize) << 16) | paint::pixels(x, dpi) as isize)
            };
            assert_eq!(row_at(point(12, 10), dpi, 5), Some(0));
            assert_eq!(row_at(point(12, 40), dpi, 5), Some(1));
            assert_eq!(row_at(point(12, 4), dpi, 5), None);
            assert_eq!(row_at(point(140, 40), dpi, 5), None);
        }
    }
}
