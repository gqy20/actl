use actl_core::{CtlError, ErrorCode};
use actl_flow::{Invocation, engine::*, store::Store};

struct Desktop {
    effect: &'static str,
    text: String,
    interrupted: bool,
    calls: Vec<String>,
    observation_failed: bool,
}
impl Host for Desktop {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.command.clone());
        match call.command.as_str() {
            "scene" if self.observation_failed => {
                Err(CtlError::new(ErrorCode::StaleRef, "unknown scene"))
            }
            "write" => {
                if !self.interrupted {
                    self.interrupted = true;
                    self.text.push_str(self.effect);
                    return Err(CtlError::with_evidence(
                        ErrorCode::NotActionable,
                        "yield",
                        serde_json::json!({"stage":"handoff","reason":"human_takeover_or_grant_expired",
                            "action_started":!self.effect.is_empty()}),
                    ));
                }
                self.text.push_str("hello");
                Ok(())
            }
            "result" if self.text != "hello" => {
                Err(CtlError::new(ErrorCode::AssertionFailed, "result differs"))
            }
            _ => Ok(()),
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
fn continue_reobserves_and_never_replays_an_uncertain_write() {
    for effect in ["", "hello", "hel"] {
        let plan = actl_flow::parse("version: 1\nname: Resume\nsteps:\n - id: input\n   title: Input\n   resume_check: {command: scene}\n   run: {command: write}\n   verify: {command: result}\n - id: next\n   title: Next\n   run: {command: observe}\n").unwrap();
        let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
        let store = Store::at(&root, "resume").unwrap();
        let mut run = store.create(&plan, "resume").unwrap();
        let mut host = Desktop {
            effect,
            text: String::new(),
            interrupted: false,
            calls: vec![],
            observation_failed: false,
        };
        execute(&plan, &mut run, &store, &mut host, None).unwrap();
        assert_eq!(run.status, Status::Paused);
        assert_eq!(
            run.steps[0].status,
            if effect.is_empty() {
                StepStatus::Pending
            } else {
                StepStatus::Uncertain
            }
        );
        let (saved, mut resumed) = store.load().unwrap();
        host.calls.clear();
        execute(&saved, &mut resumed, &store, &mut host, None).unwrap();
        assert_eq!(host.calls[0], "scene");
        if effect == "hel" {
            assert_eq!(resumed.status, Status::NeedsReview);
            assert_eq!(host.calls, ["scene", "result"]);
            host.calls.clear();
            execute(&saved, &mut resumed, &store, &mut host, None).unwrap();
            assert_eq!(host.calls, ["scene", "result"]);
            assert_eq!(host.text, "hel");
        } else {
            assert_eq!(resumed.status, Status::Completed);
            assert_eq!(host.text, "hello");
            assert_eq!(
                host.calls.iter().filter(|c| *c == "write").count(),
                usize::from(effect.is_empty())
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn unknown_scene_blocks_even_a_previously_completed_effect() {
    let plan = actl_flow::parse("version: 1\nname: Resume\nsteps:\n - id: input\n   title: Input\n   resume_check: {command: scene}\n   run: {command: write}\n   verify: {command: result}\n").unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "resume").unwrap();
    let mut run = store.create(&plan, "resume").unwrap();
    run.status = Status::Paused;
    run.steps[0].status = StepStatus::Uncertain;
    let mut host = Desktop {
        effect: "hello",
        text: "hello".into(),
        interrupted: true,
        calls: vec![],
        observation_failed: true,
    };
    execute(&plan, &mut run, &store, &mut host, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    assert_eq!(run.reason.as_deref(), Some("resume_check_failed"));
    assert_eq!(host.calls, ["scene"]);
    assert_eq!(host.text, "hello");
    std::fs::remove_dir_all(root).unwrap();
}
