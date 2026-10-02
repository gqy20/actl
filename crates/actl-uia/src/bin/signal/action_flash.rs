//! 动作开始提示:屏幕居中偏上的一次性胶囊闪现(docs/72 用户设计)。
//! 从左滑入 → 停留 → 向右滑出;全程 ~2.7s,自动销毁。
//! 窗口属性:分层+穿透+置顶+不激活(与 overlay.rs 的 marker 同模式)。
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

const TIMER_ID: usize = 42;
const SLIDE_IN_MS: u32 = 350;
const HOLD_MS: u32 = 1600;
const SLIDE_OUT_MS: u32 = 350;
const SLIDE_DISTANCE: i32 = 80;

#[derive(Clone)]
struct Flash {
    hwnd: usize,
    /// 最终停留的 X(屏幕中心)
    target_x: i32,
    y: i32,
    width: i32,
    height: i32,
    started: std::time::Instant,
    text: String,
}

static FLASH: Mutex<Option<Flash>> = Mutex::new(None);

/// 触发一次闪现。kind 为动作类型(browser-click/launch/...), text 为显示文字。
pub unsafe fn show(kind: &str, text: &str) {
    hide();
    let Ok(instance) = GetModuleHandleW(None) else {
        return;
    };
    let class = w!("actl_signal_flash");
    let _ = RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    });

    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    // 用虚拟屏幕获取真实宽度
    let vw = GetSystemMetrics(SM_CXSCREEN);
    let vh = GetSystemMetrics(SM_CYSCREEN);

    // 计算文字宽度(粗略:每字符 ~10px + padding)
    let text_len = text.chars().count() as i32 + kind.chars().count() as i32;
    let width = (text_len * 10 + 120).clamp(200, 400);
    let height = 48;

    let cx = vw / 2;
    let target_x = cx - width / 2;
    let y = (vh as f32 * 0.28) as i32;

    // 创建窗口:初始位置在左侧(target_x - SLIDE_DISTANCE)
    let Ok(hwnd) = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class,
        w!("actl flash"),
        WS_POPUP,
        target_x - SLIDE_DISTANCE,
        y,
        width,
        height,
        None,
        None,
        Some(instance.into()),
        None,
    ) else {
        return;
    };

    // 颜色键实现圆角透明
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0x000001), 255, LWA_COLORKEY | LWA_ALPHA);
    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);

    let display = format!("ACTL | {text}");
    if let Ok(mut state) = FLASH.lock() {
        *state = Some(Flash {
            hwnd: hwnd.0 as usize,
            target_x,
            y,
            width,
            height,
            started: std::time::Instant::now(),
            text: display,
        });
    }
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(Some(hwnd), TIMER_ID, 16, None); // ~60fps
}

pub unsafe fn hide() {
    let old = FLASH.lock().ok().and_then(|mut f| f.take());
    if let Some(f) = old {
        let _ = DestroyWindow(HWND(f.hwnd as *mut _));
    }
}

#[allow(dead_code)]
pub fn is_showing() -> bool {
    FLASH.lock().ok().is_some_and(|f| f.is_some())
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wp.0 == TIMER_ID => {
            let Some(flash) = FLASH.lock().ok().and_then(|f| f.clone()) else {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            };
            let elapsed = flash.started.elapsed().as_millis() as u32;

            if elapsed < SLIDE_IN_MS {
                // 阶段 1:从左滑入(ease-out cubic)
                let t = elapsed as f32 / SLIDE_IN_MS as f32;
                let ease = 1.0 - (1.0 - t).powi(3);
                let x = flash.target_x - SLIDE_DISTANCE + (SLIDE_DISTANCE as f32 * ease) as i32;
                let alpha = (ease * 255.0) as u8;
                let _ = SetWindowPos(hwnd, None, x, flash.y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
                let _ = SetLayeredWindowAttributes(
                    hwnd,
                    COLORREF(0x000001),
                    alpha,
                    LWA_COLORKEY | LWA_ALPHA,
                );
            } else if elapsed < SLIDE_IN_MS + HOLD_MS {
                // 阶段 2:停留(完全可见)
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    flash.target_x,
                    flash.y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER,
                );
                let _ = SetLayeredWindowAttributes(
                    hwnd,
                    COLORREF(0x000001),
                    255,
                    LWA_COLORKEY | LWA_ALPHA,
                );
            } else if elapsed < SLIDE_IN_MS + HOLD_MS + SLIDE_OUT_MS {
                // 阶段 3:向右滑出+淡出
                let t = (elapsed - SLIDE_IN_MS - HOLD_MS) as f32 / SLIDE_OUT_MS as f32;
                let ease = t * t;
                let x = flash.target_x + (SLIDE_DISTANCE as f32 * ease * 0.7) as i32;
                let alpha = ((1.0 - ease) * 255.0) as u8;
                let _ = SetWindowPos(hwnd, None, x, flash.y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
                let _ = SetLayeredWindowAttributes(
                    hwnd,
                    COLORREF(0x000001),
                    alpha,
                    LWA_COLORKEY | LWA_ALPHA,
                );
            } else {
                // 动画结束
                let _ = KillTimer(Some(hwnd), TIMER_ID);
                let _ = DestroyWindow(hwnd);
                if let Ok(mut state) = FLASH.lock() {
                    *state = None;
                }
                return LRESULT(0);
            }
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let Some(flash) = FLASH.lock().ok().and_then(|f| f.clone()) else {
                let _ = EndPaint(hwnd, &ps);
                return LRESULT(0);
            };
            paint_flash(dc, &flash);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            if let Ok(mut state) = FLASH.lock() {
                *state = None;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn paint_flash(dc: HDC, flash: &Flash) {
    let (w, h) = (flash.width, flash.height);

    // 背景:深色半透明(用纯色近似,颜色键挖圆角)
    let bg = CreateSolidBrush(COLORREF(0x161214));
    let _ = SelectObject(dc, bg.into());
    let radius = h / 2;
    let _ = RoundRect(dc, 0, 0, w, h, radius, radius);
    let _ = DeleteObject(bg.into());

    // 边框:淡蓝色
    let border = GetStockObject(DC_PEN);
    let _ = SelectObject(dc, border);
    SetDCPenColor(dc, COLORREF(0x3A5068));
    let _ = RoundRect(dc, 0, 0, w, h, radius, radius);

    // 文字
    let _ = SetTextColor(dc, COLORREF(0xDCD8D4));
    let _ = SetBkMode(dc, TRANSPARENT);
    let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
    let mut face_name = [0u16; 32];
    for (i, &ch) in face.iter().enumerate() {
        if i < 31 {
            face_name[i] = ch;
        }
    }
    let logfont = LOGFONTW {
        lfHeight: -(h * 38 / 100),
        lfWeight: 500,
        lfFaceName: face_name,
        ..Default::default()
    };
    let font = CreateFontIndirectW(&logfont);
    let old_font = SelectObject(dc, font.into());
    let mut wide_text = super::paint::wide(&flash.text);
    let mut draw_rect = RECT {
        left: 16,
        top: 0,
        right: w - 16,
        bottom: h,
    };
    DrawTextW(
        dc,
        &mut wide_text,
        &mut draw_rect,
        DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_END_ELLIPSIS,
    );
    SelectObject(dc, old_font);
    let _ = DeleteObject(font.into());
}
