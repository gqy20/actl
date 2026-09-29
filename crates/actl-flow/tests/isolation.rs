use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};

fn plan() -> Plan {
    parse(
        "version: 1\nname: isolation\nsteps:\n- id: a1\n  title: Fragile\n  section: first\n  on_failure: skip_section\n  run: {command: write}\n  verify: {command: check}\n- id: a2\n  title: Same section\n  section: first\n  run: {command: write}\n  verify: {command: check}\n- id: b1\n  title: Other section\n  section: second\n  run: {command: write}\n  verify: {command: check}\n",
    )
    .unwrap()
}
struct Fake {
    /// Number of leading `check` calls that fail; `usize::MAX` fails them all.
    check_failures: usize,
    checks: usize,
    calls: Vec<String>,
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        match call.command.as_str() {
            "check" => {
                self.checks += 1;
                if self.checks <= self.check_failures {
                    Err(CtlError::new(
                        ErrorCode::NotActionable,
                        "clipboard text unavailable",
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
    fn read_only(&self, call: &Invocation) -> bool {
        call.command == "check"
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}
fn setup() -> (Plan, Store, Run, Fake) {
    let plan = plan();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "iso").unwrap();
    let run = store.create(&plan, "iso").unwrap();
    (
        plan,
        store,
        run,
        Fake {
            check_failures: 1,
            checks: 0,
            calls: vec![],
        },
    )
}

#[test]
fn section_failure_isolates_and_later_sections_still_run() {
    let (plan, store, mut run, mut host) = setup();
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Uncertain);
    assert_eq!(run.steps[1].status, StepStatus::Skipped);
    assert_eq!(run.steps[2].status, StepStatus::Completed);
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("section_failure:a1"));
    assert!(host.calls.iter().filter(|c| *c == "write").count() >= 2);
    // The skipped status is durable and does not execute on a plain resume.
    let (_, run2) = store.load().unwrap();
    assert_eq!(
        serde_json::to_value(&run2.steps[1]).unwrap()["status"],
        "skipped"
    );
}

#[test]
fn authorized_retry_restores_the_skipped_section() {
    let (plan, store, mut run, mut host) = setup();
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    host.check_failures = 0;
    execute(&plan, &mut run, &store, &mut host, Some("a1")).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Completed);
    assert_eq!(run.steps[1].status, StepStatus::Completed);
    assert_eq!(run.steps[2].status, StepStatus::Completed);
    assert_eq!(run.status, Status::Completed);
}

#[test]
fn repeated_identical_failures_are_classified_as_route_defects() {
    let plan = parse(
        "version: 1\nname: defect\nsteps:\n- id: s\n  title: S\n  section: only\n  on_failure: skip_section\n  run: {command: write}\n  verify: {command: check}\n",
    )
    .unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "defect").unwrap();
    let mut run = store.create(&plan, "defect").unwrap();
    let mut host = Fake {
        check_failures: usize::MAX,
        checks: 0,
        calls: vec![],
    };
    for round in 1..=3 {
        let retry = (round > 1).then_some("s");
        execute(&plan, &mut run, &store, &mut host, retry).unwrap();
        assert_eq!(run.steps[0].attempt_errors.len(), round);
        assert_eq!(run.steps[0].status, StepStatus::Uncertain);
        assert_eq!(
            route_defect(&run.steps[0]),
            round >= 3,
            "round {round} misclassified"
        );
    }
    assert!(
        run.reason
            .as_deref()
            .is_some_and(|r| r.contains("route_defect_suspected"))
    );
}

#[test]
fn failures_without_source_evidence_get_structured_synthesis() {
    let (plan, store, mut run, mut host) = setup();
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    let evidence = run.steps[0].error_evidence.as_ref().unwrap();
    assert_eq!(evidence["phase"], "verify");
    assert_eq!(evidence["command"], "check");
    assert_eq!(evidence["synthetic"], true);
    // 错误原文随证据保留:归因不再依赖手工复现(docs/68 Word 锁文件教训)。
    assert!(
        evidence["message"]
            .as_str()
            .is_some_and(|m| !m.trim().is_empty())
    );
}

#[test]
fn skip_section_requires_a_declared_section() {
    let error = parse(
        "version: 1\nname: bad\nsteps:\n- id: s\n  title: S\n  on_failure: skip_section\n  run: {command: write}\n",
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
    let error = parse(
        "version: 1\nname: bad\nsteps:\n- id: s\n  title: S\n  on_failure: explode\n  run: {command: write}\n",
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::Protocol);
}
