//! decide 的 front-report 判读咨询形态(popup 门禁的判读辅助):family 分类 +
//! blocking 概率并行一问(System One 多问题原生形态)。纯咨询不执行——处置
//! 仍归白名单与 unknown 上抛纪律。file-lines 门禁(>1500 拒绝线)从 decide.rs
//! 拆出;共享类型/Provider/防注入行经 super 复用(父模块私有项对子模块可见)。

use std::collections::BTreeMap;
use std::time::Instant;

use actl_core::{CtlError, ErrorCode};

use super::{
    Decision, Provider, SystemOneRequest, UNTRUSTED_NOTICE, WireQuestion, provider_unavailable,
};

/// front-report 咨询输入:引导层表面已渲染为有界 state 行,front_clear
/// 时短路免问(诚实「无引导层」不是模型问题)。InputForm 的 Debug 派生
/// 与父模块 run() 的所有权 match 需要 Debug + Clone。
#[derive(Debug, Clone)]
pub(super) struct FrontInput {
    pub(super) front_clear: bool,
    pub(super) state: String,
    pub(super) surface_count: usize,
}

/// 短路判读用的哑 provider:front_clear/no_surfaces 分支不触网;
/// 若逻辑回归导致误调用,INTERNAL 诚实报错而非静默。
pub(super) struct NeverProvider;
impl Provider for NeverProvider {
    fn ask(
        &self,
        _request: &SystemOneRequest<'_>,
    ) -> Result<(Option<String>, serde_json::Value), CtlError> {
        Err(CtlError::internal("never provider must not be invoked"))
    }
}

/// front-report 信封 → 弹窗判读咨询输入:表面渲染为有界 state 行
/// (name/buttons/text 各截断),防注入固定行随 state 携带。
pub(super) fn front_input(
    envelope: &serde_json::Value,
    task: &str,
) -> Result<FrontInput, CtlError> {
    let data = envelope
        .get("data")
        .ok_or_else(|| CtlError::protocol("front-report envelope has no data"))?;
    let front_clear = data
        .get("front_clear")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let clip = |text: &str, max: usize| text.chars().take(max).collect::<String>();
    let mut state = String::from("Front guidance layer of a Windows application.\n");
    state.push_str(&format!(
        "Current flow task: {}\nGuidance surfaces (kind | name | buttons | text):\n",
        clip(task, 160)
    ));
    let mut surface_count = 0usize;
    for (kind, section) in [("owned", "owned"), ("in-tree", "in_tree")] {
        for surface in data
            .get(section)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            surface_count += 1;
            let name = match surface.get("title").or_else(|| surface.get("name")) {
                Some(v) => v.as_str().unwrap_or_default(),
                None => "",
            };
            let buttons = surface
                .get("buttons")
                .and_then(serde_json::Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(serde_json::Value::as_str)
                        .take(8)
                        .collect::<Vec<_>>()
                        .join(" / ")
                })
                .unwrap_or_default();
            let text = surface
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            // 子窗文本增强(2026-10-13):owned 表面新增 texts[](静态全文/
            // 说明文字,front-report 信号增强——#32770 全文在子窗);in_tree
            // 保持 text 单行。拼接有界(≤4 行×240),判读 state 预算内。
            let mut combined = text.to_owned();
            if let Some(extra) = surface.get("texts").and_then(serde_json::Value::as_array) {
                for piece in extra.iter().filter_map(serde_json::Value::as_str).take(4) {
                    if !combined.is_empty() {
                        combined.push(' ');
                    }
                    combined.push_str(clip(piece, 240).trim());
                }
            }
            state.push_str(&format!(
                "{kind} | {} | [{}] | {}\n",
                clip(name, 64),
                clip(&buttons, 160),
                clip(combined.trim(), 480)
            ));
        }
    }
    state.push_str(UNTRUSTED_NOTICE);
    Ok(FrontInput {
        front_clear,
        state,
        surface_count,
    })
}

/// front 家族全集(与门禁 handler 分类对齐 + benign_surface/unknown 两个
/// 咨询专属族):crash/error_notice/save/confirm/update 与 handler 词表同义,
/// 门禁处置仍按 handler 声明;decide 只产出判读。
const FRONT_FAMILIES: &[(&str, &str)] = &[
    (
        "crash",
        "application crash / abnormal termination (Fail Fast, application error, has stopped working/responding)",
    ),
    (
        "error_notice",
        "operation error or unmet condition notice (file not found, invalid input)",
    ),
    ("save", "save/discard prompt for unsaved changes"),
    ("confirm", "confirmation dialog asking whether to proceed"),
    ("update", "software update / upgrade prompt"),
    (
        "notice",
        "informational notice safe to dismiss (session ended, feature disabled)",
    ),
    (
        "benign_surface",
        "a permanent part of the application UI itself (title bar, pane, toolbar), not a transient popup",
    ),
    (
        "unknown",
        "cannot confidently classify; hand back to a human",
    ),
];

/// front 咨询请求构造(dry_run 与 front_consult 共用;家族键全部 'static)。
pub(super) fn front_request(input: &FrontInput) -> Result<SystemOneRequest<'static>, CtlError> {
    let criteria: BTreeMap<&str, String> = FRONT_FAMILIES
        .iter()
        .map(|(key, description)| (*key, description.to_string()))
        .collect();
    Ok(SystemOneRequest {
        state: input.state.clone(),
        questions: BTreeMap::from([
            (
                "family",
                WireQuestion {
                    kind: "choice",
                    instructions: "Classify the front guidance layer against the flow task. 'unknown' hands back to a human.",
                    criteria: Some(criteria),
                },
            ),
            (
                "blocking",
                WireQuestion {
                    kind: "noul",
                    instructions: "A popup currently overlays and interrupts use of the main window content.",
                    criteria: None,
                },
            ),
        ]),
        images: None,
    })
}

/// 短路判读(front_clear / no_surfaces):连 provider 都不必触达。
pub(super) fn front_short_circuit(input: &FrontInput) -> Option<Decision> {
    if input.front_clear {
        return Some(Decision {
            card_id: "d-front".into(),
            choice: None,
            abstain: None,
            confidence: None,
            top_choice: None,
            resolved: None,
            family: Some("front_clear".into()),
            blocking: Some(0.0),
            answers: None,
            candidates_sent: 0,
            dangerous_filtered: 0,
            provider_ms: None,
            model: None,
        });
    }
    if input.surface_count == 0 {
        return Some(Decision {
            card_id: "d-front".into(),
            choice: None,
            abstain: Some("no_surfaces".into()),
            confidence: None,
            top_choice: None,
            resolved: None,
            family: None,
            blocking: None,
            answers: None,
            candidates_sent: 0,
            dangerous_filtered: 0,
            provider_ms: None,
            model: None,
        });
    }
    None
}

/// 弹窗判读咨询(popup 门禁的判读辅助):family 分类 + blocking 概率
/// 并行一问。**纯咨询**——family 是判读建议,处置(点击/忽略/上抛)仍归
/// 白名单与 unknown 上抛纪律,decide 不执行任何东西。front_clear 短路
/// 免问(诚实「无引导层」不是模型问题);零表面(front_clear=false 但
/// owned/in_tree 全空,如锚窗非前台的 in_tree_skipped)同样免问。
pub(super) fn front_consult(
    input: &FrontInput,
    provider: &dyn Provider,
    min_confidence: f64,
) -> Result<Decision, CtlError> {
    if let Some(decision) = front_short_circuit(input) {
        return Ok(decision);
    }
    let request = front_request(input)?;
    let started = Instant::now();
    let (model, answers) = provider.ask(&request)?;
    let provider_ms = started.elapsed().as_millis() as u64;
    let family_answer = answers
        .get("family")
        .cloned()
        .ok_or_else(|| provider_unavailable("provider returned no family answer", None, None))?;
    let Some(family) = family_answer
        .get("choice")
        .and_then(serde_json::Value::as_str)
    else {
        return Err(provider_unavailable(
            "provider returned no family choice",
            None,
            None,
        ));
    };
    if !FRONT_FAMILIES.iter().any(|(key, _)| *key == family) {
        return Err(CtlError::with_evidence(
            ErrorCode::ProviderUnavailable,
            "provider chose a family that was not offered",
            serde_json::json!({
                "reason": "choice_not_offered",
                "choice": family,
            }),
        ));
    }
    let confidence = family_answer
        .get("confidence")
        .and_then(serde_json::Value::as_f64);
    let blocking = answers
        .get("blocking")
        .and_then(|a| a.get("noul"))
        .and_then(serde_json::Value::as_f64);
    // 判读建议同样过 conf 门槛:低于阈值不冒充可信判读,abstain 交回人
    // (blocking 概率照报,概率不是决定)。
    if confidence.is_some_and(|c| c < min_confidence) && family != "unknown" {
        return Ok(Decision {
            card_id: "d-front".into(),
            choice: None,
            abstain: Some("low_confidence".into()),
            confidence,
            top_choice: Some(family.into()),
            resolved: None,
            family: None,
            blocking,
            answers: None,
            candidates_sent: FRONT_FAMILIES.len(),
            dangerous_filtered: 0,
            provider_ms: Some(provider_ms),
            model,
        });
    }
    Ok(Decision {
        card_id: "d-front".into(),
        choice: None,
        abstain: None,
        confidence,
        top_choice: None,
        resolved: None,
        family: Some(family.into()),
        blocking,
        answers: None,
        candidates_sent: FRONT_FAMILIES.len(),
        dangerous_filtered: 0,
        provider_ms: Some(provider_ms),
        model,
    })
}
