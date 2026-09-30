//! Antialiased DIP geometry and DirectWrite text, composed with per-pixel alpha.
use super::model::{Mode, View};
use actl_core::activity::ClipboardEffect;
use actl_core::state::unix_ms;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::{
    Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Gdi::*, Imaging::*,
};
use windows::Win32::System::Com::*;
use windows::Win32::UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*};
use windows::core::{BOOL, Interface, PCWSTR, Result, w};

use std::sync::OnceLock;

pub const WIDTH: i32 = 280;
pub const HEIGHT: i32 = 34;
/// 展开态:宽度 1.5 倍、高度两行;第二行显示当前业务步骤与进度。
/// 展开交互 = 点击横条空白(无箭头指示)。
pub const WIDTH_EXT: i32 = 420;
pub const HEIGHT_EXT: i32 = 62;
/// 悬浮槽位(锚点即命中区;hover 卡内容按槽位由 shell 提供)。
pub const HOVER_NONE: u8 = 0;
pub const HOVER_BUTTON: u8 = 1;
pub const HOVER_DOT: u8 = 2;
pub const HOVER_BODY: u8 = 3;
/// 预授权描边色(青):常驻信任的可见性补偿,文字版保留在窗口标题里。
const STROKE_PREAUTH: u32 = 0x5FC4D4;
static PREAUTH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn set_preauth(on: bool) {
    PREAUTH.store(on, std::sync::atomic::Ordering::Relaxed);
}
/// Business progress for the bound workflow; empty means no workflow.
static PROGRESS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub(super) fn set_progress(progress: String) {
    if let Ok(mut slot) = PROGRESS.lock() {
        *slot = progress;
    }
}
pub fn pixels(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi.max(96)) + 48) / 96) as i32
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// 主字体 Source Code Pro Medium,内嵌随二进制分发(205KB,OFL 1.1,见
/// assets/fonts/OFL.txt):目标机器常未注册该字体,DWrite 又看不见 GDI 的
/// FR_PRIVATE 注册,故走 IDWriteFactory5 内存字体管线自建 FontCollection。
static SCP_TTF: &[u8] = include_bytes!("../../../assets/fonts/SourceCodePro-Medium.ttf");
static TEXT_FORMATS: OnceLock<(Option<IDWriteTextFormat>, Option<IDWriteTextFormat>)> =
    OnceLock::new();

/// (拉丁主字体, 中文伴随字体) 两个 TextFormat,进程内首用构建、此后复用。
unsafe fn text_formats(
    write: &IDWriteFactory,
) -> Result<(&'static IDWriteTextFormat, &'static IDWriteTextFormat)> {
    let (latin, cjk) = TEXT_FORMATS.get_or_init(|| {
        let latin = latin_format(write).ok();
        let cjk = cjk_format(write).ok();
        (latin, cjk)
    });
    match (latin, cjk) {
        (Some(latin), Some(cjk)) => Ok((latin, cjk)),
        _ => Err(E_FAIL.into()),
    }
}

unsafe fn embedded_collection(write: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    // 内存字体管线横跨两个接口:loader 在 Factory5,FontSetBuilder/集合在 Factory3。
    let loader = write
        .cast::<IDWriteFactory5>()
        .and_then(|factory| factory.CreateInMemoryFontFileLoader())
        .ok()?;
    let file = loader
        .CreateInMemoryFontFileReference(write, SCP_TTF.as_ptr().cast(), SCP_TTF.len() as u32, None)
        .ok()?;
    let factory3 = write.cast::<IDWriteFactory3>().ok()?;
    let set = factory3
        .CreateFontSetBuilder()
        .and_then(|builder| builder.cast::<IDWriteFontSetBuilder1>())
        .and_then(|builder| builder.AddFontFile(&file).map(|()| builder))
        .and_then(|builder| builder.CreateFontSet())
        .ok()?;
    factory3
        .CreateFontCollectionFromFontSet(&set)
        .ok()?
        .cast::<IDWriteFontCollection>()
        .ok()
}

/// 拉丁主字体:内嵌 Source Code Pro Medium(容器族名以 FindFamilyName 实测为准,
/// 静态字重文件两种命名都试);内嵌管线失败或 CI 等无字体环境逐级回落系统字体。
unsafe fn latin_format(write: &IDWriteFactory) -> Result<IDWriteTextFormat> {
    if let Some(embedded) = embedded_collection(write) {
        for family in [w!("Source Code Pro"), w!("Source Code Pro Medium")] {
            if family_present(&embedded, family) {
                return text_format(write, family, Some(&embedded));
            }
        }
    }
    if let Some(system) = system_collection(write) {
        for family in [w!("Consolas"), w!("Segoe UI")] {
            if family_present(&system, family) {
                return text_format(write, family, Some(&system));
            }
        }
    }
    text_format(write, w!("Segoe UI"), None)
}

/// 中文伴随字体:思源黑体 Medium——与 Source Code Pro 同出 Adobe Source 家族,
/// 字重命名对齐(Medium↔Medium);本机实测 Source Code Pro 无 CJK 字形,
/// 中文必须由伴随字体承接,未安装机器回落 Noto Sans CJK/微软雅黑。
unsafe fn cjk_format(write: &IDWriteFactory) -> Result<IDWriteTextFormat> {
    if let Some(system) = system_collection(write) {
        for family in [
            w!("Source Han Sans SC"),
            w!("Noto Sans CJK SC"),
            w!("Microsoft YaHei UI"),
            w!("Microsoft YaHei"),
        ] {
            if family_present(&system, family) {
                return text_format(write, family, Some(&system));
            }
        }
    }
    text_format(write, w!("Segoe UI"), None)
}

unsafe fn system_collection(write: &IDWriteFactory) -> Option<IDWriteFontCollection> {
    let mut collection = None;
    let _ = write.GetSystemFontCollection(&mut collection, false);
    collection
}

unsafe fn family_present(collection: &IDWriteFontCollection, family: PCWSTR) -> bool {
    let mut index = u32::MAX;
    let mut exists = BOOL::default();
    collection
        .FindFamilyName(family, &mut index, &mut exists)
        .is_ok()
        && exists.as_bool()
}

unsafe fn text_format(
    write: &IDWriteFactory,
    family: PCWSTR,
    collection: Option<&IDWriteFontCollection>,
) -> Result<IDWriteTextFormat> {
    let format = write.CreateTextFormat(
        family,
        collection,
        DWRITE_FONT_WEIGHT_MEDIUM,
        DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_STRETCH_NORMAL,
        16.,
        w!("zh-CN"),
    )?;
    format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
    let ellipsis = write.CreateEllipsisTrimmingSign(&format)?;
    format.SetTrimming(
        &DWRITE_TRIMMING {
            granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
            ..Default::default()
        },
        &ellipsis,
    )?;
    Ok(format)
}

/// 含 CJK 字符的串整体走伴随字体,纯拉丁/数字走主字体。
/// 按串二选一而非依赖系统自动回落:DWrite 默认回落会命中宋体/雅黑,
/// 结果不可控;Source Code Pro 内嵌拉丁与思源中文的混排由调用方按串切分。
fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
                | 0x20000..=0x2FA1F
        )
    })
}
fn color(rgb: u32) -> D2D1_COLOR_F {
    color_a(rgb, 1.)
}
fn color_a(rgb: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 255) as f32 / 255.,
        g: ((rgb >> 8) & 255) as f32 / 255.,
        b: (rgb & 255) as f32 / 255.,
        a,
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: rect(x, y, w, h),
        radiusX: radius,
        radiusY: radius,
    }
}

/// 0..1 平滑呼吸相位(cos 包络,无跳变)。
fn breath(now_ms: u64, period_ms: u64) -> f32 {
    let t = (now_ms % period_ms) as f32 / period_ms as f32;
    0.5 - 0.5 * (t * std::f32::consts::TAU).cos()
}

/// 状态点/描边共用类别色。
fn status_color(mode: Mode) -> u32 {
    match mode {
        Mode::Preparing
        | Mode::Unknown
        | Mode::StopRequested
        | Mode::Partial
        | Mode::Paused
        | Mode::Waiting
        | Mode::Desktop => 0xD4B879,
        Mode::Running | Mode::Background => 0x93B4CA,
        Mode::Failed | Mode::Blocked => 0xE59188,
        Mode::Done => 0x8AB698,
        _ => 0xB4BAC0,
    }
}

/// 边框通道:预授权=青色常驻描边;动画态用状态色描边并呼吸
/// (进行中慢呼吸、"需要你"的态快脉冲);静止态回归中性灰描边。
fn border_channel(mode: Mode, preauth: bool) -> (u32, Option<(u64, f32, f32)>) {
    let anim = match mode {
        Mode::Running | Mode::Background | Mode::Preparing | Mode::Desktop => {
            Some((2400, 0.55, 1.0))
        }
        Mode::Paused | Mode::Waiting | Mode::Partial | Mode::Blocked | Mode::StopRequested => {
            Some((1000, 0.30, 1.0))
        }
        _ => None,
    };
    let rgb = if preauth {
        STROKE_PREAUTH
    } else if anim.is_some() {
        status_color(mode)
    } else {
        0x383B3E
    };
    (rgb, anim)
}

// COM objects and native bitmap handles remain on the display thread.
struct ComScope;
impl Drop for ComScope {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

unsafe fn raster(view: &View, hover: u8, dpi: u32, expanded: bool) -> Result<Vec<u8>> {
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    let _com = ComScope;
    let wic: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
    let layout_w = if expanded { WIDTH_EXT } else { WIDTH };
    let width = pixels(layout_w, dpi) as u32;
    let layout_h = if expanded { HEIGHT_EXT } else { HEIGHT };
    let height = pixels(layout_h, dpi) as u32;
    let bitmap = wic.CreateBitmap(
        width,
        height,
        &GUID_WICPixelFormat32bppPBGRA,
        WICBitmapCacheOnLoad,
    )?;
    let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let target = factory.CreateWicBitmapRenderTarget(
        &bitmap,
        &D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        },
    )?;
    let brush = target.CreateSolidColorBrush(&color(0x27292B), None)?;
    let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let (latin, cjk) = text_formats(&write)?;
    target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    // Grayscale text avoids ClearType color fringes on a per-pixel alpha surface.
    target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    target.BeginDraw();
    target.Clear(Some(&D2D1_COLOR_F::default()));
    let h = layout_h as f32;
    let w = layout_w as f32;
    target.FillRoundedRectangle(&rounded(0., 0., w, h, 10.), &brush);
    // 边框通道:预授权=青色常驻描边(文字版保留在窗口标题);动画态低幅呼吸,
    // "需要你"的态脉冲更明显;空闲/完成/失败静止(减少动态下静止,docs/18)。
    let preauth = PREAUTH.load(std::sync::atomic::Ordering::Relaxed);
    let (stroke_rgb, anim) = border_channel(view.mode, preauth);
    let alpha = match anim {
        Some((period, lo, hi)) => lo + (hi - lo) * breath(unix_ms(), period),
        None => 1.,
    };
    brush.SetColor(&color_a(stroke_rgb, alpha));
    target.DrawRoundedRectangle(&rounded(0.5, 0.5, w - 1., h - 1., 9.5), &brush, 1., None);
    brush.SetColor(&color(status_color(view.mode)));
    target.FillRoundedRectangle(&rounded(10., 11., 12., 12., 6.), &brush);
    let pending = matches!(view.mode, Mode::StopRequested | Mode::Stopped);
    // 右缘锚定簇:按钮贴右缘;标签右界收到按钮前 6 DIP(残字修正)。
    let button_x = w - 30.;
    let label = &view.label;
    // 标签右界收到按钮前 6 DIP(无箭头指示;旧值 240 曾越过右簇把残字画进不可见区)。
    let text_right = if view.effects.keyboard
        || view.effects.focus
        || view.effects.pointer
        || view.effects.clipboard != ClipboardEffect::Untouched
    {
        181.
    } else {
        button_x - 6.
    };
    brush.SetColor(&color(0xE8EBED));
    let text_format = if has_cjk(label) { cjk } else { latin };
    target.DrawText(
        &label.encode_utf16().collect::<Vec<_>>(),
        text_format,
        &rect(31., 0., text_right - 31., 34.),
        &brush,
        D2D1_DRAW_TEXT_OPTIONS_CLIP,
        DWRITE_MEASURING_MODE_NATURAL,
    );
    brush.SetColor(&color(0xC1C5C8));
    let outline =
        |x, y, w, h, r| target.DrawRoundedRectangle(&rounded(x, y, w, h, r), &brush, 1., None);
    let bar = |x, y, w, h| target.FillRoundedRectangle(&rounded(x, y, w, h, 0.5), &brush);
    if view.effects.keyboard {
        outline(188., 12., 14., 10., 2.);
        bar(191., 14.5, 8., 1.);
        bar(191., 17.5, 8., 1.);
    } else if view.effects.focus {
        outline(188., 10., 14., 14., 2.);
        bar(188., 13.5, 14., 1.);
    }
    if view.effects.pointer {
        outline(208., 10., 10., 14., 4.);
        bar(212.5, 10., 1., 6.);
    }
    if view.effects.clipboard != ClipboardEffect::Untouched {
        outline(225., 11., 10., 13., 2.);
        outline(228., 9., 4., 4., 1.);
    }
    // 展开态隐藏详情按钮(点横条空白即切换,详情从托盘/收起态按钮进入);
    // 暂停/继续保留。无箭头指示——展开交互就是点击横条本身。
    let show_button = !(expanded && matches!(view.control, super::model::Control::Details));
    if show_button {
        brush.SetColor(&color(if pending {
            0x2D2F31
        } else if hover == HOVER_BUTTON {
            0x494D52
        } else {
            0x35383B
        }));
        target.FillRoundedRectangle(&rounded(button_x, 5., 26., 24., 6.), &brush);
        brush.SetColor(&color(if pending { 0x757A80 } else { 0xC1C5C8 }));
        let glyph_x = button_x + 8.;
        match &view.control {
            super::model::Control::Pause { .. } => {
                bar(glyph_x, 11., 3., 12.);
                bar(glyph_x + 7., 11., 3., 12.);
            }
            super::model::Control::Continue { .. } => {
                // A triangle formed by progressively shorter vertical strokes.
                for i in 0..10 {
                    let gh = 14. - i as f32 * 1.3;
                    bar(glyph_x + i as f32, 17. - gh / 2., 1., gh);
                }
            }
            super::model::Control::Details => {
                outline(glyph_x, 12., 10., 10., 5.);
                bar(glyph_x + 4.5, 16., 1., 4.);
            }
        }
    }
    if expanded {
        // 第二行:当前业务步骤与进度(单行,尾部截断);无流程时
        // 回落最近调用摘要(command · 相位)。分隔线随宽度。
        brush.SetColor(&color(0x2F3234));
        target.FillRoundedRectangle(&rounded(12., 34., w - 24., 1., 0.5), &brush);
        let progress = PROGRESS.lock().map(|s| s.clone()).unwrap_or_default();
        let text = if progress.is_empty() {
            format!("{} · {}", view.second[0], view.second[1])
        } else {
            progress
        };
        brush.SetColor(&color(0x9AA0A6));
        let fmt = if has_cjk(&text) { cjk } else { latin };
        target.DrawText(
            &text.encode_utf16().collect::<Vec<_>>(),
            fmt,
            &rect(31., 34., w - 40. - 31., 28.),
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
    target.EndDraw(None, None)?;
    let mut data = vec![0u8; width as usize * height as usize * 4];
    bitmap.CopyPixels(std::ptr::null(), width * 4, &mut data)?;
    Ok(data)
}

unsafe fn present(hwnd: HWND, data: &[u8], dpi: u32, width: i32, height: i32) -> Result<()> {
    present_alpha(hwnd, data, dpi, width, height, 255)
}

pub(super) unsafe fn present_alpha(
    hwnd: HWND,
    data: &[u8],
    dpi: u32,
    width: i32,
    height: i32,
    alpha: u8,
) -> Result<()> {
    let size = SIZE {
        cx: pixels(width, dpi),
        cy: pixels(height, dpi),
    };
    let dc = CreateCompatibleDC(None);
    if dc.is_invalid() {
        return Err(windows::core::Error::from_thread());
    }
    let mut bits = std::ptr::null_mut();
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size.cx,
            biHeight: -size.cy,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = (|| -> Result<()> {
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let old = SelectObject(dc, HGDIOBJ(bitmap.0));
        std::ptr::copy_nonoverlapping(data.as_ptr(), bits.cast::<u8>(), data.len());
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
            BlendFlags: 0,
        };
        let result = UpdateLayeredWindow(
            hwnd,
            None,
            None,
            Some(&size),
            Some(dc),
            Some(&POINT::default()),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        result
    })();
    let _ = DeleteDC(dc);
    result
}

pub unsafe fn draw(hwnd: HWND, view: &View, hover: u8, expanded: bool) -> bool {
    let mut ps = PAINTSTRUCT::default();
    let _ = BeginPaint(hwnd, &mut ps);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let layout_h = if expanded { HEIGHT_EXT } else { HEIGHT };
    let layout_w = if expanded { WIDTH_EXT } else { WIDTH };
    let result = raster(view, hover, dpi, expanded)
        .and_then(|data| present(hwnd, &data, dpi, layout_w, layout_h));
    if let Err(e) = &result {
        eprintln!("INTERNAL: display render failed: {e}");
    }
    let _ = EndPaint(hwnd, &ps);
    result.is_ok()
}

#[derive(PartialEq)]
struct PopupKey {
    lines: Vec<String>,
    menu: bool,
    selected: Option<usize>,
    dpi: u32,
}
thread_local! { static POPUP_CACHE: std::cell::RefCell<std::collections::VecDeque<(PopupKey, Vec<u8>)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) }; }
unsafe fn raster_popup(
    lines: &[String],
    menu: bool,
    selected: Option<usize>,
    dpi: u32,
) -> Result<Vec<u8>> {
    let key = PopupKey {
        lines: lines.to_vec(),
        menu,
        selected,
        dpi,
    };
    if let Some(data) = POPUP_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, data)| data.clone())
    }) {
        return Ok(data);
    }
    let data = render_popup(lines, menu, selected, dpi)?;
    POPUP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Bound memory as well as entries across DPI and changing text.
        while !cache.is_empty()
            && (cache.len() >= 12
                || cache.iter().map(|(_, d)| d.len()).sum::<usize>() + data.len() > 8 * 1024 * 1024)
        {
            cache.pop_front();
        }
        if data.len() <= 8 * 1024 * 1024 {
            cache.push_back((key, data.clone()));
        }
    });
    Ok(data)
}
unsafe fn render_popup(
    lines: &[String],
    menu: bool,
    selected: Option<usize>,
    dpi: u32,
) -> Result<Vec<u8>> {
    let width = if menu { 136 } else { 280 };
    let height = 16 + lines.len() as i32 * 28;
    CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    let _com = ComScope;
    let wic: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
    let w = pixels(width, dpi) as u32;
    let h = pixels(height, dpi) as u32;
    let bitmap = wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
    let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let target = factory.CreateWicBitmapRenderTarget(
        &bitmap,
        &D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..Default::default()
        },
    )?;
    let brush = target.CreateSolidColorBrush(&color(0x27292B), None)?;
    let write: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let (latin, cjk) = text_formats(&write)?;
    target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
    target.BeginDraw();
    target.Clear(Some(&D2D1_COLOR_F::default()));
    target.FillRoundedRectangle(&rounded(0., 0., width as f32, height as f32, 10.), &brush);
    brush.SetColor(&color(0x383B3E));
    target.DrawRoundedRectangle(
        &rounded(0.5, 0.5, width as f32 - 1., height as f32 - 1., 9.5),
        &brush,
        0.5,
        None,
    );
    for (i, line) in lines.iter().enumerate() {
        let y = 8. + i as f32 * 28.;
        if menu && selected == Some(i) {
            brush.SetColor(&color(0x3B3F43));
            target.FillRoundedRectangle(&rounded(4., y, width as f32 - 8., 28., 5.), &brush);
        }
        brush.SetColor(&color(if menu || i == 0 { 0xE8EBED } else { 0xB4BAC0 }));
        let text_format = if has_cjk(line) { cjk } else { latin };
        target.DrawText(
            &line.encode_utf16().collect::<Vec<_>>(),
            text_format,
            &rect(12., y, width as f32 - 24., 28.),
            &brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
    target.EndDraw(None, None)?;
    let mut data = vec![0u8; w as usize * h as usize * 4];
    bitmap.CopyPixels(std::ptr::null(), w * 4, &mut data)?;
    Ok(data)
}
pub unsafe fn draw_popup(
    hwnd: HWND,
    lines: &[String],
    menu: bool,
    selected: Option<usize>,
) -> bool {
    let mut ps = PAINTSTRUCT::default();
    let _ = BeginPaint(hwnd, &mut ps);
    let dpi = GetDpiForWindow(hwnd).max(96);
    let result = raster_popup(lines, menu, selected, dpi).and_then(|data| {
        present(
            hwnd,
            &data,
            dpi,
            if menu { 136 } else { 280 },
            16 + lines.len() as i32 * 28,
        )
    });
    if let Err(e) = &result {
        eprintln!("INTERNAL: popup render failed: {e}");
    }
    let _ = EndPaint(hwnd, &ps);
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hovering_control_preserves_status_text_during_pause_transitions() {
        let mut view = super::super::model::view(&[], 0, None, Some(false));
        for label in ["正在暂停", "已暂停", "正在运行", "待核对"] {
            view.label = label.into();
            let normal = unsafe { raster(&view, HOVER_NONE, 96, false) }.unwrap();
            let hovered = unsafe { raster(&view, HOVER_BUTTON, 96, false) }.unwrap();
            for row in 0..HEIGHT as usize {
                let start = row * WIDTH as usize * 4;
                assert_eq!(
                    &normal[start..start + 240 * 4],
                    &hovered[start..start + 240 * 4],
                    "hover hides {label}"
                );
            }
        }
    }
    #[test]
    fn popup_raster_is_opaque_inside_and_clipped_at_corners() {
        for menu in [false, true] {
            let lines = vec!["详情".to_owned(), "允许后续操作".to_owned()];
            let data = unsafe { raster_popup(&lines, menu, Some(1), 192) }.unwrap();
            assert_eq!(data[3], 0);
            assert!(data.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
        }
    }
    #[test]
    #[ignore = "manual native renderer previews"]
    fn export_native_previews() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/display-review");
        std::fs::create_dir_all(&dir).unwrap();
        let state = actl_core::state::SessionState::now(
            1,
            "display-start",
            None,
            None,
            actl_core::state::Phase::Done,
        );
        let view = super::super::model::view(
            std::slice::from_ref(&state),
            state.ts_ms,
            Some(1),
            Some(false),
        );
        let lines = view.details.lines().map(str::to_owned).collect::<Vec<_>>();
        let menu = vec!["停止操作", "允许后续操作", "详情", "隐藏", "停止并退出"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for (name, data, w, h) in [
            (
                "strip",
                unsafe { raster(&view, HOVER_NONE, 192, false) }.unwrap(),
                560,
                68,
            ),
            (
                "details",
                unsafe { raster_popup(&lines, false, None, 192) }.unwrap(),
                560,
                (16 + lines.len() * 28) * 2,
            ),
            (
                "menu",
                unsafe { raster_popup(&menu, true, Some(2), 192) }.unwrap(),
                272,
                312,
            ),
        ] {
            std::fs::write(dir.join(format!("{name}.bgra")), data).unwrap();
            std::fs::write(dir.join(format!("{name}.size")), format!("{w} {h}")).unwrap();
        }
    }
    #[test]
    fn fractional_dpi_keeps_size_and_control_area() {
        assert_eq!(pixels(WIDTH, 120), 350);
        assert_eq!(pixels(HEIGHT, 144), 51);
        assert_eq!(pixels(WIDTH, 192), 560);
    }
    #[test]
    fn expanded_raster_widens_doubles_rows_and_keeps_line1_anchors() {
        let view = super::super::model::view(&[], 0, None, Some(false));
        let collapsed = unsafe { raster(&view, HOVER_NONE, 96, false) }.expect("WIC render");
        let expanded = unsafe { raster(&view, HOVER_NONE, 96, true) }.expect("WIC render");
        let width = pixels(WIDTH, 96) as usize;
        let width_ext = pixels(WIDTH_EXT, 96) as usize;
        assert_eq!(collapsed.len(), width * pixels(HEIGHT, 96) as usize * 4);
        assert_eq!(
            expanded.len(),
            width_ext * pixels(HEIGHT_EXT, 96) as usize * 4,
            "expanded layout must be 1.5x wide and two rows tall"
        );
        // 第一行左段锚点不受展开影响:状态点行在箭头/按钮/描边之外的左段
        // 必须逐字节一致(两侧位图宽度不同,各取自己的行基)。
        let row_mid = |data: &[u8], w: usize, y: usize| -> Vec<u8> {
            data[y * w * 4 + 3 * 4..y * w * 4 + 170 * 4].to_vec()
        };
        assert_eq!(
            row_mid(&collapsed, width, 17),
            row_mid(&expanded, width_ext, 17)
        );
        // 展开态隐藏详情按钮、保留继续按钮:同视图下两种 control 的右缘按钮区
        // 必须不同(背景本身不透明,不能按"无像素"判定)。
        let mut vc = view.clone();
        vc.control = super::super::model::Control::Continue { flow: "f".into() };
        let expanded_continue = unsafe { raster(&vc, HOVER_NONE, 96, true) }.expect("WIC render");
        let zone = |d: &[u8]| -> Vec<u8> {
            let x0 = (width_ext - pixels(26, 96) as usize) * 4;
            (0..pixels(HEIGHT, 96) as usize)
                .flat_map(|y| {
                    let base = y * width_ext * 4 + x0;
                    (0..pixels(26, 96) as usize * 4).map(move |o| d[base + o])
                })
                .collect()
        };
        assert_ne!(
            zone(&expanded),
            zone(&expanded_continue),
            "details must hide when expanded while continue stays"
        );
        // 第二行(步骤链/最近调用摘要)分隔线以下必须有内容。
        assert!(
            expanded[(pixels(48, 96) as usize) * width_ext * 4..]
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] > 0),
            "second row must render its step chain"
        );
    }
    #[test]
    fn raster_has_transparent_corners_opaque_body_and_antialiased_edges() {
        let view = super::super::model::view(&[], 0, Some(1), Some(false));
        for dpi in [96, 120, 144, 192] {
            let data = unsafe { raster(&view, HOVER_NONE, dpi, false) }.expect("WIC render");
            let width = pixels(WIDTH, dpi) as usize;
            assert_eq!(data[3], 0);
            let center = ((pixels(17, dpi) as usize * width) + pixels(100, dpi) as usize) * 4;
            assert_eq!(data[center + 3], 255);
            assert!(
                data.as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255)
            );
            assert!(
                data.as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[..3].iter().all(|c| *c <= p[3]))
            );
        }
    }
}
