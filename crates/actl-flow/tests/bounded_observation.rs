use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
struct Fake {
    reads: usize,
    writes: usize,
    fail: bool,
}

struct Matrix {
    errors: std::collections::VecDeque<CtlError>,
    calls: Vec<String>,
    now: u64,
    stop: bool,
}
impl Host for Matrix {
    fn execute(&mut self, c: &Invocation) -> Result<(), CtlError> {
        self.calls.push(c.command.clone());
        self.now += 100;
        self.errors.pop_front().map_or(Ok(()), Err)
    }
    fn read_only(&self, c: &Invocation) -> bool {
        c.command != "write"
    }
    fn stopped(&self) -> bool {
        self.stop && !self.calls.is_empty()
    }
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn publish(&mut self, _: &Run) {}
}
fn matrix(error: CtlError, timeout: u64, stop: bool) -> (Plan, Store, Run, Matrix) {
    let p=parse(&format!("version: 1\nname: test\nsteps:\n- id: a\n  title: Test\n  recovery: {{max_retries: 2, timeout_ms: {timeout}, interval_ms: 1}}\n  when: {{command: check}}\n  run: {{command: write}}\n  verify: {{command: check}}\n")).unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "matrix").unwrap();
    let run = store.create(&p, "matrix").unwrap();
    (
        p,
        store,
        run,
        Matrix {
            errors: [error].into(),
            calls: vec![],
            now: 1000,
            stop,
        },
    )
}
#[test]
fn transient_errors_recover_but_ambiguity_and_truncation_do_not_retry() {
    for (error, retries) in [
        (CtlError::new(ErrorCode::StaleRef, "transient"), 1),
        (CtlError::new(ErrorCode::Ambiguous, "ambiguous"), 0),
        (
            CtlError::with_evidence(
                ErrorCode::NotFound,
                "partial",
                serde_json::json!({"truncated":true}),
            ),
            0,
        ),
    ] {
        let (p, store, mut run, mut host) = matrix(error, 1000, false);
        execute(&p, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.steps[0].recovery.as_ref().unwrap().retries, retries);
        assert_eq!(
            host.calls.iter().filter(|c| *c == "write").count(),
            retries as usize
        );
    }
}
#[test]
fn stop_prevents_retry_and_timeout_prevents_another_attempt() {
    let (p, store, mut run, mut host) =
        matrix(CtlError::new(ErrorCode::StaleRef, "temporary"), 1000, true);
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Stopped);
    assert_eq!(host.calls, vec!["check"]);
    let (p, store, mut run, mut host) =
        matrix(CtlError::new(ErrorCode::StaleRef, "temporary"), 50, false);
    host.errors
        .push_back(CtlError::new(ErrorCode::StaleRef, "again"));
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, vec!["check", "check"]);
    assert_eq!(run.status, Status::Paused);
    assert_eq!(
        run.steps[0]
            .recovery
            .as_ref()
            .unwrap()
            .events
            .last()
            .unwrap()
            .outcome,
        "budget_exhausted"
    );
}
#[test]
fn policy_and_progress_contract_is_stable() {
    let (p, store, mut run, mut host) = matrix(
        CtlError::new(ErrorCode::Ambiguous, "ambiguous"),
        1000,
        false,
    );
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    let progress = serde_json::to_value(run.steps[0].recovery.as_ref().unwrap()).unwrap();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("golden/recovery_progress_v1.json")).unwrap();
    assert_eq!(progress, golden);
    let mut bad = p.clone();
    bad.steps[0].recovery.as_mut().unwrap().max_retries = 100;
    assert!(actl_flow::data::validate(&bad).is_err());
    let mut bad = p;
    bad.steps[0].retry = Retry::StaleRead;
    assert!(actl_flow::data::validate(&bad).is_err());
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        if call.command == "write" {
            self.writes += 1;
            return Ok(());
        }
        self.reads += 1;
        if self.fail {
            Err(CtlError::new(ErrorCode::StaleRef, "temporary"))
        } else {
            Ok(())
        }
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command != "write"
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}
#[test]
fn retries_are_durable_and_continue_does_not_refill_budget() {
    let p=parse("version: 1\nname: recovery\nsteps:\n- id: a\n  title: Test\n  recovery: {max_retries: 2, timeout_ms: 1000, interval_ms: 1}\n  when: {command: check}\n  run: {command: write}\n  verify: {command: check}\n").unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "bounded").unwrap();
    let mut run = store.create(&p, "bounded").unwrap();
    let mut host = Fake {
        reads: 0,
        writes: 0,
        fail: true,
    };
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.reads, 3);
    assert_eq!(host.writes, 0);
    let (p, mut run) = store.load().unwrap();
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.reads, 4);
    assert_eq!(host.writes, 0);
    host.fail = false;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.writes, 1);
}
