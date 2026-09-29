use actl_flow::inputs::resolve;

const TEMPLATE: &str = r#"
version: 1
name: inputs
inputs:
  app: {}
  text: {default: 'literal ${unchanged}'}
steps:
  - id: read
    title: Read
    run:
      command: get
      args: [{input: text}, --app, {input: app}]
"#;

#[test]
fn resolves_whole_arguments_and_persists_only_concrete_values() {
    let plan = resolve(TEMPLATE, Some("app: 'a b 中文'\n")).unwrap();
    assert_eq!(
        plan.steps[0].run.args,
        ["literal ${unchanged}", "--app", "a b 中文"]
    );
    let saved = serde_json::to_string(&plan).unwrap();
    assert_eq!(actl_flow::parse(&saved).unwrap(), plan);
    // No later dependency on the input source, nor evaluation of supplied syntax.
    let injection = "text: '$(calc); --physical'\napp: test";
    let other = resolve(TEMPLATE, Some(injection)).unwrap();
    assert_eq!(other.steps[0].run.args[0], "$(calc); --physical");
    assert_eq!(other.steps[0].run.args.len(), 3);
}

#[test]
fn rejects_missing_unknown_duplicate_and_non_string_inputs() {
    for input in [
        None,
        Some("app: 42"),
        Some("app: x\nextra: y"),
        Some("app: x\napp: y"),
        Some("app: null"),
    ] {
        assert!(resolve(TEMPLATE, input).is_err(), "{input:?}");
    }
    for source in [
        TEMPLATE.replace("default: 'literal ${unchanged}'", "default: null"),
        TEMPLATE.replace("input: app", "input: absent"),
        TEMPLATE.replace("input: app", "input: app, extra: x"),
        TEMPLATE.replace("default:", "unknown:"),
    ] {
        assert!(resolve(&source, Some("app: x")).is_err());
    }
    assert!(resolve(TEMPLATE, Some(&format!("app: '{}'", "x".repeat(16385)))).is_err());
}

#[test]
fn all_check_phases_resolve_and_contract_is_stable() {
    let source = include_str!("golden/input_template_v1.json");
    let plan = resolve(source, None).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/input_resolved_v1.json")).unwrap();
    assert_eq!(serde_json::to_value(plan).unwrap(), expected);
}
