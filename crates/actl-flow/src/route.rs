//! 探索路线回写(docs/42 S3):从 goal_met 的探索运行抽取 pack 格式路线
//! 片段 + quirks 建议 + suggested_priors。**自动化只做证据收集**——片段
//! status 恒为 `unverified`,`verified` 迁移必须人工复核(PACKS.md 修订
//! 纪律 + AGENTS「修正与验收分开」);本模块不写 pack 文件、不迁状态,
//! 合并与迁移由调用方(agent/人)在 git 里完成。
//!
//! 分层约束(PACKS.md):核心 CLI 永不读 pack——本模块只产出片段文本,
//! 不感知任何 pack 文件。`{window}` 占位替换 spec.app 的精确出现(集成方
//! 的 pack `window` 须与锚点同族,PACKS.md 占位符纪律)。

use crate::ExploreSpec;
use crate::explore::ExploreState;
use actl_core::{CtlError, ErrorCode};
use serde::Serialize;
use std::collections::BTreeMap;

/// 回写证据头:run id 与 token 审计,随片段落注释,PR review 可直接核对。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteEvidence {
    pub run_id: String,
    pub actions: u32,
    pub decisions: u32,
    pub observations: u32,
    pub errors: u32,
    pub observation_bytes: u64,
    pub card_bytes: u64,
    pub goal_retries: u32,
}

/// quirks 建议片段(死候选/无效果候选的沉淀;note 为事实性描述)。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuirkFragment {
    pub id: String,
    pub note: String,
}

/// 探索路线片段:结构化数据 + 可渲染 YAML,状态恒 `unverified`。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RouteFragment {
    pub route_id: String,
    pub description: String,
    pub status: &'static str,
    /// 该路线的核对方法(PACKS.md verify 字段:自由文本)。
    pub verify: String,
    /// 步骤序列:acted 点击 + 终态 goal 断言;`--app` 已替换为 `{window}`。
    pub steps: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub quirks: Vec<QuirkFragment>,
    /// 下一轮探索的 priors 数据源(统计剪枝闭环):dead → -4,no_effect → -2。
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub suggested_priors: BTreeMap<String, i32>,
    pub evidence: RouteEvidence,
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            'a'..='z' | '0'..='9' => out.push(ch),
            'A'..='Z' => out.push(ch.to_ascii_lowercase()),
            _ => out.push('_'),
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "route".into()
    } else {
        trimmed.into()
    }
}

/// 从探索状态抽取路线片段。前置:run 已 completed 且 goal_met=true;
/// action_log 无截断(达到上限时拒绝——路线不完整宁可不给)。
/// `run_id` 由调用方提供(store 目录名),写入证据头与描述。
pub fn extract(state: &ExploreState, run_id: &str) -> Result<RouteFragment, CtlError> {
    if state.goal_met != Some(true) {
        return Err(CtlError::with_evidence(
            ErrorCode::Protocol,
            "route export requires a completed exploration with goal_met=true",
            serde_json::json!({"status_hint": "keep the blocked run as evidence; quirk export from failed runs is a separate path"}),
        ));
    }
    const LOG_CAP: usize = crate::explore::ACTION_LOG_CAP;
    if state.action_log.len() >= LOG_CAP {
        return Err(CtlError::protocol(
            "action log reached its cap; the route would be truncated and is not exported",
        ));
    }
    let spec: &ExploreSpec = &state.spec;
    let steps: Vec<Vec<String>> = state
        .action_log
        .iter()
        .filter(|entry| entry.outcome == "acted")
        .map(|entry| {
            // 输入原语:text 动作导出为 type 步骤(文本随证据保存);其余为 click。
            // 通道判据与引擎执行侧同口径(非 ASCII → --paste,record.rs 内容子句):
            // 导出步骤直接可重放,中文路线不在重放时退回键盘通道。
            match &entry.text {
                Some(text) => {
                    let mut step = vec!["type".into(), entry.selector.clone(), text.clone()];
                    if !text.is_ascii() {
                        step.push("--paste".into());
                    }
                    step.push("--app".into());
                    step.push("{window}".into());
                    step
                }
                None => vec![
                    "click".into(),
                    entry.selector.clone(),
                    "--app".into(),
                    "{window}".into(),
                ],
            }
        })
        .chain(std::iter::once(goal_step(spec)))
        .collect();
    if steps.len() < 2 {
        return Err(CtlError::protocol(
            "route requires at least one acted click and the goal assertion",
        ));
    }
    let quirks: Vec<QuirkFragment> = state
        .tried
        .iter()
        .filter(|(_, effect)| effect.as_str() == "dead")
        .map(|(selector, _)| QuirkFragment {
            id: format!(
                "explore_{}_dead",
                slug(selector.trim_start_matches("id:").trim_start_matches("name:"))
            ),
            note: format!(
                "探索 run {run_id} 真机:selector {selector} probe 判死(虚拟化或瞬态);重新观察后可恢复,勿据单次 NOT_FOUND 判定控件不存在"
            ),
        })
        .collect();
    let mut suggested_priors = BTreeMap::new();
    for (selector, effect) in &state.tried {
        match effect.as_str() {
            "dead" => {
                suggested_priors.insert(selector.clone(), -4);
            }
            "no_effect" => {
                suggested_priors.insert(selector.clone(), -2);
            }
            _ => {}
        }
    }
    let goal_text = format!("{} {}", spec.goal.command, spec.goal.args.join(" "))
        .replace(&spec.app, "{window}");
    Ok(RouteFragment {
        route_id: format!("explore_{}", slug(run_id)),
        description: format!("{}——探索回写(run {})", spec.task, run_id),
        status: "unverified",
        verify: format!(
            "目标断言:{goal_text};人工复核后按 PACKS 纪律迁移 verified,证据见 run {run_id}"
        ),
        steps,
        quirks,
        suggested_priors,
        evidence: RouteEvidence {
            run_id: run_id.to_owned(),
            actions: state.actions,
            decisions: state.decisions,
            observations: state.observations,
            errors: state.errors,
            observation_bytes: state.audit.observation_bytes,
            card_bytes: state.audit.card_bytes,
            goal_retries: state.goal_retries,
        },
    })
}

/// goal 断言步骤:命令 + 参数,`--app <spec.app>` 值替换为 `{window}`。
fn goal_step(spec: &ExploreSpec) -> Vec<String> {
    let mut step = vec![spec.goal.command.clone()];
    for arg in &spec.goal.args {
        if arg == &spec.app {
            step.push("{window}".into());
        } else {
            step.push(arg.clone());
        }
    }
    step
}

/// YAML 标量渲染:需要时双引号(冒号/引号/控制字符/首尾空白/空串/流标点)。
/// 注意:{input: name} 绑定**不走这里**(arg_scalar)——它必须是 YAML 流映射
/// 而非字符串(inputs 替换只认映射对象,真机发现 #6)。
pub(crate) fn arg_scalar(text: &str) -> String {
    if is_input_binding(text) {
        return text.into();
    }
    yaml_scalar(text)
}

/// `{input: name}` 形态的完整参数绑定(generalize_windows 生成的精确格式)。
pub(crate) fn is_input_binding(text: &str) -> bool {
    text.starts_with("{input: ") && text.ends_with('}') && !text[8..text.len() - 1].contains('}')
}

pub(crate) fn yaml_scalar(text: &str) -> String {
    // 数字/布尔/null 形态的纯量必须加引号:YAML 会把它们解析成非字符串,
    // 而 flow schema 要求 args 全字符串(真机发现 #5:--timeout 8022 裸数字
    // 被解析为整数,flow-validate 拒绝;离线测试的 wait 轮没断言该参数)。
    let parses_non_string = text.parse::<f64>().is_ok()
        || matches!(
            text,
            "true"
                | "false"
                | "null"
                | "~"
                | "True"
                | "False"
                | "Null"
                | "TRUE"
                | "FALSE"
                | "NULL"
                | "yes"
                | "no"
                | "on"
                | "off"
                | "Yes"
                | "No"
                | "On"
                | "Off"
        );
    let needs_quote = text.is_empty()
        || parses_non_string
        || text.starts_with(' ')
        || text.ends_with(' ')
        || text.chars().any(|c| {
            matches!(
                c,
                ':' | '"'
                    | '\''
                    | '\n'
                    | '\t'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '#'
                    | '&'
                    | '*'
                    | '!'
                    | '|'
                    | '>'
                    | '%'
                    | '@'
                    | ','
            )
        });
    if !needs_quote {
        return text.into();
    }
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn yaml_flow_seq(items: &[String], indent: usize) -> String {
    let pad = " ".repeat(indent);
    let rendered: Vec<String> = items.iter().map(|s| yaml_scalar(s)).collect();
    format!("{pad}- [{}]", rendered.join(", "))
}

/// 渲染 pack 格式 YAML 片段(注释证据头 + routes + 可选 quirks),可直接
/// 粘贴进 `apps/<slug>.yml` 的对应分节;`cargo xtask lint-packs` 为最终校验。
pub fn render_yaml(fragment: &RouteFragment) -> String {
    let mut out = String::new();
    out.push_str("# 探索回写(explore-writeback):自动化只收集证据,status 恒 unverified;\n");
    out.push_str("# verified 迁移须人工复核(PACKS.md 修订纪律),replay 证据随 PR 提交。\n");
    out.push_str(&format!(
        "# 证据:run {} · actions {} · decisions {} · observations {} · errors {} · goal_retries {} · observation_bytes {} · card_bytes {}\n",
        fragment.evidence.run_id,
        fragment.evidence.actions,
        fragment.evidence.decisions,
        fragment.evidence.observations,
        fragment.evidence.errors,
        fragment.evidence.goal_retries,
        fragment.evidence.observation_bytes,
        fragment.evidence.card_bytes
    ));
    out.push_str("routes:\n");
    out.push_str(&format!(
        "  - id: {}\n    description: {}\n    status: {}\n    verify: {}\n    steps:\n",
        yaml_scalar(&fragment.route_id),
        yaml_scalar(&fragment.description),
        fragment.status,
        yaml_scalar(&fragment.verify)
    ));
    for step in &fragment.steps {
        out.push_str(&yaml_flow_seq(step, 6));
        out.push('\n');
    }
    if !fragment.quirks.is_empty() {
        out.push_str("quirks:\n");
        for quirk in &fragment.quirks {
            out.push_str(&format!(
                "  - id: {}\n    note: {}\n",
                yaml_scalar(&quirk.id),
                yaml_scalar(&quirk.note)
            ));
        }
    }
    if !fragment.suggested_priors.is_empty() {
        out.push_str("# suggested_priors(下一轮 explore.priors 数据源):dead=-4, no_effect=-2\n");
        for (selector, adjustment) in &fragment.suggested_priors {
            out.push_str(&format!("#   {}: {}\n", yaml_scalar(selector), adjustment));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explore::{ActionRecord, TokenAudit};
    use crate::{Budgets, Invocation};
    use actl_core::snapshot::ElementOut;

    fn spec() -> ExploreSpec {
        ExploreSpec {
            task: "在计算器中按下数字七".into(),
            app: "计算器".into(),
            keywords: vec!["七".into()],
            goal: Invocation {
                command: "verify".into(),
                args: vec![
                    "--value".into(),
                    "id:CalculatorResults".into(),
                    "显示为 7".into(),
                    "--property".into(),
                    "name".into(),
                    "--app".into(),
                    "计算器".into(),
                ],
                bind: Default::default(),
            },
            priors: BTreeMap::new(),
            texts: BTreeMap::new(),
            budgets: Budgets::default(),
        }
    }

    fn state(
        goal_met: bool,
        log: Vec<ActionRecord>,
        tried: BTreeMap<String, String>,
    ) -> ExploreState {
        ExploreState {
            version: 1,
            spec: spec(),
            started_ms: 1,
            actions: log.len() as u32,
            decisions: 0,
            observations: 2,
            errors: 0,
            tried,
            seen: Default::default(),
            visited: Default::default(),
            widened: false,
            anchor_app: None,
            anchor_stack: Vec::new(),
            last_fingerprint: None,
            last_elements: vec![ElementOut {
                ref_id: None,
                role: "Button".into(),
                name: Some("七".into()),
                automation_id: Some("num7Button".into()),
                depth: 1,
            }],
            last_actor: None,
            pending: None,
            queue: Vec::new(),
            action_log: log,
            answers: Vec::new(),
            audit: TokenAudit {
                observation_bytes: 828,
                card_bytes: 556,
            },
            frames_truncated: 0,
            goal_retries: 0,
            observation_retries: 0,
            window_pid: None,
            window_title: None,
            snapshot_id: None,
            goal_window: None,
            anchor_transfers: 0,
            probe_episodes: 0,
            goal_met: Some(goal_met),
        }
    }

    fn acted(selector: &str, mode: &str) -> ActionRecord {
        ActionRecord {
            step: 1,
            selector: selector.into(),
            mode: mode.into(),
            outcome: "acted".into(),
            text: None,
        }
    }

    fn typed(selector: &str, text: &str) -> ActionRecord {
        ActionRecord {
            step: 2,
            selector: selector.into(),
            mode: "auto".into(),
            outcome: "acted".into(),
            text: Some(text.into()),
        }
    }

    #[test]
    fn extract_requires_goal_met() {
        let failed = state(
            false,
            vec![acted("id:num7Button", "auto")],
            Default::default(),
        );
        let error = extract(&failed, "run-x").unwrap_err();
        assert_eq!(error.code, ErrorCode::Protocol);
        assert!(error.message.contains("goal_met"));
    }

    #[test]
    fn extract_rejects_truncated_action_log() {
        let log: Vec<ActionRecord> = (0..crate::explore::ACTION_LOG_CAP)
            .map(|i| ActionRecord {
                step: i as u32 + 1,
                selector: format!("id:b{i}"),
                mode: "auto".into(),
                outcome: "acted".into(),
                text: None,
            })
            .collect();
        let full = state(true, log, Default::default());
        assert!(
            extract(&full, "run-x")
                .unwrap_err()
                .message
                .contains("truncated")
        );
    }

    #[test]
    fn extract_builds_route_with_window_substitution() {
        let log = vec![
            acted("id:num8Button", "answer"),
            acted("id:num7Button", "fallback"),
        ];
        let mut tried = BTreeMap::new();
        tried.insert("id:num8Button".to_string(), "no_effect".into());
        tried.insert("id:ClearMemoryButton".to_string(), "dead".into());
        let s = state(true, log, tried);
        let fragment = extract(&s, "explore-s2-calc-tie").unwrap();
        // 点击序列 + 终态 goal 断言;--app 已替换 {window}
        assert_eq!(
            fragment.steps,
            vec![
                vec![
                    "click".to_string(),
                    "id:num8Button".to_string(),
                    "--app".to_string(),
                    "{window}".to_string()
                ],
                vec![
                    "click".to_string(),
                    "id:num7Button".to_string(),
                    "--app".to_string(),
                    "{window}".to_string()
                ],
                vec![
                    "verify".to_string(),
                    "--value".to_string(),
                    "id:CalculatorResults".to_string(),
                    "显示为 7".to_string(),
                    "--property".to_string(),
                    "name".to_string(),
                    "--app".to_string(),
                    "{window}".to_string()
                ],
            ]
        );
        assert_eq!(fragment.status, "unverified");
        // 死候选 → quirk + prior -4;no_effect → prior -2
        assert_eq!(fragment.quirks.len(), 1);
        assert_eq!(fragment.quirks[0].id, "explore_clearmemorybutton_dead");
        assert!(fragment.quirks[0].note.contains("explore-s2-calc-tie"));
        assert_eq!(
            fragment.suggested_priors.get("id:ClearMemoryButton"),
            Some(&-4)
        );
        assert_eq!(fragment.suggested_priors.get("id:num8Button"), Some(&-2));
        // run_id 注入证据与标识
        assert_eq!(fragment.route_id, "explore_explore_s2_calc_tie");
        assert_eq!(fragment.evidence.run_id, "explore-s2-calc-tie");
        assert!(fragment.description.contains("run explore-s2-calc-tie"));
    }

    #[test]
    fn typed_steps_export_as_type_with_text() {
        // 输入原语:text 动作导出为 type 步骤,文本随路线保存(重放可用)。
        // 非 ASCII 文本导出时注入 --paste(与引擎执行侧同口径)——重放直接
        // 走剪贴板通道,不在真机上退回不可靠的键盘路径。
        let log = vec![
            acted("id:openDialog", "auto"),
            typed("id:fileName", "报告.txt"),
            typed("id:fileName2", "report.txt"),
        ];
        let s = state(true, log, Default::default());
        let fragment = extract(&s, "explore-t1-input").unwrap();
        assert_eq!(
            fragment.steps[0],
            vec![
                "click".to_string(),
                "id:openDialog".to_string(),
                "--app".to_string(),
                "{window}".to_string()
            ]
        );
        assert_eq!(
            fragment.steps[1],
            vec![
                "type".to_string(),
                "id:fileName".to_string(),
                "报告.txt".to_string(),
                "--paste".to_string(),
                "--app".to_string(),
                "{window}".to_string()
            ]
        );
        assert_eq!(
            fragment.steps[2],
            vec![
                "type".to_string(),
                "id:fileName2".to_string(),
                "report.txt".to_string(),
                "--app".to_string(),
                "{window}".to_string()
            ],
            "ASCII text keeps the keyboard channel in exported routes"
        );
        // YAML 渲染往返保真(含中文文本)
        let parsed: serde_json::Value =
            serde_saphyr::from_str(&render_yaml(&fragment)).expect("renders");
        assert_eq!(parsed["routes"][0]["steps"][1][2], "报告.txt");
    }

    #[test]
    fn rendered_yaml_round_trips_through_saphyr() {
        let log = vec![acted("id:num7Button", "auto")];
        let mut tried = BTreeMap::new();
        tried.insert("id:ClearMemoryButton".to_string(), "dead".into());
        let s = state(true, log, tried);
        let fragment = extract(&s, "explore-s1-calc-auto").unwrap();
        let text = render_yaml(&fragment);
        // 渲染结果必须可被 YAML 解析器吃下(与 lint-packs 同族解析器)
        let mut options = serde_saphyr::Options::default();
        options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
        let parsed: serde_json::Value =
            serde_saphyr::from_str_with_options(&text, options).expect("rendered yaml parses");
        let route = &parsed["routes"][0];
        assert_eq!(route["id"], "explore_explore_s1_calc_auto");
        assert_eq!(route["status"], "unverified");
        assert_eq!(route["steps"][0][0], "click");
        assert_eq!(route["steps"][0][3], "{window}");
        // 中文与含冒号值经引号转义保真
        assert_eq!(route["steps"][1][3], "显示为 7");
        assert_eq!(parsed["quirks"][0]["id"], "explore_clearmemorybutton_dead");
    }

    #[test]
    fn yaml_scalar_quotes_special_values() {
        assert_eq!(yaml_scalar("plain"), "plain");
        assert_eq!(yaml_scalar(""), "\"\"");
        assert_eq!(yaml_scalar("a: b"), "\"a: b\"");
        assert_eq!(yaml_scalar("带\"引号\""), "\"带\\\"引号\\\"\"");
        assert_eq!(yaml_scalar("显示为 7"), "显示为 7");
        assert_eq!(yaml_scalar("{window}"), "\"{window}\"");
    }

    #[test]
    fn yaml_scalar_quotes_number_like_and_bool_like_values() {
        // 真机发现 #5:数字/布尔/null 形态裸渲染会被 YAML 解析为非字符串,
        // flow schema 要求 args 全字符串。
        assert_eq!(yaml_scalar("8022"), "\"8022\"");
        assert_eq!(yaml_scalar("0"), "\"0\"");
        assert_eq!(yaml_scalar("-1"), "\"-1\"");
        assert_eq!(yaml_scalar("1e5"), "\"1e5\"");
        assert_eq!(yaml_scalar("true"), "\"true\"");
        assert_eq!(yaml_scalar("Null"), "\"Null\"");
        // 中文与普通含空格文本不受影响。
        // 中文不受影响;尾随空格本就触发引号(既有规则)。
        assert_eq!(yaml_scalar("测试一下小过"), "测试一下小过");
    }
}
