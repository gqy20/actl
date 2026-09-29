use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
fn plan() -> Plan {
    parse("version: 1\nname: test\nsteps:\n  - id: write\n    title: Write\n    run: {command: write}\n    verify: {command: check}\n  - id: read\n    title: Read\n    run: {command: read}\n").unwrap()
}

#[test]
fn assertion_evidence_is_redacted_unless_explicitly_enabled() {
    let error = CtlError::with_evidence(
        ErrorCode::AssertionFailed,
        "mismatch",
        serde_json::json!({
            "expected":"SECRET", "actual":"PRIVATE", "target":"name:PERSON", "property":"name", "channel":"name", "comparison":"exact"
        }),
    );
    let metadata = failure_evidence(EvidencePolicy::Metadata, &error).unwrap();
    assert!(!metadata.to_string().contains("PRIVATE"));
    assert!(!metadata.to_string().contains("SECRET"));
    assert_eq!(
        metadata,
        serde_json::from_str::<serde_json::Value>(include_str!("golden/evidence_metadata_v1.json"))
            .unwrap()
    );
    assert_eq!(
        failure_evidence(EvidencePolicy::Full, &error).unwrap()["actual"],
        "PRIVATE"
    );
    let huge = CtlError::with_evidence(
        ErrorCode::AssertionFailed,
        "large",
        serde_json::json!({"actual":"X".repeat(9000)}),
    );
    assert_eq!(
        failure_evidence(EvidencePolicy::Full, &huge).unwrap()["omitted"],
        "evidence_exceeds_8_kib"
    );
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../schemas/flow-v1.schema.json")).unwrap();
    assert_eq!(
        schema["$defs"]["step"]["properties"]["evidence"]["default"],
        "metadata"
    );
}

#[test]
fn recovery_metadata_preserves_safe_delivery_facts_only() {
    let error = CtlError::with_evidence(
        ErrorCode::NotActionable,
        "private",
        serde_json::json!({
        "stage":"handoff","reason":"human_takeover_or_grant_expired","action_started":true,
        "delivered_before_error":3,"failed_segment_size":2,"native_code":-1,
        "actual":"PRIVATE","target":"PRIVATE","expected":"PRIVATE"}),
    );
    let value = failure_evidence(EvidencePolicy::Metadata, &error).unwrap();
    assert_eq!(value["delivered_before_error"], 3);
    assert_eq!(value["action_started"], true);
    assert_eq!(value["native_code"], -1);
    assert!(!value.to_string().contains("PRIVATE"));
    let malformed = CtlError::with_evidence(
        ErrorCode::Internal,
        "private",
        serde_json::json!({"reason":"PRIVATE","native_code":"PRIVATE","action_started":"PRIVATE"}),
    );
    assert_eq!(
        failure_evidence(EvidencePolicy::Metadata, &malformed).unwrap(),
        serde_json::json!({"redacted":true})
    );
}

#[test]
fn semantic_rejection_metadata_contract() {
    let error = CtlError::with_evidence(
        ErrorCode::NotActionable,
        "target changed",
        serde_json::json!({"reason":"target_identity_changed","action_started":false,
            "target":"PRIVATE", "enabled":true}),
    );
    assert_eq!(
        failure_evidence(EvidencePolicy::Metadata, &error).unwrap(),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "golden/semantic_rejection_v1.json"
        ))
        .unwrap()
    );
}
fn setup(plan: &Plan) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "actl-flow-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "test").unwrap();
    let run = store.create(plan, "test").unwrap();
    (root, store, run)
}
struct Fake {
    calls: Vec<String>,
    verified: bool,
    stop: bool,
    pause: Option<Store>,
    failures: u32,
    stop_on_check: bool,
    interrupt_check: Option<Store>,
    handoff: bool,
    before_start: bool,
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        if call.command == "write" && self.handoff {
            return Err(CtlError::with_evidence(
                ErrorCode::NotActionable,
                "yielded",
                serde_json::json!({"stage":"handoff","reason":"human_takeover_or_grant_expired", "action_started":!self.before_start}),
            ));
        }
        if call.command == "check"
            && let Some(store) = self.interrupt_check.take()
        {
            store.pause().unwrap();
            return Err(CtlError::new(ErrorCode::Aborted, "pause wait"));
        }
        if call.command == "check" && self.stop_on_check {
            self.stop = true;
            return Err(CtlError::new(
                ErrorCode::Aborted,
                "stopped during verification",
            ));
        }
        if call.command == "write"
            && let Some(store) = self.pause.take()
        {
            store.pause().unwrap();
        }
        if call.command == "check" && !self.verified {
            return Err(CtlError::new(ErrorCode::AssertionFailed, "not verified"));
        }
        if call.command == "read" && self.failures > 0 {
            self.failures -= 1;
            return Err(CtlError::new(ErrorCode::Timeout, "later"));
        }
        Ok(())
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command != "write"
    }
    fn stopped(&self) -> bool {
        self.stop
    }
    fn publish(&mut self, _: &Run) {}
}
fn fake() -> Fake {
    Fake {
        calls: vec![],
        verified: true,
        stop: false,
        pause: None,
        failures: 0,
        stop_on_check: false,
        interrupt_check: None,
        handoff: false,
        before_start: false,
    }
}

#[test]
fn pause_in_checks_preserves_stage_without_false_assertion_failure() {
    for before in [true, false] {
        let mut p = plan();
        if before {
            p.steps[0].before = Some(Invocation {
                command: "check".into(),
                args: vec![],
                bind: Default::default(),
            });
        }
        let (root, store, mut run) = setup(&p);
        let mut host = fake();
        host.interrupt_check = Some(store.clone());
        execute(&p, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.status, Status::Paused);
        assert_eq!(run.reason.as_deref(), Some("pause_requested"));
        assert_eq!(
            run.steps[0].status,
            if before {
                StepStatus::Pending
            } else {
                StepStatus::Verifying
            }
        );
        assert!(run.steps[0].error_code.is_none());
        store.clear_pause().unwrap();
        execute(&p, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.status, Status::Completed);
        assert_eq!(host.calls.iter().filter(|c| *c == "write").count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn stop_during_verification_is_stopped_not_needs_review() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut host = fake();
    host.stop_on_check = true;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Stopped);
    assert_eq!(run.steps[0].status, StepStatus::Uncertain);
    assert_eq!(run.steps[1].attempts, 0);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn parser_rejects_ambiguous_or_unbounded_documents() {
    for text in [
        "version: 1\nversion: 1\nname: x\nsteps: []",
        "version: 2\nname: x\nsteps: []",
        "version: 1\nname: x\nsteps: []\nunknown: 1",
        "version: 1\nname: &a x\nsteps: *a",
        "version: 1\nname: x\nsteps: []\n---\nversion: 1",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
    assert!(parse(&"x".repeat(262_145)).is_err());
    let mut p = plan();
    p.steps[1].id = p.steps[0].id.clone();
    assert!(parse(&serde_json::to_string(&p).unwrap()).is_err());
}
#[test]
fn pause_waits_for_action_then_resume_verifies_without_repeating_write() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut h = fake();
    h.pause = Some(store.clone());
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert_eq!(run.steps[0].status, StepStatus::Verifying);
    assert_eq!(h.calls, vec!["write"]);
    store.clear_pause().unwrap();
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(h.calls, vec!["write", "check", "read"]);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn uncertain_result_requires_verification_or_explicit_retry() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut h = fake();
    h.verified = false;
    run.steps[0].status = StepStatus::Running;
    run.steps[0].attempts = 1;
    store.save(&run).unwrap();
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(h.calls, vec!["check"]);
    assert_eq!(run.steps[0].error_code, Some(ErrorCode::AssertionFailed));
    execute(&p, &mut run, &store, &mut h, Some("write")).unwrap();
    assert_eq!(h.calls, vec!["check", "check", "write", "check"]);
    assert_eq!(run.status, Status::NeedsReview);
    h.verified = true;
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(h.calls.iter().filter(|c| c.as_str() == "write").count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn checkpoints_stop_and_retries_preserve_progress() {
    let mut p = plan();
    p.steps[0].pause_before = true;
    p.steps[1].retry = Retry::ReadOnly;
    let (root, store, mut run) = setup(&p);
    let mut h = fake();
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert!(h.calls.is_empty());
    h.stop = true;
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Stopped);
    h.stop = false;
    h.failures = 2;
    execute(&p, &mut run, &store, &mut h, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(run.steps[1].attempts, 3);
    assert_eq!(run.steps[0].attempts, 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn locks_release_and_write_failure_prevents_action() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let lock = store.lock(false).unwrap();
    assert!(store.active().unwrap());
    assert!(store.lock(false).is_err());
    drop(lock);
    assert!(!store.active().unwrap());
    std::fs::remove_file(store.dir.join("state.json")).unwrap();
    std::fs::create_dir(store.dir.join("state.json")).unwrap();
    let mut h = fake();
    assert!(execute(&p, &mut run, &store, &mut h, None).is_err());
    assert!(h.calls.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn run_state_golden_and_immutable_id() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    assert!(store.create(&p, "test").is_err());
    run.updated_ms = 0;
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/run_v1.json")).unwrap();
    assert_eq!(serde_json::to_value(run).unwrap(), expected);
    assert!(Store::at(&root, "../escape").is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn handoff_pauses_and_does_not_replay_uncertain_action() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut host = fake();
    host.handoff = true;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert_eq!(
        run.reason.as_deref(),
        Some("human_takeover_or_grant_expired")
    );
    assert_eq!(run.steps[0].status, StepStatus::Uncertain);
    host.handoff = false;
    host.verified = false;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls.iter().filter(|x| *x == "write").count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn consent_refusal_before_any_action_can_resume_without_replay_override() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut host = fake();
    host.handoff = true;
    host.before_start = true;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert_eq!(run.steps[0].status, StepStatus::Pending);
    host.handoff = false;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    std::fs::remove_dir_all(root).unwrap();
}
