use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
use serde_json::{Value, json};

fn plan() -> Plan {
    parse(
        r#"version: 1
name: data
steps:
  - id: source
    title: Read
    run: {command: read}
    outputs:
      text: {pointer: /text, type: string}
      count: {pointer: /count, type: number}
  - id: sink
    title: Write
    pause_before: true
    run:
      command: write
      args: [placeholder]
      bind: {"0": source.text}
    verify: {command: check}
"#,
    )
    .unwrap()
}
fn setup(p: &Plan) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "flow-data-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "data").unwrap();
    let run = store.create(p, "data").unwrap();
    (root, store, run)
}
struct Fake {
    calls: Vec<Invocation>,
    value: Value,
    fail_save: Option<std::path::PathBuf>,
    valid: bool,
}
impl Host for Fake {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(call.clone());
        Ok(())
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
        self.execute(call)?;
        if let Some(path) = self.fail_save.take() {
            std::fs::create_dir(path).unwrap();
        }
        Ok(self.value.clone())
    }
    fn validate_step(&self, step: &Step) -> Result<(), CtlError> {
        if !self.valid && step.id == "sink" {
            Err(CtlError::protocol("invalid resolved arguments"))
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
fn host() -> Fake {
    Fake {
        calls: vec![],
        value: json!({"text":"hello 中文", "count":2}),
        fail_save: None,
        valid: true,
    }
}

#[test]
fn persist_resume_and_do_not_replay_completed_write() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut h = host();
    execute(&p, &mut run, &store, &mut h, None, None).unwrap();
    assert_eq!(run.status, Status::Paused);
    let (saved, mut run) = store.load().unwrap();
    assert_eq!(run.steps[0].outputs, json_map());
    h.value = json!({"text":"changed", "count":9});
    execute(&saved, &mut run, &store, &mut h, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(h.calls.iter().filter(|c| c.command == "read").count(), 1);
    assert_eq!(
        h.calls.iter().find(|c| c.command == "write").unwrap().args,
        ["hello 中文"]
    );
    execute(&saved, &mut run, &store, &mut h, None, None).unwrap();
    assert_eq!(h.calls.iter().filter(|c| c.command == "write").count(), 1);
    let mut actual = serde_json::to_value(&run).unwrap();
    actual["updated_ms"] = json!(0);
    actual["steps"][0]["outputs_captured_ms"] = json!(0);
    for step in actual["steps"].as_array_mut().unwrap() {
        step["started_ms"] = json!(0);
        step["finished_ms"] = json!(0);
    }
    let expected: Value =
        serde_json::from_str(include_str!("golden/data_completed_v1.json")).unwrap();
    assert_eq!(actual, expected);
    std::fs::remove_dir_all(root).unwrap();
}
fn json_map() -> std::collections::BTreeMap<String, Value> {
    [
        ("text".into(), json!("hello 中文")),
        ("count".into(), json!(2)),
    ]
    .into()
}
#[test]
fn missing_or_wrong_type_output_blocks_consumer() {
    for value in [
        json!({"count":2}),
        json!({"text":null,"count":2}),
        json!({"text":3,"count":2}),
    ] {
        let p = plan();
        let (root, store, mut run) = setup(&p);
        let mut h = host();
        h.value = value;
        execute(&p, &mut run, &store, &mut h, None, None).unwrap();
        assert_eq!(run.status, Status::Paused);
        assert!(run.steps[0].outputs.is_empty());
        assert_eq!(run.steps[0].error_code, Some(ErrorCode::Protocol));
        assert_eq!(h.calls.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn reject_forward_unknown_self_and_collection_bindings() {
    for reference in ["sink.text", "source.unknown", "unknown.text"] {
        let mut p = plan();
        p.steps[1].run.bind.insert("0".into(), reference.into());
        assert!(data::validate(&p).is_err());
    }
    let mut p = plan();
    p.steps[0].run.args = vec!["placeholder".into()];
    p.steps[0].run.bind.insert("0".into(), "source.text".into());
    assert!(data::validate(&p).is_err());
    let mut p = plan();
    p.steps[0].outputs.get_mut("text").unwrap().kind = data::Kind::Array;
    assert!(data::validate(&p).is_err());
}
#[test]
fn empty_string_is_valid_and_null_is_not_missing() {
    let p = plan();
    assert_eq!(
        data::extract(&p.steps[0], &json!({"text":"","count":0})).unwrap()["text"],
        ""
    );
    let mut step = p.steps[0].clone();
    step.outputs.clear();
    step.outputs.insert(
        "nil".into(),
        data::Output {
            pointer: "/nil".into(),
            kind: data::Kind::Null,
            max_age_ms: None,
        },
    );
    assert!(data::extract(&step, &json!({"nil":null})).unwrap()["nil"].is_null());
    assert!(data::extract(&step, &json!({})).is_err());
}
#[test]
fn invalid_resolved_value_or_host_preflight_never_writes() {
    for (text, valid) in [("--physical", true), ("@e3", true), ("fine", false)] {
        let p = plan();
        let (root, store, mut run) = setup(&p);
        let mut h = host();
        h.value = json!({"text":text,"count":1});
        h.valid = valid;
        execute(&p, &mut run, &store, &mut h, None, None).unwrap();
        assert_eq!(run.reason.as_deref(), Some("binding_validation_failed"));
        assert_eq!(h.calls.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn interrupted_read_without_outputs_is_read_again_even_if_verify_passes() {
    let mut p = plan();
    p.steps[0].verify = Some(Invocation {
        command: "check".into(),
        args: vec![],
        bind: Default::default(),
    });
    let (root, store, mut run) = setup(&p);
    run.steps[0].status = StepStatus::Verifying;
    store.save(&run).unwrap();
    let mut h = host();
    execute(&p, &mut run, &store, &mut h, None, None).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Completed);
    assert_eq!(run.steps[0].outputs, json_map());
    assert_eq!(h.calls.iter().filter(|c| c.command == "read").count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn failed_output_persistence_prevents_next_action() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    let mut h = host();
    h.fail_save = Some(store.dir.join("state.tmp"));
    assert!(execute(&p, &mut run, &store, &mut h, None, None).is_err());
    assert_eq!(h.calls.len(), 1);
    let (_, saved) = store.load().unwrap();
    assert_eq!(saved.steps[0].status, StepStatus::Running);
    assert!(saved.steps[0].outputs.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn corrupted_saved_outputs_and_oversize_are_rejected() {
    let p = plan();
    let (root, store, mut run) = setup(&p);
    run.steps[0].status = StepStatus::Completed;
    store.save(&run).unwrap();
    assert!(store.load().is_err());
    assert!(data::extract(&p.steps[0], &json!({"text":"x".repeat(16385),"count":2})).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn typed_values_escaped_pointers_and_budget_are_checked() {
    let mut p = plan();
    p.steps.truncate(1);
    p.steps[0].outputs.clear();
    for (name, kind, pointer) in [
        ("list", data::Kind::Array, "/a~1b"),
        ("obj", data::Kind::Object, "/obj"),
        ("flag", data::Kind::Boolean, "/flag"),
    ] {
        p.steps[0].outputs.insert(
            name.into(),
            data::Output {
                pointer: pointer.into(),
                kind,
                max_age_ms: None,
            },
        );
    }
    data::validate(&p).unwrap();
    let outputs = data::extract(&p.steps[0], &json!({"a/b":[],"obj":{},"flag":false})).unwrap();
    assert_eq!(outputs["list"], json!([]));
    assert_eq!(outputs["flag"], false);
    p.steps[0].outputs.get_mut("list").unwrap().pointer = "/bad~2".into();
    assert!(data::validate(&p).is_err());
    let mut p = plan();
    p.steps.truncate(1);
    p.steps[0].outputs.clear();
    let mut run = Run::new("budget", &p);
    for n in 0..5 {
        let key = format!("v{n}");
        p.steps[0].outputs.insert(
            key.clone(),
            data::Output {
                pointer: "".into(),
                kind: data::Kind::String,
                max_age_ms: None,
            },
        );
        run.steps[0].outputs.insert(key, json!("x".repeat(14000)));
    }
    assert!(data::check_saved(&p, &run).is_err());
}

#[test]
fn interrupted_read_with_saved_outputs_only_verifies() {
    let mut p = plan();
    p.steps[0].verify = Some(Invocation {
        command: "check".into(),
        args: vec![],
        bind: Default::default(),
    });
    let (root, store, mut run) = setup(&p);
    run.steps[0].status = StepStatus::Verifying;
    run.steps[0].outputs = json_map();
    store.save(&run).unwrap();
    let mut h = host();
    execute(&p, &mut run, &store, &mut h, None, None).unwrap();
    assert_eq!(run.steps[0].status, StepStatus::Completed);
    assert_eq!(h.calls.iter().filter(|c| c.command == "read").count(), 0);
    std::fs::remove_dir_all(root).unwrap();
}
