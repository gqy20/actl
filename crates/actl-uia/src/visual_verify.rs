//! verify-visual:像素对比断言(verify 链的视觉末端)。
//! 两形态:--a/--b 双路径(两张已有截图,验收脚本对照历史证据)与
//! --baseline+现拍(窗口当前状态 vs 基准图);断言方向 --changed/--unchanged
//! 必选其一。判据 = mean_abs_diff 与 changed_ratio 双通道(阈值 0 = 该通道
//! 不判);变化区域 = 变化胞 8 连通域(阅读序 bbox,坐标换算屏幕物理)。
//! 只读:零输入注入、零授权;读基线通道(read_only 分类入 flow 观察步)。
//! 算法核在 actl-core::visual_diff(合成图单测);本模块只做 PNG 解码、
//! 窗口捕获、坐标换算与断言判定的胶合。
use crate::find_image::{capture_window, decode_png_file_path};
use actl_core::template_match::{self, Gray};
use actl_core::visual_diff;
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::path::Path;

pub struct VerifyVisualSpec<'a> {
    /// 双路径形态:截图 A(与 --b 成对;与 --baseline 互斥)
    pub a: Option<&'a str>,
    pub b: Option<&'a str>,
    /// 现拍形态:基准 PNG(与 --a/--b 互斥;须配 --app/--rect 锚定现拍面)
    pub baseline: Option<&'a str>,
    /// 现拍窗口锚(标题子串或 hwnd:<n>;--baseline 形态必填)
    pub app: Option<&'a str>,
    /// 现拍屏幕子区 <x>,<y>,<w>,<h>(物理像素;省略 = 整窗)
    pub rect: Option<&'a str>,
    /// 断言方向:changed = 断言有变化;unchanged = 断言无变化(互斥,必选一)
    pub changed: bool,
    pub unchanged: bool,
    /// mean_abs_diff 阈值:0(默认)= 不判该通道;>0 时按方向比较
    pub diff_threshold: Option<f64>,
    /// changed_ratio 阈值:0(默认)= 不判该通道;>0 时按方向比较
    pub ratio_threshold: f64,
}

pub fn verify_visual(spec: VerifyVisualSpec<'_>) -> Result<Value, CtlError> {
    if spec.changed == spec.unchanged {
        return Err(CtlError::protocol(
            "verify-visual requires exactly one of --changed / --unchanged",
        ));
    }
    let diff_threshold = match spec.diff_threshold {
        Some(t) if !(0.0..=255.0).contains(&t) => {
            return Err(CtlError::protocol(format!(
                "verify-visual --diff-threshold must be 0..=255 (0 = don't judge this channel), got {t}"
            )));
        }
        t => t.unwrap_or(0.0),
    };
    if !(0.0..=1.0).contains(&spec.ratio_threshold) {
        return Err(CtlError::protocol(format!(
            "verify-visual --ratio-threshold must be 0..=1 (0 = don't judge this channel), got {}",
            spec.ratio_threshold
        )));
    }
    if diff_threshold == 0.0 && spec.ratio_threshold == 0.0 {
        return Err(CtlError::protocol(
            "verify-visual needs at least one judging channel: --diff-threshold > 0 or --ratio-threshold > 0",
        ));
    }
    // 形态互斥:--a/--b 成对;--baseline 独占(现拍)。app 前置校验在
    // 文件读取之前(缺锚的 PROTOCOL 不依赖文件存在性)
    if spec.baseline.is_some() && spec.app.is_none() {
        return Err(CtlError::protocol(
            "verify-visual --baseline requires --app (or --rect with --app) for the live capture",
        ));
    }
    let (gray_a, gray_b, window, mode) = match (spec.a, spec.b, spec.baseline) {
        (Some(a), Some(b), None) => {
            let ga = load_gray(a)?;
            let gb = load_gray(b)?;
            (ga, gb, None, "two-files")
        }
        (None, None, Some(base)) => {
            let gb = load_gray(base)?;
            let (ga, win) = capture_gray(spec.app, spec.rect)?;
            (ga, gb, Some(win), "live-vs-baseline")
        }
        _ => {
            return Err(CtlError::protocol(
                "verify-visual: pass --a + --b (two files), or --baseline (+ --app for the live capture)",
            ));
        }
    };

    let Some(d) = visual_diff::diff(&gray_a, &gray_b) else {
        return Err(CtlError::with_evidence(
            ErrorCode::Protocol,
            format!(
                "frames differ in size: {}x{} vs {}x{} — same source and coordinate system required",
                gray_a.w, gray_a.h, gray_b.w, gray_b.h
            ),
            json!({"a": {"width": gray_a.w, "height": gray_a.h}, "b": {"width": gray_b.w, "height": gray_b.h}}),
        ));
    };
    // 现拍形态:变化区域坐标 = 基准面本地 → 屏幕物理(baseline 是捕获面坐标系)
    let (ox, oy) = window
        .as_ref()
        .map(|w| (w.origin_x, w.origin_y))
        .unwrap_or((0, 0));
    let regions: Vec<Value> = d
        .regions
        .iter()
        .map(|r| {
            json!({
                "x": ox + r.x, "y": oy + r.y, "w": r.w, "h": r.h,
                "center": [ox + r.x + r.w / 2, oy + r.y + r.h / 2],
                "density": r.density, "cells": r.cells,
            })
        })
        .collect();

    // 判定:断言方向 × 双通道(阈值 0 = 跳过)。任一受判通道违背方向 = 失败。
    let wants_change = spec.changed;
    let mut failures: Vec<String> = Vec::new();
    if diff_threshold > 0.0 {
        let ok = if wants_change {
            d.mean_abs_diff >= diff_threshold
        } else {
            d.mean_abs_diff <= diff_threshold
        };
        if !ok {
            failures.push(format!(
                "mean_abs_diff {:.2} vs threshold {diff_threshold:.2} (expected {})",
                d.mean_abs_diff,
                if wants_change { ">=" } else { "<=" }
            ));
        }
    }
    if spec.ratio_threshold > 0.0 {
        let ok = if wants_change {
            d.changed_ratio >= spec.ratio_threshold
        } else {
            d.changed_ratio <= spec.ratio_threshold
        };
        if !ok {
            failures.push(format!(
                "changed_ratio {:.4} vs threshold {:.4} (expected {})",
                d.changed_ratio,
                spec.ratio_threshold,
                if wants_change { ">=" } else { "<=" }
            ));
        }
    }
    // 无阈值通道时兜底语义:changed 断言须 regions 非空,unchanged 须空
    if diff_threshold == 0.0 && spec.ratio_threshold == 0.0 {
        unreachable!("guarded above");
    }
    let base = json!({
        "action": "verify-visual",
        "mode": mode,
        "assertion": if wants_change { "changed" } else { "unchanged" },
        "mean_abs_diff": (d.mean_abs_diff * 1000.0).round() / 1000.0,
        "changed_ratio": (d.changed_ratio * 10000.0).round() / 10000.0,
        "cells_total": d.cells_total,
        "samples": d.samples,
        "cell_diff_threshold": visual_diff::CELL_DIFF_THRESHOLD,
        "diff_threshold": diff_threshold,
        "ratio_threshold": spec.ratio_threshold,
        "changed_regions": regions,
        "resolved_by": "pixel-diff",
    });
    if failures.is_empty() {
        Ok(base)
    } else {
        Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            format!("visual assertion failed: {}", failures.join("; ")),
            base,
        ))
    }
}

/// 读 PNG 文件为灰度面(decode_png_file 是 uia 的安全封装,经 find_image 暴露)。
fn load_gray(path: &str) -> Result<Gray, CtlError> {
    let (bgra, w, h) = decode_png_file_path(Path::new(path))?;
    template_match::to_gray(&bgra, w as usize, h as usize)
        .ok_or_else(|| CtlError::internal("decoded pixels shorter than declared size"))
}

/// 现拍:窗口捕获(PrintWindow 抗遮挡,PMv2 物理)→ 灰度;--rect 求交裁剪。
/// 返回 (灰度, 窗口元数据);裁剪面的坐标原点在 to_json 换算处补回。
fn capture_gray(app: Option<&str>, rect: Option<&str>) -> Result<(Gray, CaptureMeta), CtlError> {
    // app 前置校验在文件读取前完成(verify_visual 顶部),此处只走捕获
    let _dpi = crate::capture::Pmv2Guard::enter();
    let win = capture_window(app)?;
    let meta = CaptureMeta {
        origin_x: win.origin_x,
        origin_y: win.origin_y,
    };
    let gray = template_match::to_gray(&win.bgra, win.w as usize, win.h as usize)
        .ok_or_else(|| CtlError::internal("window pixels shorter than declared size"))?;
    match rect {
        None => Ok((gray, meta)),
        Some(raw) => {
            let nums: Vec<i32> = raw
                .split(',')
                .map(|s| s.trim().parse::<i32>())
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    CtlError::protocol(format!(
                        "verify-visual --rect expects X,Y,W,H physical pixels, got {raw:?}"
                    ))
                })?;
            if nums.len() != 4 || nums[2] <= 0 || nums[3] <= 0 {
                return Err(CtlError::protocol(format!(
                    "verify-visual --rect expects X,Y,W,H with positive W,H, got {raw:?}"
                )));
            }
            let x0 = (nums[0] - win.origin_x).max(0).min(win.w);
            let y0 = (nums[1] - win.origin_y).max(0).min(win.h);
            let x1 = (nums[0] + nums[2] - win.origin_x).max(0).min(win.w);
            let y1 = (nums[1] + nums[3] - win.origin_y).max(0).min(win.h);
            if x1 <= x0 || y1 <= y0 {
                return Err(CtlError::with_evidence(
                    ErrorCode::Protocol,
                    "verify-visual --rect does not intersect the window surface".to_string(),
                    json!({"rect": raw, "window": {"origin": [win.origin_x, win.origin_y], "width": win.w, "height": win.h}}),
                ));
            }
            let cropped = crop_gray(&gray, x0, y0, x1, y1);
            // 裁剪面原点 = rect 左上角(与 image-regions 同换算纪律)
            Ok((
                cropped,
                CaptureMeta {
                    origin_x: nums[0],
                    origin_y: nums[1],
                },
            ))
        }
    }
}

struct CaptureMeta {
    origin_x: i32,
    origin_y: i32,
}

fn crop_gray(g: &Gray, x0: i32, y0: i32, x1: i32, y1: i32) -> Gray {
    let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
    let w = x1 - x0;
    let h = y1 - y0;
    let mut data = Vec::with_capacity(w * h);
    for y in y0..(y0 + h) {
        data.extend_from_slice(&g.data[y * g.w + x0..y * g.w + x0 + w]);
    }
    Gray { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(changed: bool, unchanged: bool) -> VerifyVisualSpec<'static> {
        VerifyVisualSpec {
            a: None,
            b: None,
            baseline: None,
            app: None,
            rect: None,
            changed,
            unchanged,
            diff_threshold: None,
            ratio_threshold: 0.0,
        }
    }

    #[test]
    fn argument_gates() {
        // 方向互斥:双选/双空都 PROTOCOL
        assert_eq!(
            verify_visual(spec(true, true)).unwrap_err().code,
            ErrorCode::Protocol
        );
        assert_eq!(
            verify_visual(spec(false, false)).unwrap_err().code,
            ErrorCode::Protocol
        );
        // 无判定通道 PROTOCOL
        let mut s = spec(true, false);
        s.diff_threshold = Some(0.0);
        assert_eq!(verify_visual(s).unwrap_err().code, ErrorCode::Protocol);
        // 形态互斥:a 无 b / baseline 无 app → PROTOCOL
        let mut s = spec(true, false);
        s.diff_threshold = Some(5.0);
        s.a = Some("x.png");
        assert_eq!(verify_visual(s).unwrap_err().code, ErrorCode::Protocol);
        let mut s = spec(true, false);
        s.diff_threshold = Some(5.0);
        s.baseline = Some("x.png");
        assert_eq!(verify_visual(s).unwrap_err().code, ErrorCode::Protocol);
        // 阈值越界 PROTOCOL
        let mut s = spec(true, false);
        s.diff_threshold = Some(300.0);
        assert_eq!(verify_visual(s).unwrap_err().code, ErrorCode::Protocol);
        let mut s = spec(true, false);
        s.diff_threshold = Some(5.0);
        s.ratio_threshold = 1.5;
        assert_eq!(verify_visual(s).unwrap_err().code, ErrorCode::Protocol);
    }
}
