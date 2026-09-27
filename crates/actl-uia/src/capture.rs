//! screenshot —— 像素捕获兜底通道(显式请求;默认观察仍是 UIA,定位哲学不变)。
//!
//! 三域:窗口(PrintWindow+PW_RENDERFULLCONTENT,抗遮挡)/ 区域(BitBlt+CAPTUREBLT,
//! 屏幕所见)/ 元素(定位链 bounds 裁剪)。DPI:捕获全程持**线程级 PMv2**
//! (collector 模式;spike 2026-09-27 实证:PMv2 线程内 UIA bounds 即物理像素,
//! bounds 与 bitmap 同坐标系,无需比例映射)。PNG 经 WIC(纯 COM,零新依赖)。
//! 卫生:像素上限防失控;shots 目录 keep-20 滚动;最小化窗口如实拒绝。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;

use crate::internal;
use crate::locate::locate;
use crate::window::{WindowRef, resolve_window};

/// 像素总量上限(5000 万 ≈ 8K 全屏 ×2;防失控内存)
pub const MAX_PIXELS: i64 = 50_000_000;
const SHOT_KEEP: usize = 20;

pub struct ShotResult {
    pub path: String,
    pub width: i32,
    pub height: i32,
    pub bytes: u64,
}

/// 截图输出目录(懒建;keep-20 滚动,同 snapshots 纪律)
fn shots_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let dir = std::path::Path::new(&base).join("actl").join("shots");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn default_out_path() -> Option<String> {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let dir = shots_dir()?;
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(
        dir.join(format!("shot_{nanos:x}{n:x}.png"))
            .to_string_lossy()
            .into_owned(),
    )
}

/// 滚动保留最近 SHOT_KEEP 份(按修改时间淘汰;失败静默——截图不该因清理失败而失败)
fn retain_shots(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "png"))
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            Some((m, e.path()))
        })
        .collect();
    if files.len() <= SHOT_KEEP {
        return;
    }
    files.sort();
    let excess = files.len() - SHOT_KEEP;
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

/// 线程级 PMv2 上下文(spike 实证:此上下文内 UIA/Win32 坐标同为物理像素)。
/// pub(crate):坐标点击(pointer_at_physical)复用——截图与坐标守卫同坐标系。
pub(crate) struct Pmv2Guard(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);

impl Pmv2Guard {
    pub(crate) fn enter() -> Self {
        use windows::Win32::UI::HiDpi::{
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
        };
        let prev =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        Self(prev)
    }
}

impl Drop for Pmv2Guard {
    fn drop(&mut self) {
        use windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext;
        if !self.0.is_invalid() {
            unsafe {
                let _ = SetThreadDpiAwarenessContext(self.0);
            }
        }
    }
}

/// 捕获区域(物理屏幕坐标;CAPTUREBLT 含分层窗,截"屏幕所见")
unsafe fn blt_region(x: i32, y: i32, w: i32, h: i32) -> Result<(Vec<u8>, i32, i32), CtlError> {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HGDIOBJ, SRCCOPY,
    };
    if w <= 0 || h <= 0 || (w as i64) * (h as i64) > MAX_PIXELS {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("region {w}x{h} invalid or over the {MAX_PIXELS}-pixel guard"),
        ));
    }
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down:与 WIC 行序一致
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib = windows::Win32::Graphics::Gdi::CreateDIBSection(
        Some(mem),
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
    )
    .map_err(internal)?;
    let old = windows::Win32::Graphics::Gdi::SelectObject(mem, HGDIOBJ(dib.0));
    let ok = BitBlt(mem, 0, 0, w, h, Some(screen), x, y, SRCCOPY | CAPTUREBLT).is_ok();
    let _ = windows::Win32::Graphics::Gdi::SelectObject(mem, old);
    if !ok {
        let _ = DeleteDC(mem);
        let _ = release_dc_safe(screen);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        return Err(CtlError::internal("BitBlt failed for the requested region"));
    }
    // GetDIBits 须在 DC 存活时调用(mem 尚未删除)
    let stride = (w * 4) as usize;
    let mut buf = vec![0u8; stride * h as usize];
    bmi.bmiHeader.biHeight = -h;
    let got = GetDIBits(
        mem,
        windows::Win32::Graphics::Gdi::HBITMAP(dib.0),
        0,
        h as u32,
        Some(buf.as_mut_ptr().cast()),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    let _ = DeleteObject(HGDIOBJ(dib.0));
    if got != h {
        return Err(CtlError::internal("GetDIBits returned incomplete rows"));
    }
    Ok((buf, w, h))
}

fn release_dc_safe(hdc: windows::Win32::Graphics::Gdi::HDC) -> i32 {
    unsafe { windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc) }
}

/// 捕获窗口本体(PrintWindow 抗遮挡;失败降级窗口矩形 blt)
unsafe fn blt_window(
    hwnd: windows::Win32::Foundation::HWND,
) -> Result<(Vec<u8>, i32, i32), CtlError> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic};
    // windows 0.62 bindings 未生成 PrintWindow(user32 实有)——手写声明,常量 PW_RENDERFULLCONTENT=2
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(
            hwnd: windows::Win32::Foundation::HWND,
            hdc: windows::Win32::Graphics::Gdi::HDC,
            flags: u32,
        ) -> i32;
    }
    const PW_RENDERFULLCONTENT: u32 = 2;
    if IsIconic(hwnd).as_bool() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "window is minimized - restore it before capturing (no implicit restore)",
        ));
    }
    // DWM 可见边界优先(最大化窗口的 GetWindowRect 含屏幕外阴影边距)
    let mut r = RECT::default();
    let dwm_ok = windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
        hwnd,
        windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut r as *mut RECT as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_ok()
        && r.right > r.left;
    if !dwm_ok {
        GetWindowRect(hwnd, &mut r)
            .map_err(|e| CtlError::internal(format!("GetWindowRect: {e}")))?;
    }
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "window rect is empty",
        ));
    }
    // 先 PrintWindow 到全窗 DC(用 GetWindowRect 全量,避免 DWM 边界裁掉内容)
    let mut full = RECT::default();
    GetWindowRect(hwnd, &mut full)
        .map_err(|e| CtlError::internal(format!("GetWindowRect: {e}")))?;
    let (fw, fh) = (full.right - full.left, full.bottom - full.top);
    if (fw as i64) * (fh as i64) > MAX_PIXELS {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("window {fw}x{fh} over the {MAX_PIXELS}-pixel guard"),
        ));
    }
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, GetWindowDC, HGDIOBJ,
    };
    let wdc = GetWindowDC(Some(hwnd));
    let mem = CreateCompatibleDC(Some(wdc));
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: fw,
            biHeight: -fh,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib =
        CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).map_err(internal)?;
    let old = windows::Win32::Graphics::Gdi::SelectObject(mem, HGDIOBJ(dib.0));
    let printed = PrintWindow(hwnd, mem, PW_RENDERFULLCONTENT) != 0;
    let _ = windows::Win32::Graphics::Gdi::SelectObject(mem, old);
    let _ = release_dc_safe(windows::Win32::Graphics::Gdi::HDC(wdc.0));
    if !printed {
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem);
        // 降级:窗口矩形的屏幕捕获(遮挡者会入镜,如实行为)
        return blt_region(full.left, full.top, fw, fh);
    }
    let _ = DeleteDC(mem);
    let stride = (fw * 4) as usize;
    let mut buf = vec![0u8; stride * fh as usize];
    let mut hdr = bmi;
    let got = windows::Win32::Graphics::Gdi::GetDIBits(
        CreateCompatibleDC(None),
        windows::Win32::Graphics::Gdi::HBITMAP(dib.0),
        0,
        fh as u32,
        Some(buf.as_mut_ptr().cast()),
        &mut hdr,
        DIB_RGB_COLORS,
    );
    let _ = DeleteObject(HGDIOBJ(dib.0));
    if got != fh {
        return Err(CtlError::internal("GetDIBits(window) incomplete"));
    }
    // DWM 可见边界裁剪(相对窗口左上的物理偏移)
    if dwm_ok && (r.left != full.left || r.top != full.top || w != fw || h != fh) {
        let (ox, oy) = (r.left - full.left, r.top - full.top);
        let mut out = Vec::with_capacity((w * 4) as usize * h as usize);
        for row in 0..h {
            let start = ((oy + row) as usize) * stride + (ox as usize) * 4;
            let end = start + (w as usize) * 4;
            if end <= buf.len() {
                out.extend_from_slice(&buf[start..end]);
            }
        }
        return Ok((out, w, h));
    }
    Ok((buf, fw, fh))
}

/// WIC PNG 编码(BGRA top-down → 文件)
unsafe fn encode_png(
    pixels: &[u8],
    w: i32,
    h: i32,
    path: &std::path::Path,
) -> Result<u64, CtlError> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA,
        IWICBitmapEncoder, IWICImagingFactory, WICBitmapEncoderNoCache,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        STGM_CREATE,
    };
    use windows::core::Interface;
    // 区域路径不经 UIA(其内部已 CoInit),编码入口自查 COM
    let _com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    use windows::Win32::System::Com::STGM_WRITE;
    let wpath: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let stream = windows::Win32::UI::Shell::SHCreateStreamOnFileW(
        windows::core::PCWSTR(wpath.as_ptr()),
        STGM_CREATE.0 | STGM_WRITE.0,
    )
    .map_err(internal)?;
    let factory: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).map_err(internal)?;
    let encoder: IWICBitmapEncoder = factory
        .CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())
        .map_err(internal)?;
    encoder
        .Initialize(&stream, WICBitmapEncoderNoCache)
        .map_err(internal)?;
    let mut frame: Option<windows::Win32::Graphics::Imaging::IWICBitmapFrameEncode> = None;
    let mut props: Option<windows::Win32::System::Com::StructuredStorage::IPropertyBag2> = None;
    encoder
        .CreateNewFrame(&mut frame as *mut _, &mut props as *mut _)
        .map_err(internal)?;
    let frame = frame.ok_or_else(|| CtlError::internal("WIC frame missing"))?;
    let f = frame
        .cast::<windows::Win32::Graphics::Imaging::IWICBitmapFrameEncode>()
        .map_err(internal)?;
    f.Initialize(props.as_ref()).map_err(internal)?;
    f.SetSize(w as u32, h as u32).map_err(internal)?;
    let mut fmt = GUID_WICPixelFormat32bppBGRA;
    f.SetPixelFormat(&mut fmt).map_err(internal)?;
    let stride = (w * 4) as u32;
    f.WritePixels(h as u32, stride, pixels).map_err(internal)?;
    f.Commit().map_err(internal)?;
    encoder.Commit().map_err(internal)?;
    let _ = Interface::downgrade(&stream);
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| CtlError::internal(format!("stat: {e}")))
}

fn finish(pixels: Vec<u8>, w: i32, h: i32, out: Option<&str>) -> Result<ShotResult, CtlError> {
    let path = match out {
        Some(p) => std::path::PathBuf::from(p),
        None => std::path::PathBuf::from(
            default_out_path()
                .ok_or_else(|| CtlError::internal("no LOCALAPPDATA for shots dir"))?,
        ),
    };
    let bytes = unsafe { encode_png(&pixels, w, h, &path)? };
    if let Some(dir) = path.parent() {
        retain_shots(dir);
    }
    Ok(ShotResult {
        path: path.to_string_lossy().into_owned(),
        width: w,
        height: h,
        bytes,
    })
}

/// 区域域:物理坐标矩形,截屏幕所见
pub fn screenshot_rect(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    out: Option<&str>,
) -> Result<ShotResult, CtlError> {
    let _dpi = Pmv2Guard::enter();
    let (px, w, h) = unsafe { blt_region(x, y, w, h)? };
    finish(px, w, h, out)
}

/// 窗口域:标题/hwnd 解析唯一窗口;PrintWindow 抗遮挡,最小化如实拒
pub fn screenshot_window(r: WindowRef<'_>, out: Option<&str>) -> Result<ShotResult, CtlError> {
    let _dpi = Pmv2Guard::enter();
    let win = resolve_window(r)?;
    let hwnd = crate::window::hwnd_of(&win)
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "window has no native handle"))?;
    let (px, w, h) = unsafe { blt_window(hwnd)? };
    finish(px, w, h, out)
}

/// 元素域:定位链解析 → bounds 即物理(PMv2 线程内,spike 实证)→ 直接裁屏幕
pub fn screenshot_element(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    out: Option<&str>,
) -> Result<ShotResult, CtlError> {
    let _dpi = Pmv2Guard::enter();
    let loc = locate(app, target, near)?;
    let r = loc
        .element
        .get_bounding_rectangle()
        .map_err(|e| CtlError::internal(format!("element bounds: {e}")))?;
    let (x, y, w, h) = (
        r.get_left(),
        r.get_top(),
        r.get_right() - r.get_left(),
        r.get_bottom() - r.get_top(),
    );
    if w <= 0 || h <= 0 {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} has an empty bounding rectangle (virtualized/offscreen?)",
                target.describe()
            ),
        ));
    }
    let (px, w, h) = unsafe { blt_region(x, y, w, h)? };
    finish(px, w, h, out)
}

/// 供窗口域取原生句柄(resolve_window 返回 UIElement)
pub(crate) fn _unused(_: &UIElement) {}
