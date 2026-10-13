use super::*;
use crate::engine::StepStatus;
use actl_core::decision::CARD_MAX_BYTES;
use actl_core::snapshot::ElementOut;
use serde_json::{Value, json};

mod cross_window;
mod texts;

/// MockHost 快照 window.pid(跨窗同进程判定用)。
const MOCK_PID: u32 = 4242;

fn spec(keywords: &[&str], goal_exists: &str) -> ExploreSpec {
    ExploreSpec {
        task: "完成保存任务".into(),
        app: "测试应用".into(),
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        goal: Invocation {
            command: "verify".into(),
            args: vec![
                "--exists".into(),
                goal_exists.into(),
                "--app".into(),
                "测试应用".into(),
            ],
            bind: Default::default(),
        },
        priors: Default::default(),
        stats: Default::default(),
        texts: Default::default(),
        budgets: Default::default(),
    }
}

/// 值断言 goal(--value ... --property name 形态,P1 折叠覆盖面)。
fn value_spec(keywords: &[&str], target: &str, expected: &str, contains: bool) -> ExploreSpec {
    let mut args = vec![
        "--value".to_string(),
        target.into(),
        expected.into(),
        "--property".into(),
        "name".into(),
    ];
    if contains {
        args.push("--contains".into());
    }
    args.push("--app".into());
    args.push("测试应用".into());
    ExploreSpec {
        task: "完成显示任务".into(),
        app: "测试应用".into(),
        keywords: keywords.iter().map(|s| s.to_string()).collect(),
        goal: Invocation {
            command: "verify".into(),
            args,
            bind: Default::default(),
        },
        priors: Default::default(),
        stats: Default::default(),
        texts: Default::default(),
        budgets: Default::default(),
    }
}

fn el(role: &str, name: &str, id: Option<&str>) -> ElementOut {
    ElementOut {
        ref_id: None,
        role: role.into(),
        name: Some(name.into()),
        automation_id: id.map(str::to_string),
        depth: 1,
    }
}

/// 带 ref 的元素(ref 绑定动作测试用):快照给可交互元素分配 @eN。
fn el_ref(role: &str, name: &str, id: Option<&str>, r: &str) -> ElementOut {
    ElementOut {
        ref_id: Some(r.into()),
        role: role.into(),
        name: Some(name.into()),
        automation_id: id.map(str::to_string),
        depth: 1,
    }
}

/// 脚本化 MockHost(docs/42 §9.2.5:固定应答驱动全链路,离线可测)。
/// 帧序列驱动观察;click 按 selector 转移;verify 按 --exists/--value
/// 断言(缺席=NOT_FOUND,值断言按 name 通道);dead_probes 模拟"快照
/// 有但 find 失效"的死候选;goal_not_found_times 模拟瞬态 NOT_FOUND。
/// verify_calls/snapshot_keywords 记录供折叠测试断言零采集;
/// dialog 模拟同进程跨窗(点击 dialog_openers 中的 selector 后出现,
/// verify 按 --app 路由到该窗;list-windows 枚举它)。
struct MockHost {
    frames: Vec<Vec<ElementOut>>,
    index: usize,
    /// selector → 点击后的帧偏移(None = 无效果,停留在当前帧)。
    transitions: BTreeMap<String, Option<usize>>,
    /// probe 一律失败的 selector(死候选模拟)。
    dead_probes: BTreeSet<String>,
    /// goal 断言前 N 次返回 NOT_FOUND(瞬态模拟)。
    goal_not_found_times: u32,
    // 前 N 次 verify 返回 INTERNAL(item-container 查询故障模拟,发现 #5)。
    goal_fault_times: u32,
    /// --exists 目标返回 AMBIGUOUS(存在型歧义语义测试)。
    goal_ambiguous: bool,
    /// selector → 剩余存活探测次数:递减到 0 后 probe 判死(模拟"卡发出
    /// 后世界变化",测应答时的队列预过滤)。
    probe_lives: BTreeMap<String, u32>,
    /// 前 N 次 find 返回 TIMEOUT(瞬态探测错误,不应判死)。
    transient_probes: u32,
    // 前 N 次快照返回 INTERNAL(瞬态观察失败,docs/77 发现 #3 模拟)。
    snapshot_failures: u32,
    /// 快照计数(execute_snapshot 发号)。
    snapshot_counter: u32,
    /// 最近一次快照 id(引擎 --snapshot 新鲜度核对用)。
    last_snapshot_id: Option<String>,
    /// 最近帧的 ref → 元素(@eN 解析模拟)。
    ref_map: BTreeMap<String, ElementOut>,
    /// 解析即失败的 ref(STALE_REF 模拟:元素重建/身份漂移)。
    stale_refs: BTreeSet<String>,
    /// 原始动作参数记录(断言 ref 绑定路径用)。
    raw_actions: Vec<Vec<String>>,
    clicks: Vec<String>,
    /// 输入原语记录:(selector, text)。
    types: Vec<(String, String)>,
    verify_calls: u32,
    snapshot_keywords: Vec<String>,
    /// 弹窗:(标题, 元素, 是否已出现, 是否跨进程 broker 形态)。
    /// broker=true 模拟 RuntimeBroker 承载的对话框:list-windows 枚举时
    /// pid 与宿主不同且带 owner_title=宿主标题(轮 C 放宽的验证形态)。
    dialog: Option<(String, Vec<ElementOut>, bool, bool)>,
    /// 点击后打开弹窗的 selector。
    dialog_openers: BTreeSet<String>,
    stopped_flag: bool,
    paused_flag: bool,
    published: Vec<String>,
}

impl MockHost {
    fn new(frames: Vec<Vec<ElementOut>>) -> Self {
        Self {
            frames,
            index: 0,
            transitions: BTreeMap::new(),
            dead_probes: BTreeSet::new(),
            goal_not_found_times: 0,
            goal_fault_times: 0,
            goal_ambiguous: false,
            probe_lives: BTreeMap::new(),
            transient_probes: 0,
            snapshot_failures: 0,
            snapshot_counter: 0,
            last_snapshot_id: None,
            ref_map: BTreeMap::new(),
            stale_refs: BTreeSet::new(),
            raw_actions: Vec::new(),
            clicks: Vec::new(),
            types: Vec::new(),
            verify_calls: 0,
            snapshot_keywords: Vec::new(),
            dialog: None,
            dialog_openers: BTreeSet::new(),
            stopped_flag: false,
            paused_flag: false,
            published: Vec::new(),
        }
    }

    /// 配置同进程弹窗与打开它的 selector(跨窗探测测试)。
    fn with_dialog(mut self, title: &str, elements: Vec<ElementOut>, opener: &str) -> Self {
        self.dialog = Some((title.into(), elements, false, false));
        self.dialog_openers.insert(opener.into());
        self
    }

    /// 配置跨进程 broker 弹窗(RuntimeBroker 形态,轮 C 放宽的验证):
    /// list-windows 枚举时 pid 与宿主不同且带 owner_title=宿主标题。
    fn with_broker_dialog(mut self, title: &str, elements: Vec<ElementOut>, opener: &str) -> Self {
        self.dialog = Some((title.into(), elements, false, true));
        self.dialog_openers.insert(opener.into());
        self
    }

    fn frame(&self) -> &[ElementOut] {
        self.frames
            .get(self.index)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// verify/list-windows 按 --app 值路由:弹窗已出现且标题匹配 → 弹窗帧。
    fn frame_for_app(&self, app: Option<&str>) -> &[ElementOut] {
        if let Some(app) = app
            && let Some((title, elements, present, _)) = &self.dialog
            && *present
            && (title == app || app.contains(title.as_str()))
        {
            return elements;
        }
        self.frame()
    }

    fn selector_of_frame(&self, selector: &str) -> Option<ElementOut> {
        let (kind, value) = match selector.split_once(':') {
            Some((k, v)) => (k, v),
            None => return None,
        };
        // focused: 键(② 收口):焦点模拟——帧内有 Edit 即视为焦点可达。
        if kind == "focused" {
            return self.frame().iter().find(|e| e.role == "Edit").cloned();
        }
        self.frame()
            .iter()
            .find(|e| match kind {
                "id" => e.automation_id.as_deref() == Some(value),
                "name" => e.name.as_deref() == Some(value),
                _ => false,
            })
            .cloned()
    }

    fn app_of_call(call: &Invocation) -> Option<&str> {
        call.args
            .iter()
            .position(|a| a == "--app")
            .and_then(|index| call.args.get(index + 1))
            .map(String::as_str)
    }

    /// 动作目标解析:选择器原样;@eN 走 ref 通道——核对 --snapshot 新鲜度
    /// (id 必须等于最近快照)、查 ref_map 取元素并还原其选择器;stale_refs
    /// 或未知 ref 返回 STALE_REF(元素重建/身份漂移模拟)并**应用转移**
    /// (世界已变,帧推进——引擎下轮重新观察拿新 ref)。
    fn resolve_target(&mut self, call: &Invocation) -> Result<String, CtlError> {
        let target = call.args.first().cloned().unwrap_or_default();
        if !target.starts_with('@') {
            return Ok(target);
        }
        let snapshot_arg = call
            .args
            .iter()
            .position(|a| a == "--snapshot")
            .and_then(|index| call.args.get(index + 1));
        if let Some(expected) = snapshot_arg
            && self.last_snapshot_id.as_deref() != Some(expected.as_str())
        {
            return Err(CtlError::new(
                ErrorCode::StaleRef,
                "snapshot mismatch (stale frame)",
            ));
        }
        if self.stale_refs.contains(&target) {
            // 身份漂移:点击未送达,但世界已变——应用转移让下轮观察拿新帧。
            if let Some(element) = self.ref_map.get(&target).cloned()
                && let Some(selector) = Self::selector_of_element(&element)
                && let Some(Some(next)) = self.transitions.get(&selector).cloned()
            {
                self.index = next;
            }
            return Err(CtlError::new(ErrorCode::StaleRef, "ref rebuilt"));
        }
        let element = self
            .ref_map
            .get(&target)
            .cloned()
            .ok_or_else(|| CtlError::new(ErrorCode::StaleRef, "ref unknown"))?;
        Ok(Self::selector_of_element(&element).unwrap_or_default())
    }

    fn selector_of_element(element: &ElementOut) -> Option<String> {
        if let Some(id) = element.automation_id.as_deref().filter(|s| !s.is_empty()) {
            return Some(format!("id:{id}"));
        }
        element
            .name
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|n| format!("name:{n}"))
    }
}

impl Host for MockHost {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        match call.command.as_str() {
            "find" => {
                let id = call
                    .args
                    .iter()
                    .position(|a| a == "--id")
                    .and_then(|p| call.args.get(p + 1));
                let name = call
                    .args
                    .iter()
                    .position(|a| a == "--name")
                    .and_then(|p| call.args.get(p + 1));
                let selector = id
                    .map(|v| format!("id:{}", v))
                    .or_else(|| name.map(|v| format!("name:{}", v)));
                // 瞬态探测错误(TIMEOUT):不应判死(引擎侧仅确定性码标 dead)。
                if self.transient_probes > 0 {
                    self.transient_probes -= 1;
                    return Err(CtlError::new(ErrorCode::Timeout, "provider jitter"));
                }
                // 存活配额:递减到 0 后判死(模拟卡发出后候选消失)。
                if let Some(selector) = &selector
                    && let Some(lives) = self.probe_lives.get_mut(selector)
                {
                    if *lives == 0 {
                        return Err(CtlError::new(ErrorCode::NotFound, "lives exhausted"));
                    }
                    *lives -= 1;
                }
                if selector
                    .as_ref()
                    .is_some_and(|s| self.dead_probes.contains(s))
                {
                    return Err(CtlError::new(ErrorCode::NotFound, "probe dead"));
                }
                let present = self.frame().iter().any(|e| {
                    id.is_some_and(|v| e.automation_id.as_deref() == Some(v.as_str()))
                        || name.is_some_and(|v| e.name.as_deref() == Some(v.as_str()))
                });
                if present {
                    Ok(())
                } else {
                    Err(CtlError::new(ErrorCode::NotFound, "probe dead"))
                }
            }
            "click" => {
                if self.stopped_flag || self.paused_flag {
                    return Err(CtlError::new(ErrorCode::Aborted, "aborted"));
                }
                self.raw_actions.push(call.args.clone());
                let selector = self.resolve_target(call)?;
                if self.selector_of_frame(&selector).is_none() {
                    return Err(CtlError::new(ErrorCode::NotFound, "click target gone"));
                }
                self.clicks.push(selector.clone());
                if let Some(Some(next)) = self.transitions.get(&selector).cloned() {
                    self.index = next;
                }
                // 跨窗模拟:打开弹窗(同 pid,后续 list-windows 可枚举)。
                if self.dialog_openers.contains(&selector)
                    && let Some((_, _, present, _)) = self.dialog.as_mut()
                {
                    *present = true;
                }
                Ok(())
            }
            "type" => {
                if self.stopped_flag || self.paused_flag {
                    return Err(CtlError::new(ErrorCode::Aborted, "aborted"));
                }
                // 输入原语:selector + text 两个位置参数;转移语义同 click
                // (调用方用 transitions 模拟输入后的界面变化)。
                self.raw_actions.push(call.args.clone());
                let selector = self.resolve_target(call)?;
                let text = call.args.get(1).cloned().unwrap_or_default();
                if self.selector_of_frame(&selector).is_none() {
                    return Err(CtlError::new(ErrorCode::NotFound, "type target gone"));
                }
                self.types.push((selector.clone(), text));
                if let Some(Some(next)) = self.transitions.get(&selector).cloned() {
                    self.index = next;
                }
                Ok(())
            }
            "verify" => {
                self.verify_calls += 1;
                if self.goal_ambiguous {
                    return Err(CtlError::new(
                        ErrorCode::Ambiguous,
                        "goal target matches multiple elements",
                    ));
                }
                if self.goal_fault_times > 0 {
                    self.goal_fault_times -= 1;
                    return Err(CtlError::internal(
                        "item-container query fault (server threw)",
                    ));
                }
                if self.goal_not_found_times > 0 {
                    self.goal_not_found_times -= 1;
                    return Err(CtlError::new(
                        ErrorCode::NotFound,
                        "goal element transiently unavailable",
                    ));
                }
                // 目标取自 --exists 或 --value(位置参数:目标、期望值);
                // 缺席返回 NotFound(镜像真实 verify 定位失败),命中后
                // 存在型即通过,值断言按 name 通道比较。
                let target = call
                    .args
                    .iter()
                    .position(|a| a == "--exists" || a == "--value")
                    .and_then(|p| call.args.get(p + 1))
                    .map(|s| s.as_str())
                    .unwrap_or_default();
                let expected = call
                    .args
                    .iter()
                    .position(|a| a == "--value")
                    .and_then(|p| call.args.get(p + 2))
                    .cloned();
                let contains = call.args.iter().any(|a| a == "--contains");
                let (kind, value) = match target.split_once(':') {
                    Some((k, v)) => (k, v),
                    None => return Err(CtlError::protocol("bad verify target")),
                };
                // 按 goal 的 --app 路由窗口(跨窗探测的候选窗重写后指向弹窗)。
                let frame = self.frame_for_app(Self::app_of_call(call));
                let element = frame.iter().find(|e| match kind {
                    "id" => e.automation_id.as_deref() == Some(value),
                    "name" => e.name.as_deref().is_some_and(|n| n.contains(value)),
                    _ => false,
                });
                match element {
                    None => Err(CtlError::new(ErrorCode::NotFound, "goal target absent")),
                    Some(element) => {
                        let assertion = call.args.iter().any(|a| a == "--exists");
                        if assertion {
                            return Ok(());
                        }
                        let name = element.name.as_deref().unwrap_or_default();
                        let expected = expected.as_deref().unwrap_or_default();
                        let matched = if contains {
                            name.contains(expected)
                        } else {
                            name == expected
                        };
                        if matched {
                            Ok(())
                        } else {
                            Err(CtlError::new(
                                ErrorCode::AssertionFailed,
                                "goal not met yet",
                            ))
                        }
                    }
                }
            }
            _ => Err(CtlError::protocol(format!("unexpected {}", call.command))),
        }
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
        if call.command == "list-windows" {
            // 同进程窗口枚举:主窗 + 已出现的弹窗(跨窗探测数据源)。
            let mut windows = vec![json!({
                "title": "测试应用", "class": "Mock", "pid": MOCK_PID
            })];
            if let Some((title, _, present, broker)) = &self.dialog
                && *present
            {
                // broker 形态:跨进程 pid + owner_title 指宿主(RuntimeBroker
                // 承载的 Store 应用对话框,轮 C 放宽判据的验证形态)。
                if *broker {
                    windows.push(json!({
                        "title": title, "class": "MockDialog",
                        "pid": MOCK_PID + 1000,
                        "owner_title": "测试应用"
                    }));
                } else {
                    windows.push(json!({
                        "title": title, "class": "MockDialog", "pid": MOCK_PID
                    }));
                }
            }
            return Ok(json!({"count": windows.len(), "windows": windows}));
        }
        if call.command != "snapshot" {
            return self.execute(call).map(|_| json!({"verified": true}));
        }
        // 瞬态观察失败:前 N 次快照返回 INTERNAL(发现 #3 模拟)。
        if self.snapshot_failures > 0 {
            self.snapshot_failures -= 1;
            return Err(CtlError::internal("provider jitter"));
        }
        // 按 --app 路由窗口(锚点转移探测候选窗/转移后的观察均指向弹窗);
        // 未出现或标题不匹配 → 主窗帧。窗元数据随路由如实(标题/pid)。
        let app = Self::app_of_call(call);
        let routed_to_dialog = app.is_some_and(|app| {
            self.dialog.as_ref().is_some_and(|(title, _, present, _)| {
                *present && (title == app || app.contains(title.as_str()))
            })
        });
        let window_title = if routed_to_dialog {
            self.dialog
                .as_ref()
                .map(|(title, _, _, _)| title.clone())
                .unwrap_or_else(|| "测试应用".into())
        } else {
            "测试应用".to_string()
        };
        let frame: Vec<ElementOut> = if routed_to_dialog {
            self.dialog
                .as_ref()
                .map(|(_, elements, _, _)| elements.clone())
                .unwrap_or_default()
        } else {
            self.frame().to_vec()
        };
        let elements: Vec<Value> = frame
            .iter()
            .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
            .collect();
        let keywords = call
            .args
            .iter()
            .position(|a| a == "--keywords")
            .and_then(|p| call.args.get(p + 1))
            .cloned();
        if let Some(keywords) = keywords {
            self.snapshot_keywords.push(keywords);
        }
        Ok(json!({
            "window": {"title": window_title, "class": "Mock", "pid": MOCK_PID},
            "elements": elements,
            "truncated": false
        }))
    }
    fn execute_snapshot(&mut self, args: Vec<String>) -> Result<(String, Value), CtlError> {
        let call = Invocation {
            command: "snapshot".into(),
            args,
            bind: Default::default(),
        };
        let value = self.execute_output(&call)?;
        // 发号 + 记录帧内 ref 映射(引擎 --snapshot 新鲜度核对与 @eN 解析)。
        self.snapshot_counter += 1;
        let id = format!("s-mock{}", self.snapshot_counter);
        self.ref_map = self
            .frame()
            .iter()
            .filter_map(|e| e.ref_id.clone().map(|r| (r, e.clone())))
            .collect();
        self.last_snapshot_id = Some(id.clone());
        Ok((id, value))
    }
    fn condition(&mut self, call: &Invocation) -> Result<bool, CtlError> {
        match self.execute(call) {
            Ok(()) => Ok(true),
            Err(e) if e.code == ErrorCode::AssertionFailed => Ok(false),
            Err(e) => Err(e),
        }
    }
    fn read_only(&self, call: &Invocation) -> bool {
        matches!(
            call.command.as_str(),
            "snapshot" | "find" | "verify" | "wait"
        )
    }
    fn stopped(&self) -> bool {
        self.stopped_flag
    }
    fn publish(&mut self, run: &Run) {
        self.published
            .push(format!("{}", run.status == Status::Completed));
    }
    fn sleep_ms(&mut self, _ms: u64) {}
    fn now_ms(&self) -> u64 {
        1_000
    }
}

fn setup(spec: &ExploreSpec) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "actl-explore-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let plan = crate::Plan {
        version: 1,
        name: "explore-test".into(),
        handoff: None,
        auto_resume: None,
        explore: Some(spec.clone()),
        popup_handlers: Vec::new(),
        popup_ignores: Vec::new(),
        steps: Vec::new(),
    };
    let store = Store::at(&root, "run1").expect("store");
    let run = store.create(&plan, "run1").expect("run");
    (root, store, run)
}

#[test]
fn clear_winner_auto_acts_and_completes() {
    let mut spec = spec(&["保存", "save"], "name:完成");
    spec.budgets.actions = 4;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("MenuItem", "文件", Some("menuFile")),
            el("Button", "保存", Some("btnSave")),
            el("Button", "其他", Some("btnOther")),
        ],
        vec![
            el("Button", "保存", Some("btnSave")),
            el("Text", "完成", None),
        ],
    ]);
    host.transitions.insert("id:btnSave".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("explore completes");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:btnSave"]);
    let state = load(&store).unwrap().expect("state persisted");
    assert_eq!(state.decisions, 0, "clear winner must not externalize");
    assert_eq!(state.goal_met, Some(true));
    assert_eq!(state.actions, 1);
    assert!(state.audit.observation_bytes > 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn tie_externalizes_card_then_answer_with_fallback() {
    let mut spec = spec(&["导出", "另存"], "name:完成");
    spec.budgets.actions = 6;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "导出", Some("btnExport")),
            el("Button", "另存", Some("btnSaveAs")),
        ],
        // 点导出无效果(同帧);点另存进终态。
        vec![
            el("Button", "导出", Some("btnExport")),
            el("Button", "另存", Some("btnSaveAs")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:btnExport".into(), None);
    host.transitions.insert("id:btnSaveAs".into(), Some(2));
    // 第一段:平局外抛
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("decision_pending"));
    let state = load(&store).unwrap().expect("state");
    let pending = state.pending.as_ref().expect("card pending");
    assert!(decision::card_bytes(&pending.card) <= CARD_MAX_BYTES);
    assert!(pending.card.candidates.len() >= 2);
    // 卡内候选 id 形如 c1/c2;导出应为 c1(评分并列取 selector 字典序:id:btnExport < id:btnSaveAs)
    assert_eq!(pending.card.candidates[0].id, "c1");
    // 第二段:应答 c1(导出),无效果后自动回退 c2(另存),一次往返完成任务
    execute(&spec, &mut run, &store, &mut host, Some("c1"), Some("0.8")).expect("resume");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:btnExport", "id:btnSaveAs"]);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.decisions, 1, "fallback must not emit a new card");
    assert_eq!(state.actions, 2);
    assert_eq!(
        state.tried.get("id:btnExport").map(String::as_str),
        Some("no_effect")
    );
    assert_eq!(state.answers.len(), 1);
    assert_eq!(state.answers[0].choice, "c1");
    assert_eq!(state.answers[0].confidence.as_deref(), Some("0.8"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dangerous_candidates_are_never_auto_executed() {
    let mut spec = spec(&["清理"], "name:完成");
    spec.budgets.actions = 4;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "删除全部", Some("btnDelete")),
            el("Button", "清理缓存", Some("btnClean")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:btnClean".into(), Some(1));
    host.transitions.insert("id:btnDelete".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("auto on safe candidate");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.clicks,
        vec!["id:btnClean"],
        "dangerous head must be skipped"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn all_dangerous_frontier_externalizes_and_answer_authorizes() {
    let mut spec = spec(&["清理"], "name:完成");
    spec.budgets.actions = 4;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el("Button", "删除全部", Some("btnDelete"))],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:btnDelete".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    assert_eq!(run.reason.as_deref(), Some("decision_pending"));
    let state = load(&store).unwrap().expect("state");
    let card = state.pending.as_ref().expect("card").card.clone();
    assert!(card.candidates[0].dangerous);
    // 显式应答即授权,危险候选被执行
    execute(&spec, &mut run, &store, &mut host, Some("c1"), None).expect("authorized");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:btnDelete"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn action_budget_exhaustion_is_honest() {
    let mut spec = spec(&["保存"], "name:完成");
    spec.budgets.actions = 1;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el("Button", "保存一", Some("b1"))],
        vec![el("Button", "保存二", Some("b2"))],
        vec![el("Button", "保存三", Some("b3"))],
    ]);
    host.transitions.insert("id:b1".into(), Some(1));
    host.transitions.insert("id:b2".into(), Some(2));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("bounded exit");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("budget_exhausted:actions"));
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.actions, 1);
    assert_eq!(state.goal_met, Some(false));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stop_and_pause_boundaries_hold() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.stopped_flag = true;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("stop boundary");
    assert_eq!(run.status, Status::Stopped);
    assert_eq!(run.reason.as_deref(), Some("stop_requested"));
    let mut run2 = store.load().unwrap().1;
    let mut host2 = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    execute(&spec, &mut run2, &store, &mut host2, None, None).expect("resume works");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn no_candidates_after_widen_is_needs_review() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![Vec::new()]);
    execute(&spec, &mut run, &store, &mut host, None, None).expect("honest empty");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("no_candidates"));
    let state = load(&store).unwrap().expect("state");
    assert!(state.widened, "zero-hit keyword must widen once");
    assert_eq!(state.observations, 2, "keyword + skeleton snapshots");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn answer_without_pending_card_is_rejected() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.transitions.insert("id:b1".into(), Some(1));
    let error =
        execute(&spec, &mut run, &store, &mut host, Some("c1"), None).expect_err("no pending card");
    assert_eq!(error.code, ErrorCode::Protocol);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn resume_without_decision_is_rejected_when_pending() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![
        el("Button", "保存甲", Some("a1")),
        el("Button", "保存乙", Some("a2")),
    ]]);
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    let error =
        execute(&spec, &mut run, &store, &mut host, None, None).expect_err("decision required");
    assert_eq!(error.code, ErrorCode::Protocol);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn framed_candidate_survives_flaky_probe() {
    // 发现 #6(帧背书):前沿候选来自本轮刚观察的帧——COM find 对它
    // 间歇性失明(0 结果)不代表死亡;快照是权威视图,点击自身解析裁决。
    // probe 失败的帧内候选照常点击。非帧内候选(队列/陈旧)保持探测判死。
    let mut spec = spec(&["保存", "main"], "name:完成");
    spec.budgets.actions = 2;
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存主", Some("btnMain")),
            el("Button", "保存次", Some("b2")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    // find 对 btnMain 失明,但它在当前帧内且可点击(快照视图是对的)
    host.dead_probes.insert("id:btnMain".into());
    host.transitions.insert("id:btnMain".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("frame vouches");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.clicks,
        vec!["id:btnMain"],
        "framed candidate is clicked despite the flaky probe"
    );
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.actions, 1);
    assert!(
        state.tried.get("id:btnMain").map(String::as_str) != Some("dead"),
        "framed candidate is never death-marked by the locate channel"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn observation_failure_is_needs_review_not_silent_success() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![Vec::new()]);
    host.stopped_flag = false;
    // snapshot 输出损坏 → PROTOCOL → run Failed,错误如实传播
    struct BrokenHost(MockHost);
    impl Host for BrokenHost {
        fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
            self.0.execute(call)
        }
        fn execute_output(&mut self, _call: &Invocation) -> Result<Value, CtlError> {
            Ok(json!({"truncated": false}))
        }
        fn read_only(&self, call: &Invocation) -> bool {
            self.0.read_only(call)
        }
        fn stopped(&self) -> bool {
            false
        }
        fn publish(&mut self, _run: &Run) {}
    }
    let mut broken = BrokenHost(host);
    let error =
        execute(&spec, &mut run, &store, &mut broken, None, None).expect_err("broken observation");
    assert_eq!(error.code, ErrorCode::Protocol);
    assert_eq!(run.reason.as_deref(), Some("spec_rejected"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn goal_check_error_fails_closed() {
    // 非瞬态错误(PERM_DENIED)立即失败闭合,不消耗重试。
    let spec = value_spec(&["保存"], "name:完成", "完成", false);
    let (root, store, mut run) = setup(&spec);
    struct GoalErrHost(MockHost);
    impl Host for GoalErrHost {
        fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
            if call.command == "verify" {
                return Err(CtlError::new(ErrorCode::PermDenied, "goal window gone"));
            }
            self.0.execute(call)
        }
        fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
            // 与引擎同路径:非快照命令经 execute 走 verify 拦截。
            if call.command != "snapshot" {
                return self.execute(call).map(|_| json!({"verified": true}));
            }
            self.0.execute_output(call)
        }
        fn read_only(&self, call: &Invocation) -> bool {
            self.0.read_only(call)
        }
        fn stopped(&self) -> bool {
            false
        }
        fn publish(&mut self, _run: &Run) {}
    }
    let host = GoalErrHost(MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]));
    let mut host = host;
    let error =
        execute(&spec, &mut run, &store, &mut host, None, None).expect_err("goal check fails");
    assert_eq!(error.code, ErrorCode::PermDenied);
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("goal_check_failed"));
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.goal_retries, 0, "non-transient errors do not retry");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn persisted_state_round_trips() {
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![
        el("Button", "保存甲", Some("a1")),
        el("Button", "保存乙", Some("a2")),
    ]]);
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.version, 1);
    assert_eq!(state.spec, spec);
    // 步骤恒空:探索不与静态步骤混用
    let (plan, _) = store.load().unwrap();
    assert!(plan.steps.is_empty());
    assert!(plan.explore.is_some());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn step_status_enum_untouched() {
    // 探索不使用 StepStatus;此断言钉住探索运行不产生步骤记录的契约。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.transitions.insert("id:b1".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).unwrap();
    assert!(run.steps.is_empty());
    let _ = std::mem::take(&mut run.steps);
    let _: Option<StepStatus> = None;
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn goal_transient_not_found_retries_then_completes() {
    // docs/77 发现 1:值断言目标瞬态 NOT_FOUND(提供程序抖动)先重试,
    // 恢复后 COM 判定通过。goal 用不可折叠形态(--property value)以
    // 保持权威 COM 路径参与瞬态语义。
    let mut spec = value_spec(&["完成"], "name:完成", "完成", false);
    // 换 --property value → 不折叠,逐轮走 COM
    if let Some(index) = spec.goal.args.iter().position(|a| a == "name") {
        spec.goal.args[index] = "value".into();
    }
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Text", "完成", None)]]);
    host.goal_not_found_times = 1;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("retry recovers");
    assert_eq!(run.status, Status::Completed);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.goal_retries, 1);
    assert_eq!(state.errors, 0, "recovered retries are not errors");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn goal_retry_exhausts_and_fails_closed() {
    // 永久性 NOT_FOUND:重试 2 次后失败闭合,错误码原样传播。
    let spec = value_spec(&["保存"], "name:完成", "完成", false);
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.goal_not_found_times = 10;
    let error = execute(&spec, &mut run, &store, &mut host, None, None).expect_err("fails closed");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("goal_check_failed"));
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.goal_retries, 2, "bounded retries");
    assert_eq!(state.errors, 1, "only the terminal failure counts");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn value_goal_folds_into_frame_zero_com_calls() {
    // P1 折叠:name 通道值断言帧内判定——全程零 verify 采集。
    let spec = value_spec(&["保存"], "id:CalculatorResults", "显示为 7", false);
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存", Some("btnSave")),
            el("Text", "显示为 0", Some("CalculatorResults")),
        ],
        vec![
            el("Button", "保存", Some("btnSave")),
            el("Text", "显示为 7", Some("CalculatorResults")),
        ],
    ]);
    host.transitions.insert("id:btnSave".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("fold completes");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.verify_calls, 0,
        "fold must not touch the verify command"
    );
    // 折叠词把目标元素种进帧:观察关键词含 CalculatorResults
    assert!(
        host.snapshot_keywords
            .iter()
            .any(|k| k.contains("CalculatorResults"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn value_goal_contains_fold_and_negative_fold_continue() {
    // contains 形态折叠;负判定(未满足)同样零采集,继续探索。
    let spec = value_spec(&["保存"], "id:display", "87", true);
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        // 点击保存后显示 87 → contains 7 满足
        vec![
            el("Button", "保存", Some("btnSave")),
            el("Text", "显示为 8", Some("display")),
        ],
        vec![
            el("Button", "保存", Some("btnSave")),
            el("Text", "显示为 87", Some("display")),
        ],
    ]);
    host.transitions.insert("id:btnSave".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("contains fold");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.verify_calls, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn fold_falls_back_when_element_absent_or_goal_non_foldable() {
    // 目标缺席(帧内 0 命中)→ 回退 COM(权威错误语义);存在型断言
    // 缺席按未满足处理,不进入重试路径。
    let spec = value_spec(&["保存"], "id:gone", "值", false);
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    let mut host = host;
    host.goal_not_found_times = 10;
    let error = execute(&spec, &mut run, &store, &mut host, None, None).expect_err("exhausts");
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(
        host.verify_calls >= 3,
        "absent fold target falls back to COM"
    );
    let _ = std::fs::remove_dir_all(&root);

    // 非 name 通道(--property value)不折叠,逐轮走 COM。
    let mut args = vec!["--value".to_string(), "id:display".into(), "值".into()];
    args.extend([
        "--property".into(),
        "value".into(),
        "--app".into(),
        "测试应用".into(),
    ]);
    let spec = ExploreSpec {
        task: "t".into(),
        app: "测试应用".into(),
        keywords: vec!["保存".into()],
        goal: Invocation {
            command: "verify".into(),
            args,
            bind: Default::default(),
        },
        priors: Default::default(),
        stats: Default::default(),
        texts: Default::default(),
        budgets: crate::Budgets {
            actions: 1,
            decisions: 1,
            wall_ms: 600_000,
        },
    };
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.goal_not_found_times = 10;
    execute(&spec, &mut run, &store, &mut host, None, None).expect_err("non-foldable");
    assert!(
        host.verify_calls >= 1,
        "non-foldable goal uses the COM path"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn priors_demote_risky_candidate_and_avoid_card() {
    // 两个同分候选本应平局外抛;先验压低风险侧 → 熵门直接自动执行安全侧。
    let mut spec = spec(&["保存"], "name:完成");
    spec.keywords = vec!["保存".into()];
    spec.priors.insert("id:riskyButton".into(), -4);
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存甲", Some("riskyButton")),
            el("Button", "保存乙", Some("safeButton")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:riskyButton".into(), None);
    host.transitions.insert("id:safeButton".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("auto on safe side");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:safeButton"]);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.decisions, 0, "prior broke the tie; no card");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn card_contains_framed_candidates_verbatim() {
    // 发现 #6 修正:卡构建与观察同轮,帧即新鲜度——卡面不再批量预检
    // (COM 定位对帧内候选间歇失明会错杀卡面);死候选的过滤交给
    // 应答时队列预检与动作时的点击权威。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![
        el("Button", "保存甲", Some("a1")),
        el("Button", "保存乙", Some("a2")),
        el("Button", "保存丙", Some("a3")),
    ]]);
    // 即便 find 对 a2 失明(死探测模拟),卡仍如实反映观察帧。
    host.dead_probes.insert("id:a2".into());
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    assert_eq!(run.reason.as_deref(), Some("decision_pending"));
    let state = load(&store).unwrap().expect("state");
    let card = state.pending.as_ref().expect("card").card.clone();
    let selectors: Vec<&str> = card
        .candidates
        .iter()
        .map(|c| c.selector.as_str())
        .collect();
    assert_eq!(
        selectors,
        vec!["id:a1", "id:a2", "id:a3"],
        "the card mirrors the observed frame; no card-time probing"
    );
    assert!(state.tried.is_empty(), "no card-time dead marking");
    assert_eq!(state.errors, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn answer_queue_prefilters_dead_members() {
    // P3 应答队列预检:卡发出到应答之间候选死亡(存活配额耗尽)→
    // 应答时批量探测出队,死项不消耗动作预算、不白耗观察轮。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存甲", Some("a1")),
            el("Button", "保存乙", Some("a2")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    // a2 在卡发出后死亡:应答时的队列预检(唯一探测点)判死出队。
    host.probe_lives.insert("id:a2".into(), 0);
    host.transitions.insert("id:a1".into(), Some(1));
    host.transitions.insert("id:a2".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.pending.as_ref().expect("card").card.candidates.len(),
        2
    );
    // 应答 c2(=a2,已在应答时死亡)→ 预检出队,回退继续走 a1 完成
    execute(&spec, &mut run, &store, &mut host, Some("c2"), None).expect("fallback to a1");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:a1"], "dead answer never executed");
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.tried.get("id:a2").map(String::as_str), Some("dead"));
    assert_eq!(state.actions, 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn transient_probe_errors_do_not_kill_queue_members() {
    // 发现 #3 经验规则(应答路径):应答时队列预检的瞬态探测错误
    // (TIMEOUT)不判死——队列保留全部成员;确定性死亡才出队。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存甲", Some("a1")),
            el("Button", "保存乙", Some("a2")),
        ],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:a1".into(), Some(1));
    host.transitions.insert("id:a2".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    // 应答时队列预检全程瞬态失败 → 队列不滤;首选 a1 照常执行完成。
    host.transient_probes = 2;
    execute(&spec, &mut run, &store, &mut host, Some("c1"), None).expect("transient queue");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:a1"]);
    let state = load(&store).unwrap().expect("state");
    assert!(
        state.tried.get("id:a2").map(String::as_str) != Some("dead"),
        "transient queue probe does not kill members"
    );
    assert_eq!(state.errors, 0, "transient errors are not counted");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn observation_transient_retries_then_recovers() {
    // docs/77 发现 #3:紧随动作的观察在过渡期 INTERNAL 失败——有界重试
    // 恢复;重试不计 errors,如实入 observation_retries。
    let spec = spec(&["完成"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Text", "完成", None)]]);
    host.snapshot_failures = 1;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("retry recovers");
    assert_eq!(run.status, Status::Completed);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.observation_retries, 1);
    assert_eq!(state.errors, 0);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn observation_retry_exhausts_and_fails_closed() {
    // 永久性 INTERNAL:重试 2 次后失败闭合,错误码原样传播。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.snapshot_failures = 10;
    let error = execute(&spec, &mut run, &store, &mut host, None, None).expect_err("fails closed");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("observation_failed"));
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.observation_retries, 2, "bounded retries");
    assert_eq!(state.errors, 1, "only the terminal failure counts");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn goal_query_fault_continues_exploration_and_completes() {
    // docs/77 发现 #5:菜单开启态下 goal 定位持续 item-container RPC 故障
    // (INTERNAL)——核验先于动作,若判死会饿死本该执行的菜单项点击。
    // 语义:查询故障计 errors 如实记录,按未满足继续;世界变化后核验通过。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el("Button", "保存", Some("b1"))],
        vec![el("Text", "完成", None)],
    ]);
    host.goal_fault_times = 1;
    host.transitions.insert("id:b1".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("fault then complete");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.clicks,
        vec!["id:b1"],
        "query fault did not starve the act"
    );
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.errors, 1, "the goal fault is recorded honestly");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn goal_query_fault_budget_bounded_not_fail_closed() {
    // 永久性查询故障:不判死运行,预算耗尽给出诚实终态(errors 计数反映
    // 每轮故障;goal_check_failed 保留给非查询类致命错误)。
    let mut spec = spec(&["保存"], "name:完成");
    spec.budgets = crate::Budgets {
        actions: 1,
        decisions: 1,
        wall_ms: 600_000,
    };
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![
        el("Button", "保存甲", Some("a1")),
        el("Button", "保存乙", Some("a2")),
    ]]);
    host.goal_fault_times = 10;
    // 平局外抛后应答回退:全程 goal 查询故障 → 动作仍执行 → 预算耗尽。
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    execute(&spec, &mut run, &store, &mut host, Some("c1"), None).expect("budget exhausts");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("budget_exhausted:actions"));
    let state = load(&store).unwrap().expect("state");
    assert!(
        state.errors >= 2,
        "each faulted goal check counts: {}",
        state.errors
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ref_bound_action_uses_captured_identity() {
    // 发现 #6 结构性收口:帧内元素携带 ref → 动作走 `click @eN --snapshot
    // <id> --app` 捕获身份解析,绕开间歇失明的 item-container 定位通道。
    // 快照发号、ref 解析、点击转移全链路验证。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el_ref("Button", "保存", Some("btnSave"), "@e1")],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:btnSave".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("ref path completes");
    assert_eq!(run.status, Status::Completed);
    // 动作走了 ref 绑定路径(@eN + --snapshot + 快照 id)
    assert_eq!(
        host.raw_actions[0],
        vec![
            "@e1".to_string(),
            "--snapshot".to_string(),
            "s-mock1".to_string(),
            "--app".to_string(),
            "测试应用".to_string()
        ],
        "actions must go through the captured-identity channel"
    );
    assert_eq!(host.clicks, vec!["id:btnSave"]);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.snapshot_id.as_deref(),
        Some("s-mock2"),
        "the post-action frame carries its own id"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stale_ref_retries_with_fresh_observation() {
    // STALE_REF(元素重建):点击未送达但世界已变——引擎按帧内候选的
    // 有界重试语义,下轮重新观察拿新帧新 ref,重试后完成。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    // 帧0 元素 a1(@e1,重建后失效);帧1 元素 b2(@e9);帧2 goal 达成。
    let mut host = MockHost::new(vec![
        vec![el_ref("Button", "保存甲", Some("a1"), "@e1")],
        vec![el_ref("Button", "保存乙", Some("b2"), "@e9")],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:a1".into(), Some(1));
    host.transitions.insert("id:b2".into(), Some(2));
    host.stale_refs.insert("@e1".into());
    execute(&spec, &mut run, &store, &mut host, None, None).expect("retry recovers");
    assert_eq!(run.status, Status::Completed);
    // 第一次 @e1 失败(身份漂移,世界已变),第二次走帧1的 @e9 落地
    assert_eq!(host.raw_actions.len(), 2, "one stale attempt, one landed");
    assert!(host.raw_actions[0].first().is_some_and(|t| t == "@e1"));
    assert!(host.raw_actions[1].first().is_some_and(|t| t == "@e9"));
    assert_eq!(host.clicks, vec!["id:b2"]);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.errors, 1, "the stale attempt is recorded honestly");
    assert_eq!(
        state.actions, 2,
        "the failed attempt consumed one bounded retry"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn refless_frame_falls_back_to_selector_actions() {
    // 无 ref 的帧(含 MockHost 默认/历史语义)回退选择器动作——
    // 兼容既有行为:动作参数即选择器,无 --snapshot。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el("Button", "保存", Some("btnSave"))],
        vec![el("Text", "完成", None)],
    ]);
    host.transitions.insert("id:btnSave".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("selector fallback");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.raw_actions[0],
        vec![
            "id:btnSave".to_string(),
            "--app".to_string(),
            "测试应用".to_string()
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn ambiguous_existence_goal_is_met() {
    // 真机链路收官语义:另存为对话框打开后 name:文件名 命中 3 个元素
    // (AMBIGUOUS)——存在型断言的多匹配同样满足存在性,判定达成;
    // 值断言的歧义维持失败闭合(下测)。
    let spec = spec(&["保存"], "name:完成");
    let (root, store, mut run) = setup(&spec);
    // 帧内无 goal 目标(折叠回退 COM)——AMBIGUOUS 分支真实触发。
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.goal_ambiguous = true;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("ambiguous exists = met");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.verify_calls, 1, "the ambiguous COM path was exercised");
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.goal_met, Some(true));
    let _ = std::fs::remove_dir_all(&root);

    // 值断言歧义:无法确定比较目标 → 失败闭合。
    let spec = value_spec(&["保存"], "name:完成", "完成", false);
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![vec![el("Button", "保存", Some("b1"))]]);
    host.goal_ambiguous = true;
    let error = execute(&spec, &mut run, &store, &mut host, None, None)
        .expect_err("ambiguous value goal fails");
    assert_eq!(error.code, ErrorCode::Ambiguous);
    assert_eq!(run.reason.as_deref(), Some("goal_check_failed"));
    let _ = std::fs::remove_dir_all(&root);
}
