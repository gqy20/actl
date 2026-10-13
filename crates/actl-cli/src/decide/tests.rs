//! decide 单测(file-lines 门禁:>1500 拒绝线,测试体拆出;与生产代码
//! 同 crate,经 `#[path]` 挂回 decide 模块树,`use super::*` 语义不变)。

use super::*;
use actl_core::decision::{CardCandidate, CardDiff};

fn card(candidates: Vec<CardCandidate>) -> DecisionCard {
    DecisionCard {
        version: 1,
        card_id: "d-test".into(),
        task: "保存文档".into(),
        app: "记事本".into(),
        goal: "verify-file ..".into(),
        step: 3,
        diff: CardDiff {
            added: 2,
            removed: 0,
            changed: 1,
            added_names: vec![],
            removed_names: vec![],
        },
        candidates,
    }
}

fn candidate(id: &str, name: &str, dangerous: bool, score: u32) -> CardCandidate {
    CardCandidate {
        id: id.into(),
        selector: format!("name:{name}"),
        role: "Button".into(),
        name: Some(name.into()),
        automation_id: None,
        effect: None,
        context: None,
        score,
        dangerous,
    }
}

struct FixedProvider {
    choice: &'static str,
    confidence: f64,
    model: Option<String>,
    /// 可选概率分布(lead-margin 门槛用;缺省=provider 未回分布,
    /// 裕度门槛让位——旧 provider 兼容路径)。
    probabilities: Option<serde_json::Value>,
}

impl Provider for FixedProvider {
    fn ask(
        &self,
        _request: &SystemOneRequest<'_>,
    ) -> Result<(Option<String>, serde_json::Value), CtlError> {
        let mut pick = serde_json::json!({
            "type": "choice", "choice": self.choice, "confidence": self.confidence,
        });
        if let Some(probabilities) = &self.probabilities {
            pick["probabilities"] = probabilities.clone();
        }
        Ok((self.model.clone(), serde_json::json!({"pick": pick})))
    }
}

#[test]
fn build_request_strips_dangerous_and_forces_none() {
    let card = card(vec![
        candidate("c1", "保存", false, 8),
        candidate("c2", "删除全部", true, 9),
    ]);
    let (request, dangerous) = build_request(&card);
    assert_eq!(dangerous, 1);
    let request = request.expect("safe candidates remain");
    assert!(!request.state.contains("删除全部"), "dangerous invisible");
    assert!(request.state.contains("保存"));
    assert!(request.state.contains(UNTRUSTED_NOTICE));
    assert!(
        request.questions["pick"]
            .criteria
            .as_ref()
            .unwrap()
            .contains_key("none")
    );
    assert_eq!(
        request.questions["pick"].criteria.as_ref().unwrap().len(),
        2,
        "c1 + none"
    );
}

#[test]
fn all_dangerous_abstains_without_provider_call() {
    let card = card(vec![candidate("c1", "格式化", true, 9)]);
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.99,
            model: None,
            probabilities: None,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("dangerous_only"));
    assert_eq!(decision.choice, None);
    assert_eq!(decision.dangerous_filtered, 1);
}

#[test]
fn low_confidence_abstains_with_top_choice() {
    let card = card(vec![candidate("c1", "保存", false, 8)]);
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.37,
            model: None,
            probabilities: None,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("low_confidence"));
    assert_eq!(decision.top_choice.as_deref(), Some("c1"));
    assert_eq!(decision.choice, None);
}

#[test]
fn confident_answer_passes_gate() {
    let card = card(vec![candidate("c1", "保存", false, 8)]);
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.82,
            model: Some("kev-4b".into()),
            probabilities: None,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.choice.as_deref(), Some("c1"));
    assert_eq!(decision.confidence, Some(0.82));
    assert_eq!(decision.abstain, None);
}

#[test]
fn provider_none_is_abstain_not_error() {
    let card = card(vec![candidate("c1", "设置", false, 4)]);
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "none",
            confidence: 0.7,
            model: None,
            probabilities: None,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("provider_none"));
    assert_eq!(decision.choice, None);
}

#[test]
fn hallucinated_choice_is_rejected() {
    let card = card(vec![candidate("c1", "保存", false, 8)]);
    let error = answer_card(
        &card,
        &FixedProvider {
            choice: "c99",
            confidence: 0.99,
            model: None,
            probabilities: None,
        },
        0.5,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ProviderUnavailable);
    assert!(error.message.contains("not offered"));
}

#[test]
fn parse_input_accepts_bare_and_envelope_forms() {
    let bare = card(vec![candidate("c1", "保存", false, 8)]);
    let text = serde_json::to_string(&bare).unwrap();
    assert!(matches!(
        parse_input(&text, None).unwrap(),
        InputForm::Card(_)
    ));
    let envelope = serde_json::json!({
        "command": "flow-status",
        "data": { "decision_card": serde_json::to_value(&bare).unwrap() }
    });
    let parsed = parse_input(&envelope.to_string(), None).unwrap();
    let InputForm::Card(parsed) = parsed else {
        panic!("envelope form must parse as card");
    };
    assert_eq!(parsed.card_id, "d-test");
    assert!(parse_input("{}", None).is_err());
    assert!(parse_input("not json", None).is_err());
    // 卡形态配 --task = PROTOCOL(意图只对 AMBIGUOUS 形态有意义)
    assert!(parse_input(&text, Some("点击保存")).is_err());
}

#[test]
fn ambiguous_envelope_requires_task_and_maps_candidates() {
    let envelope = serde_json::json!({
        "version": "1", "ok": false, "command": "click",
        "error": {
            "code": "AMBIGUOUS",
            "message": "name:浏览 matches 2 elements",
            "evidence": { "candidates": [
                {"ref": "@e3", "role": "Button", "name": "浏览", "automation_id": "btnBrowse1", "context": "文件名行"},
                {"ref": "@e7", "role": "Button", "name": "浏览", "automation_id": "btnBrowse2", "context": "联机图片区"}
            ]}
        }
    });
    let text = envelope.to_string();
    // 无 --task:PROTOCOL + 结构化 evidence + 专属 hint
    let error = parse_input(&text, None).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert_eq!(error.evidence.unwrap()["reason"], "task_missing");
    // 有 --task:合成卡,候选序即证据序,context 保留
    let InputForm::Ambiguous(card, metas, _) =
        parse_input(&text, Some("打开本地文件选择器")).unwrap()
    else {
        panic!("ambiguous form expected");
    };
    assert_eq!(card.candidates.len(), 2);
    assert_eq!(card.candidates[0].id, "c1");
    assert_eq!(card.candidates[0].context.as_deref(), Some("文件名行"));
    assert_eq!(card.candidates[0].selector, "@e3");
    assert_eq!(metas[1].ref_id, "@e7");
    // 危险词法闸独立重算:危险候选入卡但带 dangerous 位(构造侧剥离)
    let envelope = serde_json::json!({
        "error": {
            "code": "AMBIGUOUS",
            "evidence": { "candidates": [
                {"ref": "@e1", "role": "Button", "name": "删除"},
                {"ref": "@e2", "role": "Button", "name": "保存"}
            ]}
        }
    });
    let InputForm::Ambiguous(card, _, _) =
        parse_input(&envelope.to_string(), Some("保存文档")).unwrap()
    else {
        panic!("ambiguous form expected");
    };
    assert!(card.candidates[0].dangerous);
    assert!(!card.candidates[1].dangerous);
    // 零候选/缺 evidence = PROTOCOL
    assert!(
        parse_input(
            &serde_json::json!({"error": {"code": "AMBIGUOUS", "evidence": {"candidates": []}}})
                .to_string(),
            Some("t")
        )
        .is_err()
    );
}

#[test]
fn config_requires_endpoint() {
    let error = load_config(std::path::Path::new("Z:/definitely-missing.json"));
    assert!(error.is_err());
    let dir = std::env::temp_dir().join(format!("actl-decide-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("decision.json");
    std::fs::write(&path, r#"{"timeout_ms": 5000}"#).unwrap();
    let error = load_config(&path).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    let _ = std::fs::remove_dir_all(&dir);
}

fn front(surfaces: usize, front_clear: bool) -> front::FrontInput {
    let mut state = String::from("Front guidance layer.\nGuidance surfaces:\n");
    for i in 0..surfaces {
        state.push_str(&format!("owned | 对话框{i} | [确定] | 提示文本\n"));
    }
    front::FrontInput {
        front_clear,
        state,
        surface_count: surfaces,
    }
}

struct FrontProvider {
    family: &'static str,
    confidence: f64,
    blocking: f64,
}

impl Provider for FrontProvider {
    fn ask(
        &self,
        request: &SystemOneRequest<'_>,
    ) -> Result<(Option<String>, serde_json::Value), CtlError> {
        assert!(
            request.questions.contains_key("family") && request.questions.contains_key("blocking"),
            "family + blocking ride one request"
        );
        Ok((
            Some("kev-test".into()),
            serde_json::json!({
                "family": {"type": "choice", "choice": self.family, "confidence": self.confidence},
                "blocking": {"type": "noul", "noul": self.blocking}
            }),
        ))
    }
}

#[test]
fn front_clear_short_circuits_without_provider() {
    // front_clear 诚实「无引导层」:NeverProvider 若被触达会 INTERNAL——
    // 短路必须先于任何网络调用。
    let decision = front_consult(&front(2, true), &NeverProvider, 0.5).unwrap();
    assert_eq!(decision.family.as_deref(), Some("front_clear"));
    assert_eq!(decision.blocking, Some(0.0));
    assert_eq!(decision.provider_ms, None);
    // 零表面(in_tree_skipped 形态)同样免问
    let decision = front_consult(&front(0, false), &NeverProvider, 0.5).unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("no_surfaces"));
}

#[test]
fn front_consult_classifies_and_reports_blocking() {
    let decision = front_consult(
        &front(1, false),
        &FrontProvider {
            family: "save",
            confidence: 0.9,
            blocking: 0.85,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.family.as_deref(), Some("save"));
    assert_eq!(decision.blocking, Some(0.85));
    assert_eq!(decision.abstain, None);
    assert_eq!(decision.card_id, "d-front");
}

#[test]
fn front_hallucinated_family_is_rejected() {
    let error = front_consult(
        &front(1, false),
        &FrontProvider {
            family: "definitely_not_a_family",
            confidence: 0.99,
            blocking: 0.5,
        },
        0.5,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ProviderUnavailable);
    assert_eq!(error.evidence.unwrap()["reason"], "choice_not_offered");
}

#[test]
fn front_low_confidence_abstains_with_top_family() {
    let decision = front_consult(
        &front(1, false),
        &FrontProvider {
            family: "confirm",
            confidence: 0.4,
            blocking: 0.7,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("low_confidence"));
    assert_eq!(decision.top_choice.as_deref(), Some("confirm"));
    assert_eq!(decision.family, None, "不冒充可信判读");
    assert_eq!(decision.blocking, Some(0.7), "概率照报,概率不是决定");
    // unknown 家族低置信不是 abstain——unknown 本身就是交回人的诚实出口
    let decision = front_consult(
        &front(1, false),
        &FrontProvider {
            family: "unknown",
            confidence: 0.3,
            blocking: 0.2,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.family.as_deref(), Some("unknown"));
    assert_eq!(decision.abstain, None);
}

#[test]
fn front_input_parses_envelope_and_renders_state() {
    let envelope = serde_json::json!({
        "version": "1", "ok": true, "command": "front-report",
        "data": {
            "front_clear": false,
            "owned": [{"title": "应用程序错误", "pid": 12, "hwnd": 34, "visible": true}],
            "in_tree": [{"name": "计算器", "classification": "unknown",
                "buttons": ["最小化", "关闭"], "text": "计算器 表达式"}]
        }
    });
    let InputForm::Front(input) =
        parse_input(&envelope.to_string(), Some("导出表格为 CSV")).unwrap()
    else {
        panic!("front form expected");
    };
    assert_eq!(input.surface_count, 2);
    assert!(input.state.contains("应用程序错误"));
    assert!(input.state.contains("最小化 / 关闭"));
    assert!(input.state.contains("untrusted data"));
    // front_clear 形态照实读入(短路在 consult 层)
    let envelope = serde_json::json!({
        "version": "1", "ok": true, "command": "front-report",
        "data": {"front_clear": true, "owned": [], "in_tree": []}
    });
    let InputForm::Front(input) = parse_input(&envelope.to_string(), Some("任务")).unwrap()
    else {
        panic!("front form expected");
    };
    assert!(input.front_clear);
    assert_eq!(input.surface_count, 0);
    // 缺 --task:task_missing
    let error = parse_input(
            &serde_json::json!({"command": "front-report", "data": {"front_clear": false, "owned": []}}).to_string(),
            None,
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert_eq!(error.evidence.unwrap()["reason"], "task_missing");
}

#[test]
fn lead_margin_gate_abstains_on_thin_margin() {
    let card = card(vec![
        candidate("c1", "导出", false, 8),
        candidate("c2", "另存为", false, 6),
    ]);
    // 薄裕度(探针 t_g 实证形态):conf 过门槛但次名有实质权重——
    // conf 单门槛会放行,裕度门槛拦下(top_choice 照报供判读)。
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.9,
            model: None,
            probabilities: Some(serde_json::json!({"c1": 0.55, "c2": 0.40, "none": 0.05})),
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.abstain.as_deref(), Some("low_margin"));
    assert_eq!(decision.top_choice.as_deref(), Some("c1"));
    assert_eq!(decision.choice, None);
    // 宽裕度(calc-tie 真机实证形态 0.848/0.05):自动放行
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.848,
            model: None,
            probabilities: Some(serde_json::json!({"c1": 0.85, "c2": 0.05, "none": 0.10})),
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.choice.as_deref(), Some("c1"));
    assert_eq!(decision.abstain, None);
    // 旧 provider 无概率分布:裕度让位(不能因分布缺席废掉整答),
    // conf 门槛独立工作——向后兼容路径。
    let decision = answer_card(
        &card,
        &FixedProvider {
            choice: "c1",
            confidence: 0.8,
            model: None,
            probabilities: None,
        },
        0.5,
    )
    .unwrap();
    assert_eq!(decision.choice.as_deref(), Some("c1"));
}

#[test]
fn ambiguous_truncation_flag_parses() {
    // 截断护栏的解析面:>8 候选被 ambiguous_error 截断的证据带
    // truncated:true;decide 消费侧见标志即免网弃权(run 臂)。
    let envelope = serde_json::json!({
        "error": {"code": "AMBIGUOUS", "evidence": {
            "candidates": [
                {"ref": "@e1", "role": "Button", "name": "a"},
                {"ref": "@e2", "role": "Button", "name": "b"}
            ],
            "total": 12, "returned": 2, "truncated": true
        }}
    });
    let InputForm::Ambiguous(_, _, truncated) =
        parse_input(&envelope.to_string(), Some("意图")).unwrap()
    else {
        panic!("ambiguous form expected");
    };
    assert!(truncated, "truncation flag reaches the consumer");
    // 旧格式信封(无标志)按未截断——不能拒绝旧证据
    let legacy = serde_json::json!({
        "error": {"code": "AMBIGUOUS", "evidence": {
            "candidates": [{"ref": "@e1", "role": "Button", "name": "a"}]
        }}
    });
    let InputForm::Ambiguous(_, _, truncated) =
        parse_input(&legacy.to_string(), Some("意图")).unwrap()
    else {
        panic!("ambiguous form expected");
    };
    assert!(!truncated);
}

#[test]
fn front_owned_texts_render_into_state() {
    // ④信号增强(2026-10-13):owned 表面的 texts[](原生子窗 Static 全文,
    // front-report EnumChildWindows 采集)拼进判读 state——A/B 实证
    // 同一崩溃框 conf 0.32→0.622 跨门槛,判读信号恢复的主通道。
    let envelope = serde_json::json!({
        "version": "1", "ok": true, "command": "front-report",
        "data": {
            "anchor_foreground": true, "front_clear": false,
            "owned": [{
                "title": "wmexternal.exe - Fail Fast Exception",
                "class": "#32770", "hwnd": 395238, "visible": true, "foreground": true,
                "texts": ["应用程序错误: 应用程序无法正常启动(0xc0000409)。Fail Fast。"],
                "buttons": ["确定"]
            }],
            "in_tree": []
        }
    });
    let InputForm::Front(input) =
        parse_input(&envelope.to_string(), Some("continue the meeting flow")).unwrap()
    else {
        panic!("front form expected");
    };
    assert_eq!(input.surface_count, 1);
    assert!(
        input.state.contains("应用程序错误"),
        "subwindow text reaches the consult state: {}",
        input.state
    );
    assert!(input.state.contains("[确定]"), "buttons render");
}
