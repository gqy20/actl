mod support;

#[test]
fn guarded_input_flags_are_discoverable_and_snapshot_requires_target() {
    for (command, flag) in [
        ("type", "--verify-value"),
        ("press", "--target"),
        ("press", "--snapshot"),
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args([command, "--help"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains(flag));
    }
    let out = support::IsolatedCommand::new()
        .command
        .args(["press", "enter", "--snapshot", "s123", "--app", "unused"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--target"));
}

#[test]
fn verify_value_help_documents_line_ending_fold() {
    // spec:换行三种表示(\r、\r\n、\n)比较前折叠为 \n——语义为行内容
    // 精确、表示无关(docs/06 §type;Store 记事本 ValuePattern 实测报孤立 \r)。
    let out = support::IsolatedCommand::new()
        .command
        .args(["type", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(help.contains("line endings fold to"), "help: {help}");
}

#[test]
fn launch_readiness_options_precede_the_program() {
    let out = support::IsolatedCommand::new()
        .command
        .args(["launch", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    assert!(help.contains("--wait-window"));
    assert!(help.contains("--timeout"));
    let invalid = support::IsolatedCommand::new()
        .command
        .args(["launch", "--timeout", "500", "notepad"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
}
