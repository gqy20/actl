//! 探索模式引擎 S2(docs/42 §9.3):观察→差分→前沿评分→熵门→probe→动作→
//! 本地目标核验的有界循环。语义僵局外抛为决策卡(`needs_review` +
//! `reason="decision_pending"`,docs/42 §9.2.1),`flow-resume --decision` 续跑。
//!
//! token 纪律(docs/42 §3)在此引擎的落点:观察走关键词投影(零命中放宽一次
//! skeleton)、差分卡驱动效果标注、决策卡硬上限 4 KiB、目标核验完全本地谓词
//! (LLM 永远不被问"这步成功了吗")、应答后的排序回退在引擎侧摊平往返。
//!
//! 与静态 plan 引擎的关系:复用 `Host`(执行/护栏/停止/发布)与 `Store`
//! (锁/暂停文件/目录治理);不复用 `engine::execute`(其假设冻结步骤序列)。
//! 探索状态自持久化为 `explore.json`,与 `state.json` 并存;`steps` 恒空。

use crate::{
    ExploreSpec, Invocation,
    engine::{Host, Run, Status},
    store::Store,
};
use actl_core::decision::{self, CardInput, DecisionCard, GateOutcome, ScoredCandidate};
use actl_core::diff;
use actl_core::snapshot::ElementOut;
use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 持久化帧元素上限(词表投影帧典型 <100;防御失控 UI)。
const ELEMENTS_CAP: usize = 512;
/// 动作日志上限(环形丢弃最旧;route 回写达上限即拒绝,防截断路线)。
pub const ACTION_LOG_CAP: usize = 64;
/// 前沿候选去重集上限。
const SEEN_CAP: usize = 1024;
/// 状态指纹集上限。
const VISITED_CAP: usize = 512;
/// 应答记录上限。
const ANSWER_CAP: usize = 64;
/// explore.json 序列化上限(与 store 的 256 KiB 预算对齐)。
const STATE_MAX_BYTES: usize = 262_144;
/// goal 断言瞬态错误的有界重试上限(docs/77 发现 1 的收口:失败闭合前给
/// 提供程序抖动/窗口未物化两次恢复机会;重试耗尽保持诚实终态)。
const GOAL_RETRY_LIMIT: u32 = 2;
/// 每次重试的沉降等待毫秒。
const GOAL_RETRY_SETTLE_MS: u64 = 300;
/// 观察瞬态错误的有界重试上限(docs/77 发现 #3:菜单开合/窗口重构过渡期
/// 提供程序短暂锁住,紧随动作的观察 INTERNAL 失败;~1s 后恢复已实证)。
const OBSERVATION_RETRY_LIMIT: u32 = 2;
/// 每次观察重试的沉降毫秒(flyout 动画量级)。
const OBSERVATION_RETRY_SETTLE_MS: u64 = 500;

/// 悬挂中的决策卡:run 处于 needs_review/decision_pending 时存在,
/// 应答后消费并转换为引擎侧排序回退队列。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingDecision {
    pub card: DecisionCard,
}

/// 一次动作尝试的日志行(含 probe 拒绝的死候选;不消耗动作预算也留痕)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRecord {
    pub step: u32,
    pub selector: String,
    /// auto | answer | fallback
    pub mode: String,
    /// acted | dead | failed
    pub outcome: String,
    /// 输入原语:type 动作的文本(回写路线导出 type 步骤;click 为 None)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// 决策应答记录(置信度仅入报告,不影响执行语义)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerRecord {
    pub decision_id: String,
    pub choice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    pub answered_ms: u64,
}

/// token 审计(docs/42 §9.4:推算数字以实测替换)。观察体积 = 引擎实际消费的
/// 快照载荷字节数(朴素循环 agent 的同口径对照),卡体积 = 外抛决策卡字节数。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenAudit {
    pub observation_bytes: u64,
    pub card_bytes: u64,
}

/// 探索运行的可持久化全部状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExploreState {
    pub version: u32,
    pub spec: ExploreSpec,
    pub started_ms: u64,
    pub actions: u32,
    pub decisions: u32,
    pub observations: u32,
    pub errors: u32,
    /// selector → 效果标注(acted | dead | no_effect | reveals)。
    #[serde(default)]
    pub tried: BTreeMap<String, String>,
    /// 前沿候选去重(计数用,封顶)。
    #[serde(default)]
    pub seen: BTreeSet<String>,
    /// 已访问状态指纹(覆盖率用,封顶)。
    #[serde(default)]
    pub visited: BTreeSet<String>,
    /// 关键词投影零命中后已放宽为 skeleton(每任务一次)。
    #[serde(default)]
    pub widened: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fingerprint: Option<String>,
    #[serde(default)]
    pub last_elements: Vec<ElementOut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingDecision>,
    /// 应答后的排序回退队列(应答首选打头,其余为卡内非危险候选按序)。
    #[serde(default)]
    pub queue: Vec<String>,
    #[serde(default)]
    pub action_log: Vec<ActionRecord>,
    #[serde(default)]
    pub answers: Vec<AnswerRecord>,
    #[serde(default)]
    pub audit: TokenAudit,
    /// 观察帧被截断的次数(帧超 ELEMENTS_CAP 如实计数)。
    #[serde(default)]
    pub frames_truncated: u32,
    /// goal 断言瞬态错误已重试次数(有界,docs/77 发现 1)。
    #[serde(default)]
    pub goal_retries: u32,
    /// 观察瞬态错误已重试次数(有界,docs/77 发现 #3)。
    #[serde(default)]
    pub observation_retries: u32,
    /// 观察窗(锚窗)pid——同进程跨窗探测的过滤依据,取自每次快照的 window。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_pid: Option<u32>,
    /// 观察窗标题(同上,跨窗探测排除自身用)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    /// 当前帧的快照 id——帧内 ref(@eN)的身份锚点,ref 绑定动作
    /// (`click @eN --snapshot`)走捕获身份解析,绕开间歇失明的定位通道
    /// (发现 #6 结构性收口)。与 last_elements 同轮更新。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    /// 跨窗 goal 探测转移:goal 命中的同进程窗口标题(证据/复核用)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_window: Option<String>,
    /// 已发生的跨窗转移次数(有界)。
    #[serde(default)]
    pub anchor_transfers: u32,
    /// 已消耗的跨窗探测轮次预算(每轮一次 list-windows + ≤3 次 goal 重写)。
    #[serde(default)]
    pub probe_episodes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_met: Option<bool>,
}

impl ExploreState {
    fn new(spec: &ExploreSpec, started_ms: u64) -> Self {
        Self {
            version: 1,
            spec: spec.clone(),
            started_ms,
            actions: 0,
            decisions: 0,
            observations: 0,
            errors: 0,
            tried: BTreeMap::new(),
            seen: BTreeSet::new(),
            visited: BTreeSet::new(),
            widened: false,
            last_fingerprint: None,
            last_elements: Vec::new(),
            last_actor: None,
            pending: None,
            queue: Vec::new(),
            action_log: Vec::new(),
            answers: Vec::new(),
            audit: TokenAudit::default(),
            frames_truncated: 0,
            goal_retries: 0,
            observation_retries: 0,
            window_pid: None,
            window_title: None,
            snapshot_id: None,
            goal_window: None,
            anchor_transfers: 0,
            probe_episodes: 0,
            goal_met: None,
        }
    }
}

fn invocation(command: &str, args: Vec<String>) -> Invocation {
    Invocation {
        command: command.into(),
        args,
        bind: Default::default(),
    }
}

/// explore.json 载入;缺失返回 None,损坏如实报错(不静默重置现场)。
pub fn load(store: &Store) -> Result<Option<ExploreState>, CtlError> {
    let path = store.dir.join("explore.json");
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(CtlError::internal(format!("explore state read: {e}")));
        }
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| CtlError::protocol(format!("invalid explore state: {e}")))
}

/// 原子持久化(临时文件+改名,与 store.rs 同纪律);超预算拒绝写并报错。
pub fn persist(store: &Store, state: &ExploreState) -> Result<(), CtlError> {
    let bytes = serde_json::to_vec(state).map_err(|e| CtlError::internal(e.to_string()))?;
    if bytes.len() > STATE_MAX_BYTES {
        return Err(CtlError::protocol("explore state exceeds 256 KiB"));
    }
    let path = store.dir.join("explore.json");
    let temp = store.dir.join("explore.json.tmp");
    std::fs::write(&temp, &bytes)
        .and_then(|()| std::fs::rename(&temp, &path))
        .map_err(|e| CtlError::internal(format!("explore state write: {e}")))
}

/// 动作尝试的处置结果。
enum ActOutcome {
    Acted,
    /// 候选死亡(probe/点击失败)——前沿排除,继续。
    Dead,
    /// 交接暂停(handoff 证据),run 状态已定。
    HandoffPaused,
    /// 急停/暂停观察点命中——回到循环顶部由边界检查定状态。
    Boundary,
}

fn observe(
    spec: &ExploreSpec,
    state: &mut ExploreState,
    host: &mut impl Host,
) -> Result<Vec<ElementOut>, CtlError> {
    let mut snapshot =
        |args: Vec<String>, state: &mut ExploreState| -> Result<Vec<ElementOut>, CtlError> {
            // 快照通道(ref 绑定动作的身份锚点):execute_snapshot 返回
            // snapshot_id,与帧元素同轮记录——引擎动作用 `click @eN
            // --snapshot <id>` 走捕获身份解析(发现 #6 的结构性收口)。
            let (snapshot_id, value) = host.execute_snapshot(args)?;
            let bytes = serde_json::to_vec(&value)
                .map_err(|e| CtlError::internal(e.to_string()))?
                .len() as u64;
            state.observations += 1;
            state.audit.observation_bytes += bytes;
            state.snapshot_id = (!snapshot_id.is_empty()).then_some(snapshot_id);
            // 观察窗身份(pid/标题)——跨窗探测的过滤依据(同进程弹窗/对话框)。
            if let Some(window) = value.get("window") {
                if let Some(pid) = window.get("pid").and_then(|p| p.as_u64()) {
                    state.window_pid = Some(pid as u32);
                }
                if let Some(title) = window.get("title").and_then(|t| t.as_str()) {
                    state.window_title = Some(title.to_owned());
                }
            }
            let elements: Vec<ElementOut> = serde_json::from_value(
                value
                    .get("elements")
                    .cloned()
                    .ok_or_else(|| CtlError::protocol("snapshot output missing elements"))?,
            )
            .map_err(|e| CtlError::protocol(format!("snapshot elements decode: {e}")))?;
            if elements.len() > ELEMENTS_CAP {
                state.frames_truncated += 1;
            }
            Ok(elements)
        };
    let keywords = effective_keywords(spec).join(",");
    let mut args = vec!["--app".into(), spec.app.clone()];
    if !keywords.is_empty() {
        args.push("--keywords".into());
        args.push(keywords);
        args.push("--skeleton".into());
    } else {
        args.push("--skeleton".into());
    }
    let mut elements = snapshot(args, state)?;
    // 零命中放宽阶梯:关键词→skeleton,每任务一次,放宽事件入状态(widened)。
    if elements.is_empty() && !spec.keywords.is_empty() && !state.widened {
        state.widened = true;
        elements = snapshot(
            vec!["--app".into(), spec.app.clone(), "--skeleton".into()],
            state,
        )?;
    }
    Ok(elements)
}

/// goal 折叠目标:选择器形态(镜像 verify 语义——id 精确、name 子串)。
#[derive(Debug, Clone, PartialEq)]
enum FoldTarget {
    Id(String),
    Name(String),
}

/// 折叠断言形态:仅存在型与 name 通道的值断言可精确镜像;其余回退 COM。
#[derive(Debug, Clone, PartialEq)]
enum FoldAssertion {
    Exists,
    ValueName { expected: String, contains: bool },
}

fn parse_fold_target(target: &str) -> Option<FoldTarget> {
    if let Some(value) = target.strip_prefix("id:")
        && !value.is_empty()
    {
        return Some(FoldTarget::Id(value.into()));
    }
    if let Some(value) = target.strip_prefix("name:")
        && !value.is_empty()
    {
        return Some(FoldTarget::Name(value.into()));
    }
    None
}

/// 解析 goal 折叠(P1):要求 goal 的 --app 与探索锚一致(折叠帧=锚窗观察),
/// 且断言形态可精确镜像(存在型,或 --value ... --property name [--contains])。
/// 其他旗标(--normalize-lines/--url-query/--checked/其他通道)不折叠。
fn parse_goal_fold(spec: &ExploreSpec) -> Option<(FoldTarget, FoldAssertion)> {
    let args = &spec.goal.args;
    let app_index = args.iter().position(|a| a == "--app")?;
    if args.get(app_index + 1) != Some(&spec.app) {
        return None;
    }
    if args
        .iter()
        .any(|a| a == "--normalize-lines" || a == "--url-query" || a == "--checked")
    {
        return None;
    }
    if let Some(index) = args.iter().position(|a| a == "--exists") {
        let target = parse_fold_target(args.get(index + 1)?)?;
        return Some((target, FoldAssertion::Exists));
    }
    if let Some(index) = args.iter().position(|a| a == "--value") {
        let target = parse_fold_target(args.get(index + 1)?)?;
        let expected = args.get(index + 2)?.clone();
        let has_property_name = args
            .windows(2)
            .any(|window| window[0] == "--property" && window[1] == "name");
        if !has_property_name {
            return None;
        }
        let contains = args.iter().any(|a| a == "--contains");
        return Some((target, FoldAssertion::ValueName { expected, contains }));
    }
    None
}

/// 观察与评分共用的生效关键词:spec 词表 + texts 键的选择器值(种子+评分
/// 激活 Edit 输入框)+ goal 折叠词(把 goal 目标选择器的值当关键词,使
/// goal 元素入帧——存在型/name 值断言即可帧内判定)。
/// CLI 快照关键词上限 16;validation 保证 keywords+texts ≤16,折叠词按
/// 余量机会性加入(词表占满即放弃折叠,回退 COM)。
fn effective_keywords(spec: &ExploreSpec) -> Vec<String> {
    let mut list = spec.keywords.clone();
    for key in spec.texts.keys() {
        let value = key
            .strip_prefix("id:")
            .or_else(|| key.strip_prefix("name:"))
            .unwrap_or(key);
        if !list.iter().any(|k| k == value) && list.len() < 16 {
            list.push(value.to_owned());
        }
    }
    let fold_keyword = parse_goal_fold(spec).map(|(target, _)| match target {
        FoldTarget::Id(value) | FoldTarget::Name(value) => value,
    });
    if let Some(keyword) = fold_keyword
        && list.len() < 16
        && !list.iter().any(|k| k == &keyword)
    {
        list.push(keyword);
    }
    list
}

/// 帧内折叠判定:与 verify 定位语义一致(id 精确且唯一;name 子串首个命中)。
/// 判不了(缺席/歧义)返回 None 回退 COM 调用——错误码语义留在权威路径。
/// 传入的是**本轮刚观察的帧**(goal 核验先于 last_elements 更新)。
fn evaluate_fold(elements: &[ElementOut], fold: &(FoldTarget, FoldAssertion)) -> Option<bool> {
    let (target, assertion) = fold;
    let element = match target {
        FoldTarget::Id(value) => {
            let matches: Vec<&ElementOut> = elements
                .iter()
                .filter(|e| e.automation_id.as_deref() == Some(value.as_str()))
                .collect();
            if matches.len() == 1 {
                matches[0]
            } else {
                return None;
            }
        }
        FoldTarget::Name(value) => elements.iter().find(|e| {
            e.name
                .as_deref()
                .is_some_and(|n| n.contains(value.as_str()))
        })?,
    };
    match assertion {
        FoldAssertion::Exists => Some(true),
        FoldAssertion::ValueName { expected, contains } => {
            let name = element.name.as_deref().unwrap_or("");
            Some(if *contains {
                name.contains(expected.as_str())
            } else {
                name == expected
            })
        }
    }
}

fn probe_args(selector: &str) -> Result<(String, String), CtlError> {
    if let Some(value) = selector.strip_prefix("id:") {
        Ok(("--id".into(), value.into()))
    } else if let Some(value) = selector.strip_prefix("name:") {
        Ok(("--name".into(), value.into()))
    } else {
        Err(CtlError::protocol(format!(
            "frontier selector is not semantic: {selector}"
        )))
    }
}

/// 探测结果的死活分类:确定性错误(定位失败/歧义/不可操作/权限)→ 死;
/// 瞬态错误(超时/内部/中止)与成功 → 不标记——瞬态留给动作时的单点
/// probe 权威复核,避免并发抖动把活候选误判死(P3 并行探测的稳健性
/// 规则,也是 docs/77 发现 #3 的经验:过渡期错误不代表控件不存在)。
fn probe_is_definitively_dead(result: &Result<(), CtlError>) -> bool {
    match result {
        Ok(()) => false,
        Err(error) => matches!(
            error.code,
            ErrorCode::NotFound
                | ErrorCode::Ambiguous
                | ErrorCode::NotActionable
                | ErrorCode::PermDenied
        ),
    }
}

/// 批量存活性预检(P3 并行探测的引擎侧入口):对 selector 列表构造
/// find 调用经 Host 的 execute_probes 通道执行(CLI 侧并行,MockHost
/// 串行);确定性死亡标 tried=dead 并计 errors。瞬态错误静默跳过——
/// 卡/队列的行进路径会再经 act() 的单点 probe 复核。
fn batch_probe_liveness(
    spec: &ExploreSpec,
    state: &mut ExploreState,
    host: &mut impl Host,
    selectors: &[String],
) -> Result<(), CtlError> {
    if selectors.is_empty() {
        return Ok(());
    }
    let calls: Vec<Invocation> = selectors
        .iter()
        .map(|selector| {
            let (flag, value) = probe_args(selector)?;
            Ok(invocation(
                "find",
                vec!["--app".into(), spec.app.clone(), flag, value],
            ))
        })
        .collect::<Result<Vec<_>, CtlError>>()?;
    let results = host.execute_probes(&calls);
    for (selector, result) in selectors.iter().zip(results) {
        if probe_is_definitively_dead(&result) {
            state.errors += 1;
            state.tried.insert(selector.clone(), "dead".into());
        }
    }
    Ok(())
}

fn log_action(
    state: &mut ExploreState,
    selector: &str,
    mode: &str,
    outcome: &str,
    text: Option<&str>,
) {
    state.action_log.push(ActionRecord {
        step: state.actions + 1,
        selector: selector.into(),
        mode: mode.into(),
        outcome: outcome.into(),
        text: text.map(str::to_owned),
    });
    if state.action_log.len() > ACTION_LOG_CAP {
        state.action_log.remove(0);
    }
}

/// 帧背书(docs/77 发现 #6):当前观察帧是引擎对存活性的权威视图——
/// find/click 的 item-container 定位对 flyout 项**间歇性失明**(同现场
/// 0 结果与命中交替,快照树稳定可见),两条通道可靠性倒挂。元素级解析
/// (id 精确/name 子串首个,镜像选择器语义):返回帧内元素及其 ref
/// (若有)——ref 供绑定动作走捕获身份。
fn frame_element<'a>(elements: &'a [ElementOut], selector: &str) -> Option<&'a ElementOut> {
    match selector.split_once(':') {
        Some(("id", value)) => elements
            .iter()
            .find(|e| e.automation_id.as_deref() == Some(value)),
        Some(("name", value)) => elements
            .iter()
            .find(|e| e.name.as_deref().is_some_and(|n| n.contains(value))),
        _ => None,
    }
}

fn act(
    spec: &ExploreSpec,
    state: &mut ExploreState,
    store: &Store,
    host: &mut impl Host,
    selector: &str,
    mode: &str,
) -> Result<ActOutcome, CtlError> {
    // 帧背书:本轮刚观察的帧内候选,快照即新鲜度证据——跳过 COM 探测
    // (find 对 flyout 间歇失明,发现 #6)。陈旧上下文(应答回退队列等
    // 不在当前帧的候选)保持 probe-before-act 原语义(docs/42 §4.1:
    // 死控件不进动作路径,不消耗动作预算)。
    let framed = frame_element(&state.last_elements, selector);
    let vouched = framed.is_some();
    if !vouched {
        let (flag, value) = probe_args(selector)?;
        let probe = invocation("find", vec!["--app".into(), spec.app.clone(), flag, value]);
        if let Err(error) = host.execute(&probe) {
            if error.code == ErrorCode::Aborted && (host.stopped() || store.pause_requested()) {
                return Ok(ActOutcome::Boundary);
            }
            state.errors += 1;
            state.tried.insert(selector.into(), "dead".into());
            log_action(state, selector, mode, "dead", None);
            return Ok(ActOutcome::Dead);
        }
    }
    // ref 绑定动作(发现 #6 结构性收口):帧内元素携带 ref 且当前快照
    // id 在案 → 动作走捕获身份解析(`click/type @eN --snapshot <id>
    // --app`,docs/31 身份纪律),绕开间歇失明的 item-container 定位
    // 通道;STALE_REF(元素重建)按帧内候选的有界重试语义,下轮重新
    // 观察获得新 ref。无 ref/无快照 id(含 MockHost 默认)回退选择器路径。
    let typed = spec.texts.get(selector);
    let ref_target = framed
        .and_then(|element| element.ref_id.clone())
        .filter(|_| state.snapshot_id.as_deref().is_some_and(|s| !s.is_empty()));
    let action = match (ref_target, typed) {
        (Some(ref_id), Some(text)) => invocation(
            "type",
            vec![
                ref_id,
                text.clone(),
                "--snapshot".into(),
                state.snapshot_id.clone().unwrap_or_default(),
                "--app".into(),
                spec.app.clone(),
            ],
        ),
        (Some(ref_id), None) => invocation(
            "click",
            vec![
                ref_id,
                "--snapshot".into(),
                state.snapshot_id.clone().unwrap_or_default(),
                "--app".into(),
                spec.app.clone(),
            ],
        ),
        (None, Some(text)) => invocation(
            "type",
            vec![
                selector.into(),
                text.clone(),
                "--app".into(),
                spec.app.clone(),
            ],
        ),
        (None, None) => invocation(
            "click",
            vec![selector.into(), "--app".into(), spec.app.clone()],
        ),
    };
    match host.execute(&action) {
        Ok(()) => {
            state.actions += 1;
            state.tried.insert(selector.into(), "acted".into());
            log_action(state, selector, mode, "acted", typed.cloned().as_deref());
            state.last_actor = Some(selector.into());
            Ok(ActOutcome::Acted)
        }
        Err(error) => {
            if error.code == ErrorCode::Aborted && (host.stopped() || store.pause_requested()) {
                return Ok(ActOutcome::Boundary);
            }
            if let Some(evidence) = error.evidence.as_ref()
                && evidence["stage"] == "handoff"
            {
                return Ok(ActOutcome::HandoffPaused);
            }
            state.errors += 1;
            if vouched {
                // 帧内候选的动作失败:疑似定位通道抖动/身份重建(STALE_REF)
                // 而非控件死亡——不判死,计一次动作预算(有界重试,下轮前沿
                // 重导再试);真死的候选在预算内自然耗尽,终态保持诚实。
                state.actions += 1;
                log_action(state, selector, mode, "failed", None);
                return Ok(ActOutcome::Dead);
            }
            state.tried.insert(selector.into(), "dead".into());
            log_action(state, selector, mode, "failed", None);
            Ok(ActOutcome::Dead)
        }
    }
}

fn needs_review(run: &mut Run, reason: &str) {
    run.status = Status::NeedsReview;
    run.reason = Some(reason.into());
}

/// 把 goal 调用的 --app 参数改写为指定窗口(跨窗探测/转移用)。
/// goal 无 --app(如 verify-file)时原样返回——这类 goal 无窗口语义,
/// 探测侧自行跳过。
fn rewrite_goal_app(goal: &Invocation, app: &str) -> Invocation {
    let mut rewritten = goal.clone();
    if let Some(index) = rewritten.args.iter().position(|a| a == "--app")
        && rewritten.args.get(index + 1).is_some()
    {
        rewritten.args[index + 1] = app.to_owned();
    }
    rewritten
}

/// 跨窗 goal 探测预算:整个运行的探测轮次上限(每轮一次 list-windows +
/// ≤3 次 goal 窗口重写)。
const PROBE_EPISODE_BUDGET: u32 = 2;

/// 同进程跨窗探测:goal 目标在锚窗缺席时,枚举同 pid 的其他窗口(对话框/
/// 弹窗与主窗同进程),把 goal 的 --app 逐个改写为候选窗标题重试。
/// 命中即记录 goal_window(转移)并返回 true;预算耗尽/无候选/枚举失败
/// 返回 false(由调用方走存在性/重试/失败闭合路径)。list-windows 枚举
/// 失败不传播——探测是机会性增强,主错误语义留在 goal 调用本身。
fn probe_same_pid_goal(
    spec: &ExploreSpec,
    state: &mut ExploreState,
    host: &mut impl Host,
) -> Result<bool, CtlError> {
    if state.probe_episodes >= PROBE_EPISODE_BUDGET {
        return Ok(false);
    }
    // 无 --app 的 goal 无窗口语义,不可探测。
    if !spec.goal.args.iter().any(|a| a == "--app") {
        return Ok(false);
    }
    let Some(pid) = state.window_pid else {
        return Ok(false);
    };
    let current_title = state
        .window_title
        .clone()
        .unwrap_or_else(|| spec.app.clone());
    state.probe_episodes += 1;
    let Ok(value) = host.execute_output(&invocation("list-windows", vec![])) else {
        return Ok(false);
    };
    let windows = value
        .get("windows")
        .and_then(|w| w.as_array())
        .cloned()
        .unwrap_or_default();
    let mut tried = 0;
    let mut seen_titles = BTreeSet::new();
    for window in &windows {
        let title = window
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or_default();
        let same_pid = window.get("pid").and_then(|p| p.as_u64()) == Some(u64::from(pid));
        // 过滤:同进程、有标题、非当前观察窗;标题去重(UWP 同题多窗)。
        if !same_pid || title.is_empty() || !seen_titles.insert(title.to_owned()) {
            continue;
        }
        if title == current_title || current_title.contains(title) || title.contains(&current_title)
        {
            continue;
        }
        let candidate = rewrite_goal_app(&spec.goal, title);
        if host.execute_output(&candidate).is_ok() {
            state.goal_window = Some(title.to_owned());
            state.anchor_transfers += 1;
            return Ok(true);
        }
        tried += 1;
        if tried >= 3 {
            break;
        }
    }
    Ok(false)
}

/// 探索主入口。`answer` 为 `flow-resume --decision` 传入的候选 id;
/// 悬挂卡存在时必答,不存在时传入即 PROTOCOL。
pub fn execute(
    spec: &ExploreSpec,
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
    answer: Option<&str>,
    confidence: Option<&str>,
) -> Result<(), CtlError> {
    if !host.read_only(&spec.goal) {
        return Err(CtlError::protocol(
            "explore goal must be a read-only assertion (verify family)",
        ));
    }
    if run.status == Status::Completed {
        host.publish(run);
        return Ok(());
    }
    let mut state = match load(store)? {
        Some(state) => {
            if state.spec != *spec {
                return Err(CtlError::protocol(
                    "explore state does not match the frozen plan",
                ));
            }
            state
        }
        None => ExploreState::new(spec, host.now_ms()),
    };
    // 应答校验:悬挂卡必答、未知候选拒绝、危险候选允许(显式选择即授权)。
    if let Some(answer) = answer {
        let pending = state
            .pending
            .take()
            .ok_or_else(|| CtlError::protocol("no pending decision card to answer"))?;
        let choice = decision::parse_answer(answer, &pending.card)?;
        let chosen = pending
            .card
            .candidates
            .iter()
            .find(|c| c.id == choice)
            .ok_or_else(|| CtlError::protocol("decision card candidate missing"))?;
        // 排序回退队列:首选打头;其后为卡内非危险候选按卡序(docs/42 §3:
        // 一次往返摊平平局区间的多次尝试;危险候选永不自动回退)。
        let mut queue = vec![chosen.selector.clone()];
        queue.extend(
            pending
                .card
                .candidates
                .iter()
                .filter(|c| c.id != choice && !c.dangerous)
                .map(|c| c.selector.clone()),
        );
        // P3 队列存活性预检:卡发出到应答之间世界可能已变——批量探测把
        // 确定性死亡的候选出队,避免每个死项白耗一轮观察(~450ms/项)。
        batch_probe_liveness(spec, &mut state, host, &queue)?;
        queue.retain(|selector| !state.tried.contains_key(selector));
        state.queue = queue;
        if state.answers.len() < ANSWER_CAP {
            state.answers.push(AnswerRecord {
                decision_id: pending.card.card_id.clone(),
                choice,
                confidence: confidence
                    .filter(|c| !c.is_empty())
                    .map(|c| c.chars().take(32).collect()),
                answered_ms: host.now_ms(),
            });
        }
        state.last_actor = None;
    } else if state.pending.is_some() {
        return Err(CtlError::protocol(
            "run is waiting for a decision: pass --decision <choice_id> (see the card in flow-status)",
        ));
    }
    let mut next_mode = if answer.is_some() {
        Some("answer")
    } else {
        None
    };
    run.status = Status::Running;
    run.reason = None;
    state.persist_now(store)?;
    run.save(store, host)?;
    // 看门狗心跳:观察/动作轮次 = 进展(探索长任务不误杀)。
    actl_core::watchdog::reset();
    loop {
        // 边界:急停/暂停(与 engine::boundary 同语义,状态先持久化再返回)。
        if host.stopped() {
            run.status = Status::Stopped;
            run.reason = Some("stop_requested".into());
            state.goal_met = Some(false);
            persist(store, &state)?;
            return run.save(store, host);
        }
        if store.pause_requested() {
            run.status = Status::Paused;
            run.reason = Some("pause_requested".into());
            persist(store, &state)?;
            return run.save(store, host);
        }
        // 预算:动作/决策/时长(docs/42 §2:探索是有界运行)。
        if state.actions >= spec.budgets.actions {
            state.goal_met = Some(false);
            needs_review(run, "budget_exhausted:actions");
            persist(store, &state)?;
            return run.save(store, host);
        }
        if host.now_ms().saturating_sub(state.started_ms) >= spec.budgets.wall_ms {
            state.goal_met = Some(false);
            needs_review(run, "budget_exhausted:wall");
            persist(store, &state)?;
            return run.save(store, host);
        }

        // 观察(词表投影;失败如实停,不静默转空)。瞬态错误(INTERNAL/
        // TIMEOUT,如菜单开合过渡锁提供程序——docs/77 发现 #3)有界重试;
        // 协议错误(坏 spec)与窗口缺失(NOT_FOUND,结构性)按原语义立即处理。
        let elements = match observe(spec, &mut state, host) {
            Ok(elements) => elements,
            Err(error)
                if (error.code == ErrorCode::Internal || error.code == ErrorCode::Timeout)
                    && state.observation_retries < OBSERVATION_RETRY_LIMIT =>
            {
                state.observation_retries += 1;
                persist(store, &state)?;
                host.sleep_ms(OBSERVATION_RETRY_SETTLE_MS);
                continue;
            }
            Err(error) => {
                state.errors += 1;
                state.goal_met = Some(false);
                if error.code == ErrorCode::Protocol {
                    run.status = Status::Failed;
                    run.reason = Some("spec_rejected".into());
                } else {
                    needs_review(run, "observation_failed");
                }
                persist(store, &state)?;
                run.save(store, host)?;
                return Err(error);
            }
        };

        // 本地目标核验:P1 折叠优先(帧内判定,零采集),折叠未覆盖走权威
        // COM 调用。走 execute_output 而非 condition——保留 ASSERTION_FAILED
        // 与 NOT_FOUND 的区分(未满足/目标缺席两态,供有界重试与跨窗探测)。
        // LLM 永远不被问"成功了吗"。
        enum GoalCheck {
            Met,
            NotMet,
            Retry,
            Fatal(CtlError),
        }
        let existence_goal = spec.goal.args.iter().any(|a| a == "--exists");
        // 跨窗转移后 goal 已锚定其他窗口:折叠(锚窗帧)不适用。
        let folded = if state.goal_window.is_none() {
            parse_goal_fold(spec).and_then(|fold| evaluate_fold(&elements, &fold))
        } else {
            None
        };
        let check = if let Some(met) = folded {
            if met {
                GoalCheck::Met
            } else {
                GoalCheck::NotMet
            }
        } else {
            // goal 窗口:转移后的目标窗,否则探索锚窗。
            let goal_app = state
                .goal_window
                .clone()
                .unwrap_or_else(|| spec.app.clone());
            let goal_call = rewrite_goal_app(&spec.goal, &goal_app);
            match host.execute_output(&goal_call) {
                Ok(_) => GoalCheck::Met,
                // 目标缺席(NotFound)或查询故障(INTERNAL,如菜单开启态下
                // item-container 定位抛 RPC 服务器异常——docs/77 发现 #5,
                // 持续而非瞬态):都先做同进程跨窗探测(机会性,有界预算)。
                Err(error) if matches!(error.code, ErrorCode::NotFound | ErrorCode::Internal) => {
                    if state.goal_window.is_some() {
                        state.goal_window = None;
                    }
                    if probe_same_pid_goal(spec, &mut state, host)? {
                        GoalCheck::Met
                    } else {
                        match error.code {
                            ErrorCode::NotFound if existence_goal => GoalCheck::NotMet,
                            ErrorCode::NotFound if state.goal_retries < GOAL_RETRY_LIMIT => {
                                GoalCheck::Retry
                            }
                            // 查询故障不判死运行:goal 核验先于动作,持续故障
                            // 会饿死本该执行的探索动作(如点开菜单项)。计
                            // errors 如实记录,按未满足继续——预算有界兜底,
                            // 让世界变化后再核验。
                            ErrorCode::Internal => {
                                state.errors += 1;
                                GoalCheck::NotMet
                            }
                            _ => GoalCheck::Fatal(error),
                        }
                    }
                }
                Err(error) => match error.code {
                    // 断言未满足;wait 族超时=谓词未满(CLI 语义)。
                    ErrorCode::AssertionFailed | ErrorCode::Timeout => GoalCheck::NotMet,
                    // 存在型断言的歧义:多个匹配同样满足"存在"(真机实证:
                    // 另存为对话框打开后 name:文件名 命中 3 个元素)——CLI 的
                    // 唯一性纪律服务动作定位,对存在性过严。值断言的歧义
                    // (无法确定比较目标)维持失败闭合。
                    ErrorCode::Ambiguous if existence_goal => GoalCheck::Met,
                    _ => GoalCheck::Fatal(error),
                },
            }
        };
        match check {
            GoalCheck::Met => {
                state.goal_met = Some(true);
                run.status = Status::Completed;
                run.reason = None;
                persist(store, &state)?;
                return run.save(store, host);
            }
            GoalCheck::NotMet => {}
            GoalCheck::Retry => {
                state.goal_retries += 1;
                persist(store, &state)?;
                host.sleep_ms(GOAL_RETRY_SETTLE_MS);
                continue;
            }
            GoalCheck::Fatal(error) => {
                state.errors += 1;
                state.goal_met = Some(false);
                needs_review(run, "goal_check_failed");
                persist(store, &state)?;
                run.save(store, host)?;
                return Err(error);
            }
        }

        // 差分 + 效果归因(docs/42 §4.2 affordance 预标注的数据源)。
        let diff = diff::diff(&state.last_elements, &elements);
        let fingerprint = decision::state_fingerprint(&elements);
        let changed_any =
            !diff.added.is_empty() || !diff.removed.is_empty() || !diff.changed.is_empty();
        if let Some(actor) = state.last_actor.take() {
            let effect = if state.last_fingerprint.as_deref() == Some(fingerprint.as_str())
                && !changed_any
            {
                "no_effect"
            } else {
                "reveals"
            };
            state.tried.insert(actor, effect.into());
        }
        if state.visited.len() < VISITED_CAP {
            state.visited.insert(fingerprint.clone());
        }
        state.last_fingerprint = Some(fingerprint);
        if elements.len() > ELEMENTS_CAP {
            state.last_elements = elements[..ELEMENTS_CAP].to_vec();
        } else {
            state.last_elements = elements;
        }

        // 应答回退队列优先(不经熵门;队列成员是应答者已见候选)。
        if let Some(selector) = state.queue.first().cloned() {
            state.queue.remove(0);
            let mode = next_mode.take().unwrap_or("fallback");
            match act(spec, &mut state, store, host, &selector, mode)? {
                ActOutcome::Acted | ActOutcome::Dead => {}
                ActOutcome::HandoffPaused => {
                    run.status = Status::Paused;
                    run.reason = Some("handoff_pending".into());
                    persist(store, &state)?;
                    return run.save(store, host);
                }
                ActOutcome::Boundary => continue,
            }
            persist(store, &state)?;
            run.save(store, host)?;
            continue;
        }
        next_mode = None;

        // 前沿:未尝试候选,评分排序(词表命中+角色先验+新鲜度)。
        let added_selectors: BTreeSet<String> = state
            .last_elements
            .iter()
            .filter(|e| {
                diff.added.iter().any(|a| {
                    a.name == e.name && a.role == e.role && a.automation_id == e.automation_id
                })
            })
            .filter_map(decision::selector_of)
            .collect();
        let candidates: Vec<ScoredCandidate> = decision::frontier(
            &state.last_elements,
            &effective_keywords(spec),
            &added_selectors,
            &spec.priors,
        )
        .into_iter()
        .filter(|c| !state.tried.contains_key(&c.selector))
        .collect();
        if state.seen.len() < SEEN_CAP {
            state
                .seen
                .extend(candidates.iter().map(|c| c.selector.clone()));
        }

        if candidates.is_empty() {
            state.goal_met = Some(false);
            needs_review(run, "no_candidates");
            persist(store, &state)?;
            return run.save(store, host);
        }

        // 熵门:明确领先自动执行;平局/全危险外抛决策卡。
        match decision::gate(&candidates) {
            GateOutcome::AutoExecute(index) => {
                let selector = candidates[index].selector.clone();
                match act(spec, &mut state, store, host, &selector, "auto")? {
                    ActOutcome::Acted | ActOutcome::Dead => {}
                    ActOutcome::HandoffPaused => {
                        run.status = Status::Paused;
                        run.reason = Some("handoff_pending".into());
                        persist(store, &state)?;
                        return run.save(store, host);
                    }
                    ActOutcome::Boundary => continue,
                }
                persist(store, &state)?;
                run.save(store, host)?;
            }
            GateOutcome::Externalize => {
                if state.decisions >= spec.budgets.decisions {
                    state.goal_met = Some(false);
                    needs_review(run, "budget_exhausted:decisions");
                    persist(store, &state)?;
                    return run.save(store, host);
                }
                // 卡候选不再批量预检(发现 #6 修正):卡构建与观察同轮,帧
                // 即新鲜度——COM 定位对该帧的间歇失明会错杀卡面;死候
                // 选的过滤交给应答时的队列预检(跨轮,陈旧帧需真探测)与
                // 动作时的最终权威(点击自身解析)。
                let goal_text = format!("{} {}", spec.goal.command, spec.goal.args.join(" "));
                let card = decision::build_card(&CardInput {
                    card_id: &format!("d{}", state.decisions + 1),
                    task: &spec.task,
                    app: &spec.app,
                    goal: &goal_text,
                    step: state.actions,
                    diff: &diff,
                    candidates: &candidates,
                    effects: &state.tried,
                })?;
                state.audit.card_bytes += decision::card_bytes(&card) as u64;
                state.decisions += 1;
                state.pending = Some(PendingDecision { card });
                state.goal_met = Some(false);
                needs_review(run, "decision_pending");
                persist(store, &state)?;
                return run.save(store, host);
            }
        }
    }
}

impl ExploreState {
    fn persist_now(&self, store: &Store) -> Result<(), CtlError> {
        persist(store, self)
    }
}

#[cfg(test)]
#[path = "explore_tests.rs"]
mod tests;
