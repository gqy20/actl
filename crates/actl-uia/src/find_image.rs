//! find-image:确定性模板匹配定位(传统 RPA 图色定位的 actl 形态)。
//! 算法在 actl-core::template_match(纯像素算术,单测主场);本模块只做
//! 两端的原生胶合——WIC 解码模板 PNG + 窗口像素捕获(PrintWindow 抗遮挡,
//! PMv2 物理像素与 screenshot/xy: 同坐标系)+ 窗口原点换算屏幕坐标。
//! 零新依赖:PNG 解码走 WIC(与截图编码同一套 COM,actl-uia unsafe 豁免)。
//!
//! 诚实语义:score < threshold 一律 NOT_FOUND(不降级猜测);多命中不 --all
//! 时 AMBIGUOUS 附候选;均匀模板(纯色块,ZNCC 无定义)PROTOCOL 拒绝;
//! 模板大于窗口 PROTOCOL。匹配是像素相关,不理解内容——语义视觉仍是
//! 非目标(PRD §1.3 修订:排除的是 OCR/识别/VLM,确定性模板匹配除外)。
use crate::window::{WindowRef, resolve_window};
use actl_core::template_match::{self, Match};
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::path::Path;

pub struct FindImageSpec<'a> {
    /// 窗口锚(标题子串或 hwnd:<n>);None = 前台窗口
    pub app: Option<&'a str>,
    pub template: &'a str,
    /// ZNCC 阈值(0,1],默认 0.85
    pub threshold: Option<f64>,
    /// 枚举全部命中(默认唯一语义:多命中 AMBIGUOUS)
    pub all: bool,
    pub limit: Option<usize>,
    /// 多尺度搜索(0.5×–2× 六档,模板 DPI/缩放失配兜底;慢 ~6×)
    pub scales: bool,
}

/// 多尺度因子(1.0 恒在;--scales 时全跑)。
const SCALE_FACTORS: [f32; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

/// 窗口像素捕获结果(BGRA top-down + 屏幕物理原点 + 身份)。
pub(crate) struct WindowPixels {
    pub bgra: Vec<u8>,
    pub w: i32,
    pub h: i32,
    pub origin_x: i32,
    pub origin_y: i32,
    pub title: String,
    pub hwnd: isize,
}

pub fn find_image(spec: FindImageSpec<'_>) -> Result<Value, CtlError> {
    let threshold = spec.threshold.unwrap_or(0.85);
    if !(threshold > 0.0 && threshold <= 1.0) {
        return Err(CtlError::protocol(format!(
            "find-image --threshold must be in (0,1], got {threshold}"
        )));
    }
    let limit = spec.limit.unwrap_or(20);
    if !(1..=50).contains(&limit) {
        return Err(CtlError::protocol(format!(
            "find-image --limit must be 1..=50, got {limit}"
        )));
    }
    let path = Path::new(spec.template);
    let (needle_bgra, tw, th) = unsafe { decode_png(path) }.map_err(|e| {
        CtlError::with_evidence(
            ErrorCode::Internal,
            format!("template unreadable: {e}"),
            json!({"template": spec.template, "hint": "PNG required (screenshot --element/--rect produces compatible files)"}),
        )
    })?;
    if tw == 0 || th == 0 {
        return Err(CtlError::protocol("template has zero size"));
    }

    // 窗口像素:PrintWindow 抗遮挡(与 screenshot --app 同路径),PMv2 物理
    // 像素;窗口原点 GetWindowRect 同口径 → 匹配坐标 + 原点 = click xy: 可
    // 直接消费的屏幕物理坐标。
    let _dpi = crate::capture::Pmv2Guard::enter();
    let win = capture_window(spec.app)?;
    let (hw, hh) = (win.w, win.h);
    if (tw as i32) > hw || (th as i32) > hh {
        return Err(CtlError::protocol(format!(
            "template {tw}x{th} is larger than the window surface {hw}x{hh}"
        )));
    }
    let hay = template_match::to_gray(&win.bgra, hw as usize, hh as usize)
        .ok_or_else(|| CtlError::internal("window pixels shorter than declared size"))?;
    let needle = template_match::to_gray(&needle_bgra, tw as usize, th as usize)
        .ok_or_else(|| CtlError::internal("template pixels shorter than declared size"))?;
    if template_match::degenerate(&needle) {
        return Err(CtlError::protocol(
            "template is uniform (zero variance) — ZNCC is undefined for flat images; crop a distinctive region",
        ));
    }
    // 搜索范围可视化(2026-10-14 用户现场反馈):搜索期间窗口画红框,Drop 擦除;
    // best-effort(绘制失败静默跳过,不影响匹配语义)
    let _frame = crate::search_frame::SearchFrameGuard::enter(win.hwnd);

    let factors: &[f32] = if spec.scales { &SCALE_FACTORS } else { &[1.0] };
    let mut pooled: Vec<(Match, f32)> = Vec::new();
    for &f in factors {
        let scaled = if (f - 1.0).abs() < 1e-6 {
            needle.clone()
        } else {
            let (nw, nh) = (
                ((tw as f32) * f).round().max(3.0) as usize,
                ((th as f32) * f).round().max(3.0) as usize,
            );
            if nw > hay.w || nh > hay.h {
                continue; // 该尺度放不下:如实跳过(其他尺度继续)
            }
            template_match::resize(&needle, nw, nh)
        };
        for m in template_match::find(&hay, &scaled, threshold as f32) {
            pooled.push((m, f));
        }
    }
    // 跨尺度合并:同坐标空间,统一 NMS(重叠 >50% 保最高分,尺度随优胜者)
    pooled.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<(Match, f32)> = Vec::new();
    for (m, scale) in pooled {
        let overlaps = kept.iter().any(|(k, _)| {
            let ix = k.x.max(m.x)..(k.x + k.w).min(m.x + m.w);
            let iy = k.y.max(m.y)..(k.y + k.h).min(m.y + m.h);
            if ix.is_empty() || iy.is_empty() {
                return false;
            }
            let inter = (ix.end - ix.start) * (iy.end - iy.start);
            let smaller = (k.w * k.h).min(m.w * m.h);
            smaller > 0 && inter * 2 > smaller
        });
        if !overlaps {
            kept.push((m, scale));
        }
    }

    let total = kept.len();
    let to_json = |(m, scale): &(Match, f32)| {
        json!({
            "x": win.origin_x + m.x as i32,
            "y": win.origin_y + m.y as i32,
            "width": m.w,
            "height": m.h,
            "score": (m.score * 1000.0).round() / 1000.0,
            "scale": scale,
            // 中心点:click xy: 直接可消费
            "center": [win.origin_x + m.x as i32 + (m.w as i32) / 2, win.origin_y + m.y as i32 + (m.h as i32) / 2],
        })
    };
    let window = json!({"title": win.title, "hwnd": win.hwnd, "width": hw, "height": hh, "origin": [win.origin_x, win.origin_y]});
    if total == 0 {
        return Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!(
                "template not found at threshold {threshold:.2} (searched {hw}x{hh} window surface{})",
                if spec.scales { ", 6 scales" } else { "" }
            ),
            json!({"threshold": threshold, "template": {"width": tw, "height": th}, "window": window, "hint": "lower --threshold, enable --scales for DPI mismatch, or re-crop the template on this machine"}),
        ));
    }
    if total > 1 && !spec.all {
        let candidates: Vec<Value> = kept.iter().take(20).map(to_json).collect();
        return Err(CtlError::with_evidence(
            ErrorCode::Ambiguous,
            format!(
                "{total} matches at threshold {threshold:.2}; pass --all to enumerate or crop a more distinctive template"
            ),
            json!({"total": total, "candidates": candidates, "window": window}),
        ));
    }
    let items: Vec<Value> = kept.iter().take(limit).map(to_json).collect();
    Ok(json!({
        "action": "find-image",
        "total": total,
        "returned": items.len(),
        "truncated_by_limit": total > items.len(),
        "threshold": threshold,
        "scales": spec.scales,
        "template": {"width": tw, "height": th},
        "window": window,
        "matches": items,
        "resolved_by": "template-match",
    }))
}

/// 捕获窗口像素 + 客户区屏幕原点(PMv2 物理)。origin = GetWindowRect 加上
/// 边框偏移(窗宽-位图宽)/2——PrintWindow 渲染的是不含 DWM 扩展边框的
/// 窗口内容(比 GetWindowRect 小一圈),两者差即边框(真机实证 13px 系统
/// 偏移;对称近似,不对称标题栏场景残余误差 ≤ 数像素)。
pub(crate) fn capture_window(app: Option<&str>) -> Result<WindowPixels, CtlError> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
    let r = match app {
        Some(a) => match a.strip_prefix("hwnd:") {
            Some(raw) => WindowRef::Hwnd(crate::window::parse_hwnd(raw)?),
            None => WindowRef::Title(a),
        },
        None => {
            let fg = unsafe { GetForegroundWindow() };
            if fg.is_invalid() {
                return Err(CtlError::new(ErrorCode::NotFound, "no foreground window"));
            }
            WindowRef::Hwnd(fg.0 as isize)
        }
    };
    let resolved = resolve_window(r)?;
    let title = resolved
        .get_name()
        .map_err(|e| CtlError::internal(format!("window name: {e}")))?;
    let hwnd = crate::window::hwnd_of(&resolved)
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "window has no native handle"))?;
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.map_err(crate::internal)?;
    let (bgra, w, h) = unsafe { crate::capture::blt_window(hwnd) }?;
    // 边框偏移:窗宽-位图宽 → 对称除 2(标题栏场景为近似)
    let dx = (rect.right - rect.left - w) / 2;
    let dy = (rect.bottom - rect.top - h) / 2;
    Ok(WindowPixels {
        bgra,
        w,
        h,
        origin_x: rect.left + dx,
        origin_y: rect.top + dy,
        title,
        hwnd: hwnd.0 as isize,
    })
}

/// PNG 解码的安全封装(BGRA top-down + 宽高):供跨 crate 消费
/// (actl-browser 的挑战解题循环对 CDP 截图做 ZNCC);WIC COM 收敛在
/// 本 crate 的 unsafe 豁免内,对外保持安全面。
pub fn decode_png_file(path: &Path) -> Result<(Vec<u8>, u32, u32), CtlError> {
    unsafe { decode_png(path) }
}

/// WIC PNG 解码 → BGRA top-down(与 encode_png 对称;MTA 公寓纪律同规)。
unsafe fn decode_png(path: &Path) -> Result<(Vec<u8>, u32, u32), CtlError> {
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, STGM_READ,
    };
    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { windows::Win32::System::Com::CoUninitialize() };
            }
        }
    }
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _com = Com(hr.is_ok());
    if !hr.is_ok() && hr.0 != 0x80010106u32 as i32 {
        return Err(crate::internal(hr));
    }
    let wpath: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let stream = windows::Win32::UI::Shell::SHCreateStreamOnFileW(
        windows::core::PCWSTR(wpath.as_ptr()),
        STGM_READ.0,
    )
    .map_err(crate::internal)?;
    let factory: IWICImagingFactory =
        CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
            .map_err(crate::internal)?;
    let decoder = factory
        .CreateDecoderFromStream(&stream, std::ptr::null(), Default::default())
        .map_err(crate::internal)?;
    let frame = decoder.GetFrame(0).map_err(crate::internal)?;
    let converter = factory.CreateFormatConverter().map_err(crate::internal)?;
    converter
        .Initialize(
            &frame,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .map_err(crate::internal)?;
    let mut w = 0u32;
    let mut h = 0u32;
    converter.GetSize(&mut w, &mut h).map_err(crate::internal)?;
    if w == 0 || h == 0 || (w as u64) * (h as u64) > 64_000_000 {
        return Err(CtlError::protocol(format!(
            "template {w}x{h} invalid or over the 64M-pixel guard"
        )));
    }
    let stride = (w * 4) as usize;
    let mut buf = vec![0u8; stride * h as usize];
    converter
        .CopyPixels(std::ptr::null(), stride as u32, &mut buf)
        .map_err(crate::internal)?;
    Ok((buf, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_and_limit_validation() {
        let spec = |threshold: Option<f64>, limit: Option<usize>| FindImageSpec {
            app: None,
            template: "missing.png",
            threshold,
            all: false,
            limit,
            scales: false,
        };
        assert_eq!(
            find_image(spec(Some(0.0), None)).unwrap_err().code,
            ErrorCode::Protocol
        );
        assert_eq!(
            find_image(spec(Some(1.5), None)).unwrap_err().code,
            ErrorCode::Protocol
        );
        assert_eq!(
            find_image(spec(None, Some(999))).unwrap_err().code,
            ErrorCode::Protocol
        );
        // 合法参数走到模板读取才失败(missing file → Internal with evidence)
        let err = find_image(spec(Some(0.9), Some(10))).unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert!(err.message.contains("template unreadable"));
    }
}
