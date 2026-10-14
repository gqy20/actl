//! find-image 搜索范围可视化(2026-10-14 用户现场反馈):**分层覆盖窗红框+
//! 闪烁**。第一版 GetWindowDC 画框在 DWM 合成窗上不可见(真机否证),换
//! layered window(截屏工具同款机制):检索期间目标窗口四围 6px 红框以
//! 亮 300ms/灭 160ms 闪烁——「正在检索」可感知,结束自动销毁。覆盖窗
//! WS_EX_TRANSPARENT|NOACTIVATE(点击穿透、不抢焦点),创建/销毁都在
//! 同一 overlay 线程(DestroyWindow 必须由创建线程调)。best-effort:
//! 任一步失败静默跳过,不影响匹配语义。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ,
    ReleaseDC, SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, RegisterClassExW, SW_HIDE,
    SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

/// 边框厚度(物理 px)。
const BORDER: i32 = 6;
const SHOW_MS: u64 = 300;
const HIDE_MS: u64 = 160;

static CLASS_ATOM: OnceLock<u16> = OnceLock::new();

/// DefWindowProcW 的 extern "system" 壳(WNDPROC 要求 system ABI)。
unsafe extern "system" fn def_proc(
    hwnd: HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn register_class() -> Option<u16> {
    let atom = *CLASS_ATOM.get_or_init(|| unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return 0;
        };
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(def_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: w!("actl_search_frame"),
            ..Default::default()
        };
        RegisterClassExW(&wc)
    });
    (atom != 0).then_some(atom)
}

pub(crate) struct SearchFrameGuard {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl SearchFrameGuard {
    pub(crate) fn enter(hwnd: isize) -> Option<Self> {
        let atom = register_class()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("actl-search-frame".into())
            .spawn(move || {
                overlay_loop(hwnd, atom, &stop_thread);
            })
            .ok()?;
        Some(Self {
            stop,
            thread: Some(thread),
        })
    }

    /// 多框静态覆盖(preview-map):元素 rect 集合一次绘入 overlay——
    /// interactive=橙色(BGR 0x00A5FF)3px、其他=暗青色 1px;无闪烁
    /// (与 find-image 的红框闪烁是两种消费形态)。rect 为屏幕物理坐标,
    /// 由调用方换算到 overlay 本地坐标系(窗口原点差)。
    pub(crate) fn enter_with_boxes(
        hwnd: isize,
        boxes: Vec<([i32; 4], bool)>,
    ) -> Option<SearchFrameGuard> {
        let atom = register_class()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("actl-map-frame".into())
            .spawn(move || {
                boxes_loop(hwnd, atom, &boxes, &stop_thread);
            })
            .ok()?;
        Some(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for SearchFrameGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn overlay_loop(hwnd_target: isize, atom: u16, stop: &AtomicBool) {
    unsafe {
        let target = HWND(hwnd_target as *mut _);
        let mut rect = RECT::default();
        if GetWindowRect(target, &mut rect).is_err() {
            return;
        }
        let (x, y, w, h) = (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        if w <= 0 || h <= 0 {
            return;
        }
        let Ok(overlay) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(atom as usize as *const u16),
            w!("actl-search-frame"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            None,
            None,
        ) else {
            return;
        };
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            let _ = DestroyWindow(overlay);
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DestroyWindow(overlay);
            return;
        };
        let old = SelectObject(mem_dc, HGDIOBJ::from(dib));
        // DIB 初始为零=全透明;仅边框环着红色(BGRA)
        let bits = bits as *mut u8;
        let stride = w as usize * 4;
        for row in 0..h as usize {
            let top_band =
                row < BORDER as usize || row >= (h as usize).saturating_sub(BORDER as usize);
            let base = bits.add(row * stride);
            for col in 0..w as usize {
                if top_band
                    || col < BORDER as usize
                    || col >= (w as usize).saturating_sub(BORDER as usize)
                {
                    let p = base.add(col * 4);
                    *p = 0x00; // B
                    *p.add(1) = 0x00; // G
                    *p.add(2) = 0xFF; // R
                    *p.add(3) = 0xFF; // A
                }
            }
        }
        let size = SIZE { cx: w, cy: h };
        let point = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = windows::Win32::UI::WindowsAndMessaging::UpdateLayeredWindow(
            overlay,
            Some(screen_dc),
            None,
            Some(&size),
            Some(mem_dc),
            Some(&point),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
            std::thread::sleep(Duration::from_millis(SHOW_MS));
            if stop.load(Ordering::Acquire) {
                break;
            }
            let _ = ShowWindow(overlay, SW_HIDE);
            std::thread::sleep(Duration::from_millis(HIDE_MS));
        }
        let _ = DestroyWindow(overlay);
        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ::from(dib));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
    }
}

/// preview-map 的静态多框覆盖:元素 rect 集一次绘入,不闪烁,stop 置位
/// (guard Drop)后销毁。绘制色彩:interactive 橙(BGR 0x00A5FF)3px、
/// 其他暗青(BGR 0x8A8A00→视觉暗黄绿调低饱和;实际取青系 0xCCCC00)
/// 1px。rect 物理坐标 → overlay 本地(减窗口原点)。
fn boxes_loop(hwnd_target: isize, atom: u16, boxes: &[([i32; 4], bool)], stop: &AtomicBool) {
    unsafe {
        let target = HWND(hwnd_target as *mut _);
        let mut rect = RECT::default();
        if GetWindowRect(target, &mut rect).is_err() {
            return;
        }
        let (x, y, w, h) = (
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        if w <= 0 || h <= 0 {
            return;
        }
        let Ok(overlay) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            windows::core::PCWSTR(atom as usize as *const u16),
            w!("actl-search-frame"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            None,
            None,
        ) else {
            return;
        };
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            let _ = DestroyWindow(overlay);
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
        let Ok(dib) = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DestroyWindow(overlay);
            return;
        };
        let old = SelectObject(mem_dc, HGDIOBJ::from(dib));
        let bits = bits as *mut u8;
        let stride = w as usize * 4;
        // 逐框描边(像素直写;框间重叠取或——多框密度下视觉可辨即可)
        for ([bx, by, bw, bh], interactive) in boxes {
            // 物理屏幕坐标 → overlay 本地
            let lx0 = (*bx - x).max(0) as usize;
            let ly0 = (*by - y).max(0) as usize;
            let lw = (*bw as usize).min((w as usize).saturating_sub(lx0));
            let lh = (*bh as usize).min((h as usize).saturating_sub(ly0));
            let thickness = if *interactive { 3 } else { 1 };
            let (b, g, r) = if *interactive {
                (0x00, 0xA5, 0xFF)
            } else {
                (0xCC, 0xCC, 0x00)
            };
            let paint = |px: usize, py: usize| {
                if px < w as usize && py < h as usize {
                    let p = bits.add(py * stride + px * 4);
                    *p = b;
                    *p.add(1) = g;
                    *p.add(2) = r;
                    *p.add(3) = 0xFF;
                }
            };
            for t in 0..thickness {
                for cx in lx0..(lx0 + lw) {
                    paint(cx, ly0 + t);
                    paint(cx, ly0 + lh.saturating_sub(1 + t));
                }
                for cy in ly0..(ly0 + lh) {
                    paint(lx0 + t, cy);
                    paint(lx0 + lw.saturating_sub(1 + t), cy);
                }
            }
        }
        let size = SIZE { cx: w, cy: h };
        let point = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = windows::Win32::UI::WindowsAndMessaging::UpdateLayeredWindow(
            overlay,
            Some(screen_dc),
            None,
            Some(&size),
            Some(mem_dc),
            Some(&point),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        let _ = ShowWindow(overlay, SW_SHOWNOACTIVATE);
        while !stop.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = DestroyWindow(overlay);
        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ::from(dib));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
    }
}
