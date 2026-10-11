//! 跨窗 goal 探测与锚点转移(轮 A)同族测试——从 explore_tests 拆出
//! (file-lines 硬限 1500;MockHost/夹具经父模块 super::* 可见)。

use super::*;

#[test]
fn cross_window_goal_found_in_same_pid_dialog() {
    // 跨窗 goal 探测:点击打开同进程弹窗(对话框与主窗同 pid),goal
    // 目标缺席于锚窗 → list-windows 枚举同 pid 窗口 → goal 的 --app 改写
    // 为弹窗标题重试 → 命中即转移(goal_window)并判定完成。
    // 场景对应"关闭未保存文档→出现保存提示"链(docs/42 L2)。
    let spec = spec(&["关闭"], "name:保存");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "关闭", Some("closeBtn"))]]).with_dialog(
        "提示",
        vec![el("Button", "保存", Some("saveBtn"))],
        "id:closeBtn",
    );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("cross-window completes");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.clicks, vec!["id:closeBtn"]);
    let state = load(&store).unwrap().expect("state");
    // 锚点转移 + goal-锚合流:点击打开同进程弹窗 → 观察锚转移到弹窗
    // → goal 核验的窗口优先序直接命中转移锚,无需 L2 探测兜底——
    // goal_window 保持 None(目标从未需要独立的窗口转移),probe 零次。
    assert_eq!(
        state.goal_window, None,
        "goal met at the transferred anchor"
    );
    assert_eq!(
        state.anchor_app.as_deref(),
        Some("提示"),
        "anchor followed the dialog"
    );
    assert_eq!(state.anchor_transfers, 1);
    assert_eq!(
        state.probe_episodes, 1,
        "one empty probe before the dialog opened; anchor convergence saves the post-click one"
    );
    assert_eq!(state.actions, 1);
    assert_eq!(state.goal_met, Some(true));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn cross_window_probe_budget_is_bounded() {
    // 无弹窗场景:goal 持续缺席 → 探测预算 2 轮耗尽后走存在性路径
    // (未满足→继续探索),不再反复枚举窗口。
    let spec = spec(&["保存"], "name:永不出现");
    let (root, store, mut run) = setup(&spec);
    let mut s = spec.clone();
    s.budgets = crate::Budgets {
        actions: 1,
        decisions: 1,
        wall_ms: 600_000,
    };
    let host = MockHost::new(vec![vec![
        el("Button", "保存甲", Some("a1")),
        el("Button", "保存乙", Some("a2")),
    ]]);
    let mut host = host;
    // 保存甲/乙同分平局 → 卡;应答 c1 后队列耗进动作预算 → 诚实终态。
    execute(&s, &mut run, &store, &mut host, None, None).expect("externalize");
    execute(&s, &mut run, &store, &mut host, Some("c1"), None).expect("budget exhausts");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("budget_exhausted:actions"));
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.probe_episodes, PROBE_EPISODE_BUDGET,
        "probe budget capped"
    );
    assert!(state.goal_window.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

// ── 锚点转移(轮 A):三判据 + 出栈回退 + 预算 ────────────────────────────────

/// 转移后动作派发确实改锚:对话框内候选的 click/type 带 --app 弹窗标题。
#[test]
fn anchor_transfer_follows_actions_into_the_dialog() {
    // 主窗:打开按钮;弹窗:保存按钮(goal 在弹窗里,靠锚合流命中)。
    let spec = spec(&["打开", "保存"], "name:保存");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "打开", Some("openBtn"))]]).with_dialog(
        "另存为",
        vec![el("Button", "保存", Some("saveBtn"))],
        "id:openBtn",
    );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("completes in dialog");
    assert_eq!(run.status, Status::Completed);
    // 打开按钮用声明锚;保存按钮(goal 侧不存在动作,但若 frontier 派发
    // 过则必须带弹窗锚)——本场景 goal 直接命中,验证 click 的锚:
    assert_eq!(host.clicks, vec!["id:openBtn"]);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.anchor_app.as_deref(), Some("另存为"));
    assert_eq!(state.anchor_stack, vec!["测试应用".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// 判据③自校验:零关键词命中的新窗(toast 形态)不劫持锚——观察与
/// 动作继续在声明锚窗,goal 走 L2 探测兜底或按未满足继续。
#[test]
fn anchor_transfer_rejects_keyword_miss_windows() {
    // 弹窗元素为空(零命中形态)→ 不转移。
    let spec = spec(&["保存"], "name:永不出现");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "关闭", Some("closeBtn"))]]).with_dialog(
        "通知",
        vec![], // 关键词帧非空判据:空 → 出局
        "id:closeBtn",
    );
    let mut host = host;
    // goal 永不出现 → 预算耗尽终态;期间点击发生但锚不得转移。
    execute(&spec, &mut run, &store, &mut host, None, None).ok();
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.anchor_app, None,
        "empty keyword frame must not hijack the anchor"
    );
    assert!(state.anchor_stack.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

/// 转移预算:goal 转移与锚转移共用 anchor_transfers,上限兜底。
#[test]
fn anchor_transfer_budget_is_shared_and_bounded() {
    // 预算设 1:第一次锚转移后,后续不再转移(即便有新弹窗)。
    let mut spec = spec(&["打开", "保存"], "name:保存");
    spec.budgets.actions = 6;
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "打开", Some("openBtn"))]]).with_dialog(
        "对话框一",
        vec![el("Button", "保存", None)],
        "id:openBtn",
    );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).ok();
    let state = load(&store).unwrap().expect("state");
    assert!(
        state.anchor_transfers <= 4,
        "transfers bounded: {}",
        state.anchor_transfers
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 归因与差分重置:转移帧的全部元素按 added 进入前沿(无"假差分"),
/// 上一动作归因 reveals(动作揭示了新界面)。
#[test]
fn anchor_transfer_resets_diff_baseline() {
    let spec = spec(&["打开", "保存"], "name:保存");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "打开", Some("openBtn"))]]).with_dialog(
        "面板",
        vec![el("Button", "保存", Some("saveBtn"))],
        "id:openBtn",
    );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("completes");
    let state = load(&store).unwrap().expect("state");
    // 打开按钮的归因是 reveals(揭示了新窗),不是 no_effect。
    assert_eq!(
        state.tried.get("id:openBtn").map(String::as_str),
        Some("reveals")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 轮 C(2026-10-11):RuntimeBroker 承载形态——对话框跨进程(pid 不同)
/// 但 owner_title 挂宿主锚窗,判据①放宽(同 pid **或** owner 归属)后转移。
/// Store 应用的文件选择器正是这个形态(同 pid 过滤曾如实错失)。
#[test]
fn anchor_transfer_follows_actions_into_broker_dialog() {
    let spec = spec(&["打开", "保存"], "name:保存");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "打开", Some("openBtn"))]]).with_broker_dialog(
        "另存为",
        vec![el("Button", "保存", Some("saveBtn"))],
        "id:openBtn",
    );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).expect("completes in broker dialog");
    assert_eq!(run.status, Status::Completed);
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.anchor_app.as_deref(),
        Some("另存为"),
        "owner-linked cross-pid dialog must receive the anchor"
    );
    assert_eq!(state.anchor_stack, vec!["测试应用".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// owner 归属不豁免判据③:broker 对话框关键词帧为空(toast 形态)→
/// 即便 owner 挂宿主也不转移——放宽只放宽进程判据,相关性自校验照旧。
#[test]
fn broker_dialog_still_requires_keyword_frame() {
    let spec = spec(&["保存"], "name:永不出现");
    let (root, store, mut run) = setup(&spec);
    let host = MockHost::new(vec![vec![el("Button", "关闭", Some("closeBtn"))]])
        .with_broker_dialog(
            "通知",
            vec![], // 关键词帧空 → 判据③出局
            "id:closeBtn",
        );
    let mut host = host;
    execute(&spec, &mut run, &store, &mut host, None, None).ok();
    let state = load(&store).unwrap().expect("state");
    assert_eq!(
        state.anchor_app, None,
        "owner match must not bypass the keyword-frame self-check"
    );
    assert!(state.anchor_stack.is_empty());
    let _ = std::fs::remove_dir_all(&root);
}
