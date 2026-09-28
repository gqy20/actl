//! Shared native panel styling. Buttons retain their native roles and keyboard behavior.
use super::paint;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{Controls::*, HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
    },
    core::{BOOL, w},
};

const BACKGROUND: u32 = 0x222426;
const SURFACE: u32 = 0x2C2F32;
const TEXT: u32 = 0xECEEEF;
const MUTED: u32 = 0xB1B8BE;
fn rgb(value: u32) -> COLORREF {
    COLORREF(((value & 255) << 16) | (value & 0xFF00) | (value >> 16))
}
struct Fonts {
    body: HFONT,
    title: HFONT,
    caption: HFONT,
}
impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [self.body, self.title, self.caption] {
                let _ = DeleteObject(font.into());
            }
        }
    }
}
pub unsafe fn release(hwnd: HWND) {
    let old = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    if old != 0 {
        drop(Box::from_raw(old as *mut Fonts));
    }
}
pub unsafe fn fonts(hwnd: HWND, dpi: u32, ids: &[i32]) {
    let dark = BOOL(1);
    let _ = windows::Win32::Graphics::Dwm::DwmSetWindowAttribute(
        hwnd,
        windows::Win32::Graphics::Dwm::DWMWA_USE_IMMERSIVE_DARK_MODE,
        (&dark as *const BOOL).cast(),
        std::mem::size_of::<BOOL>() as u32,
    );
    let make = |size, weight| {
        CreateFontW(
            -paint::pixels(size, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        )
    };
    let fonts = Box::new(Fonts {
        body: make(16, 400),
        title: make(22, 600),
        caption: make(14, 400),
    });
    for &id in ids {
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
            let font = match id {
                102 => fonts.title,
                103..=105 => fonts.caption,
                _ => fonts.body,
            };
            SendMessageW(
                child,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
            let mut class = [0u16; 16];
            let len = GetClassNameW(child, &mut class).max(0) as usize;
            if String::from_utf16_lossy(&class[..len]).eq_ignore_ascii_case("button") {
                let _ =
                    windows::Win32::UI::Shell::SetWindowSubclass(child, Some(button_proc), 1, 0);
            }
        }
    }
    release(hwnd);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(fonts) as isize);
    super::frame::update(hwnd, dpi);
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if msg == WM_MOUSEMOVE && GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 0 {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 1);
        let mut track = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            ..Default::default()
        };
        let _ = TrackMouseEvent(&mut track);
        let _ = InvalidateRect(Some(hwnd), None, false);
    } else if msg == WM_MOUSELEAVE {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    windows::Win32::UI::Shell::DefSubclassProc(hwnd, msg, wp, lp)
}

pub unsafe fn scroll(edit: HWND) {
    let dc = GetDC(Some(edit));
    let font = SendMessageW(edit, WM_GETFONT, None, None);
    let old = SelectObject(dc, HGDIOBJ(font.0 as *mut _));
    let mut metrics = TEXTMETRICW::default();
    let measured = GetTextMetricsW(dc, &mut metrics).as_bool();
    SelectObject(dc, old);
    ReleaseDC(Some(edit), dc);
    if measured {
        let mut rect = RECT::default();
        SendMessageW(
            edit,
            EM_GETRECT,
            None,
            Some(LPARAM((&mut rect as *mut RECT) as isize)),
        );
        let lines = SendMessageW(edit, EM_GETLINECOUNT, None, None).0 as i32;
        let _ = ShowScrollBar(
            edit,
            SB_VERT,
            lines * (metrics.tmHeight + metrics.tmExternalLeading) > rect.bottom - rect.top,
        );
        // Toggling a native scrollbar resets the edit's formatting rectangle.
        SendMessageW(
            edit,
            EM_SETRECT,
            None,
            Some(LPARAM((&rect as *const RECT) as isize)),
        );
    }
}
unsafe fn fill(dc: HDC, rect: &RECT, color: u32) {
    SetDCBrushColor(dc, rgb(color));
    FillRect(dc, rect, HBRUSH(GetStockObject(DC_BRUSH).0));
}
/// Called before each panel's behavioral window procedure.
pub unsafe fn message(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    match msg {
        WM_NCHITTEST => Some(super::frame::hit_test(hwnd, lp)),
        WM_ERASEBKGND => {
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            fill(HDC(wp.0 as *mut _), &rect, BACKGROUND);
            super::frame::border(hwnd, HDC(wp.0 as *mut _), &rect);
            Some(LRESULT(1))
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
            let dc = HDC(wp.0 as *mut _);
            let id = GetDlgCtrlID(HWND(lp.0 as *mut _));
            let background = if id == 100 || id == 10 && GetDlgItem(Some(hwnd), 1).is_ok() {
                SURFACE
            } else {
                BACKGROUND
            };
            SetTextColor(
                dc,
                rgb(match id {
                    101 => 0xEDB88F,
                    103..=105 => MUTED,
                    _ => TEXT,
                }),
            );
            SetBkColor(dc, rgb(background));
            SetDCBrushColor(dc, rgb(background));
            Some(LRESULT(GetStockObject(DC_BRUSH).0 as isize))
        }
        WM_DRAWITEM => {
            let item = &*(lp.0 as *const DRAWITEMSTRUCT);
            if item.CtlType != ODT_BUTTON {
                return None;
            }
            let disabled = item.itemState.0 & ODS_DISABLED.0 != 0;
            let pressed = item.itemState.0 & ODS_SELECTED.0 != 0;
            let primary = matches!(item.CtlID, 1 | 11);
            let danger = item.CtlID == 12;
            let window_button = matches!(item.CtlID, 15 | 16);
            let hovered = GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA) != 0;
            let bg = if disabled {
                0x292C2F
            } else if pressed {
                0x465663
            } else if primary {
                if hovered { 0xC0D9EA } else { 0xA8C8DE }
            } else if hovered {
                0x464C52
            } else if window_button {
                BACKGROUND
            } else {
                0x34383D
            };
            let fg = if disabled {
                0x788088
            } else if primary {
                0x18242C
            } else if danger {
                0xF0A69D
            } else {
                TEXT
            };
            let dc = item.hDC;
            let saved = SaveDC(dc);
            fill(dc, &item.rcItem, BACKGROUND);
            SetDCBrushColor(dc, rgb(bg));
            SelectObject(dc, GetStockObject(DC_BRUSH));
            SelectObject(dc, GetStockObject(NULL_PEN));
            let radius = paint::pixels(12, GetDpiForWindow(hwnd));
            let r = item.rcItem;
            let _ = RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
            let font = SendMessageW(item.hwndItem, WM_GETFONT, None, None);
            SelectObject(dc, HGDIOBJ(font.0 as *mut _));
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, rgb(fg));
            let mut label = vec![0u16; GetWindowTextLengthW(item.hwndItem).max(0) as usize + 1];
            let len = GetWindowTextW(item.hwndItem, &mut label).max(0) as usize;
            let mut text_rect = r;
            if window_button {
                SelectObject(dc, GetStockObject(DC_PEN));
                SetDCPenColor(dc, rgb(fg));
                let cx = (r.left + r.right) / 2;
                let cy = (r.top + r.bottom) / 2;
                let half = paint::pixels(5, GetDpiForWindow(hwnd));
                if item.CtlID == 15 {
                    let _ = MoveToEx(dc, cx - half, cy - half, None);
                    let _ = LineTo(dc, cx + half + 1, cy + half + 1);
                    let _ = MoveToEx(dc, cx - half, cy + half, None);
                    let _ = LineTo(dc, cx + half + 1, cy - half - 1);
                } else {
                    let _ = MoveToEx(dc, cx - half, cy, None);
                    let _ = LineTo(dc, cx + half + 1, cy);
                }
            } else {
                DrawTextW(
                    dc,
                    &mut label[..len],
                    &mut text_rect,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
            }
            if item.itemState.0 & ODS_FOCUS.0 != 0 && item.itemState.0 & ODS_NOFOCUSRECT.0 == 0 {
                let inset = paint::pixels(4, GetDpiForWindow(hwnd));
                let _ = InflateRect(&mut text_rect, -inset, -inset);
                let _ = DrawFocusRect(dc, &text_rect);
            }
            let _ = RestoreDC(dc, saved);
            Some(LRESULT(1))
        }
        _ => None,
    }
}
