//! Borderless native frame with explicit header hit testing and accessible window buttons.
use super::paint;
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

pub unsafe fn update(hwnd: HWND, dpi: u32) {
    let mut rect = RECT::default();
    if GetClientRect(hwnd, &mut rect).is_err() {
        return;
    }
    let diameter = paint::pixels(28, dpi);
    let region = CreateRoundRectRgn(0, 0, rect.right + 1, rect.bottom + 1, diameter, diameter);
    if !region.is_invalid() && SetWindowRgn(hwnd, Some(region), true) == 0 {
        let _ = DeleteObject(region.into());
    }
    // Successful SetWindowRgn transfers ownership to Windows.
}

pub unsafe fn border(hwnd: HWND, dc: HDC, rect: &RECT) {
    let saved = SaveDC(dc);
    SelectObject(dc, GetStockObject(NULL_BRUSH));
    SelectObject(dc, GetStockObject(DC_PEN));
    SetDCPenColor(dc, COLORREF(0x44403D));
    let radius = paint::pixels(28, GetDpiForWindow(hwnd));
    let _ = RoundRect(dc, 0, 0, rect.right, rect.bottom, radius, radius);
    let _ = RestoreDC(dc, saved);
}

pub unsafe fn hit_test(hwnd: HWND, lp: LPARAM) -> LRESULT {
    let mut point = POINT {
        x: lp.0 as u16 as i16 as i32,
        y: (lp.0 >> 16) as u16 as i16 as i32,
    };
    let _ = ScreenToClient(hwnd, &mut point);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let dpi = GetDpiForWindow(hwnd);
    let buttons = if GetDlgItem(Some(hwnd), 16).is_ok() {
        112
    } else {
        64
    };
    LRESULT(if point.y >= 0
        && point.y < paint::pixels(88, dpi)
        && point.x >= 0
        && point.x < client.right - paint::pixels(buttons, dpi)
    {
        HTCAPTION
    } else {
        HTCLIENT
    } as isize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_frame_keeps_header_draggable_and_buttons_clickable() {
        unsafe {
            let hwnd = super::super::controls::create_window().unwrap();
            let region = CreateRectRgn(0, 0, 0, 0);
            assert_ne!(GetWindowRgn(hwnd, region), GDI_REGION_TYPE(0));
            assert!(!PtInRegion(region, 0, 0).as_bool());
            let dpi = GetDpiForWindow(hwnd);
            assert!(PtInRegion(region, paint::pixels(24, dpi), paint::pixels(24, dpi)).as_bool());
            let screen = |x, y| {
                let mut point = POINT {
                    x: paint::pixels(x, dpi),
                    y: paint::pixels(y, dpi),
                };
                let _ = ClientToScreen(hwnd, &mut point);
                LPARAM(((point.x as u16 as u32) | ((point.y as u16 as u32) << 16)) as isize)
            };
            assert_eq!(hit_test(hwnd, screen(200, 30)).0, HTCAPTION as isize);
            assert_eq!(hit_test(hwnd, screen(510, 30)).0, HTCLIENT as isize);
            assert_eq!(hit_test(hwnd, screen(464, 30)).0, HTCLIENT as isize);
            assert_eq!(hit_test(hwnd, screen(200, 120)).0, HTCLIENT as isize);
            let _ = DeleteObject(region.into());
            DestroyWindow(hwnd).unwrap();
        }
    }
}
