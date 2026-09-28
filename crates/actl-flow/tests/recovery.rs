use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};

#[derive(Default)]
struct Fake {
    calls: Vec<String>,
    errors: Vec<ErrorCode>,
    pause_on_error: Option<std::path::PathBuf>,
    stop_on_error: bool,
    stopped: bool,
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        if call.command == "check" && !self.errors.is_empty() {
            if let Some(path) = self.pause_on_error.take() {
                std::fs::write(path, "pause").unwrap();
            }
            self.stopped = self.stop_on_error;
            return Err(CtlError::new(self.errors.remove(0), "observation failed"));
        }
        Ok(())
    }
    fn read_only(&self, c: &Invocation) -> bool {
        c.command != "write"
    }
    fn stopped(&self) -> bool {
        self.stopped
    }
    fn publish(&mut self, _: &Run) {}
}
fn plan(extra: &str) -> Plan {
    parse(&format!("version: 1\nname: Recovery\nsteps:\n - id: a\n   title: A\n   run: {{command: write}}\n   verify: {{command: check}}\n{extra}")).unwrap()
}
fn setup(p: &Plan) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "flow-recovery-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "test").unwrap();
    let run = store.create(p, "test").unwrap();
    (root, store, run)
}
#[test]
fn explicit_retry_does_not_replay_when_result_observation_is_unknown() {
    let p = plan("");
    let (root, store, mut run) = setup(&p);
    run.steps[0].status = StepStatus::Uncertain;
    run.steps[0].attempts = 1;
    let mut host = Fake {
        calls: vec![],
        errors: vec![ErrorCode::StaleRef],
        ..Default::default()
    };
    execute(&p, &mut run, &store, &mut host, Some("a")).unwrap();
    assert_eq!(host.calls, ["check"]);
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("result_observation_failed"));
    assert_eq!(run.steps[0].attempts, 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn stale_observation_recovers_without_replaying_write_and_keeps_audit() {
    let p = plan("   retry: stale_read\n");
    let (root, store, mut run) = setup(&p);
    let mut host = Fake {
        errors: vec![ErrorCode::StaleRef],
        ..Default::default()
    };
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["write", "check", "check"]);
    assert_eq!(run.status, Status::Completed);
    assert_eq!(run.steps[0].attempts, 1);
    let audit =
        serde_json::to_value(&store.load().unwrap().1.steps[0].observation_retries).unwrap();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("golden/reobservation_v1.json")).unwrap();
    assert_eq!(audit, golden);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exhaustion_and_nontransient_errors_stop_without_replay() {
    for error in [
        ErrorCode::StaleRef,
        ErrorCode::AssertionFailed,
        ErrorCode::PermDenied,
        ErrorCode::Internal,
        ErrorCode::NotFound,
        ErrorCode::Ambiguous,
        ErrorCode::Aborted,
    ] {
        let p = plan("   retry: stale_read\n");
        let (root, store, mut run) = setup(&p);
        run.steps[0].status = StepStatus::Uncertain;
        run.steps[0].attempts = 1;
        let mut host = Fake {
            errors: vec![error; 4],
            ..Default::default()
        };
        execute(&p, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(
            host.calls.len(),
            if error == ErrorCode::StaleRef { 3 } else { 1 }
        );
        assert!(host.calls.iter().all(|c| c == "check"));
        assert_eq!(run.status, Status::NeedsReview);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn resume_check_precedes_result_verification_and_can_block_it() {
    let p = plan("   resume_check: {command: check}\n");
    let (root, store, mut run) = setup(&p);
    run.status = Status::Paused;
    run.steps[0].status = StepStatus::Uncertain;
    run.steps[0].attempts = 1;
    let mut host = Fake {
        errors: vec![ErrorCode::AssertionFailed],
        ..Default::default()
    };
    execute(&p, &mut run, &store, &mut host, Some("a")).unwrap();
    assert_eq!(host.calls, ["check"]);
    assert_eq!(run.reason.as_deref(), Some("resume_check_failed"));
    assert_eq!(run.steps[0].status, StepStatus::Uncertain);
    host.calls.clear();
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["check", "check"]);
    assert_eq!(run.status, Status::Completed);
    assert_eq!(run.steps[0].attempts, 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pause_or_stop_prevents_another_observation() {
    for stop in [false, true] {
        let p = plan("   retry: stale_read\n");
        let (root, store, mut run) = setup(&p);
        let mut host = Fake {
            errors: vec![ErrorCode::StaleRef],
            stop_on_error: stop,
            pause_on_error: (!stop).then(|| store.dir.join("pause-requested")),
            ..Default::default()
        };
        execute(&p, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(host.calls, ["write", "check"]);
        assert_eq!(
            run.status,
            if stop {
                Status::Stopped
            } else {
                Status::Paused
            }
        );
        assert_ne!(run.steps[0].status, StepStatus::Completed);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn read_run_retries_count_each_actual_call_and_budget_survives_resume() {
    let mut p = plan("   retry: stale_read\n");
    p.steps[0].run.command = "check".into();
    p.steps[0].verify = None;
    let (root, store, mut run) = setup(&p);
    let mut host = Fake {
        errors: vec![ErrorCode::StaleRef; 3],
        ..Default::default()
    };
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.steps[0].attempts, 3);
    assert_eq!(run.steps[0].observation_retries.len(), 2);
    while run.steps[0].observation_retries.len() < 32 {
        run.steps[0]
            .observation_retries
            .push(recovery::ObservationRetry {
                phase: "run".into(),
                error_code: ErrorCode::StaleRef,
                retry_number: 1,
            });
    }
    store.save(&run).unwrap();
    let (_, mut run) = store.load().unwrap();
    host.calls.clear();
    host.errors = vec![ErrorCode::StaleRef];
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["check"]);
    assert_eq!(run.steps[0].observation_retries.len(), 32);
    std::fs::remove_dir_all(root).unwrap();
}
