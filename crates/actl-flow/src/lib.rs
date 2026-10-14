//! Optional workflow layer. No Windows dependencies, shell evaluation or asynchronous runtime.
pub mod bounded;
pub mod branch;
pub mod data;
pub mod engine;
pub mod explore;
pub mod for_each;
pub mod freshness;
pub mod inputs;
pub mod packs;
pub mod record;
pub mod recovery;
pub mod resources;
pub mod revise;
pub mod route;
mod schema;
pub mod sequence;
pub mod store;
pub mod task_config;
use actl_core::CtlError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<actl_core::handoff::Policy>,
    /// 修复检测的自动续跑策略(批次4):`action_failed` = 障碍清除且结果未现时
    /// 由显示端自动发起继续(预算 2 次/运行);默认关——授权通道与重试策略正交。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_resume: Option<AutoResume>,
    /// 探索模式 spec(docs/42 S2):存在时 steps 必须为空,运行走 explore 引擎。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explore: Option<ExploreSpec>,
    /// 弹层处理器声明(运行时治理第一轮,docs/06):写步步前门禁发现前台
    /// 弹层时,匹配面板按声明点击处置(留痕);未匹配面板 → popup_pending
    /// 上抛。AGENTS 人肉弹层白名单的机制化形态;unknown 分类不可声明。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub popup_handlers: Vec<PopupHandler>,
    /// 常驻表面认领声明(降噪第二轮,2026-10-11 spec):匹配表面是**声明
    /// 已知的常驻部件**(停靠窗格/折叠挂件),门禁跳过不阻断——从词表
    /// 启发(is_docked_pane,Word 实例)升级为声明形态(VS Code Find
    /// 面板第二实例证启发不泛化)。name_contains 必填(最窄形态,禁止
    /// "忽略该 app 全部 unknown"的宽声明);同一表面同时声明 handler
    /// 与 ignore → PROTOCOL(处置与放行语义互斥)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub popup_ignores: Vec<PopupIgnore>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// 单条常驻表面认领:此表面是该应用的已知常驻部件(非弹窗),门禁放行。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PopupIgnore {
    /// 目标应用锚(标题子串):仅当写步的 --app 包含此锚时生效。
    pub app: String,
    /// 表面名/文本包含词(必填——按名认领,禁止宽形态)。
    pub name_contains: String,
}

/// 单条弹层处理器:classification 必须显式(error/save/confirm/update/
/// notice——未声明的 unknown 永远上抛);click 是作用于该应用窗口的选择器
/// (如 `uia:{"match":{"role":"Button","name":"不保存"}}`),处置走既有
/// click 护栏(前台守卫/预告/审计不变)。notice(2026-10-11 spec,最窄
/// 形态):认领 naive 词表落 unknown 的 INFO 级流程自产表面(如 VS Code
/// 「扩展已禁用」通知),name_contains **必填**(按名认领,禁止宽形态)。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PopupHandler {
    /// 目标应用锚(标题子串):仅当写步的 --app 包含此锚时生效。
    pub app: String,
    /// 面板分类(error/save/confirm/update/notice;notice 匹配 unknown 表面)。
    pub classification: String,
    /// 面板名/文本包含词,收窄匹配;notice 分类必填,其余可选。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    /// 处置动作:click 选择器。
    pub click: String,
}

/// 探索任务 spec(docs/42):自然语言任务一句话 + 应用锚 + 关键词词表 +
/// 只读目标断言 + 有界预算。任务编译(自然语言→词表+谓词)由调用方完成;
/// actl 只接收结构化产物,不做语义理解。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExploreSpec {
    /// 任务一句话(≤160 UTF-8 字节),入决策卡静态头。
    pub task: String,
    /// 目标窗口锚(标题子串,≤80 字节)。
    pub app: String,
    /// 任务关键词词表(1..=16,每个 1..=64 字节):词表投影与候选评分共用。
    #[serde(default)]
    pub keywords: Vec<String>,
    /// 只读目标断言(verify/wait 族):探索的本地后置条件谓词。
    pub goal: Invocation,
    /// 效果风险先验(docs/42 §4.5 v1):选择器 → 分数调整(-4..=4,≤32 项),
    /// 由调用方从 pack quirks / 上轮 suggested_priors 注入——已知死/无效果/
    /// 窗口重构类候选被压低,已知有效候选提升。引擎不读 pack,先验显式入参。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub priors: std::collections::BTreeMap<String, i32>,
    /// 历史统计(docs/42 §4.5 v2,UCB):选择器 → (tried, effective) 计数,
    /// 由调用方从 pack 累积的 suggested_stats 注入。条目在场且 tried ≥3 时
    /// UCB 项**替代**该选择器的静态先验(同知识不双计);冷启动(tried<3)
    /// 回落静态先验,零注入时行为与 v1 完全一致(纯向后兼容)。≤32 项。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub stats: std::collections::BTreeMap<String, actl_core::decision::SelectorStats>,
    /// 输入原语(docs/42 L2):完整语义选择器(id:/name:) → 待输入文本。
    /// 键的选择器值并入观察关键词(种子+评分激活 Edit),候选命中即执行
    /// `type`(复用键盘护栏与 --app 绑定)。≤8 项,词表+texts 键 ≤16。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub texts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub budgets: Budgets,
}

/// 探索预算(docs/42 §2 原则 3:有界运行)。到达任一上限即 needs_review,
/// 不静默续跑。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// 动作上限(默认 12)。
    #[serde(default = "default_actions")]
    pub actions: u32,
    /// 决策外抛上限(默认 6)。
    #[serde(default = "default_decisions")]
    pub decisions: u32,
    /// 运行时长上限毫秒(默认 600000)。
    #[serde(default = "default_wall_ms")]
    pub wall_ms: u64,
}
fn default_actions() -> u32 {
    12
}
fn default_decisions() -> u32 {
    6
}
fn default_wall_ms() -> u64 {
    600_000
}
impl Default for Budgets {
    fn default() -> Self {
        Self {
            actions: default_actions(),
            decisions: default_decisions(),
            wall_ms: default_wall_ms(),
        }
    }
}
/// 显式声明,唯一取值 action_failed(user_paused/checkpoint/needs_review 永不自动)。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoResume {
    ActionFailed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub run: Invocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<Invocation>,
    pub before: Option<Invocation>,
    pub verify: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_check: Option<Invocation>,
    /// ensure: 幂等语义(运行时治理第二轮,docs/06):check(只读)通过 = 步骤
    /// 已满足,直接 Completed+verified,run 不派发——环境准备类步骤的
    /// "先验证后执行"一等形态,重跑/修订续跑的前缀成本趋零。
    /// 与 outputs 互斥(满足路径不产出绑定值)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ensure: Option<EnsureSpec>,
    /// 背景观察步(用户裁定 2026-10-13「方案 a」):派发后不等待,流程继续
    /// 走后续步,退出前收割(阻塞 join)结果入 StepRecord.background_result。
    /// 语义边界(引擎 PROTOCOL 强制):run 必须只读、禁 verify/outputs/
    /// pause_before/ensure/resume_check/otherwise/recovery——观察者不是
    /// 流程控制点,产物只在收割后可读(bind 结构性不可用)。
    #[serde(default)]
    pub background: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(default)]
    pub pause_before: bool,
    #[serde(default)]
    pub retry: Retry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<bounded::Policy>,
    #[serde(default)]
    pub on_failure: Failure,
    #[serde(default)]
    pub evidence: EvidencePolicy,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub outputs: std::collections::BTreeMap<String, data::Output>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePolicy {
    #[default]
    Metadata,
    Full,
}

/// ensure: 的满足性检查(只读调用):通过即短路。观察错误如实传播
/// (不能判定满足就不派发写步——与 when/unless 的错误语义同族)。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnsureSpec {
    pub check: Invocation,
}

pub fn failure_evidence(policy: EvidencePolicy, error: &CtlError) -> Option<serde_json::Value> {
    let source = error.evidence.as_ref()?;
    if policy == EvidencePolicy::Full {
        return Some(
            if serde_json::to_vec(source).is_ok_and(|v| v.len() <= 8192) {
                source.clone()
            } else {
                serde_json::json!({"omitted":"evidence_exceeds_8_kib"})
            },
        );
    }
    let mut value = serde_json::json!({
        "redacted": true,
        // 错误原文(docs/68 待办收口):message 来自错误契约本身(envelope
        // 已公开),不属于被 redaction 保护的秘密载荷——有界 200 字符随
        // 证据携带,恢复路径不再需要重放才能看到失败原因。
        "message": message_excerpt(error),
    });
    for key in ["property", "channel", "comparison"] {
        if let Some(v) = source.get(key) {
            value[key] = v.clone();
        }
    }
    for key in [
        "native_code",
        "delivered_before_error",
        "failed_segment_size",
        "sequence_index",
        "count",
        "foreground_before",
        "foreground_hwnd",
        "hwnd",
        "caller_pid",
        "waited_ms",
        "spawned_pid",
    ] {
        if let Some(v) = source.get(key).filter(|v| v.is_i64() || v.is_u64()) {
            value[key] = v.clone();
        }
    }
    for key in ["observation_span_id", "observation_duration_ms"] {
        if let Some(v) = source.get(key).filter(|v| v.is_u64()) {
            value[key] = v.clone();
        }
    }
    if let Some(stage) = source.get("observation_stage").filter(|v| {
        matches!(
            v.as_str(),
            Some(
                "window_inventory"
                    | "automation_initialize"
                    | "desktop_root"
                    | "children"
                    | "find_all_build_cache"
                    | "array_length"
                    | "array_elements"
                    | "window_handle"
                    | "window_role"
                    | "window_properties"
            )
        )
    }) {
        value["observation_stage"] = stage.clone();
    }
    if let Some(v) = source.get("action_started").filter(|v| v.is_boolean()) {
        value["action_started"] = v.clone();
    }
    if let Some(v) = source.get("truncated").filter(|v| v.is_boolean()) {
        value["truncated"] = v.clone();
    }
    if let Some(v) = source
        .get("phase")
        .filter(|v| matches!(v.as_str(), Some("match" | "within" | "window")))
    {
        value["phase"] = v.clone();
    }
    for key in [
        "activation_requested",
        "activation_returned",
        "minimized",
        "visible",
        "process_started",
        "relaunch",
    ] {
        if let Some(v) = source.get(key).filter(|v| v.is_boolean() || v.is_null()) {
            value[key] = v.clone();
        }
    }
    if matches!(
        source["stage"].as_str(),
        Some("handoff" | "window_prepare" | "window_discovery")
    ) {
        value["stage"] = source["stage"].clone();
    }
    if matches!(
        source["stage"].as_str(),
        Some("window_prepare" | "window_discovery")
    ) && let Some(reason) = source["reason"].as_str().filter(|r| {
        matches!(
            *r,
            "window_gone"
                | "window_disabled_or_modal"
                | "window_unresponsive"
                | "restore_rejected"
                | "restore_timeout"
                | "window_hidden"
                | "bounds_unavailable"
                | "window_offscreen"
                | "foreground_not_acquired"
                | "window_not_found"
        )
    }) {
        value["reason"] = reason.into();
    }
    if source["reason"].as_str().is_some_and(|r| {
        matches!(
            r,
            "observation_failed"
                | "human_takeover_or_grant_expired"
                | "handoff_pending"
                | "user_deferred"
                | "user_paused"
                | "pattern_unsupported"
                | "target_identity_changed"
                | "target_disabled"
        )
    }) {
        value["reason"] = source["reason"].clone();
    }
    Some(value)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub bind: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Retry {
    #[default]
    Never,
    ReadOnly,
    StaleRead,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    #[default]
    Pause,
    Fail,
    /// Isolate the failure: mark the step, skip the rest of its section and keep
    /// executing later sections. The run ends `needs_review`, never `completed`.
    SkipSection,
}

/// AMBIGUOUS 目标串联记录(运行时候选纠正,docs/63):run 相定位多命中时
/// 引擎锚定一份骨架快照并全量保留候选,run 进 needs_review
/// (`ambiguous_target:<step>`);调用方 `flow-resume --pick @eN` 应答后,
/// 引擎以 ref+snapshot 重写派发参数——身份核验走既有 ReplayGuard
/// (窗口重建/元素漂移即拒,冻结计划四不变式全保留,pick 不改 plan_version)。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AmbiguousTarget {
    /// 被拦步骤的 run 命令名(追溯面)。
    pub command: String,
    /// 原目标串(args[0],语义选择器)。
    pub target: String,
    /// 步骤 --app 锚(快照与重放共用)。
    pub app: Option<String>,
    /// 引擎锚定的骨架快照(候选 ref 的身份来源)。
    pub snapshot_id: String,
    /// 候选证据全量(locate 的 AMBIGUOUS evidence.candidates,不被
    /// 默认证据 redaction 吞掉)。
    pub candidates: Vec<serde_json::Value>,
    /// `--pick` 已选 ref("@eN");None = 待应答。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picked: Option<String>,
}

impl AmbiguousTarget {
    /// 候选可重放 ref 集(@eN 形态;非 ref 候选不参与 pick)。
    pub fn candidate_refs(&self) -> Vec<String> {
        self.candidates
            .iter()
            .filter_map(|c| c["ref"].as_str().map(str::to_owned))
            .collect()
    }
}

/// Route-defect classification: the same non-read-only step failing with the same
/// error code on its last three recorded attempts is a broken route, not a transient.
pub fn route_defect(record: &engine::StepRecord) -> bool {
    let history = &record.attempt_errors;
    history.len() >= 3
        && record.error_code.is_some_and(|code| {
            history[history.len() - 3..]
                .iter()
                .all(|observed| *observed == code)
        })
}

/// Redacted failure evidence, or a minimal structured synthesis when the source
/// error carries none: the phase, the failing command name, the attempt count and
/// the error message itself (truncated) — the message is engine-side diagnostic
/// text, keeping it removes the need to hand-reproduce failures for their cause.
pub fn step_failure_evidence(
    step: &Step,
    error: &CtlError,
    phase: &str,
    command: &str,
) -> Option<serde_json::Value> {
    failure_evidence(step.evidence, error).or_else(|| {
        Some(serde_json::json!({
            "phase": phase,
            "command": command,
            "message": message_excerpt(error),
            "synthetic": true,
        }))
    })
}
fn message_excerpt(error: &CtlError) -> String {
    // 只取正文,错误码由 error_code 字段承载,不重复。
    error.message.chars().take(200).collect()
}

pub fn parse(text: &str) -> Result<Plan, CtlError> {
    let raw = decode_yaml(text)?;
    let plan = schema::decode(raw)?;
    for (field, invalid, message) in [
        ("version", plan.version != 1, "expected version 1"),
        (
            "name",
            plan.name.trim().is_empty() || plan.name.len() > 256,
            "name requires 1-256 UTF-8 bytes",
        ),
        (
            "steps",
            plan.steps.len() > 256 || (plan.steps.is_empty() && plan.explore.is_none()),
            "expected 1-256 steps (empty only with explore)",
        ),
    ] {
        if invalid {
            return Err(schema::error(field, None, message));
        }
    }
    if let Some(explore) = &plan.explore {
        if !plan.steps.is_empty() {
            return Err(schema::error(
                "steps",
                None,
                "explore plans must not declare static steps",
            ));
        }
        if plan.auto_resume.is_some() {
            return Err(schema::error(
                "auto_resume",
                None,
                "explore plans do not support auto_resume",
            ));
        }
        validate_explore(explore)?;
    }
    let mut ids = std::collections::HashSet::new();
    if let Some(handoff) = &plan.handoff
        && !handoff.pardon_input.valid()
    {
        return Err(schema::error(
            "handoff.pardon_input",
            None,
            "pardon_input requires quiet_ms 1000..=10000 and budget 1..=5",
        ));
    }
    for (index, step) in plan.steps.iter().enumerate() {
        let base = format!("steps[{index}]");
        if !actl_core::history::valid_id(&step.id) || !ids.insert(&step.id) {
            return Err(schema::error(
                &format!("{base}.id"),
                Some(&step.id),
                "step ID must be unique and valid",
            ));
        }
        if step.title.trim().is_empty() || step.title.len() > 256 {
            return Err(schema::error(
                &format!("{base}.title"),
                Some(&step.id),
                "title requires 1-256 UTF-8 bytes",
            ));
        }
        if let Some(section) = &step.section
            && !actl_core::history::valid_id(section)
        {
            return Err(schema::error(
                &format!("{base}.section"),
                Some(&step.id),
                "section must be a valid identifier",
            ));
        }
        if step.on_failure == Failure::SkipSection && step.section.is_none() {
            return Err(schema::error(
                &format!("{base}.on_failure"),
                Some(&step.id),
                "skip_section requires the step to declare a section",
            ));
        }
        for (phase, call) in [
            ("run", Some(&step.run)),
            ("when", step.when.as_ref()),
            ("unless", step.unless.as_ref()),
            ("otherwise", step.otherwise.as_ref()),
            ("before", step.before.as_ref()),
            ("verify", step.verify.as_ref()),
            ("resume_check", step.resume_check.as_ref()),
        ] {
            let Some(call) = call else { continue };
            if call.command.is_empty() || call.command.len() > 64 {
                return Err(schema::error(
                    &format!("{base}.{phase}.command"),
                    Some(&step.id),
                    "command requires 1-64 bytes",
                ));
            }
            if call.args.len() > 128 {
                return Err(schema::error(
                    &format!("{base}.{phase}.args"),
                    Some(&step.id),
                    "at most 128 arguments",
                ));
            }
            for (j, arg) in call.args.iter().enumerate() {
                if arg.len() > 16_384 || arg.contains('\0') {
                    return Err(schema::error(
                        &format!("{base}.{phase}.args[{j}]"),
                        Some(&step.id),
                        "argument exceeds 16 KiB or contains NUL",
                    ));
                }
            }
        }
    }
    if serde_json::to_vec(&plan)
        .map_err(|e| CtlError::internal(e.to_string()))?
        .len()
        > 262_144
    {
        return Err(CtlError::protocol("normalized flow exceeds 256 KiB"));
    }
    data::validate(&plan)?;
    Ok(plan)
}

/// 探索 spec 校验(schema 层;CLI 层另做命令面校验)。
fn validate_explore(explore: &ExploreSpec) -> Result<(), CtlError> {
    for (field, invalid, message) in [
        (
            "explore.task",
            explore.task.trim().is_empty() || explore.task.len() > 160,
            "task requires 1-160 UTF-8 bytes",
        ),
        (
            "explore.app",
            explore.app.trim().is_empty() || explore.app.len() > 80,
            "app requires 1-80 UTF-8 bytes",
        ),
        (
            "explore.keywords",
            explore.keywords.is_empty()
                || explore.keywords.len() > 16
                || explore
                    .keywords
                    .iter()
                    .any(|k| k.trim().is_empty() || k.len() > 64),
            "keywords requires 1-16 non-empty entries of at most 64 bytes each",
        ),
        (
            "explore.goal.command",
            explore.goal.command.is_empty() || explore.goal.command.len() > 64,
            "goal command requires 1-64 bytes",
        ),
        (
            "explore.goal.args",
            explore.goal.args.len() > 128,
            "goal allows at most 128 arguments",
        ),
        (
            "explore.budgets.actions",
            !(1..=512).contains(&explore.budgets.actions),
            "actions budget must be 1..=512",
        ),
        (
            "explore.budgets.decisions",
            !(1..=64).contains(&explore.budgets.decisions),
            "decisions budget must be 1..=64",
        ),
        (
            "explore.budgets.wall_ms",
            !(1_000..=3_600_000).contains(&explore.budgets.wall_ms),
            "wall_ms budget must be 1000..=3600000",
        ),
    ] {
        if invalid {
            return Err(schema::error(field, None, message));
        }
    }
    for (index, arg) in explore.goal.args.iter().enumerate() {
        if arg.len() > 16_384 || arg.contains('\0') {
            return Err(schema::error(
                &format!("explore.goal.args[{index}]"),
                None,
                "argument exceeds 16 KiB or contains NUL",
            ));
        }
    }
    // 探索引擎不做输出绑定解析;goal 带 bind 是不可达路径,直接拒绝。
    if !explore.goal.bind.is_empty() {
        return Err(schema::error(
            "explore.goal.bind",
            None,
            "explore goal cannot use output bindings",
        ));
    }
    if explore.priors.len() > 32 {
        return Err(schema::error(
            "explore.priors",
            None,
            "at most 32 prior entries",
        ));
    }
    for (selector, adjustment) in &explore.priors {
        if selector.is_empty() || selector.len() > 96 {
            return Err(schema::error(
                "explore.priors",
                None,
                "prior selectors require 1-96 bytes",
            ));
        }
        if !(-4..=4).contains(adjustment) {
            return Err(schema::error(
                "explore.priors",
                None,
                "prior adjustments must be -4..=4",
            ));
        }
    }
    if explore.stats.len() > 32 {
        return Err(schema::error(
            "explore.stats",
            None,
            "at most 32 stats entries",
        ));
    }
    for (selector, entry) in &explore.stats {
        if selector.is_empty() || selector.len() > 96 {
            return Err(schema::error(
                "explore.stats",
                None,
                "stats selectors require 1-96 bytes",
            ));
        }
        if entry.tried == 0 || entry.tried > 10_000 {
            return Err(schema::error(
                "explore.stats",
                None,
                "stats tried must be 1-10000 (zero-tried keys are not stats)",
            ));
        }
        if entry.effective > entry.tried {
            return Err(schema::error(
                "explore.stats",
                None,
                "stats effective cannot exceed tried",
            ));
        }
    }
    if explore.texts.len() > 8 {
        return Err(schema::error(
            "explore.texts",
            None,
            "at most 8 text entries",
        ));
    }
    for (selector, text) in &explore.texts {
        let semantic = selector
            .strip_prefix("id:")
            .or_else(|| selector.strip_prefix("name:"))
            .unwrap_or_default();
        // focused: 键(docs/77 已知限制②收口,2026-10-12):键入 OS 键盘焦点
        // 元素,不指向树内定位目标——裸 `focused:` 或 `focused:<Role>`(角色
        // 核验 fail-closed 由 CLI type 承担);不接受序数/空白形态。
        let focused = selector == "focused:"
            || selector.strip_prefix("focused:").is_some_and(|role| {
                !role.is_empty() && role.chars().all(|c| c.is_ascii_alphanumeric())
            });
        if selector.len() < 4 || selector.len() > 96 || (semantic.is_empty() && !focused) {
            return Err(schema::error(
                "explore.texts",
                None,
                "text keys require semantic selectors (id:/name: with 3-92 byte value) or focused:/focused:<Role>",
            ));
        }
        if text.is_empty() || text.len() > 4096 || text.contains('\0') {
            return Err(schema::error(
                "explore.texts",
                None,
                "text values require 1-4096 bytes without NUL",
            ));
        }
    }
    if explore.keywords.len() + explore.texts.len() > 16 {
        return Err(schema::error(
            "explore.keywords",
            None,
            "keywords plus text keys must stay within the 16-keyword budget",
        ));
    }
    Ok(())
}

/// YAML/JSON 计划文本 → 原始 Value(引擎与消费方共用的解码入口;纯文本解析,
/// 无 schema 语义——schema 归 parse/inputs 解析期)。供 flow-validate、模板
/// 一致性测试等需要**原始模板形态**的消费者使用;parse 消费 inputs/resources
/// 返回强类型 Plan,两者用途不同。
pub fn decode_yaml(text: &str) -> Result<serde_json::Value, CtlError> {
    if text.len() > 262_144 {
        return Err(CtlError::protocol("flow exceeds 256 KiB"));
    }
    let mut options = serde_saphyr::Options::default();
    options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
    options.merge_keys = serde_saphyr::MergeKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    options.strict_booleans = true;
    options.with_snippet = false;
    let mut budget = serde_saphyr::Budget::default();
    budget.max_depth = 16;
    budget.max_aliases = 0;
    budget.max_anchors = 0;
    budget.max_documents = 1;
    budget.max_events = 100_000;
    budget.max_nodes = 50_000;
    budget.max_total_scalar_bytes = 262_144;
    options.budget = Some(budget);
    let raw: serde_json::Value = serde_saphyr::from_str_with_options(text, options)
        .map_err(|e| CtlError::protocol(format!("invalid flow YAML: {e}")))?;
    Ok(raw)
}

#[cfg(test)]
mod auto_resume_tests {
    use super::*;
    #[test]
    fn auto_resume_field_round_trips_and_stays_opt_in() {
        let plan = parse(
            "version: 1\nname: probe demo\nauto_resume: action_failed\nsteps:\n  - id: a\n    title: A\n    run: {command: find, args: ['--role', Button]}\n",
        )
        .unwrap();
        assert_eq!(plan.auto_resume, Some(AutoResume::ActionFailed));
        assert_eq!(
            serde_json::to_value(&plan).unwrap()["auto_resume"],
            "action_failed"
        );
        let plain = parse(
            "version: 1\nname: plain\nsteps:\n  - id: a\n    title: A\n    run: {command: find}\n",
        )
        .unwrap();
        assert_eq!(plain.auto_resume, None);
        assert!(parse(
            "version: 1\nname: bad\nauto_resume: always\nsteps:\n  - id: a\n    title: A\n    run: {command: find}\n"
        )
        .is_err());
    }
}
