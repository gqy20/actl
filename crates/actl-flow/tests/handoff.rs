use actl_core::handoff::Approval;

#[test]
fn task_config_is_strict_and_round_trips_contract() {
    let config =
        actl_flow::task_config::parse("version: 1\nhandoff:\n  approval: once_per_run").unwrap();
    assert_eq!(config.handoff.approval, Approval::OncePerRun);
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("golden/task_handoff_v1.json")).unwrap();
    assert_eq!(serde_json::to_value(config).unwrap(), golden);
    for yaml in [
        "version: 2\nhandoff: {approval: once_per_run}",
        "version: 1\nhandoff: {approval: never}",
        "version: 1\nhandoff: {approval: once_per_run, authorized: true}",
        "version: 1\nversion: 1\nhandoff: {approval: once_per_run}",
        "version: 1\nhandoff: {approval: once_per_run}\nextra: true",
    ] {
        assert!(actl_flow::task_config::parse(yaml).is_err(), "{yaml}");
    }
}

#[test]
fn flow_persists_policy_and_omission_keeps_expiring_default() {
    let base = "version: 1\nname: policy\nsteps:\n  - id: observe\n    title: Observe\n    run: {command: status}\n";
    assert!(actl_flow::parse(base).unwrap().handoff.is_none());
    let plan = actl_flow::parse(&format!("{base}handoff: {{approval: once_per_run}}\n")).unwrap();
    assert_eq!(plan.handoff.unwrap().approval, Approval::OncePerRun);
    assert_eq!(
        actl_flow::parse(&serde_json::to_string(&plan).unwrap()).unwrap(),
        plan
    );
}
