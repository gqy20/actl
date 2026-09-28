mod support;

#[test]
fn preview_cannot_be_used_as_a_persisted_action() {
    let path = std::env::temp_dir().join(format!("actl-preview-flow-{}.json", std::process::id()));
    std::fs::write(&path, r#"{"version":1,"name":"preview","steps":[{"id":"p","title":"p","run":{"command":"preview","args":["id:a","--app","unused"]},"verify":{"command":"verify","args":["--exists","id:a","--app","unused"]}}]}"#).unwrap();
    let out = support::IsolatedCommand::new()
        .command
        .args(["flow-validate", "--file"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(out.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["error"]["code"], "PROTOCOL");
    assert!(
        value["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not allowed")
    );
}

#[test]
fn preview_rejects_unbound_refs_before_touching_desktop() {
    let out = support::IsolatedCommand::new()
        .command
        .args(["preview", "@e1", "--app", "unused"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let mut value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    value.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        value,
        serde_json::from_str::<serde_json::Value>(include_str!("golden/preview_unbound.json"))
            .unwrap()
    );
}

#[test]
fn preview_help_and_invalid_options_are_offline() {
    let out = support::IsolatedCommand::new()
        .command
        .args(["preview", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for needle in [
        "Usage:",
        "--app",
        "--message",
        "--duration",
        "--candidate",
        "Example:",
        "Errors:",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    for args in [
        vec!["preview", "id:a", "--app", ""],
        vec!["preview", "id:a", "--app", "   "],
        vec!["preview", "xy:1,2", "--app", "unused"],
        vec!["preview", "id:a", "--app", "unused", "--duration", "0"],
        vec!["preview", "id:a", "--app", "unused", "--duration", "30001"],
        vec![
            "preview",
            "id:a",
            "--app",
            "unused",
            "--message",
            "bad\nmessage",
        ],
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["error"]["code"], "PROTOCOL");
    }
}

#[test]
fn candidate_selection_requires_distinct_refs_from_one_snapshot() {
    for args in [
        vec!["preview", "@e1", "--candidate", "@e2", "--app", "unused"],
        vec![
            "preview",
            "@e1",
            "--candidate",
            "@e1",
            "--snapshot",
            "s123",
            "--app",
            "unused",
        ],
        vec![
            "preview",
            "id:a",
            "--candidate",
            "@e2",
            "--snapshot",
            "s123",
            "--app",
            "unused",
        ],
        vec![
            "preview",
            "@e1",
            "--candidate",
            "xy:1,2",
            "--snapshot",
            "s123",
            "--app",
            "unused",
        ],
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args(args)
            .output()
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["error"]["code"], "PROTOCOL", "{v}");
    }
    let mut cmd = support::IsolatedCommand::new();
    cmd.command
        .args(["preview", "@e1", "--snapshot", "s123", "--app", "unused"]);
    for index in 2..=21 {
        cmd.command.args(["--candidate", &format!("@e{index}")]);
    }
    let out = cmd.command.output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL", "{v}");
}
