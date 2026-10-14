//! 挑战面:人机验证的检测(签名分型)与滑块解题循环(2026-10-14 spec)。
//!
//! 纪律边界(与 find-image/decide 视觉 carve-out 同款立场):**只对
//! 「呈现在面前的挑战」求解**——真实浏览器、真实输入事件、确定性感知
//! (ZNCC 模板匹配);不做隐身/指纹伪装,不接打码服务。检测只识别不
//! 断言;解题循环的效果验证走本地谓词(--success-css)或挑战签名消失,
//! 失败如实上报,重试有界(默认 1 次,上限 2——失败回灌风控是真实代价)。
//!
//! 解题坐标链:元素 bbox(视口 CSS px)→ CDP 截图(图像 px)→ ZNCC 命中
//! (图像 px)→ 以 图像宽/clip宽 自标定换算回 CSS px → 拖拽位移。感知面
//! 的模板=滑块件元素自身截图(或 --template 文件);搜索面=轨道元素截图。
//! 件与轨道重叠形态(件盖在轨道上)以「件当前位置邻域过滤命中」防自匹配。

use std::path::Path;
use std::time::Duration;

use actl_core::challenge::{ChallengePage, detect};
use actl_core::template_match::{self, Match};
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};

use crate::SelectorKind;
use crate::bridge::{self, eval_json};
use crate::input::{self, DragSpec};
use crate::ops::{OP_DEFAULT_TIMEOUT_MS, dur, validate_selector, validate_target_shape};
use crate::scripts::{challenge_scan_script, element_box_script};
use crate::session::{self, Paths};
use crate::shot::{ElementBox, capture, save_shot};

/// ZNCC 匹配缺省阈值(缺口匹配噪声背景的分度,低于 find-image 的 0.85
/// ——缺口边界软化,从严会漏)。
const DEFAULT_THRESHOLD: f32 = 0.75;
/// 解题效果谓词的沉降轮询上限。
const SUCCESS_POLL_MS: u64 = 4_000;
const SUCCESS_POLL_INTERVAL_MS: u64 = 400;

/// 采集脚本产出 → ChallengePage(纯解析,离线可测)。
fn page_from_json(scanned: &Value) -> ChallengePage {
    let strings = |key: &str| -> Vec<String> {
        scanned
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    ChallengePage {
        title: scanned
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        scripts: strings("scripts"),
        iframes: strings("iframes"),
        markers: scanned
            .get("markers")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter(|(_, hit)| hit.as_bool() == Some(true))
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// 检测结果信封(检测只识别;命中=形似已知挑战,未命中≠无挑战)。
fn hits_to_json(hits: &[actl_core::challenge::ChallengeHit]) -> Value {
    json!({
        "families": hits.iter().map(|hit| json!({
            "family": hit.family,
            "label": hit.label,
            "signals": hit.signals,
        })).collect::<Vec<_>>(),
        "total": hits.len(),
    })
}

/// 扫描当前页并分型(只读;verify --challenge 与 challenge-solve 共用)。
pub fn scan(
    paths: &Paths,
    session_id: &str,
    target: Option<&str>,
    timeout_ms: Option<u64>,
) -> Result<Value, CtlError> {
    if let Some(target) = target {
        validate_target_shape(target)?;
    }
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let lane = bridge::open_lane(&record, target, timeout)?;
    let scanned = eval_json(&lane.page, &challenge_scan_script(), None, timeout)?;
    let hits = detect(&page_from_json(&scanned));
    let mut payload = json!({
        "session_id": session_id,
        "title": scanned.get("title").cloned().unwrap_or(Value::Null),
    });
    if let Some(target) = target {
        payload["target"] = Value::String(target.to_string());
    }
    if hits.is_empty() {
        payload["challenges"] = json!({ "families": [], "total": 0 });
    } else {
        payload["challenges"] = hits_to_json(&hits);
    }
    Ok(payload)
}

/// 滑块解题循环参数束。
pub struct SolveSpec<'a> {
    pub session_id: &'a str,
    /// 轨道元素(搜索面:缺口所在图)。
    pub track: (SelectorKind, &'a str),
    /// 滑块件元素(模板面与位移基准:件的左沿要落到缺口左沿)。
    pub piece: (SelectorKind, &'a str),
    /// 拖拽手柄元素(按下/拖动的控件)。
    pub handle: (SelectorKind, &'a str),
    /// 模板文件覆盖(缺省截 piece 元素为模板;件元素渲染与轨道内容
    /// 不一致时用文件——如件是独立贴图)。
    pub template: Option<&'a str>,
    /// 效果谓词:成功标记选择器(如 fixture 的 #challenge-success)。
    pub success: Option<(SelectorKind, &'a str)>,
    pub threshold: Option<f64>,
    /// 尝试上限(1..=2;失败回灌风控是真实代价,默认 1)。
    pub attempts: Option<u64>,
    pub human: bool,
    pub seed: Option<u64>,
    pub target: Option<&'a str>,
    pub iframe: bool,
    pub timeout_ms: Option<u64>,
}

/// ZNCC 命中 → 拖拽位移(纯数学,离线可测):图像命中换算回 CSS px
/// (track 图像宽/clip 宽自标定),减去件当前 CSS x = 件应移动的量。
fn drag_delta(
    track_box: &ElementBox,
    track_img_w: usize,
    match_x_img: usize,
    piece_css_x: f64,
) -> f64 {
    let scale = track_img_w as f64 / track_box.width.max(1.0);
    let gap_css_x = track_box.x + match_x_img as f64 / scale.max(f64::MIN_POSITIVE);
    gap_css_x - piece_css_x
}

/// 件当前位置邻域的自匹配过滤(重叠形态:件盖在轨道上,截图含件自身)。
fn filter_self_match(
    matches: Vec<Match>,
    piece_img_x: f64,
    needle_w: usize,
) -> (Vec<Match>, usize) {
    let excluded = matches
        .iter()
        .filter(|m| (m.x as f64 - piece_img_x).abs() < needle_w as f64)
        .count();
    let kept: Vec<Match> = matches
        .into_iter()
        .filter(|m| (m.x as f64 - piece_img_x).abs() >= needle_w as f64)
        .collect();
    (kept, excluded)
}

fn element_box_at(
    page: &chromiumoxide::Page,
    kind: SelectorKind,
    value: &str,
    context_id: Option<u64>,
    timeout: Duration,
    label: &str,
) -> Result<ElementBox, CtlError> {
    let located = eval_json(page, &element_box_script(kind, value), context_id, timeout)?;
    let matches = located.get("matches").and_then(Value::as_u64).unwrap_or(0);
    match matches {
        0 => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!("no element matches the {label} selector"),
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
            format!("{n} elements match the {label} selector; tighten it"),
            json!({ "selector_kind": kind.as_str(), "selector": value, "matches": n }),
        )),
    }
}

/// 解题效果谓词:--success-css 存在性轮询(有界沉降)。
fn success_present(
    page: &chromiumoxide::Page,
    success: (SelectorKind, &str),
    timeout: Duration,
) -> Result<bool, CtlError> {
    let deadline = std::time::Instant::now() + Duration::from_millis(SUCCESS_POLL_MS);
    loop {
        let located = eval_json(
            page,
            &element_box_script(success.0, success.1),
            None,
            timeout,
        )?;
        let matches = located.get("matches").and_then(Value::as_u64).unwrap_or(0);
        if matches >= 1 {
            return Ok(true);
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(SUCCESS_POLL_INTERVAL_MS));
    }
}

/// 滑块解题循环:检测(上下文)→ 截图(轨道+件)→ ZNCC 缺口 → 位移 →
/// 人类轨迹拖拽 → 效果谓词 → 有界重试。证据截图落 shots 目录随信封上报。
pub fn solve(paths: &Paths, spec: SolveSpec<'_>) -> Result<Value, CtlError> {
    validate_selector(spec.track.0, spec.track.1)?;
    validate_selector(spec.piece.0, spec.piece.1)?;
    validate_selector(spec.handle.0, spec.handle.1)?;
    if let Some(success) = spec.success {
        validate_selector(success.0, success.1)?;
    }
    let threshold = spec.threshold.unwrap_or(DEFAULT_THRESHOLD as f64);
    if !(threshold > 0.0 && threshold <= 1.0) {
        return Err(CtlError::protocol(format!(
            "--threshold must be in (0,1], got {threshold}"
        )));
    }
    let attempts = spec.attempts.unwrap_or(1).clamp(1, 2);
    if let Some(target) = spec.target {
        validate_target_shape(target)?;
    }
    let timeout = dur(spec.timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, spec.session_id)?;
    let Some(success_predicate) = spec.success else {
        return Err(CtlError::with_evidence_hint(
            ErrorCode::Protocol,
            "challenge-solve requires a validation predicate".to_string(),
            json!({ "reason": "success_predicate_missing" }),
            "pass --success-css (fixture/已知站点的成功标记) — 效果验证是解题循环的 \
             完成判据,无谓词的拖拽只是盲放",
        ));
    };

    let initial_scan = scan(paths, spec.session_id, spec.target, spec.timeout_ms)?;
    let initial_families = initial_scan
        .pointer("/challenges/total")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut last_attempt: Option<Value> = None;
    for attempt in 1..=attempts {
        let lane = bridge::open_lane(&record, spec.target, timeout)?;
        let context_id = if spec.iframe {
            Some(bridge::find_iframe_context(&lane.page, timeout)?.1)
        } else {
            None
        };

        // 1) 感知:轨道截图(搜索面)+ 件模板(--template 文件或件元素截图)
        let track_box = element_box_at(
            &lane.page,
            spec.track.0,
            spec.track.1,
            context_id,
            timeout,
            "track",
        )?;
        let track_png = capture(
            &lane.page,
            Some((track_box.x, track_box.y, track_box.width, track_box.height)),
            timeout,
        )?;
        let track_shot = save_shot(paths, &track_png)?;

        let piece_box = element_box_at(
            &lane.page,
            spec.piece.0,
            spec.piece.1,
            context_id,
            timeout,
            "piece",
        )?;
        let (needle_bgra, needle_w, needle_h, piece_shot) = if let Some(template) = spec.template {
            let (bgra, w, h) =
                actl_uia::find_image::decode_png_file(Path::new(template)).map_err(|e| {
                    CtlError::with_evidence(
                        ErrorCode::Internal,
                        format!("template unreadable: {e}"),
                        json!({ "template": template }),
                    )
                })?;
            (bgra, w, h, template.to_string())
        } else {
            let piece_png = capture(
                &lane.page,
                Some((piece_box.x, piece_box.y, piece_box.width, piece_box.height)),
                timeout,
            )?;
            let piece_shot = save_shot(paths, &piece_png)?;
            let (bgra, w, h) = actl_uia::find_image::decode_png_file(Path::new(&piece_shot))
                .map_err(|e| CtlError::internal(format!("piece shot decode: {e}")))?;
            (bgra, w, h, piece_shot)
        };

        let (track_bgra, track_w, track_h) =
            actl_uia::find_image::decode_png_file(Path::new(&track_shot))
                .map_err(|e| CtlError::internal(format!("track shot decode: {e}")))?;
        let track_w = track_w as usize;
        let track_h = track_h as usize;
        let Some(hay) = template_match::to_gray(&track_bgra, track_w, track_h) else {
            return Err(CtlError::internal("track image has no pixels"));
        };
        let Some(needle) =
            template_match::to_gray(&needle_bgra, needle_w as usize, needle_h as usize)
        else {
            return Err(CtlError::internal("piece image has no pixels"));
        };
        if template_match::degenerate(&needle) {
            return Err(CtlError::protocol(
                "piece template is uniform (ZNCC undefined); pass --template with a \
                 distinguishable piece image",
            ));
        }
        if needle.w > hay.w || needle.h > hay.h {
            return Err(CtlError::protocol(
                "piece template is larger than the track image",
            ));
        }

        // 2) 匹配:全轨命中 → 件当前邻域自匹配过滤 → 最高分
        let scale = track_w as f64 / track_box.width.max(1.0);
        let piece_img_x = (piece_box.x - track_box.x) * scale;
        let (matches, excluded) = filter_self_match(
            template_match::find(&hay, &needle, threshold as f32),
            piece_img_x,
            needle.w,
        );
        let best = matches.iter().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let Some(best) = best else {
            last_attempt = Some(json!({
                "attempt": attempt,
                "outcome": "no_gap_match",
                "threshold": threshold,
                "matches_excluded_self": excluded,
                "track_shot": track_shot,
            }));
            continue;
        };

        // 3) 位移与拖拽
        let dx = drag_delta(&track_box, track_w, best.x, piece_box.x);
        let handle_box = element_box_at(
            &lane.page,
            spec.handle.0,
            spec.handle.1,
            context_id,
            timeout,
            "handle",
        )?;
        let from = (
            handle_box.x + handle_box.width / 2.0,
            handle_box.y + handle_box.height / 2.0,
        );
        let to = (from.0 + dx, from.1);
        let dragged = input::drag(
            paths,
            DragSpec {
                session_id: spec.session_id,
                from,
                to,
                human: spec.human,
                seed: spec.seed.map(|seed| seed + attempt),
                target: spec.target,
                timeout_ms: spec.timeout_ms,
            },
        )?;

        // 4) 效果谓词(--success-css 存在性)
        let solved = success_present(&lane.page, success_predicate, timeout)?;
        last_attempt = Some(json!({
            "attempt": attempt,
            "outcome": if solved { "solved" } else { "drag_delivered_not_validated" },
            "gap": {
                "match_x_img": best.x,
                "match_y_img": best.y,
                "score": best.score,
                "scale_img_per_css": (scale * 1000.0).round() / 1000.0,
                "drag_dx_css": (dx * 100.0).round() / 100.0,
            },
            "matches_total": matches.len() + excluded,
            "matches_excluded_self": excluded,
            "drag": dragged,
            "track_shot": track_shot,
            "piece_shot": piece_shot,
        }));
        if solved {
            break;
        }
    }

    let solved = last_attempt
        .as_ref()
        .and_then(|value| value.get("outcome"))
        .and_then(Value::as_str)
        .is_some_and(|outcome| outcome == "solved");
    let mut payload = json!({
        "session_id": spec.session_id,
        "solved": solved,
        "attempts": attempts,
        "families_detected_before": initial_families,
        "human": spec.human,
    });
    if let Some(last) = last_attempt {
        payload["last_attempt"] = last;
    }
    if !solved {
        payload["error_hint"] = json!(
            "honest failure: challenge not validated after bounded \
             attempts; evidence shots are in data.last_attempt.track_shot"
        );
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_at(x: f64, y: f64, w: f64, h: f64) -> ElementBox {
        ElementBox {
            x,
            y,
            width: w,
            height: h,
            vw: 1920.0,
            vh: 1080.0,
        }
    }

    #[test]
    fn page_from_json_parses_all_channels() {
        let scanned = json!({
            "title": "Just a moment...",
            "scripts": ["https://static.geetest.com/gt.js"],
            "iframes": ["https://turing.captcha.gtimg.com/x"],
            "markers": {"cf_form": true, "turnstile": false, "geetest": true}
        });
        let page = page_from_json(&scanned);
        assert_eq!(page.title, "Just a moment...");
        assert_eq!(page.scripts.len(), 1);
        assert_eq!(page.iframes.len(), 1);
        assert_eq!(page.markers, vec!["cf_form", "geetest"]);
        let hits = detect(&page);
        // cf(title+marker)/geetest(script+marker)/tencent(iframe) 三家
        assert_eq!(hits.len(), 3, "{hits:?}");
        let families: Vec<&str> = hits.iter().map(|hit| hit.family).collect();
        assert!(families.contains(&"cloudflare_interstitial"));
        assert!(families.contains(&"geetest"));
        assert!(families.contains(&"tencent_captcha"));
    }

    #[test]
    fn drag_delta_maps_image_match_to_css_displacement() {
        // track 320 CSS px 宽,截图 640 图像 px(2× 标定):命中在图像
        // x=300 → CSS 150;件当前在 10 → dx=140
        let track = box_at(100.0, 200.0, 320.0, 60.0);
        let dx = drag_delta(&track, 640, 300, 110.0);
        assert!((dx - 140.0).abs() < 0.01, "{dx}");
    }

    #[test]
    fn filter_self_match_excludes_piece_neighborhood() {
        let matches = vec![
            Match {
                x: 12,
                y: 5,
                w: 40,
                h: 40,
                score: 0.98,
            },
            Match {
                x: 210,
                y: 5,
                w: 40,
                h: 40,
                score: 0.91,
            },
        ];
        let (kept, excluded) = filter_self_match(matches, 10.0, 40);
        assert_eq!(excluded, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].x, 210);
    }
}
