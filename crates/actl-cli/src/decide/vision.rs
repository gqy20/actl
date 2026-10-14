//! decide 的视觉咨询形态(System One 判别式协议的图像扩展,2026-10-14 spec):
//! 文档 `{state, images[], questions{}}` → PNG 路径读取/魔数校验/base64 →
//! wire `images`(PNG data URL)单次前向多问题并行(实测 LM Studio
//! /v1/systemone + clef-flash:单图 0.97+、双图双题、确定性逐位复现)。
//! 协议纪律与卡形态同源:none 强制入 choice 选项(模型有真实弃权出口)、
//! 幻觉拒绝(应答必须 ∈ offered)、置信/领先裕度双门槛逐问题适用、防注入
//! 固定行随 state 携带。图像负载永不进审计行与 dry-run 输出(路径+字节数
//! 入行)——审计行 4KiB 预算与屏幕内容隐私(docs/08 §7 数据边界)。
//! PRD §1.3 carve-out:受限判别式视觉咨询,不是生成式 VLM 客户端。

use std::collections::BTreeMap;
use std::time::Instant;

use actl_core::{CtlError, ErrorCode};
use serde::Deserialize;

use super::{
    Decision, MIN_LEAD_MARGIN, Provider, SystemOneRequest, UNTRUSTED_NOTICE, WireQuestion,
    provider_unavailable,
};

/// 单请求图像上限(多区域裁剪的并行形态;实测双图双题单前向可用)。
const MAX_IMAGES: usize = 4;
/// 单图像文件字节上限(超限即拒——wire 载荷与内存面的双重护栏)。
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// 问题数上限(与 System One 多问题原生形态对齐的有界并行)。
const MAX_QUESTIONS: usize = 4;
/// choice 问题的选项数上限(候选密度是判别式精度天敌,bin 化候选控制在
/// 收益区;docs/42 Top-K≤8 是 UIA 候选语义,视觉 bin 放宽到 4×4 网格)。
const MAX_CRITERIA: usize = 16;
const MAX_STATE_CHARS: usize = 2000;
const MAX_INSTRUCTIONS_CHARS: usize = 256;
/// 单个选项描述的字符上限。
const MAX_CRITERION_CHARS: usize = 128;
/// 问题 id 与选项 id 的字符上限。
const MAX_KEY_CHARS: usize = 32;

/// PNG 魔数(8 字节签名的前 4 字节足够判别)。
const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];
/// 强制注入的弃权选项(与卡形态同款安全不变量)。
const NONE_KEY: &str = "none";
const NONE_DESC: &str = "no suitable option, hand back to supervisor";

/// 视觉咨询文档(serde 面;deny_unknown_fields 捕捉拼写错误)。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisionDoc {
    state: String,
    images: Vec<String>,
    questions: BTreeMap<String, VisionDocQuestion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisionDocQuestion {
    #[serde(rename = "type")]
    kind: String,
    instructions: String,
    #[serde(default)]
    criteria: Option<BTreeMap<String, String>>,
}

/// 解析后的输入(结构校验完成;图像文件在 prepare 阶段读取)。问题表按
/// BTreeMap 键序排列(确定性——同文档同序,id 与问题体原键序配对)。
#[derive(Debug, Clone)]
pub(super) struct VisionInput {
    pub(super) state: String,
    pub(super) images: Vec<String>,
    pub(super) questions: Vec<(String, VisionQuestion)>,
}

#[derive(Debug, Clone)]
pub(super) struct VisionQuestion {
    pub(super) kind: QuestionKind,
    pub(super) instructions: String,
    /// noul 恒空。
    pub(super) criteria: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QuestionKind {
    Choice,
    Noul,
}

fn protocol(reason: &str, message: String) -> CtlError {
    CtlError::with_evidence(
        ErrorCode::Protocol,
        message,
        serde_json::json!({ "reason": reason }),
    )
}

/// 文档 → 输入:结构级校验(数量/长度/类型),图像文件读取留给 prepare
/// (dry-run 也要走 prepare,文件不存在/非 PNG 在那里诚实失败)。
pub(super) fn parse(text: &str) -> Result<VisionInput, CtlError> {
    let doc: VisionDoc = serde_json::from_str(text).map_err(|e| {
        protocol(
            "doc_invalid",
            format!("vision consult document invalid: {e}"),
        )
    })?;
    let VisionDoc {
        state,
        images,
        questions: doc_questions,
    } = doc;
    if state.trim().is_empty() {
        return Err(protocol(
            "state_empty",
            "state must describe the images".into(),
        ));
    }
    if state.chars().count() > MAX_STATE_CHARS {
        return Err(protocol(
            "state_too_long",
            format!("state exceeds {MAX_STATE_CHARS} chars"),
        ));
    }
    if images.is_empty() {
        return Err(protocol(
            "images_empty",
            "at least one image path is required".into(),
        ));
    }
    if images.len() > MAX_IMAGES {
        return Err(protocol(
            "too_many_images",
            format!("images exceed the cap of {MAX_IMAGES}"),
        ));
    }
    if images
        .iter()
        .any(|path| path.trim().is_empty() || path.starts_with("data:"))
    {
        return Err(protocol(
            "image_path_expected",
            "images must be local file paths, not embedded data URLs".into(),
        ));
    }
    if doc_questions.is_empty() {
        return Err(protocol(
            "questions_empty",
            "at least one question is required".into(),
        ));
    }
    if doc_questions.len() > MAX_QUESTIONS {
        return Err(protocol(
            "too_many_questions",
            format!("questions exceed the cap of {MAX_QUESTIONS}"),
        ));
    }
    let mut questions = Vec::with_capacity(doc_questions.len());
    for (qid, question) in doc_questions {
        if qid.is_empty() || qid.chars().count() > MAX_KEY_CHARS {
            return Err(protocol(
                "qid_invalid",
                format!("question id must be 1..={MAX_KEY_CHARS} chars: {qid:?}"),
            ));
        }
        if question.instructions.trim().is_empty() {
            return Err(protocol(
                "instructions_empty",
                format!("question {qid:?} has empty instructions"),
            ));
        }
        if question.instructions.chars().count() > MAX_INSTRUCTIONS_CHARS {
            return Err(protocol(
                "instructions_too_long",
                format!("question {qid:?} instructions exceed {MAX_INSTRUCTIONS_CHARS} chars"),
            ));
        }
        let kind = match question.kind.as_str() {
            "choice" => QuestionKind::Choice,
            "noul" => QuestionKind::Noul,
            other => {
                return Err(protocol(
                    "question_type_unknown",
                    format!("question {qid:?} type must be \"choice\" or \"noul\", got {other:?}"),
                ));
            }
        };
        let criteria = match (kind, question.criteria) {
            (QuestionKind::Choice, None) => {
                return Err(protocol(
                    "criteria_missing",
                    format!("choice question {qid:?} requires criteria"),
                ));
            }
            (QuestionKind::Choice, Some(criteria)) => criteria,
            (QuestionKind::Noul, Some(_)) => {
                return Err(protocol(
                    "noul_criteria_rejected",
                    format!("noul question {qid:?} must not carry criteria"),
                ));
            }
            (QuestionKind::Noul, None) => BTreeMap::new(),
        };
        if kind == QuestionKind::Choice {
            if criteria.is_empty() {
                return Err(protocol(
                    "criteria_empty",
                    format!("choice question {qid:?} has zero criteria"),
                ));
            }
            if criteria.len() > MAX_CRITERIA {
                return Err(protocol(
                    "criteria_count",
                    format!("choice question {qid:?} criteria exceed the cap of {MAX_CRITERIA}"),
                ));
            }
            for (key, value) in &criteria {
                if key.is_empty() || key.chars().count() > MAX_KEY_CHARS {
                    return Err(protocol(
                        "criterion_key_invalid",
                        format!(
                            "criterion key must be 1..={MAX_KEY_CHARS} chars in {qid:?}: {key:?}"
                        ),
                    ));
                }
                if value.trim().is_empty() || value.chars().count() > MAX_CRITERION_CHARS {
                    return Err(protocol(
                        "criterion_too_long",
                        format!(
                            "criterion {key:?} in {qid:?} must be 1..={MAX_CRITERION_CHARS} chars"
                        ),
                    ));
                }
                if key == NONE_KEY {
                    return Err(protocol(
                        "criterion_key_reserved",
                        format!("criterion key {NONE_KEY:?} is reserved (auto-offered)"),
                    ));
                }
            }
        }
        questions.push((
            qid,
            VisionQuestion {
                kind,
                instructions: question.instructions,
                criteria,
            },
        ));
    }
    Ok(VisionInput {
        state,
        images,
        questions,
    })
}

/// 请求就绪面:图像已读取/编码,文本面全部自有(request() 借用发出)。
#[derive(Debug)]
pub(super) struct VisionPrepared {
    state: String,
    images: Vec<String>,
    ids: Vec<String>,
    kinds: Vec<&'static str>,
    instructions: Vec<String>,
    criteria: Vec<BTreeMap<String, String>>,
    /// dry-run/审计用的图像元数据(路径+字节数;负载永不外泄)。
    pub image_meta: Vec<serde_json::Value>,
}

/// 输入 → 就绪:读文件、魔数/体积校验、base64 编码、拼防注入行。
pub(super) fn prepare(input: &VisionInput) -> Result<VisionPrepared, CtlError> {
    let mut images = Vec::with_capacity(input.images.len());
    let mut image_meta = Vec::with_capacity(input.images.len());
    for path in &input.images {
        let bytes = std::fs::read(path).map_err(|e| {
            CtlError::with_evidence(
                ErrorCode::Protocol,
                format!("cannot read image {path:?}: {e}"),
                serde_json::json!({ "reason": "image_unreadable", "path": path }),
            )
        })?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                format!("image {path:?} exceeds {MAX_IMAGE_BYTES} bytes"),
                serde_json::json!({
                    "reason": "image_too_large",
                    "path": path,
                    "bytes": bytes.len(),
                }),
            ));
        }
        if !bytes.starts_with(&PNG_MAGIC) {
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                format!("image {path:?} is not a PNG"),
                serde_json::json!({ "reason": "image_not_png", "path": path }),
            ));
        }
        image_meta.push(serde_json::json!({ "path": path, "bytes": bytes.len() }));
        images.push(format!("data:image/png;base64,{}", base64_encode(&bytes)));
    }
    let state = format!("{}\n{}", input.state.trim_end(), UNTRUSTED_NOTICE);
    let ordered = &input.questions;
    let ids = ordered.iter().map(|(id, _)| id.clone()).collect();
    let kinds = ordered
        .iter()
        .map(|(_, question)| match question.kind {
            QuestionKind::Choice => "choice",
            QuestionKind::Noul => "noul",
        })
        .collect();
    let instructions = ordered
        .iter()
        .map(|(_, question)| question.instructions.clone())
        .collect();
    let criteria = ordered
        .iter()
        .map(|(_, question)| question.criteria.clone())
        .collect();
    Ok(VisionPrepared {
        state,
        images,
        ids,
        kinds,
        instructions,
        criteria,
        image_meta,
    })
}

impl VisionPrepared {
    /// 就绪面 → wire 请求(借用构建,choice 问题强制注入 none)。
    pub fn request(&self) -> SystemOneRequest<'_> {
        let questions: BTreeMap<&str, WireQuestion<'_>> = self
            .ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let criteria = if self.kinds[index] == "choice" {
                    let mut offered: BTreeMap<&str, String> = self.criteria[index]
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.clone()))
                        .collect();
                    offered.insert(NONE_KEY, NONE_DESC.to_string());
                    Some(offered)
                } else {
                    None
                };
                (
                    id.as_str(),
                    WireQuestion {
                        kind: self.kinds[index],
                        instructions: self.instructions[index].as_str(),
                        criteria,
                    },
                )
            })
            .collect();
        SystemOneRequest {
            state: self.state.clone(),
            questions,
            images: Some(self.images.clone()),
        }
    }

    /// 审计摘要:state 截断 + 问题骨架 + 图像元数据——重放语料的请求侧,
    /// 不含图像负载。
    pub fn audit_summary(&self) -> serde_json::Value {
        let clip: String = self.state.chars().take(160).collect();
        let questions: serde_json::Map<String, serde_json::Value> = self
            .ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let criteria: Vec<&str> = if self.kinds[index] == "choice" {
                    self.criteria[index].keys().map(String::as_str).collect()
                } else {
                    Vec::new()
                };
                (
                    id.clone(),
                    serde_json::json!({ "type": self.kinds[index], "criteria": criteria }),
                )
            })
            .collect();
        serde_json::json!({
            "form": "vision",
            "state": clip,
            "images": self.image_meta,
            "questions": questions,
        })
    }
}

/// 视觉咨询应答:逐问题解析 + 双门槛门控,产出 Decision(answers 携带
/// 全部逐问题结果;恰一个 choice 问题时镜像到顶层便利字段)。
pub(super) fn consult(
    prepared: &VisionPrepared,
    provider: &dyn Provider,
    min_confidence: f64,
) -> Result<Decision, CtlError> {
    let request = prepared.request();
    let started = Instant::now();
    let (model, answers) = provider.ask(&request)?;
    let provider_ms = started.elapsed().as_millis() as u64;
    let mut per_question = BTreeMap::new();
    let mut candidates_sent = 0usize;
    let mut choice_count = 0usize;
    let mut first_choice: Option<serde_json::Value> = None;
    for (index, id) in prepared.ids.iter().enumerate() {
        let answer = answers.get(id.as_str()).cloned().ok_or_else(|| {
            provider_unavailable(
                &format!("provider returned no answer for question {id:?}"),
                None,
                None,
            )
        })?;
        let entry = if prepared.kinds[index] == "choice" {
            candidates_sent += prepared.criteria[index].len();
            let entry = choice_answer(&answer, &prepared.criteria[index], id, min_confidence)?;
            choice_count += 1;
            if choice_count == 1 {
                // 仅当最终恰一个 choice 问题时用作顶层镜像;≥2 时作废
                // (真机冒烟实证:双 choice 曾误镜像首题结果)。
                first_choice = Some(entry.clone());
            }
            entry
        } else {
            noul_answer(&answer, id)?
        };
        per_question.insert(id.clone(), entry);
    }
    // 顶层便利字段:恰一个 choice 问题时镜像该问题结果(多 choice/noul 混合
    // 形态的权威结果在 answers,顶层保持中立)。
    let (choice, abstain, confidence, top_choice) = match (choice_count, &first_choice) {
        (1, Some(entry)) => (
            entry
                .get("choice")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            entry
                .get("abstain")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            entry.get("confidence").and_then(serde_json::Value::as_f64),
            entry
                .get("top_choice")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        ),
        _ => (None, None, None, None),
    };
    Ok(Decision {
        card_id: "d-vision".into(),
        choice,
        abstain,
        confidence,
        top_choice,
        resolved: None,
        family: None,
        blocking: None,
        candidates_sent,
        dangerous_filtered: 0,
        provider_ms: Some(provider_ms),
        model,
        answers: Some(per_question),
    })
}

/// choice 应答解析:形状校验 + 幻觉拒绝 + 置信/裕度双门槛(与卡形态
/// answer_card 同款语义,逐问题适用)。
fn choice_answer(
    answer: &serde_json::Value,
    offered: &BTreeMap<String, String>,
    id: &str,
    min_confidence: f64,
) -> Result<serde_json::Value, CtlError> {
    if answer.get("type").and_then(serde_json::Value::as_str) != Some("choice") {
        return Err(provider_unavailable(
            &format!("question {id:?} was answered as a non-choice question"),
            answer.get("type").and_then(serde_json::Value::as_str),
            None,
        ));
    }
    let Some(choice) = answer.get("choice").and_then(serde_json::Value::as_str) else {
        return Err(provider_unavailable(
            &format!("question {id:?} answer has no choice"),
            None,
            None,
        ));
    };
    let Some(confidence) = answer.get("confidence").and_then(serde_json::Value::as_f64) else {
        return Err(provider_unavailable(
            &format!("question {id:?} answer has no confidence"),
            None,
            None,
        ));
    };
    if choice != NONE_KEY && !offered.contains_key(choice) {
        return Err(CtlError::with_evidence(
            ErrorCode::ProviderUnavailable,
            "provider chose an option that was not offered",
            serde_json::json!({
                "reason": "choice_not_offered",
                "question": id,
                "choice": choice,
                "offered": offered.keys().collect::<Vec<_>>(),
            }),
        ));
    }
    let probabilities = answer.get("probabilities").cloned();
    if choice == NONE_KEY {
        return Ok(serde_json::json!({
            "type": "choice",
            "abstain": "provider_none",
            "confidence": confidence,
        }));
    }
    if confidence < min_confidence {
        return Ok(serde_json::json!({
            "type": "choice",
            "abstain": "low_confidence",
            "confidence": confidence,
            "top_choice": choice,
        }));
    }
    let ranked: Vec<f64> = probabilities
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            let mut values: Vec<f64> = map
                .iter()
                .filter(|(key, _)| key.as_str() != NONE_KEY)
                .filter_map(|(_, value)| value.as_f64())
                .collect();
            values.sort_by(|a, b| b.total_cmp(a));
            values
        })
        .unwrap_or_default();
    let lead_margin = match (ranked.first(), ranked.get(1)) {
        (Some(&p1), Some(&p2)) => Some(p1 - p2),
        (Some(_), None) => Some(f64::INFINITY),
        _ => None,
    };
    if lead_margin.is_some_and(|margin| margin < MIN_LEAD_MARGIN) {
        return Ok(serde_json::json!({
            "type": "choice",
            "abstain": "low_margin",
            "confidence": confidence,
            "top_choice": choice,
        }));
    }
    let mut entry = serde_json::json!({
        "type": "choice",
        "choice": choice,
        "confidence": confidence,
    });
    if let Some(probabilities) = probabilities {
        entry["probabilities"] = probabilities;
    }
    Ok(entry)
}

/// noul 应答解析:概率直读(无门槛语义——概率不是决定,front 形态同款)。
fn noul_answer(answer: &serde_json::Value, id: &str) -> Result<serde_json::Value, CtlError> {
    if answer.get("type").and_then(serde_json::Value::as_str) != Some("noul") {
        return Err(provider_unavailable(
            &format!("question {id:?} was answered as a non-noul question"),
            answer.get("type").and_then(serde_json::Value::as_str),
            None,
        ));
    }
    let Some(noul) = answer.get("noul").and_then(serde_json::Value::as_f64) else {
        return Err(provider_unavailable(
            &format!("question {id:?} answer has no noul probability"),
            None,
            None,
        ));
    };
    Ok(serde_json::json!({ "type": "noul", "noul": noul }))
}

/// RFC 4648 base64 编码(标准字母表+padding)。手写以守「零新依赖」
/// (actl-cli 直接依赖面不因 wire 编码膨胀);纯函数,单测钉 RFC 向量。
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b1 = u32::from(chunk[0]);
        let b2 = chunk.get(1).copied().map_or(0, u32::from);
        let b3 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b1 << 16) | (b2 << 8) | b3;
        out.push(ALPHABET[((triple >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 规范 1×1 透明 PNG(67 字节,公共域常量):夹具仅魔数+可读即可,
    /// 本地校验不解码图像。
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn temp_png(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("actl-vision-{name}"));
        std::fs::write(&path, TINY_PNG).unwrap();
        path
    }

    fn doc(state: &str, images: &[String], questions: &str) -> String {
        let images: Vec<String> = images
            .iter()
            .map(|path| serde_json::json!(path).to_string())
            .collect();
        format!(
            "{{\"state\": {}, \"images\": [{}], \"questions\": {}}}",
            serde_json::json!(state),
            images.join(","),
            questions
        )
    }

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(TINY_PNG).len(), 92);
    }

    #[test]
    fn parse_validates_structure_with_reasons() {
        let png = temp_png("parse.png");
        let path = png.to_string_lossy().into_owned();
        let good_questions = "{\"cell\": {\"type\": \"choice\", \"instructions\": \"which cell\", \
             \"criteria\": {\"B2\": \"cell B2\"}}}";

        let empty_state = doc("   ", std::slice::from_ref(&path), good_questions);
        let err = parse(&empty_state).unwrap_err();
        assert!(err.message.contains("state"), "{err:?}");

        let no_images = doc("grid", &[], good_questions);
        let err = parse(&no_images).unwrap_err();
        assert!(err.message.contains("image"), "{err:?}");

        let unknown_type = doc(
            "grid",
            std::slice::from_ref(&path),
            "{\"q\": {\"type\": \"score\", \"instructions\": \"x\"}}",
        );
        let err = parse(&unknown_type).unwrap_err();
        assert!(err.message.contains("score"), "{err:?}");

        let missing_criteria = doc(
            "grid",
            &[path],
            "{\"q\": {\"type\": \"choice\", \"instructions\": \"which\"}}",
        );
        let err = parse(&missing_criteria).unwrap_err();
        assert!(err.message.contains("criteria"), "{err:?}");
    }

    #[test]
    fn parse_rejects_noul_criteria_and_reserved_none() {
        let png = temp_png("noul.png");
        let path = png.to_string_lossy().into_owned();
        let with_criteria = doc(
            "grid",
            std::slice::from_ref(&path),
            "{\"q\": {\"type\": \"noul\", \"instructions\": \"dark?\", \"criteria\": {\"a\": \"b\"}}}",
        );
        let err = parse(&with_criteria).unwrap_err();
        assert!(err.message.contains("noul"), "{err:?}");

        let reserved = doc(
            "grid",
            &[path],
            "{\"q\": {\"type\": \"choice\", \"instructions\": \"which\", \"criteria\": {\"none\": \"x\"}}}",
        );
        let err = parse(&reserved).unwrap_err();
        assert!(err.message.contains("reserved"), "{err:?}");
    }

    #[test]
    fn prepare_reads_png_and_rejects_other_magic() {
        let input = parse(&doc(
            "a grid",
            &[temp_png("ok.png").to_string_lossy().into_owned()],
            "{\"q\": {\"type\": \"noul\", \"instructions\": \"dark?\"}}",
        ))
        .unwrap();
        let prepared = prepare(&input).unwrap();
        assert_eq!(prepared.images.len(), 1);
        assert!(prepared.images[0].starts_with("data:image/png;base64,"));
        assert_eq!(
            prepared.image_meta[0]["bytes"],
            serde_json::json!(TINY_PNG.len())
        );

        let text_path = std::env::temp_dir().join("actl-vision-not.png");
        std::fs::write(&text_path, b"not a png").unwrap();
        let input = parse(&doc(
            "a grid",
            &[text_path.to_string_lossy().into_owned()],
            "{\"q\": {\"type\": \"noul\", \"instructions\": \"dark?\"}}",
        ))
        .unwrap();
        let err = prepare(&input).unwrap_err();
        assert!(err.message.contains("not a PNG"), "{err:?}");
    }

    #[test]
    fn request_forces_none_and_carries_images() {
        let input = parse(&doc(
            "a 3x3 grid",
            &[temp_png("req.png").to_string_lossy().into_owned()],
            "{\"cell\": {\"type\": \"choice\", \"instructions\": \"which cell\", \"criteria\": {\"B2\": \"cell B2\", \"C3\": \"cell C3\"}}, \"dark\": {\"type\": \"noul\", \"instructions\": \"dark theme?\"}}",
        ))
        .unwrap();
        let prepared = prepare(&input).unwrap();
        let request = prepared.request();
        let cell = &request.questions["cell"];
        let criteria = cell.criteria.as_ref().unwrap();
        assert!(criteria.contains_key("none"));
        assert!(criteria.contains_key("B2"));
        assert!(request.questions["dark"].criteria.is_none());
        assert!(request.state.contains(UNTRUSTED_NOTICE));
        assert!(
            request
                .images
                .as_ref()
                .is_some_and(|images| images.len() == 1)
        );
        // wire 序列化:images 字段在,none 描述语义正确
        let wire = serde_json::to_value(&request.questions).unwrap();
        assert_eq!(
            wire["cell"]["criteria"]["none"],
            serde_json::json!(NONE_DESC)
        );
    }

    /// 桩 provider:固定应答映射(与 front 测试同款形态)。
    struct StubProvider {
        body: serde_json::Value,
    }
    impl Provider for StubProvider {
        fn ask(
            &self,
            _request: &SystemOneRequest<'_>,
        ) -> Result<(Option<String>, serde_json::Value), CtlError> {
            Ok((Some("stub".into()), self.body.clone()))
        }
    }

    #[test]
    fn consult_gates_and_builds_answers() {
        let input = parse(&doc(
            "a grid",
            &[temp_png("consult.png").to_string_lossy().into_owned()],
            "{\"cell\": {\"type\": \"choice\", \"instructions\": \"which\", \"criteria\": {\"B2\": \"cell B2\"}}, \"dark\": {\"type\": \"noul\", \"instructions\": \"dark?\"}}",
        ))
        .unwrap();
        let prepared = prepare(&input).unwrap();

        // 应答:low_confidence(cell)+ noul 0.9
        let low = StubProvider {
            body: serde_json::json!({
                "cell": {"type": "choice", "choice": "B2", "confidence": 0.3},
                "dark": {"type": "noul", "noul": 0.9},
            }),
        };
        let decision = consult(&prepared, &low, 0.5).unwrap();
        assert_eq!(decision.abstain.as_deref(), Some("low_confidence"));
        assert_eq!(decision.top_choice.as_deref(), Some("B2"));
        let answers = decision.answers.as_ref().unwrap();
        assert_eq!(answers["dark"]["noul"], serde_json::json!(0.9));
        assert_eq!(
            answers["cell"]["abstain"],
            serde_json::json!("low_confidence")
        );

        // 应答:过门槛(裕度无穷——单候选)
        let ok = StubProvider {
            body: serde_json::json!({
                "cell": {"type": "choice", "choice": "B2", "confidence": 0.9,
                         "probabilities": {"B2": 0.9, "none": 0.1}},
                "dark": {"type": "noul", "noul": 0.1},
            }),
        };
        let decision = consult(&prepared, &ok, 0.5).unwrap();
        assert_eq!(decision.choice.as_deref(), Some("B2"));
        assert_eq!(decision.candidates_sent, 1);

        // 幻觉拒绝:选项不在 offered 集
        let hallucinated = StubProvider {
            body: serde_json::json!({
                "cell": {"type": "choice", "choice": "Z9", "confidence": 0.99},
                "dark": {"type": "noul", "noul": 0.5},
            }),
        };
        let err = consult(&prepared, &hallucinated, 0.5).unwrap_err();
        assert!(err.message.contains("not offered"), "{err:?}");

        // none 应答:provider_none 弃权
        let none_answer = StubProvider {
            body: serde_json::json!({
                "cell": {"type": "choice", "choice": "none", "confidence": 0.8},
                "dark": {"type": "noul", "noul": 0.5},
            }),
        };
        let decision = consult(&prepared, &none_answer, 0.5).unwrap();
        assert_eq!(decision.abstain.as_deref(), Some("provider_none"));
    }

    #[test]
    fn consult_two_choice_questions_do_not_mirror_toplevel() {
        // 真机冒烟实证的形态:双 choice 问题只镜像 answers,顶层保持中立
        // (恰一个 choice 时才镜像——修复前曾误镜像第一个 choice 的结果)。
        let input = parse(&doc(
            "two grids",
            &[temp_png("two.png").to_string_lossy().into_owned()],
            "{\"first\": {\"type\": \"choice\", \"instructions\": \"pick A\", \
              \"criteria\": {\"B2\": \"cell B2\"}}, \
              \"second\": {\"type\": \"choice\", \"instructions\": \"pick B\", \
              \"criteria\": {\"C3\": \"cell C3\"}}}",
        ))
        .unwrap();
        let prepared = prepare(&input).unwrap();
        let provider = StubProvider {
            body: serde_json::json!({
                "first": {"type": "choice", "choice": "B2", "confidence": 0.9},
                "second": {"type": "choice", "choice": "C3", "confidence": 0.9},
            }),
        };
        let decision = consult(&prepared, &provider, 0.5).unwrap();
        assert!(decision.choice.is_none(), "toplevel stays neutral");
        assert!(decision.abstain.is_none());
        let answers = decision.answers.as_ref().unwrap();
        assert_eq!(answers["first"]["choice"], "B2");
        assert_eq!(answers["second"]["choice"], "C3");
    }
}
