mod support;

#[test]
fn local_snapshot_requires_explicit_window_golden() {
    let out = support::IsolatedCommand::new()
        .command
        .args(["snapshot", "--within", r#"uia:{"match":{"id":"panel"}}"#])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let mut value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        value,
        serde_json::from_str::<serde_json::Value>(include_str!(
            "golden/local_snapshot_unbound.json"
        ))
        .unwrap()
    );
}

#[test]
fn local_snapshot_rejects_invalid_scope_and_budgets_offline() {
    for args in [
        vec!["--within", "@e1"],
        vec!["--within", "id:panel"],
        vec!["--within", "xy:1,2"],
        vec![
            "--within",
            r#"uia:{"match":{"id":"panel"}}"#,
            "--depth",
            "41",
        ],
        vec![
            "--within",
            r#"uia:{"match":{"id":"panel"}}"#,
            "--limit",
            "0",
        ],
        vec![
            "--within",
            r#"uia:{"match":{"id":"panel"}}"#,
            "--limit",
            "5001",
        ],
        vec!["--depth", "1"],
        vec!["--limit", "1"],
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args(["snapshot", "--app", "unused"])
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["error"]["code"], "PROTOCOL", "{args:?}: {v}");
    }
}

#[test]
fn local_snapshot_help_explains_binding() {
    let out = support::IsolatedCommand::new()
        .command
        .args(["snapshot", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for term in [
        "Usage:",
        "--within",
        "--depth",
        "--limit",
        "Example:",
        "Errors:",
        "snapshot_id",
    ] {
        assert!(text.contains(term), "{term}: {text}");
    }
}
