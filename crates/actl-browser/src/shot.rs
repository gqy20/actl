//! browser-screenshot:CDP 轨的像素捕获动词(Page.captureScreenshot +
//! clip)。语义与 UIA 轨 screenshot 对齐——**读操作**(不占动作卡,与
//! browser-read 同分类):像素落 PNG 文件,envelope 只带 path+元数据
//! (视觉消费是调用方的决定;decide 视觉咨询文档直接吃这个 path)。
//!
//! 坐标语义:clip 以视口 CSS 像素为口径(getBoundingClientRect);返回
//! 图像的像素尺寸与 DPR 相关(clip×scale×设备缩放),**不承诺 1:1**——
//! 消费侧按需自标定(挑战解题循环以 图像宽/clip宽 自标定换算,诚实)。

use std::path::{Path, PathBuf};
use std::time::Duration;

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};

use crate::SelectorKind;
use crate::bridge::{self, eval_json};
use crate::ops::{OP_DEFAULT_TIMEOUT_MS, dur, validate_selector, validate_target_shape};
use crate::scripts::element_box_script;
use crate::session::{self, Paths};

/// 截图产物保留上限(与 UIA 轨 shots keep-20 同纪律)。
const KEEP_SHOTS: usize = 20;

pub struct ShotSpec<'a> {
    pub session_id: &'a str,
    /// 元素裁剪(None = 整视口)。bbox 取 getBoundingClientRect(CSS px),
    /// 与视口求交(离屏元素截到交集并如实上报原盒)。
    pub selector: Option<(SelectorKind, &'a str)>,
    pub iframe: bool,
    pub target: Option<&'a str>,
    /// 输出路径(默认 %LOCALAPPDATA%\actl\browser\shots,keep-20)。
    pub out: Option<&'a str>,
    pub timeout_ms: Option<u64>,
}

/// shots 目录(`%LOCALAPPDATA%\actl\browser\shots`;测试经 Paths 注入)。
fn shots_dir(paths: &Paths) -> PathBuf {
    paths.root.join("shots")
}

/// keep-N 清扫:文件名按前缀+快照 id 命名,按 mtime 降序保留 KEEP_SHOTS。
fn prune_shots(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "png")
        })
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), meta.modified().ok()?))
        })
        .collect();
    files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    for (path, _) in files.iter().skip(KEEP_SHOTS) {
        let _ = std::fs::remove_file(path);
    }
}

/// 证据截图落盘(challenge-solve 的轨道/模板产物):shots 目录 + keep-20。
pub(crate) fn save_shot(paths: &Paths, png: &[u8]) -> Result<String, CtlError> {
    let dir = shots_dir(paths);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CtlError::internal(format!("cannot create shots dir: {e}")))?;
    let id = actl_core::new_snapshot_id();
    let path = dir.join(format!("shot_{id}.png"));
    std::fs::write(&path, png)
        .map_err(|e| CtlError::internal(format!("cannot write screenshot: {e}")))?;
    prune_shots(&dir);
    Ok(path.display().to_string())
}

/// 元素 bbox(CSS px)+ 视口尺寸:唯一匹配硬判定(challenge-solve 复用)。
pub(crate) struct ElementBox {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) vw: f64,
    pub(crate) vh: f64,
}

fn element_box(
    page: &chromiumoxide::Page,
    kind: SelectorKind,
    value: &str,
    context_id: Option<u64>,
    timeout: Duration,
) -> Result<ElementBox, CtlError> {
    let located = eval_json(page, &element_box_script(kind, value), context_id, timeout)?;
    let matches = located.get("matches").and_then(Value::as_u64).unwrap_or(0);
    match matches {
        0 => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!("no element matches the {} selector", kind.as_str()),
            json!({ "selector_kind": kind.as_str(), "selector": value, "matches": 0 }),
        )),
        1 => {
            let read = |key: &str| located.get(key).and_then(Value::as_f64).unwrap_or(0.0);
            Ok(ElementBox {
                x: read("x"),
                y: read("y"),
                width: read("width"),
                height: read("height"),
                vw: read("vw"),
                vh: read("vh"),
            })
        }
        n => Err(CtlError::with_evidence(
            ErrorCode::Ambiguous,
            format!(
                "{} elements match the {} selector; tighten it",
                n,
                kind.as_str()
            ),
            json!({ "selector_kind": kind.as_str(), "selector": value, "matches": n }),
        )),
    }
}

pub fn screenshot(paths: &Paths, spec: ShotSpec<'_>) -> Result<Value, CtlError> {
    if let Some((kind, value)) = spec.selector {
        validate_selector(kind, value)?;
    }
    if let Some(target) = spec.target {
        validate_target_shape(target)?;
    }
    let timeout = dur(spec.timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, spec.session_id)?;
    let lane = bridge::open_lane(&record, spec.target, timeout)?;
    let context_id = if spec.iframe {
        Some(bridge::find_iframe_context(&lane.page, timeout)?.1)
    } else {
        None
    };

    let mut clip: Option<(f64, f64, f64, f64)> = None;
    let mut element = Value::Null;
    if let Some((kind, value)) = spec.selector {
        let box_at = element_box(&lane.page, kind, value, context_id, timeout)?;
        // 与视口求交(离屏/越界元素截到交集;空交集如实失败)
        let x0 = box_at.x.clamp(0.0, box_at.vw);
        let y0 = box_at.y.clamp(0.0, box_at.vh);
        let x1 = (box_at.x + box_at.width).clamp(0.0, box_at.vw);
        let y1 = (box_at.y + box_at.height).clamp(0.0, box_at.vh);
        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
            return Err(CtlError::with_evidence(
                ErrorCode::NotActionable,
                "element has no visible viewport intersection to capture".to_string(),
                json!({
                    "selector_kind": kind.as_str(),
                    "selector": value,
                    "rect": {"x": box_at.x, "y": box_at.y, "width": box_at.width, "height": box_at.height},
                    "viewport": {"width": box_at.vw, "height": box_at.vh},
                }),
            ));
        }
        element = json!({
            "selector_kind": kind.as_str(),
            "selector": value,
            "rect": {"x": box_at.x, "y": box_at.y, "width": box_at.width, "height": box_at.height},
            "captured": {"x": x0, "y": y0, "width": x1 - x0, "height": y1 - y0},
        });
        clip = Some((x0, y0, x1 - x0, y1 - y0));
    }

    // Page.captureScreenshot(clip=元素交集或整视口;PNG)
    let png = capture(&lane.page, clip, timeout)?;
    let path = match spec.out {
        Some(out) => PathBuf::from(out),
        None => {
            let dir = shots_dir(paths);
            std::fs::create_dir_all(&dir)
                .map_err(|e| CtlError::internal(format!("cannot create shots dir: {e}")))?;
            let id = actl_core::new_snapshot_id();
            dir.join(format!("shot_{id}.png"))
        }
    };
    std::fs::write(&path, &png)
        .map_err(|e| CtlError::internal(format!("cannot write screenshot: {e}")))?;
    if spec.out.is_none() {
        prune_shots(&shots_dir(paths));
    }
    let mut payload = json!({
        "session_id": spec.session_id,
        "path": path.display().to_string(),
        "bytes": png.len(),
        "viewport_capture": clip.is_none(),
    });
    if let Some((x, y, width, height)) = clip {
        payload["clip"] = json!({
            "x": (x * 100.0).round() / 100.0,
            "y": (y * 100.0).round() / 100.0,
            "width": (width * 100.0).round() / 100.0,
            "height": (height * 100.0).round() / 100.0,
        });
        payload["element"] = element;
    }
    if let Some(target) = spec.target {
        payload["target"] = Value::String(target.to_string());
    }
    Ok(payload)
}

/// CDP 截图(骨干):clip None = 整视口;返回 PNG 字节。图像像素尺寸与
/// clip CSS 尺寸的比值随 DPR(不承诺 1:1——消费侧按需自标定)。
pub(crate) fn capture(
    page: &chromiumoxide::Page,
    clip: Option<(f64, f64, f64, f64)>,
    timeout: Duration,
) -> Result<Vec<u8>, CtlError> {
    use chromiumoxide::cdp::browser_protocol::page::{CaptureScreenshotFormat, Viewport};
    use chromiumoxide::page::ScreenshotParams;
    let mut builder = ScreenshotParams::builder().format(CaptureScreenshotFormat::Png);
    if let Some((x, y, width, height)) = clip {
        builder = builder.clip(Viewport {
            x,
            y,
            width,
            height,
            scale: 1.0,
        });
    }
    let params = builder.build();
    let page_owned = page.clone();
    bridge::block_on(
        "screenshot",
        async move {
            page_owned
                .screenshot(params)
                .await
                .map_err(|e| format!("capture_screenshot: {e}"))
        },
        timeout,
    )?
    .map_err(|msg| CtlError::internal(format!("page screenshot failed: {msg}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_newest_twenty() {
        let dir = std::env::temp_dir().join(format!("actl-shot-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..25 {
            let path = dir.join(format!("shot_{i:02}.png"));
            std::fs::write(&path, b"png").unwrap();
            // mtime 单调:依次拉开
            let stamp =
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000 + i * 10);
            let _ = set_mtime(&path, stamp);
        }
        prune_shots(&dir);
        let remaining = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(remaining, KEEP_SHOTS);
        assert!(!dir.join("shot_00.png").exists(), "oldest pruned");
        assert!(dir.join("shot_24.png").exists(), "newest kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn set_mtime(path: &Path, stamp: std::time::SystemTime) -> std::io::Result<()> {
        std::fs::File::options()
            .write(true)
            .open(path)?
            .set_modified(stamp)
    }
}
