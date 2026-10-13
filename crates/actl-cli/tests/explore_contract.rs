//! 探索模式契约(docs/42 S2 spec 提交):explore spec 校验、决策应答协议、
//! 悬挂卡状态面。全部离线——观察失败用不存在的唯一窗口标题驱动,不碰真桌面。
use actl_core::decision::{CardInput, ScoredCandidate};
use actl_flow::engine::{Run, Status};
use actl_flow::explore::{ExploreState, PendingDecision, TokenAudit};
use actl_flow::{Budgets, ExploreSpec, Invocation, Plan};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

struct Sandbox {
    root: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-explore-cli-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_actl"));
        cmd.env("LOCALAPPDATA", &self.root)
            .env_remove("ACTL_FLOW_REQUEST_SOURCE")
            .env_remove("ACTL_FLOW_REQUEST_ID")
            .env_remove("ACTL_TASK_ID")
            .env_remove("ACTL_TASK_CONFIG");
        cmd
    }
    fn invoke(&self, args: &[&str]) -> (i32, Value) {
        let out = self.command().args(args).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let data = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("envelope json expected, got: {text}"));
        (out.status.code().unwrap(), data)
    }
    fn file(&self, text: &str) -> String {
        let path = self.root.join("explore.yml");
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 不存在的唯一窗口标题:任何真机环境都观察不到(离线确定性)。
const GHOST_APP: &str = "actl-explore-contract-ghost-window";

const VALID: &str = "version: 1\nname: explore-contract\nexplore:\n  task: 在测试应用中完成保存任务\n  app: actl-explore-contract-ghost-window\n  keywords: [保存, save]\n  goal: {command: verify, args: [--exists, 'name:完成', --app, actl-explore-contract-ghost-window]}\n";

#[test]
fn explore_spec_validates_without_steps_key() {
    let s = Sandbox::new();
    let file = s.file(VALID);
    let (code, mut result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/explore_validate.json")).unwrap()
    );
}

#[test]
fn explore_spec_rejects_invalid_shapes() {
    let s = Sandbox::new();
    for (text, field) in [
        // 混用静态步骤
        (
            "version: 1\nname: mixed\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\nsteps:\n  - id: a\n    title: A\n    run: {command: status}\n",
            "steps",
        ),
        // 空 keywords
        (
            "version: 1\nname: kw\nexplore:\n  task: T\n  app: A\n  keywords: []\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n",
            "explore.keywords",
        ),
        // 非断言目标
        (
            "version: 1\nname: goal\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: click, args: ['name:x', --app, A]}\n",
            "explore.goal",
        ),
        // 预算越界
        (
            "version: 1\nname: budget\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  budgets: {actions: 9999}\n",
            "explore.budgets.actions",
        ),
        // goal 携带 bind(引擎不解析)
        (
            "version: 1\nname: bind\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A], bind: {0: s.out}}\n",
            "explore.goal.bind",
        ),
    ] {
        let file = s.file(text);
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_ne!(code, 0, "{result}");
        assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
        assert_eq!(result["error"]["evidence"]["field"], field, "{result}");
    }
}

#[test]
fn decision_on_static_flow_is_protocol_error() {
    let s = Sandbox::new();
    let file = s.file(
        "version: 1\nname: static\nsteps:\n  - id: one\n    title: System\n    run: {command: status}\n",
    );
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "static-01"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    let (code, result) = s.invoke(&["flow-resume", "--id", "static-01", "--decision", "c1"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("static flow")
    );
}

#[test]
fn decision_without_pending_card_is_rejected() {
    let s = Sandbox::new();
    let file = s.file(VALID);
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "ghost-01", "--paused"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "paused");
    // 悬挂卡不存在时应答被拒
    let (code, result) = s.invoke(&["flow-resume", "--id", "ghost-01", "--decision", "c1"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    // --confidence 依附 --decision
    let out = s
        .command()
        .args(["flow-resume", "--id", "ghost-01", "--confidence", "0.8"])
        .output()
        .unwrap();
    assert_eq!(out.status.code().unwrap(), 2);
}

/// 手工构造悬挂卡现场(等价探索运行外抛后的持久化状态),验证应答协议三面:
/// 未带 --decision 拒绝、未知候选拒绝、合法应答进入回退队列后观察失败如实上报。
#[test]
fn pending_card_decision_protocol() {
    let s = Sandbox::new();
    let root = &s.root.join("actl");
    let flows = root.join("flows").join("card-01");
    std::fs::create_dir_all(&flows).unwrap();

    let spec = ExploreSpec {
        task: "完成保存任务".into(),
        app: GHOST_APP.into(),
        keywords: vec!["保存".into()],
        goal: Invocation {
            command: "verify".into(),
            args: vec![
                "--exists".into(),
                "name:完成".into(),
                "--app".into(),
                GHOST_APP.into(),
            ],
            bind: Default::default(),
        },
        priors: Default::default(),
        stats: Default::default(),
        texts: Default::default(),
        budgets: Budgets::default(),
    };
    let plan = Plan {
        version: 1,
        name: "explore-contract".into(),
        handoff: None,
        auto_resume: None,
        explore: Some(spec.clone()),
        popup_handlers: Vec::new(),
        popup_ignores: Vec::new(),
        steps: Vec::new(),
    };
    std::fs::write(flows.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
    let mut run = Run::new("card-01", &plan);
    run.status = Status::NeedsReview;
    run.reason = Some("decision_pending".into());
    std::fs::write(flows.join("state.json"), serde_json::to_vec(&run).unwrap()).unwrap();

    let candidates: Vec<ScoredCandidate> = ["btnA", "btnB"]
        .iter()
        .map(|id| ScoredCandidate {
            selector: format!("id:{id}"),
            role: "Button".into(),
            name: Some(id.to_string()),
            automation_id: Some(id.to_string()),
            score: 6,
            dangerous: false,
        })
        .collect();
    let card = actl_core::decision::build_card(&CardInput {
        card_id: "d1",
        task: &spec.task,
        app: &spec.app,
        goal: "verify --exists name:完成",
        step: 1,
        diff: &actl_core::diff::SnapshotDiff {
            added: vec![],
            removed: vec![],
            changed: vec![],
            pairing: actl_core::diff::PairingStats {
                paired_by_automation_id_role: 0,
                paired_by_name_role: 0,
                added: 0,
                removed: 0,
                changed: 0,
            },
        },
        candidates: &candidates,
        effects: &Default::default(),
    })
    .unwrap();
    let state = ExploreState {
        version: 1,
        spec: spec.clone(),
        started_ms: actl_core::state::unix_ms(),
        actions: 1,
        decisions: 1,
        observations: 1,
        errors: 0,
        tried: Default::default(),
        seen: Default::default(),
        visited: Default::default(),
        widened: false,
        last_fingerprint: None,
        last_elements: Vec::new(),
        last_actor: None,
        pending: Some(PendingDecision { card }),
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
        anchor_app: None,
        anchor_stack: Vec::new(),
        anchor_transfers: 0,
        probe_episodes: 0,
        goal_met: None,
    };
    std::fs::write(
        flows.join("explore.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();

    // flow-status:悬挂卡 + explore 概览可见
    let (code, result) = s.invoke(&["flow-status", "--id", "card-01"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "needs_review");
    assert_eq!(result["data"]["run"]["reason"], "decision_pending");
    assert!(result["data"]["decision_card"].is_object(), "{result}");
    assert_eq!(
        result["data"]["decision_card"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(result["data"]["explore"]["decisions"], 1);

    // 无应答续跑被拒
    let (code, result) = s.invoke(&["flow-resume", "--id", "card-01"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--decision")
    );

    // 未知候选拒绝并列出合法集
    let (code, result) = s.invoke(&["flow-resume", "--id", "card-01", "--decision", "c9"]);
    assert_eq!(code, 2, "{result}");
    let message = result["error"]["message"].as_str().unwrap();
    assert!(message.contains("c1") && message.contains("c2"), "{result}");

    // 合法应答:消费卡 → 回退队列入状态 → 观察真失败(ghost 窗口)如实上报
    let (code, result) = s.invoke(&[
        "flow-resume",
        "--id",
        "card-01",
        "--decision",
        "c2",
        "--confidence",
        "0.85",
    ]);
    assert_eq!(code, 1, "{result}");
    assert_eq!(result["error"]["code"], "NOT_FOUND", "{result}");
    let (code, result) = s.invoke(&["flow-status", "--id", "card-01"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "needs_review");
    assert_eq!(result["data"]["run"]["reason"], "observation_failed");
    // P3 队列预检:ghost 窗口下两个候选探测均确定性死亡(NOT_FOUND)→
    // 出队,队列清空(观察随后同样 NOT_FOUND 失败闭合,错误如实传播)。
    assert!(
        result["data"]["explore"]["queue"].is_null()
            || result["data"]["explore"]["queue"]
                .as_array()
                .is_some_and(Vec::is_empty),
        "{result}"
    );
    assert!(result["data"]["decision_card"].is_null());
}

#[test]
fn commands_schema_lists_exploration_surface() {
    let s = Sandbox::new();
    let (code, result) = s.invoke(&["commands"]);
    assert_eq!(code, 0);
    let resume = result["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("flow-resume"))
        .expect("flow-resume listed")
        .clone();
    let args: Vec<&str> = resume["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert!(args.contains(&"decision"), "{args:?}");
    assert!(args.contains(&"confidence"), "{args:?}");
    let writeback = result["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("explore-writeback"))
        .expect("explore-writeback listed")
        .clone();
    let wb_args: Vec<&str> = writeback["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert!(
        wb_args.contains(&"id") && wb_args.contains(&"out"),
        "{wb_args:?}"
    );
}

/// S3 回写契约:goal_met 的完成运行 → 片段(恒 unverified、{window} 替换、
/// quirks 与 suggested_priors)与 golden 对齐;未完成运行拒绝。
#[test]
fn writeback_extracts_unverified_route_fragment() {
    let s = Sandbox::new();
    let root = &s.root.join("actl");
    let flows = root.join("flows").join("wb-ok");
    std::fs::create_dir_all(&flows).unwrap();
    let spec = ExploreSpec {
        task: "在计算器中按下数字七".into(),
        app: GHOST_APP.into(),
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
                GHOST_APP.into(),
            ],
            bind: Default::default(),
        },
        priors: Default::default(),
        stats: Default::default(),
        texts: Default::default(),
        budgets: Budgets::default(),
    };
    let plan = Plan {
        version: 1,
        name: "writeback-contract".into(),
        handoff: None,
        auto_resume: None,
        explore: Some(spec.clone()),
        popup_handlers: Vec::new(),
        popup_ignores: Vec::new(),
        steps: Vec::new(),
    };
    std::fs::write(flows.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
    let mut run = Run::new("wb-ok", &plan);
    run.status = Status::Completed;
    std::fs::write(flows.join("state.json"), serde_json::to_vec(&run).unwrap()).unwrap();
    let mut tried = std::collections::BTreeMap::new();
    tried.insert("id:ClearMemoryButton".to_string(), "dead".into());
    tried.insert("id:num8Button".to_string(), "no_effect".into());
    let state = ExploreState {
        version: 1,
        spec: spec.clone(),
        started_ms: actl_core::state::unix_ms(),
        actions: 2,
        decisions: 1,
        observations: 4,
        errors: 0,
        tried,
        seen: Default::default(),
        visited: Default::default(),
        widened: false,
        last_fingerprint: None,
        last_elements: Vec::new(),
        last_actor: None,
        pending: None,
        queue: Vec::new(),
        action_log: vec![
            actl_flow::explore::ActionRecord {
                step: 1,
                selector: "id:num8Button".into(),
                mode: "answer".into(),
                outcome: "acted".into(),
                text: None,
            },
            actl_flow::explore::ActionRecord {
                step: 2,
                selector: "id:num7Button".into(),
                mode: "fallback".into(),
                outcome: "acted".into(),
                text: None,
            },
        ],
        answers: Vec::new(),
        audit: actl_flow::explore::TokenAudit {
            observation_bytes: 2060,
            card_bytes: 556,
        },
        frames_truncated: 0,
        goal_retries: 1,
        observation_retries: 0,
        window_pid: None,
        window_title: None,
        snapshot_id: None,
        goal_window: None,
        anchor_app: None,
        anchor_stack: Vec::new(),
        anchor_transfers: 0,
        probe_episodes: 0,
        goal_met: Some(true),
    };
    std::fs::write(
        flows.join("explore.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    let (code, mut result) = s.invoke(&["explore-writeback", "--id", "wb-ok"]);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/explore_writeback.json")).unwrap()
    );

    // 未完成运行(goal_met=false)拒绝,失败现场保留
    let flows_bad = root.join("flows").join("wb-failed");
    std::fs::create_dir_all(&flows_bad).unwrap();
    let mut state_bad = state.clone();
    state_bad.goal_met = Some(false);
    state_bad.spec = spec.clone();
    std::fs::write(
        flows_bad.join("plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let mut run_bad = Run::new("wb-failed", &plan);
    run_bad.status = Status::NeedsReview;
    run_bad.reason = Some("budget_exhausted:actions".into());
    std::fs::write(
        flows_bad.join("state.json"),
        serde_json::to_vec(&run_bad).unwrap(),
    )
    .unwrap();
    std::fs::write(
        flows_bad.join("explore.json"),
        serde_json::to_vec(&state_bad).unwrap(),
    )
    .unwrap();
    let (code, result) = s.invoke(&["explore-writeback", "--id", "wb-failed"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("goal_met")
    );
}

#[test]
fn priors_validation_rejects_out_of_range() {
    let s = Sandbox::new();
    for (text, field) in [
        (
            "version: 1\nname: p1\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  priors: {'id:btn': 9}\n",
            "explore.priors",
        ),
        (
            "version: 1\nname: p2\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  priors: {'id:btn': -9}\n",
            "explore.priors",
        ),
    ] {
        let file = s.file(text);
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_ne!(code, 0, "{result}");
        assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
        assert_eq!(result["error"]["evidence"]["field"], field, "{result}");
    }
    // 合法先验通过且 flow-validate 概览可见
    let file = s.file(
        "version: 1\nname: p3\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  priors: {'id:btnSave': 3, 'id:btnRisk': -4}\n",
    );
    let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["valid"], true);
}

#[test]
fn texts_validation_rejects_bad_shapes() {
    let s = Sandbox::new();
    for (text, field) in [
        // 键不是完整语义选择器(裸关键词)
        (
            "version: 1\nname: t1\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  texts: {keyword: text}\n",
            "explore.texts",
        ),
        // 文本为空
        (
            "version: 1\nname: t2\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  texts: {'id:file': ''}\n",
            "explore.texts",
        ),
        // 词表+texts 键超出 16 关键词预算
        (
            "version: 1\nname: t3\nexplore:\n  task: T\n  app: A\n  keywords: [k1,k2,k3,k4,k5,k6,k7,k8,k9,k10,k11,k12,k13,k14,k15]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  texts: {'id:f1': a, 'id:f2': b}\n",
            "explore.keywords",
        ),
    ] {
        let file = s.file(text);
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_ne!(code, 0, "{result}");
        assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
        assert_eq!(result["error"]["evidence"]["field"], field, "{result}");
    }
    // 合法 texts 通过
    let file = s.file(
        "version: 1\nname: t4\nexplore:\n  task: 在记事本输入文字\n  app: Notepad\n  keywords: [文件名]\n  goal: {command: verify, args: [--exists, 'name:报告', --app, Notepad]}\n  texts: {'id:fileName': '报告.txt'}\n",
    );
    let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["valid"], true);
}

#[test]
fn stats_validation_rejects_bad_shapes() {
    // 统计剪枝 v2 的 schema 面:有效形态通过,effective>tried / tried=0 /
    // 超界 tried 拒绝;有效形态可整轮通过(统计注入不破坏探索契约)
    let s = Sandbox::new();
    for (text, field) in [
        (
            "version: 1\nname: s1\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  stats: {'id:btn': {tried: 5, effective: 7}}\n",
            "explore.stats",
        ),
        (
            "version: 1\nname: s2\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  stats: {'id:btn': {tried: 0, effective: 0}}\n",
            "explore.stats",
        ),
        (
            "version: 1\nname: s3\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  stats: {'id:btn': {tried: 99999, effective: 0}}\n",
            "explore.stats",
        ),
    ] {
        let file = s.file(text);
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_ne!(code, 0, "{result}");
        assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
        assert_eq!(result["error"]["evidence"]["field"], field, "{result}");
    }
    let ok = s.file(
        "version: 1\nname: s4\nexplore:\n  task: T\n  app: A\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, A]}\n  stats: {'id:btn': {tried: 4, effective: 1}}\n",
    );
    let (code, result) = s.invoke(&["flow-validate", "--file", &ok]);
    assert_eq!(code, 0, "{result}");
}

/// flow-run --auto-decide 预检(2026-10-13 用户授权):拒绝发生在任何 run
/// 状态创建之前——静态流程与决策卡无关,--paused 是人类保留位。
#[test]
fn auto_decide_preflight_rejects_static_and_paused() {
    let s = Sandbox::new();
    let static_flow = s.file(
        "version: 1\nname: static-demo\nsteps:\n  - id: read\n    title: r\n    run:\n      command: status\n",
    );
    let (code, result) = s.invoke(&[
        "flow-run",
        "--file",
        &static_flow,
        "--id",
        "auto-static",
        "--auto-decide",
    ]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("exploration plan"),
        "names the requirement: {result}"
    );
    // 预检先于 run 创建:无半注册状态
    assert!(
        !s.root
            .join("actl")
            .join("flows")
            .join("auto-static")
            .exists(),
        "no half-registered run"
    );
    let explore = s.file(
        "version: 1\nname: explore-demo\nexplore:\n  task: T\n  app: 不存在的唯一窗口标题-xyz\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, 不存在的唯一窗口标题-xyz]}\n",
    );
    let (code, result) = s.invoke(&[
        "flow-run",
        "--file",
        &explore,
        "--id",
        "auto-paused",
        "--auto-decide",
        "--paused",
    ]);
    assert_eq!(code, 2, "{result}");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("--paused"),
        "names the conflict: {result}"
    );
}

/// --auto-decide 在探索运行上:无 decision.json(沙箱 LOCALAPPDATA 空)时
/// provider 配置缺失以 PROTOCOL 传播,run 如实停在观察失败终态(该计划
/// 用不存在的窗口标题驱动,不碰真桌面)——模型调用失败绝不静默降级,
/// 原子语义与手工应答一致。
#[test]
fn auto_decide_explore_without_config_is_honest_protocol() {
    let s = Sandbox::new();
    let explore = s.file(
        "version: 1\nname: explore-noconfig\nexplore:\n  task: T\n  app: 不存在的唯一窗口标题-abc\n  keywords: [k]\n  goal: {command: verify, args: [--exists, 'name:x', --app, 不存在的唯一窗口标题-abc]}\n",
    );
    let (code, result) = s.invoke(&[
        "flow-run",
        "--file",
        &explore,
        "--id",
        "auto-nocfg",
        "--auto-decide",
    ]);
    // 窗口预检硬失败先于任何决策(NOT_FOUND 信封,exit 1):auto 循环从未
    // 启动,审计无污染——flag 不改变既有语义,没有卡可答时诚实传播。
    assert_eq!(code, 1, "{result}");
    assert_eq!(result["error"]["code"], "NOT_FOUND", "{result}");
    assert!(
        !s.root.join("actl").join("decision-audit.jsonl").exists(),
        "auto-decide never asked the provider: no audit lines"
    );
}
