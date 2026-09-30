//! Small non-modal native popup; shares the strip's antialiased surface and typography.
use super::paint;
use std::sync::Mutex;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::*,
            Shell::{DefSubclassProc, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
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
    /// 悬浮卡归属:只有归属者可以撤下,避免误删菜单/详情弹窗。
    hover: bool,
    selected: Option<usize>,
}
static POPUP: Mutex<Option<Popup>> = Mutex::new(None);
pub unsafe fn hide() {
    let old = POPUP.lock().ok().and_then(|mut p| p.take());
    if let Some(p) = old {
        let _ = DestroyWindow(HWND(p.hwnd as *mut _));
    }
}
/// 任意弹窗(菜单/详情/悬浮卡)是否打开——悬浮卡让位的判据。
pub fn any_open() -> bool {
    POPUP.lock().ok().is_some_and(|p| p.is_some())
}
/// 当前弹窗是否为悬浮卡。
pub fn shown_by_hover() -> bool {
    POPUP
        .lock()
        .ok()
        .and_then(|p| p.as_ref().map(|p| p.hover))
        .unwrap_or(false)
}
fn report(reason: &str, detail: &str) {
    let paths = actl_core::state::SignalPaths::default();
    let _ = paths.write_display(
        "popup.json",
        &serde_json::json!({"reason":reason,"detail":detail,
        "pid":std::process::id(),"ts_ms":actl_core::state::unix_ms()}),
    );
}
pub unsafe fn show(owner: HWND, lines: Vec<String>, menu: bool) {
    open_popup(owner, lines, menu, false);
}
/// 悬浮卡:与详情同形态(只读文本行),但带 hover 归属,换槽/离开即由 shell 撤下。
pub unsafe fn show_hover(owner: HWND, lines: Vec<String>) {
    open_popup(owner, lines, false, true);
}
unsafe fn open_popup(owner: HWND, lines: Vec<String>, menu: bool, hover: bool) {
    hide();
    let Ok(instance) = GetModuleHandleW(None) else {
        report(
            "module_unavailable",
            &windows::core::Error::from_thread().to_string(),
        );
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
        WS_EX_TOPMOST | WS_EX_APPWINDOW | WS_EX_CONTROLPARENT | WS_EX_LAYERED,
        class,
        if menu {
            w!("actl · 菜单")
        } else if hover {
            w!("actl · 提示")
        } else {
            w!("actl · 详情")
        },
        WS_POPUP,
        x,
        y,
        width,
        height,
        None,
        None,
        Some(instance.into()),
        None,
    ) else {
        report(
            "create_window_failed",
            &windows::core::Error::from_thread().to_string(),
        );
        return;
    };
    if let Ok(mut p) = POPUP.lock() {
        *p = Some(Popup {
            hwnd: hwnd.0 as usize,
            owner: owner.0 as usize,
            lines,
            menu,
            hover,
            selected: None,
        });
    }
    if let Some(p) = POPUP.lock().ok().and_then(|p| p.clone()) {
        // Standard owner-drawn buttons supply native accessible names, roles and Invoke.
        // The existing layered raster remains the single visual surface.
        for (i, text) in p.lines.iter().enumerate() {
            let label = paint::wide(text);
            let child = CreateWindowExW(
                WS_EX_TRANSPARENT,
                if menu { w!("BUTTON") } else { w!("STATIC") },
                windows::core::PCWSTR(label.as_ptr()),
                WS_CHILD
                    | WS_VISIBLE
                    | if menu {
                        WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32 | BS_NOTIFY as u32)
                    } else {
                        WINDOW_STYLE(0) // Native Text role, rather than owner-drawn Image.
                    },
                paint::pixels(8, dx),
                paint::pixels(8 + i as i32 * 28, dx),
                width - paint::pixels(16, dx),
                paint::pixels(28, dx),
                Some(hwnd),
                Some(HMENU((100 + i) as *mut _)),
                Some(instance.into()),
                None,
            );
            if let Ok(child) = child {
                let _ = SetWindowSubclass(child, Some(text_proc), 1, usize::from(menu));
            }
        }
        paint::draw_popup(hwnd, &p.lines, p.menu, p.selected);
    }
    let _ = ShowWindow(hwnd, SW_SHOW);
    let requested = SetForegroundWindow(hwnd).as_bool();
    if !IsWindowVisible(hwnd).as_bool() {
        report("hidden", "popup was created but is not visible");
    } else if GetForegroundWindow() != hwnd {
        report(
            "foreground_not_acquired",
            if requested {
                "activation pending; popup remains visible"
            } else {
                "Windows rejected activation; popup remains visible"
            },
        );
        // Draw attention without stealing focus or repeatedly sending activation requests.
        let flash = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: FLASHW_TRAY,
            uCount: 3,
            dwTimeout: 0,
        };
        let _ = FlashWindowEx(&flash);
    } else {
        report("ready", "popup visible and foreground");
    }
    if menu && let Ok(button) = GetDlgItem(Some(hwnd), 100) {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(button));
    }
}
// Keep native static-control accessibility while the parent supplies the raster.
unsafe extern "system" fn text_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    menu: usize,
) -> LRESULT {
    if menu != 0 && msg == WM_MOUSEMOVE {
        let parent = POPUP.lock().ok().and_then(|mut p| {
            let p = p.as_mut()?;
            let next = GetDlgCtrlID(hwnd).saturating_sub(100) as usize;
            if p.selected == Some(next) {
                return None;
            }
            p.selected = Some(next);
            Some(HWND(p.hwnd as *mut _))
        });
        if let Some(parent) = parent {
            let _ = InvalidateRect(Some(parent), None, false);
        }
    }
    if menu == 0 && msg == WM_PAINT {
        let _ = ValidateRect(Some(hwnd), None);
        LRESULT(0)
    } else {
        DefSubclassProc(hwnd, msg, wp, lp)
    }
}
pub unsafe fn dialog_message(message: &MSG) -> bool {
    let hwnd = POPUP
        .lock()
        .ok()
        .and_then(|p| p.as_ref().map(|p| HWND(p.hwnd as *mut _)));
    hwnd.is_some_and(|h| IsDialogMessageW(h, message).as_bool())
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
        WM_DPICHANGED => {
            if let Some(p) = state {
                let dpi = wp.0 as u32 & 0xffff;
                let rect = &*(lp.0 as *const RECT);
                let width = paint::pixels(if p.menu { 136 } else { 280 }, dpi);
                let height = paint::pixels(16 + p.lines.len() as i32 * 28, dpi);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    rect.left,
                    rect.top,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                for i in 0..p.lines.len() {
                    if let Ok(child) = GetDlgItem(Some(hwnd), 100 + i as i32) {
                        let _ = SetWindowPos(
                            child,
                            None,
                            paint::pixels(8, dpi),
                            paint::pixels(8 + i as i32 * 28, dpi),
                            width - paint::pixels(16, dpi),
                            paint::pixels(28, dpi),
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
                paint::draw_popup(hwnd, &p.lines, p.menu, p.selected);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => LRESULT(1),
        WM_COMMAND => {
            let id = wp.0 & 0xffff;
            if id == 2 {
                hide();
            } else if let Some(mut p) = state
                && p.menu
                && id >= 100
                && id - 100 < p.lines.len()
            {
                p.selected = Some(id - 100);
                if wp.0 >> 16 == BN_CLICKED as usize {
                    choose(&p);
                } else if wp.0 >> 16 == BN_SETFOCUS as usize {
                    if let Ok(mut s) = POPUP.lock() {
                        *s = Some(p);
                    }
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
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
                    if let Ok(button) = GetDlgItem(Some(hwnd), 100 + i as i32) {
                        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(button));
                    }
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
    #[ignore = "interactive desktop popup regression"]
    fn child_hover_updates_parent_selection() {
        unsafe {
            show(HWND::default(), vec!["first".into(), "second".into()], true);
            let hwnd = POPUP.lock().unwrap().as_ref().unwrap().hwnd;
            let first = GetDlgItem(Some(HWND(hwnd as *mut _)), 100).unwrap();
            dialog_message(&MSG {
                hwnd: first,
                message: WM_KEYDOWN,
                wParam: WPARAM(40),
                ..Default::default()
            });
            let first_down = POPUP.lock().unwrap().as_ref().unwrap().selected;
            let child = GetDlgItem(Some(HWND(hwnd as *mut _)), 101).unwrap();
            SendMessageW(child, WM_MOUSEMOVE, Some(WPARAM(0)), Some(LPARAM(0)));
            let selected = POPUP.lock().unwrap().as_ref().unwrap().selected;
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(child));
            dialog_message(&MSG {
                hwnd: child,
                message: WM_KEYDOWN,
                wParam: WPARAM(40),
                ..Default::default()
            });
            let keyboard = POPUP.lock().unwrap().as_ref().unwrap().selected;
            hide();
            assert_eq!(selected, Some(1));
            assert_eq!(keyboard, Some(0));
            assert_eq!(first_down, Some(1));
        }
    }
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
