use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, revise, store::Store, *};

#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    /// 模拟满足性:ensure check 的命令名命中此表 → 通过。
    satisfied: Vec<String>,
}

impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        Ok(())
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command != "write"
    }
    fn condition(&mut self, call: &Invocation) -> Result<bool, CtlError> {
        self.calls.push(format!("check:{}", call.command));
        Ok(self.satisfied.contains(&call.command))
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}

fn setup(plan: &Plan) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "flow-revise-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "test").unwrap();
    let run = store.create(plan, "test").unwrap();
    (root, store, run)
}

fn yaml(steps: &str) -> String {
    format!("version: 1\nname: Revise\nsteps:\n{steps}")
}

// ── ensure: 幂等语义 ─────────────────────────────────────────────────────────

#[test]
fn ensure_check_pass_short_circuits_without_dispatch() {
    let plan = parse(&yaml(
        " - id: e1\n   title: E\n   ensure: {check: {command: probe}}\n   run: {command: write}\n",
    ))
    .unwrap();
    let (root, store, mut run) = setup(&plan);
    let mut host = Fake {
        satisfied: vec!["probe".into()],
        ..Default::default()
    };
    execute(&plan, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(run.steps[0].status, StepStatus::Completed);
    assert!(run.steps[0].verified);
    // run(写步)从未派发,只有 check 执行过
    assert_eq!(host.calls, vec!["check:probe".to_string()]);
    assert_eq!(run.steps[0].plan_version, Some(1));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ensure_check_fail_falls_through_to_run() {
    let plan = parse(&yaml(
        " - id: e1\n   title: E\n   ensure: {check: {command: probe}}\n   run: {command: write}\n",
    ))
    .unwrap();
    let (root, store, mut run) = setup(&plan);
    let mut host = Fake::default(); // probe 不在 satisfied → 不满足
    execute(&plan, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.calls,
        vec!["check:probe".to_string(), "write".to_string()]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ensure_check_must_be_read_only() {
    // check 是写命令 → execute 预检 PROTOCOL
    let plan = parse(&yaml(
        " - id: e1\n   title: E\n   ensure: {check: {command: write}}\n   run: {command: write}\n",
    ))
    .unwrap();
    let (root, store, mut run) = setup(&plan);
    let mut host = Fake::default();
    let error = execute(&plan, &mut run, &store, &mut host, None, None).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert!(error.message.contains("read-only"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ensure_and_outputs_are_mutually_exclusive() {
    let bad = yaml(
        " - id: e1\n   title: E\n   ensure: {check: {command: probe}}\n   run: {command: peek}\n   outputs: {x: {index: '0'}}\n",
    );
    assert!(parse(&bad).is_err());
}

// ── diff 与修订政策 ─────────────────────────────────────────────────────────

fn record(id: &str, status: StepStatus, verified: bool) -> StepRecord {
    StepRecord {
        id: id.into(),
        status,
        attempts: 1,
        checkpoint_passed: false,
        verified,
        error_code: None,
        error_evidence: None,
        attempt_errors: vec![],
        outputs: Default::default(),
        outputs_captured_ms: None,
        observation_retries: vec![],
        branch: None,
        recovery: None,
        plan_version: Some(1),
        started_ms: None,
        finished_ms: None,
        ambiguous: None,
    }
}

fn apply(
    old_yaml: &str,
    new_yaml: &str,
    run: &mut Run,
    accepted: &[String],
) -> Result<revise::RevisionOutcome, CtlError> {
    let old = parse(old_yaml).unwrap();
    let new = parse(new_yaml).unwrap();
    revise::apply_to_run(
        &old,
        &new,
        run,
        |step| step.run.command == "write",
        accepted,
        "fix",
        "fp",
    )
}

#[test]
fn diff_classifies_by_position_and_spec() {
    let old = parse(&yaml(
        " - id: a\n   title: A\n   run: {command: peek}\n - id: b\n   title: B\n   run: {command: write}\n",
    ))
    .unwrap();
    let new = parse(&yaml(
        " - id: a\n   title: A\n   run: {command: peek}\n - id: b2\n   title: B2\n   run: {command: write, args: [x]}\n - id: c\n   title: C\n   run: {command: peek}\n",
    ))
    .unwrap();
    let diff = revise::diff_plans(&old, &new);
    assert_eq!(diff.unchanged, vec!["a".to_string()]);
    assert_eq!(diff.modified, vec!["b→b2".to_string()]);
    assert_eq!(diff.added, vec!["c".to_string()]);
    let diff_back = revise::diff_plans(&new, &old);
    assert_eq!(diff_back.removed, vec!["c".to_string()]);
}

#[test]
fn revision_keeps_unchanged_resets_changed_read_requires_accept_for_write() {
    let old = yaml(
        " - id: a\n   title: A\n   run: {command: peek}\n - id: b\n   title: B\n   run: {command: write}\n - id: c\n   title: C\n   run: {command: peek}\n",
    );
    let new = yaml(
        " - id: a\n   title: A\n   run: {command: peek}\n - id: b\n   title: B\n   run: {command: write, args: [fixed]}\n - id: c\n   title: C\n   run: {command: peek, args: [changed]}\n",
    );
    // 三个都完成;b(写)改了,c(读)改了,a 未变
    let mut run = Run {
        version: 1,
        id: "t".into(),
        status: Status::Completed,
        updated_ms: 0,
        reason: None,
        plan_version: 1,
        steps: vec![
            record("a", StepStatus::Completed, true),
            record("b", StepStatus::Completed, true),
            record("c", StepStatus::Completed, true),
        ],
    };
    // 不点名 b → PROTOCOL,列出 b
    let error = apply(&old, &new, &mut run, &[]).unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    assert!(
        error
            .evidence
            .as_ref()
            .and_then(|e| e["steps"].as_array().cloned())
            .is_some_and(|steps| steps.iter().any(|s| s == "b"))
    );
    // 点名 b → 通过:a 保留(带溯源),b/c 重置 Pending
    let outcome = apply(&old, &new, &mut run, &["b".to_string()]).unwrap();
    assert_eq!(outcome.revision.version, 2);
    assert_eq!(run.plan_version, 2);
    assert_eq!(run.steps[0].status, StepStatus::Completed); // a 保留
    assert_eq!(run.steps[0].plan_version, Some(1));
    assert_eq!(run.steps[1].status, StepStatus::Pending); // b 重置
    assert_eq!(run.steps[2].status, StepStatus::Pending); // c(读)重置无需点名
}

#[test]
fn revision_of_uncertain_changed_step_is_its_own_authorization() {
    let old = yaml(" - id: s6\n   title: S\n   run: {command: write}\n");
    let new = yaml(" - id: s6\n   title: S\n   run: {command: write, args: [fixed]}\n");
    let mut run = Run {
        version: 1,
        id: "t".into(),
        status: Status::NeedsReview,
        updated_ms: 0,
        reason: Some("result_unconfirmed".into()),
        plan_version: 1,
        steps: vec![record("s6", StepStatus::Uncertain, false)],
    };
    apply(&old, &new, &mut run, &[]).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Pending);
}

#[test]
fn persist_and_lineage_round_trip() {
    let old = parse(&yaml(" - id: a\n   title: A\n   run: {command: peek}\n")).unwrap();
    let new = parse(&yaml(
        " - id: a\n   title: A\n   run: {command: peek, args: [x]}\n - id: n\n   title: N\n   run: {command: peek}\n",
    ))
    .unwrap();
    let (root, store, mut run) = setup(&old);
    let outcome =
        revise::apply_to_run(&old, &new, &mut run, |_| false, &[], "grow", "cafebabe").unwrap();
    let old_bytes = std::fs::read(store.dir.join("plan.json")).unwrap();
    revise::persist(&store.dir, &old_bytes, &new, &run, &outcome.revision).unwrap();
    // 旧版本字节已归档且未被覆盖
    let archived = std::fs::read(store.dir.join("revisions").join("v1.plan.json")).unwrap();
    assert_eq!(archived, old_bytes);
    // 谱系只追加一条,含指纹与 diff
    let lineage = revise::lineage(&store.dir);
    assert_eq!(lineage.len(), 1);
    assert_eq!(lineage[0].fingerprint, "cafebabe");
    assert_eq!(lineage[0].message, "grow");
    assert_eq!(lineage[0].diff.added, vec!["n".to_string()]);
    // store.load 能读回修订后的 plan/run(一致性校验通过)
    let (plan, loaded) = store.load().unwrap();
    assert_eq!(plan.steps.len(), 2);
    assert_eq!(loaded.plan_version, 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_state_without_plan_version_loads_as_v1() {
    // 修订机制引入前的 state.json 没有 plan_version 字段 → 反序列化默认 1
    let json = r#"{"version":1,"id":"t","status":"ready","updated_ms":0,"steps":[],"reason":null}"#;
    let run: Run = serde_json::from_str(json).unwrap();
    assert_eq!(run.plan_version, 1);
}
