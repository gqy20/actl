use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};

fn plan(otherwise: bool) -> Plan {
    parse(&format!("version: 1\nname: branches\nsteps:\n- id: choose\n  title: Choose\n  when: {{command: condition}}\n  run: {{command: write}}\n{}  verify: {{command: verify}}\n",
        if otherwise { "  otherwise: {command: alternate}\n" } else { "" })).unwrap()
}
struct Fake {
    condition: Result<(), ErrorCode>,
    calls: Vec<String>,
    uncertain: bool,
    verified: bool,
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        match call.command.as_str() {
            "condition" => self
                .condition
                .map_err(|code| CtlError::new(code, "condition")),
            "verify" if !self.verified => Err(CtlError::new(ErrorCode::AssertionFailed, "verify")),
            "write" | "alternate" if self.uncertain => {
                Err(CtlError::new(ErrorCode::NotActionable, "unknown delivery"))
            }
            _ => Ok(()),
        }
    }
    fn read_only(&self, call: &Invocation) -> bool {
        matches!(call.command.as_str(), "condition" | "verify")
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}
fn setup(plan: &Plan) -> (Store, Run, Fake) {
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "branch").unwrap();
    let run = store.create(plan, "branch").unwrap();
    (
        store,
        run,
        Fake {
            condition: Ok(()),
            calls: vec![],
            uncertain: false,
            verified: true,
        },
    )
}
#[test]
fn true_false_and_skip_have_distinct_durable_results() {
    for (matched, otherwise, expected) in [
        (true, true, "run"),
        (false, true, "otherwise"),
        (false, false, "skip"),
    ] {
        let plan = plan(otherwise);
        let (store, mut run, mut host) = setup(&plan);
        if !matched {
            host.condition = Err(ErrorCode::AssertionFailed);
        }
        execute(&plan, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.status, Status::Completed);
        let state = serde_json::to_value(&run.steps[0]).unwrap();
        assert_eq!(state["branch"], expected);
        assert_eq!(
            run.steps[0].attempts,
            if expected == "skip" { 0 } else { 1 }
        );
        assert_eq!(
            host.calls,
            match expected {
                "run" => vec!["condition", "write", "verify"],
                "otherwise" => vec!["condition", "alternate", "verify"],
                _ => vec!["condition"],
            }
        );
        assert_eq!(
            serde_json::to_value(store.load().unwrap().1.steps[0].clone()).unwrap()["branch"],
            expected
        );
    }
}
#[test]
fn observation_errors_never_select_the_negative_branch() {
    for code in [
        ErrorCode::NotFound,
        ErrorCode::Ambiguous,
        ErrorCode::NotActionable,
        ErrorCode::Internal,
        ErrorCode::Timeout,
        ErrorCode::Aborted,
    ] {
        let plan = plan(true);
        let (store, mut run, mut host) = setup(&plan);
        host.condition = Err(code);
        execute(&plan, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.status, Status::Paused);
        assert_eq!(host.calls, vec!["condition"]);
        assert_eq!(run.steps[0].attempts, 0);
        assert_eq!(run.reason.as_deref(), Some("condition_observation_failed"));
    }
}
#[test]
fn uncertain_write_keeps_original_branch_after_condition_changes() {
    let plan = plan(true);
    let (store, mut run, mut host) = setup(&plan);
    host.uncertain = true;
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Uncertain);
    host.calls.clear();
    host.condition = Err(ErrorCode::AssertionFailed);
    host.verified = false;
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, vec!["verify"]);
    assert_eq!(run.status, Status::NeedsReview);
    host.calls.clear();
    host.verified = true;
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, vec!["verify"]);
    assert_eq!(run.status, Status::Completed);
}
#[test]
fn pending_before_dispatch_rechecks_condition_on_resume() {
    let mut plan = plan(true);
    plan.steps[0].pause_before = true;
    let (store, mut run, mut host) = setup(&plan);
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    host.calls.clear();
    host.condition = Err(ErrorCode::AssertionFailed);
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert!(host.calls.contains(&"alternate".into()));
    assert!(!host.calls.contains(&"write".into()));
}

#[test]
fn unless_skips_completed_state_and_preserves_unknown_errors() {
    let mut plan = plan(false);
    plan.steps[0].unless = plan.steps[0].when.take();
    let (store, mut run, mut host) = setup(&plan);
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, vec!["condition"]);
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/branch_skip_v1.json")).unwrap();
    assert_eq!(serde_json::to_value(&run.steps[0]).unwrap(), expected);
    let (store, mut run, mut host) = setup(&plan);
    host.condition = Err(ErrorCode::Internal);
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert_eq!(host.calls, vec!["condition"]);
}

#[test]
fn invalid_branch_state_and_conflicting_predicates_are_rejected() {
    let mut plan = plan(true);
    plan.steps[0].unless = plan.steps[0].when.clone();
    assert!(actl_flow::data::validate(&plan).is_err());
    plan.steps[0].unless = None;
    let (store, mut run, mut host) = setup(&plan);
    run.steps[0].status = StepStatus::Uncertain;
    assert!(execute(&plan, &mut run, &store, &mut host, None).is_err());
    assert!(host.calls.is_empty());
}

#[test]
fn uncertain_alternate_survives_reload_without_replaying_either_route() {
    let plan = plan(true);
    let (store, mut run, mut host) = setup(&plan);
    host.condition = Err(ErrorCode::AssertionFailed);
    host.uncertain = true;
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.steps[0].branch, Some(branch::Branch::Otherwise));
    let (saved, mut run) = store.load().unwrap();
    host.condition = Ok(());
    host.calls.clear();
    execute(&saved, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(host.calls, vec!["verify"]);
    assert_eq!(run.status, Status::Completed);
    assert_eq!(run.steps[0].branch, Some(branch::Branch::Otherwise));
}

#[test]
fn pause_or_stop_during_condition_prevents_dispatch() {
    struct Interrupt<'a> {
        inner: Fake,
        store: &'a Store,
        stop: bool,
        observed: bool,
    }
    impl Host for Interrupt<'_> {
        fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
            self.inner.execute(call)
        }
        fn condition(&mut self, call: &Invocation) -> Result<bool, CtlError> {
            self.inner.execute(call)?;
            self.observed = true;
            if !self.stop {
                self.store.pause()?;
            }
            Ok(true)
        }
        fn read_only(&self, call: &Invocation) -> bool {
            self.inner.read_only(call)
        }
        fn stopped(&self) -> bool {
            self.stop && self.observed
        }
        fn publish(&mut self, _: &Run) {}
    }
    for stop in [false, true] {
        let plan = plan(true);
        let (store, mut run, inner) = setup(&plan);
        let mut host = Interrupt {
            inner,
            store: &store,
            stop,
            observed: false,
        };
        execute(&plan, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(
            run.status,
            if stop {
                Status::Stopped
            } else {
                Status::Paused
            }
        );
        assert_eq!(host.inner.calls, vec!["condition"]);
        assert_eq!(run.steps[0].attempts, 0);
        assert_eq!(run.steps[0].branch, None);
    }
}
