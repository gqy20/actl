//! 歧义候选串联(docs/63 运行时候选纠正):AMBIGUOUS 拦截 → 锚定骨架快照
//! → needs_review(ambiguous_target)→ `--pick` 重写派发参数。MockHost 全链。

use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
use std::cell::RefCell;

/// 可编程宿主:run 相注入 AMBIGUOUS(带候选 evidence),快照通道返回固定
/// id;派发参数全记录(断言 --pick 重写)。
struct PickHost {
    calls: RefCell<Vec<(String, Vec<String>)>>,
    ambiguous_once: std::cell::Cell<bool>,
}

impl PickHost {
    fn fresh() -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            ambiguous_once: std::cell::Cell::new(true),
        }
    }
    fn ambiguous_error() -> CtlError {
        CtlError::with_evidence(
            ErrorCode::Ambiguous,
            "matches 3 elements",
            serde_json::json!({
                "candidates": [
                    {"ref": "@e1", "role": "Button", "name": "Play"},
                    {"ref": "@e2", "role": "Button", "name": "Play", "context": "Skyfall"},
                    {"ref": "@e5", "role": "Button", "name": "Stop"},
                ]
            }),
        )
    }
}

impl Host for PickHost {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls
            .borrow_mut()
            .push((call.command.clone(), call.args.clone()));
        // run 相首次派发返回 AMBIGUOUS;此后(pick 重写后的重放)成功
        if call.command == "click" && self.ambiguous_once.replace(false) {
            return Err(Self::ambiguous_error());
        }
        Ok(())
    }
    fn execute_snapshot(
        &mut self,
        args: Vec<String>,
    ) -> Result<(String, serde_json::Value), CtlError> {
        self.calls.borrow_mut().push(("snapshot".into(), args));
        Ok(("s-ambiguous-1".into(), serde_json::json!({})))
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command != "click"
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
    fn capture_failure(&mut self, _: &str, _: &str, _: Option<&Invocation>) {}
}

fn plan() -> Plan {
    parse(
        "version: 1\nname: Pick\nsteps:\n - id: a\n   title: A\n   run: {command: click, args: [\"name:Play\", \"--app\", Spotify]}\n   verify: {command: check}\n",
    )
    .unwrap()
}

fn setup() -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "flow-pick-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let p = plan();
    let store = Store::at(&root, "test").unwrap();
    let run = store.create(&p, "test").unwrap();
    (root, store, run)
}

#[test]
fn ambiguous_run_pauses_with_snapshot_anchored_candidates() {
    let (root, store, mut run) = setup();
    let mut host = PickHost::fresh();
    execute(&plan(), &mut run, &store, &mut host, None, None).unwrap();
    // 步骤 Pending(零派发,无 uncertain 语义)、run 待复核、候选全量保留
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("ambiguous_target:a"));
    assert_eq!(run.steps[0].status, StepStatus::Pending);
    let ambiguous = run.steps[0].ambiguous.as_ref().unwrap();
    assert_eq!(ambiguous.snapshot_id, "s-ambiguous-1");
    assert_eq!(ambiguous.target, "name:Play");
    assert_eq!(ambiguous.candidates.len(), 3);
    assert_eq!(ambiguous.candidate_refs(), vec!["@e1", "@e2", "@e5"]);
    assert!(ambiguous.picked.is_none());
    // AMBIGUOUS 是确定拒绝非副作用:步骤无 error 语义、无失败采集要求
    assert!(run.steps[0].attempt_errors.contains(&ErrorCode::Ambiguous));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pick_rewrites_dispatch_with_ref_and_snapshot_binding() {
    let (root, store, mut run) = setup();
    let mut host = PickHost::fresh();
    execute(&plan(), &mut run, &store, &mut host, None, None).unwrap();
    run.status = Status::NeedsReview;
    // 应答:选中 @e2 → 引擎重写 args[0] 并追加 --snapshot
    execute(&plan(), &mut run, &store, &mut host, None, Some("@e2")).unwrap();
    assert_eq!(run.status, Status::Completed);
    let dispatched = host.calls.borrow();
    let last_click = dispatched
        .iter()
        .rev()
        .find(|(command, _)| command == "click")
        .expect("click replayed after pick");
    let last = &last_click.1;
    assert_eq!(last[0], "@e2", "target replaced by the picked ref");
    let snapshot = last.iter().position(|a| a == "--snapshot").unwrap();
    assert_eq!(last[snapshot + 1], "s-ambiguous-1", "snapshot-bound replay");
    // picked 记录在案(答案可追溯)
    assert_eq!(
        run.steps[0].ambiguous.as_ref().unwrap().picked.as_deref(),
        Some("@e2")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pick_rejects_non_candidate_or_missing_pause() {
    let (root, store, mut run) = setup();
    let mut host = PickHost::fresh();
    // 无暂停时 --pick:PROTOCOL,先于任何派发
    let error = execute(&plan(), &mut run, &store, &mut host, None, Some("@e1")).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    // 跑出暂停
    run.status = Status::Ready;
    execute(&plan(), &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    // 非候选 ref:PROTOCOL 且候选清单一并给出
    let error = execute(&plan(), &mut run, &store, &mut host, None, Some("@e9")).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert!(error.message.contains("@e1, @e2, @e5"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_pick_after_revision_is_rejected() {
    // 修订改了步骤目标:旧 pick 不再匹配现目标——PROTOCOL,不消费重写
    let (root, store, mut run) = setup();
    let mut host = PickHost::fresh();
    execute(&plan(), &mut run, &store, &mut host, None, None).unwrap();
    run.steps[0].ambiguous.as_mut().unwrap().picked = Some("@e2".into());
    let revised = parse(
        "version: 1\nname: Pick\nsteps:\n - id: a\n   title: A\n   run: {command: click, args: [\"name:Pause\", \"--app\", Spotify]}\n   verify: {command: check}\n",
    )
    .unwrap();
    let error = execute(&revised, &mut run, &store, &mut host, None, None).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert!(error.message.contains("target changed"));
    std::fs::remove_dir_all(root).unwrap();
}
