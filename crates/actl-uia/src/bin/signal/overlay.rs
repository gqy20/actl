//! Click-through, non-activating marker windows; screen pixels are never scaled twice.
use actl_core::activity::{ActionPhase, ActionState};
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, w};

#[derive(Clone)]
struct Marker {
    hwnd: usize,
    action: ActionState,
    created: std::time::Instant,
    reduced: bool,
}
static MARKER: Mutex<Option<Marker>> = Mutex::new(None);

pub unsafe fn hide() {
    let marker = MARKER.lock().ok().and_then(|mut m| m.take());
    if let Some(m) = marker {
        let _ = DestroyWindow(HWND(m.hwnd as *mut _));
    }
}

pub unsafe fn show(action: &ActionState) {
    if action.target.point.is_none() && action.target.bounds.is_none() {
        return;
    }
    // Move/scroll feedback uses a static marker, never a fictional click ripple.
    hide();
    let Ok(instance) = GetModuleHandleW(None) else {
        return;
    };
    let class = w!("actl_signal_marker");
    let _ = RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    });
    let mut action = action.clone();
    if action.kind == "drag" && action.phase == ActionPhase::Delivered {
        action.target.point = action.target.destination.or(action.target.point);
    }
    let point = action
        .target
        .point
        .or(action.target.bounds.map(|b| [b[0], b[1]]))
        .unwrap_or([0, 0]);
    let monitor = MonitorFromPoint(
        POINT {
            x: point[0],
            y: point[1],
        },
        MONITOR_DEFAULTTONEAREST,
    );
    let (mut dx, mut dy) = (96, 96);
    let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(
        monitor,
        windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
        &mut dx,
        &mut dy,
    );
    let diameter = super::paint::pixels(56, dx);
    let [x, y, w, h] = geometry(&action, diameter);
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class,
        w!("actl action marker"),
        WS_POPUP,
        x,
        y,
        w,
        h,
        None,
        None,
        Some(instance.into()),
        None,
    ) else {
        return;
    };
    // Color key leaves only the thin lines, including an entirely transparent center.
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_COLORKEY | LWA_ALPHA);
    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    let mut animate = BOOL(1);
    let _ = SystemParametersInfoW(
        SPI_GETCLIENTAREAANIMATION,
        0,
        Some((&mut animate as *mut BOOL).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    if let Ok(mut state) = MARKER.lock() {
        *state = Some(Marker {
            hwnd: hwnd.0 as usize,
            action: action.clone(),
            created: std::time::Instant::now(),
            reduced: !animate.as_bool(),
        });
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(Some(hwnd), 1, 20, None);
}

fn geometry(action: &ActionState, diameter: i32) -> [i32; 4] {
    if let Some([x, y]) = action.target.point {
        [
            x.saturating_sub(diameter / 2),
            y.saturating_sub(diameter / 2),
            diameter,
            diameter,
        ]
    } else if let Some([l, t, r, b]) = action.target.bounds {
        [
            l,
            t,
            r.saturating_sub(l).clamp(1, 16000),
            b.saturating_sub(t).clamp(1, 16000),
        ]
    } else {
        [0, 0, 1, 1]
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER => {
            let m = MARKER.lock().ok().and_then(|m| m.clone());
            if let Some(m) = m {
                let preparing = m.action.phase == ActionPhase::Preparing;
                let limit = if preparing { 600 } else { 400 };
                if m.created.elapsed().as_millis() >= limit {
                    hide();
                } else if !m.reduced && !preparing {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let m = MARKER.lock().ok().and_then(|m| m.clone());
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            let _ = GetClientRect(hwnd, &mut rc);
            FillRect(dc, &rc, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            if let Some(m) = m {
                let preparing = m.action.phase == ActionPhase::Preparing;
                let elapsed = (m.created.elapsed().as_secs_f32() / 0.4).clamp(0., 1.);
                let animated = !m.reduced && !preparing && m.action.kind == "click";
                let radius = if animated {
                    9. + 15. * (1. - (1. - elapsed).powi(2))
                } else {
                    10.
                };
                if animated {
                    let _ = SetLayeredWindowAttributes(
                        hwnd,
                        COLORREF(0),
                        ((1. - elapsed) * 255.) as u8,
                        LWA_COLORKEY | LWA_ALPHA,
                    );
                }
                let pen = CreatePen(PS_SOLID, 2, COLORREF(0x0041_B3E3));
                let old_p = SelectObject(dc, HGDIOBJ(pen.0));
                let old_b = SelectObject(dc, GetStockObject(NULL_BRUSH));
                if m.action.target.point.is_some() {
                    let center = rc.right / 2;
                    let r = (radius * rc.right as f32 / 56.) as i32;
                    let _ = Ellipse(dc, center - r, center - r, center + r, center + r);
                } else {
                    let _ = Rectangle(dc, 1, 1, rc.right - 1, rc.bottom - 1);
                }
                SelectObject(dc, old_b);
                SelectObject(dc, old_p);
                let _ = DeleteObject(HGDIOBJ(pen.0));
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), 1);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
