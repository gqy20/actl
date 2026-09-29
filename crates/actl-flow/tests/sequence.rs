#[test]
fn later_handoff_cannot_mark_an_already_started_group_pending() {
    let error = actl_core::CtlError::with_evidence(
        actl_core::ErrorCode::NotActionable,
        "handoff",
        serde_json::json!({"stage":"handoff","action_started":false}),
    );
    let error = actl_flow::sequence::failure(error, true, 1);
    assert_eq!(error.evidence.as_ref().unwrap()["action_started"], true);
    assert_eq!(error.evidence.as_ref().unwrap()["sequence_index"], 1);
}

#[test]
fn groups_resolve_inputs_and_freeze_as_concrete_calls() {
    let source = "version: 1\nname: group\ninputs: {app: {}}\nsteps:\n - id: edit\n   title: Edit\n   run:\n     sequence:\n       - {command: press, args: [ctrl+a, --app, {input: app}]}\n       - {command: type, args: ['id:edit', 'literal spaces', --app, {input: app}]}\n   verify: {command: verify, args: [--exists, 'id:edit']}\n";
    let plan = actl_flow::inputs::resolve(source, Some("app: unique")).unwrap();
    let calls = actl_flow::sequence::decode(&plan.steps[0].run)
        .unwrap()
        .unwrap();
    assert_eq!(
        calls[1].args,
        ["id:edit", "literal spaces", "--app", "unique"]
    );
    assert_eq!(
        actl_flow::parse(&serde_json::to_string(&plan).unwrap()).unwrap(),
        plan
    );
    for bad in [
        source.replace("sequence:", "command: status\n     sequence:"),
        source.replace("command: press", "command: workflow-sequence"),
    ] {
        assert!(actl_flow::inputs::resolve(&bad, Some("app: unique")).is_err());
    }
}
