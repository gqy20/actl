//! image-regions:盲区几何提议器(确定性像素扫描,零语义)。
//! UIA 盲区(自绘缩略图/视频封面/头像)的像素仍在窗口表面——对捕获面跑
//! actl-core::image_regions 的方差网格+连通域,输出候选矩形(屏幕物理坐标,
//! click xy: 直接可消费)+ density + motion 双帧差标记。算法核在 core
//! (单测主场);本模块只做捕获/参数校验/坐标换算的原生胶合,与 find_image
//! 共用同一条 PrintWindow 捕获路径。诚实语义:零提议 = NOT_FOUND(与
//! find-image/preview-map 同纪律——空结果更可能是锚错窗口而非真空);
//! --motion 需要第二帧捕获(Δ1s),motion>0 即动画内容(视频/GIF 定性依据)。
use crate::find_image::capture_window;
use actl_core::image_regions::{self, ScanParams};
use actl_core::template_match::to_gray;
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};

pub struct ImageRegionsSpec<'a> {
    /// 窗口锚(标题子串或 hwnd:<n>);None = 前台窗口
    pub app: Option<&'a str>,
    /// 屏幕物理坐标子区 <x>,<y>,<w>,<h>(与窗口面求交;省略 = 整窗)
    pub rect: Option<&'a str>,
    pub threshold: Option<f64>,
    pub min_cells: Option<usize>,
    pub density: Option<f64>,
    pub limit: Option<usize>,
    /// 双帧捕获(Δ1s)标记动画区域
    pub motion: bool,
}

pub fn image_regions(spec: ImageRegionsSpec<'_>) -> Result<Value, CtlError> {
    let params = ScanParams {
        var_threshold: match spec.threshold {
            Some(t) if !(50.0..=20000.0).contains(&t) => {
                return Err(CtlError::protocol(format!(
                    "image-regions --threshold must be 50..=20000 (gray variance), got {t}"
                )));
            }
            t => t.unwrap_or(800.0),
        },
        min_cells: match spec.min_cells {
            Some(n) if !(4..=2500).contains(&n) => {
                return Err(CtlError::protocol(format!(
                    "image-regions --min-cells must be 4..=2500, got {n}"
                )));
            }
            n => n.unwrap_or(25),
        },
        density_min: match spec.density {
            Some(d) if !(0.0..=1.0).contains(&d) => {
                return Err(CtlError::protocol(format!(
                    "image-regions --density must be 0.0..=1.0, got {d}"
                )));
            }
            d => d.unwrap_or(0.0),
        },
        limit: match spec.limit {
            Some(n) if !(1..=200).contains(&n) => {
                return Err(CtlError::protocol(format!(
                    "image-regions --limit must be 1..=200, got {n}"
                )));
            }
            n => n.unwrap_or(64),
        },
    };
    // --rect 屏幕物理坐标解析(与 screenshot --rect 同口径)
    let rect = match spec.rect {
        None => None,
        Some(raw) => {
            let nums: Vec<i32> = raw
                .split(',')
                .map(|s| s.trim().parse::<i32>())
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    CtlError::protocol(format!(
                        "image-regions --rect expects X,Y,W,H physical pixels, got {raw:?}"
                    ))
                })?;
            if nums.len() != 4 || nums[2] <= 0 || nums[3] <= 0 {
                return Err(CtlError::protocol(format!(
                    "image-regions --rect expects X,Y,W,H with positive W,H, got {raw:?}"
                )));
            }
            Some((nums[0], nums[1], nums[2], nums[3]))
        }
    };

    let _dpi = crate::capture::Pmv2Guard::enter();
    let win = capture_window(spec.app)?;
    // 子区换算到捕获面本地坐标并求交(捕获面 = 窗口内容,origin 已含边框偏移)
    let crop = rect.map(|(rx, ry, rw, rh)| {
        let x0 = (rx - win.origin_x).max(0).min(win.w);
        let y0 = (ry - win.origin_y).max(0).min(win.h);
        let x1 = (rx + rw - win.origin_x).max(0).min(win.w);
        let y1 = (ry + rh - win.origin_y).max(0).min(win.h);
        (x0, y0, x1, y1)
    });
    let (full_gray, cw, ch) = {
        let g = to_gray(&win.bgra, win.w as usize, win.h as usize)
            .ok_or_else(|| CtlError::internal("window pixels shorter than declared size"))?;
        match crop {
            None => (g, win.w, win.h),
            Some((x0, y0, x1, y1)) if x1 > x0 && y1 > y0 => {
                (crop_gray(&g, x0, y0, x1, y1), x1 - x0, y1 - y0)
            }
            Some(_) => {
                return Err(CtlError::with_evidence(
                    ErrorCode::Protocol,
                    "image-regions --rect does not intersect the window surface".to_string(),
                    json!({
                        "rect": spec.rect,
                        "window": {"origin": [win.origin_x, win.origin_y], "width": win.w, "height": win.h},
                        "hint": "rect is screen physical pixels; the window surface starts at origin"
                    }),
                ));
            }
        }
    };
    // motion:Δ1s 第二帧(同窗口再捕获一次;只对裁剪面算差)。睡眠单独入
    // breakdown——duration 里分不清是 motion 的固定 Δ 还是捕获变慢
    let second = if spec.motion {
        let _t = actl_core::trace::scope("regions.motion-sleep");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let win2 = capture_window(spec.app)?;
        let g2 = to_gray(&win2.bgra, win2.w as usize, win2.h as usize)
            .ok_or_else(|| CtlError::internal("second frame pixels shorter than declared size"))?;
        Some(match crop {
            None => g2,
            Some((x0, y0, x1, y1)) if x1 > x0 && y1 > y0 => crop_gray(&g2, x0, y0, x1, y1),
            Some(_) => unreachable!("crop validated above"),
        })
    } else {
        None
    };

    // 网格+连通域(全窗 2230px 级 O(面),大窗方差可见)
    let regions = {
        let _t = actl_core::trace::scope("regions.grid");
        image_regions::scan(&full_gray, &params, second.as_ref())
    };
    // 裁剪面偏移:crop 时区域坐标是裁剪面本地的,换算屏幕须补回裁剪原点
    // (= --rect 左上角);漏加会让输出整体偏 (rect-窗口原点) 差值,
    // click xy: 会点错位置(2026-10-14 真机复现:rect y610 却报出 y475)
    let (cx, cy) = match crop {
        Some((x0, y0, _, _)) => (x0, y0),
        None => (0, 0),
    };
    let to_json = |r: &actl_core::image_regions::Region| {
        json!({
            "x": win.origin_x + cx + r.x,
            "y": win.origin_y + cy + r.y,
            "w": r.w,
            "h": r.h,
            "center": [win.origin_x + cx + r.x + r.w / 2, win.origin_y + cy + r.y + r.h / 2],
            "density": r.density,
            "cells": r.cells,
            "motion": r.motion,
        })
    };
    let window = json!({"title": win.title, "hwnd": win.hwnd, "width": win.w, "height": win.h, "origin": [win.origin_x, win.origin_y]});
    if regions.is_empty() {
        return Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!(
                "no image-like regions at variance>={:.0} (scanned {cw}x{ch} surface{})",
                params.var_threshold,
                if spec.motion { ", motion-checked" } else { "" }
            ),
            json!({
                "params": params_json(&params, spec.motion),
                "window": window,
                "hint": "uniform/flat surfaces have no proposals by design; lower --threshold only for genuinely textured content the default misses"
            }),
        ));
    }
    let total = regions.len();
    let items: Vec<Value> = regions.iter().map(to_json).collect();
    Ok(json!({
        "action": "image-regions",
        "total": total,
        "returned": items.len(),
        "truncated_by_limit": total > items.len(),
        "params": params_json(&params, spec.motion),
        "window": window,
        "regions": items,
        "resolved_by": "variance-scan",
    }))
}

fn params_json(p: &ScanParams, motion: bool) -> Value {
    json!({
        "var_threshold": p.var_threshold,
        "min_cells": p.min_cells,
        "density_min": p.density_min,
        "limit": p.limit,
        "cell_px": image_regions::CELL,
        "motion": motion,
    })
}

/// 裁剪灰度面(本地坐标)。
fn crop_gray(
    g: &actl_core::template_match::Gray,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> actl_core::template_match::Gray {
    let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
    let w = x1 - x0;
    let h = y1 - y0;
    let mut data = Vec::with_capacity(w * h);
    for y in y0..(y0 + h) {
        data.extend_from_slice(&g.data[y * g.w + x0..y * g.w + x0 + w]);
    }
    actl_core::template_match::Gray { w, h, data }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>(app: Option<&'a str>, rect: Option<&'a str>) -> ImageRegionsSpec<'a> {
        ImageRegionsSpec {
            app,
            rect,
            threshold: None,
            min_cells: None,
            density: None,
            limit: None,
            motion: false,
        }
    }

    #[test]
    fn parameter_validation() {
        assert_eq!(
            image_regions(spec(Some("NoSuchWindowXYZ"), Some("1,2")))
                .unwrap_err()
                .code,
            ErrorCode::Protocol
        );
        assert_eq!(
            image_regions(spec(Some("NoSuchWindowXYZ"), Some("a,b,c,d")))
                .unwrap_err()
                .code,
            ErrorCode::Protocol
        );
        assert_eq!(
            image_regions(spec(Some("NoSuchWindowXYZ"), Some("1,2,0,9")))
                .unwrap_err()
                .code,
            ErrorCode::Protocol
        );
        let mut s = spec(Some("NoSuchWindowXYZ"), None);
        s.threshold = Some(10.0);
        assert_eq!(image_regions(s).unwrap_err().code, ErrorCode::Protocol);
        let mut s = spec(Some("NoSuchWindowXYZ"), None);
        s.limit = Some(500);
        assert_eq!(image_regions(s).unwrap_err().code, ErrorCode::Protocol);
        // 合法参数走到窗口解析才失败(不存在的窗口 → NotFound)
        assert_eq!(
            image_regions(spec(Some("NoSuchWindowXYZ"), None))
                .unwrap_err()
                .code,
            ErrorCode::NotFound
        );
    }
}
