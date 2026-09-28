use actl_core::CtlError;
use actl_flow::{engine::*, store::Store, *};
use serde_json::json;

fn plan() -> Plan {
    parse(
        r#"version: 1
name: freshness
steps:
  - id: read
    title: Read
    run: {command: read}
    outputs:
      text: {pointer: /text, type: string, max_age_ms: 100}
  - id: write
    title: Write
    pause_before: true
    run: {command: write, args: [placeholder], bind: {"0": read.text}}
    verify: {command: verify}
"#,
    )
    .unwrap()
}
struct Fake {
    now: u64,
    calls: Vec<String>,
    delay: u64,
}
impl Host for Fake {
    fn now_ms(&self) -> u64 {
        self.now
    }
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        if call.command == "before" {
            self.now += self.delay;
        }
        Ok(())
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<serde_json::Value, CtlError> {
        self.execute(call)?;
        Ok(json!({"text":"private content"}))
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command != "write"
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}
fn setup(p: &Plan) -> (std::path::PathBuf, Store, Run, Fake) {
    let root = std::env::temp_dir().join(format!(
        "flow-age-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "age").unwrap();
    let run = store.create(p, "age").unwrap();
    (
        root,
        store,
        run,
        Fake {
            now: 1000,
            calls: vec![],
            delay: 0,
        },
    )
}

#[test]
fn persisted_age_blocks_resume_without_replaying_source_or_write() {
    let p = plan();
    let (root, store, mut run, mut host) = setup(&p);
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    let (p, mut run) = store.load().unwrap();
    assert_eq!(run.steps[0].outputs_captured_ms, Some(1000));
    host.now = 1101;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["read"]);
    assert_eq!(run.steps[1].attempts, 0);
    assert_eq!(run.reason.as_deref(), Some("output_freshness_failed"));
    let evidence = run.steps[1].error_evidence.as_ref().unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/output_expired_v1.json")).unwrap();
    assert_eq!(evidence, &expected);
    assert!(!evidence.to_string().contains("private content"));
    assert_eq!(
        store.load().unwrap().1.steps[1].error_evidence.as_ref(),
        Some(evidence)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn boundary_clock_rollback_and_missing_time() {
    let p = plan();
    let mut run = Run::new("age", &p);
    run.steps[0].outputs_captured_ms = Some(1000);
    assert!(freshness::check(&p, &p.steps[1], &run, 1100).is_ok());
    for (time, reason) in [(1101, "expired"), (999, "clock_moved_backwards")] {
        let e = freshness::check(&p, &p.steps[1], &run, time).unwrap_err();
        assert_eq!(e.evidence.unwrap()["reason"], reason);
    }
    run.steps[0].outputs_captured_ms = None;
    assert_eq!(
        freshness::check(&p, &p.steps[1], &run, 1000)
            .unwrap_err()
            .evidence
            .unwrap()["reason"],
        "capture_time_missing"
    );
    let mut p = p;
    p.steps[0].outputs.get_mut("text").unwrap().max_age_ms = None;
    assert!(freshness::check(&p, &p.steps[1], &run, u64::MAX).is_ok());
}

#[test]
fn precondition_delay_rechecks_before_write_and_honors_fail_policy() {
    let mut p = plan();
    p.steps[1].before = Some(Invocation {
        command: "before".into(),
        args: vec![],
        bind: Default::default(),
    });
    p.steps[1].on_failure = Failure::Fail;
    let (root, store, mut run, mut host) = setup(&p);
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    host.delay = 101;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["read", "before"]);
    assert_eq!(run.status, Status::Failed);
    assert_eq!(run.steps[1].attempts, 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn fresh_resume_completes_and_completed_write_is_not_replayed_after_expiry() {
    let p = plan();
    let (root, store, mut run, mut host) = setup(&p);
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    host.now = 1100;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    host.now = 9999;
    execute(&p, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, ["read", "write", "verify"]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn age_requires_positive_integer_and_checks_all_phases() {
    let mut p = plan();
    p.steps[0].outputs.get_mut("text").unwrap().max_age_ms = Some(0);
    assert!(data::validate(&p).is_err());
    for bad in ["0", "-1", "1.5", "\"100\""] {
        let text = serde_json::to_string(&plan())
            .unwrap()
            .replace("\"max_age_ms\":100", &format!("\"max_age_ms\":{bad}"));
        assert!(parse(&text).is_err(), "{bad}");
    }
    let mut p = plan();
    let bind = std::mem::take(&mut p.steps[1].run.bind);
    p.steps[1].verify.as_mut().unwrap().bind = bind;
    let run = Run::new("age", &p);
    assert!(freshness::check(&p, &p.steps[1], &run, 1000).is_err());
}
