use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
fn plan() -> Plan {
    parse("version: 1\nname: test\nsteps:\n  - id: write\n    title: Write\n    run: {command: write}\n    verify: {command: check}\n  - id: read\n    title: Read\n    run: {command: read}\n").unwrap()
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
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
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
