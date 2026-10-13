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

#[test]
fn drag_path_and_key_duration_arg_contract() {
    let run = |args: &[&str]| -> (i32, String) {
        let out = support::IsolatedCommand::new()
            .command
            .args(args)
            .output()
            .unwrap();
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    };
    // --path 解析(handler 层 PROTOCOL envelope,发生在任何桌面交互前)
    let (code, stdout) = run(&[
        "drag", "xy:10,10", "xy:20,20", "--app", "w", "--path", "abc",
    ]);
    assert_eq!(code, 2);
    assert!(stdout.contains("PROTOCOL"), "{stdout}");
    // xy 对必须带 --app
    let (code, stdout) = run(&["drag", "xy:10,10", "xy:20,20", "--path", "15,15"]);
    assert_eq!(code, 2);
    assert!(stdout.contains("--app"), "{stdout}");
    // duration 边界走 clap(仅退出码)
    assert_eq!(
        run(&["key-down", "shift", "--app", "w", "--duration", "10"]).0,
        2
    );
    assert_eq!(
        run(&["key-down", "shift", "--app", "w", "--duration", "70000"]).0,
        2
    );
    assert_eq!(
        run(&["key-down", "shift", "--app", "w", "--duration", "x"]).0,
        2
    );
    // key-up 不接受 duration
    let (code, stdout) = run(&["key-up", "shift", "--app", "w", "--duration", "500"]);
    assert_eq!(code, 2);
    assert!(stdout.contains("PROTOCOL") || stdout.is_empty());
}

#[test]
fn parse_waypoints_unit() {
    // interact::parse_waypoints 的纯逻辑(经由 bin 内单测不可,走行为断言代替:
    // 多点合法组合应在参数层通过,进入桌面路径前由 --app 守卫拦截)
    let out = support::IsolatedCommand::new()
        .command
        .args([
            "drag", "xy:1,1", "xy:2,2", "--path", "1,2", "--path", "3,4", "--path", "5,6",
        ])
        .output()
        .unwrap();
    // 无 --app:坐标对守卫拒绝(PROTOCOL),证明多点解析本身通过
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(2));
    assert!(stdout.contains("--app"), "{stdout}");
}

#[test]
fn set_value_verify_value_and_verify_file_discover_are_documented() {
    // spec:set-value 读回核验与 type 对齐(空白读回诚实失败——provider 背书
    // 对话框读不回,走 type --verify-value);verify-file 目录发现形态按过滤器
    // 补扩展名的产物断言(docs/06;2026-10-12 Zotero 导出轮实证)。
    for (command, flag, needle) in [
        (
            "set-value",
            "--verify-value",
            "blank readback fails honestly",
        ),
        ("verify-file", "--discover", "type-filter extension"),
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args([command, "--help"])
            .output()
            .unwrap();
        let help = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success());
        assert!(help.contains(flag), "help: {help}");
        assert!(help.contains(needle), "help: {help}");
    }
}

#[test]
fn interaction_ergonomics_round_surface_in_help() {
    // 2026-10-12 交互轮:get rect 属性、指针族 --snapshot、front-report --app
    // 旗标、close-window instance 配对、find --id 动作路径说明。
    for (command, needle) in [
        ("get", "rect"),
        ("right-click", "--snapshot"),
        ("hover", "--snapshot"),
        ("double-click", "--snapshot"),
        ("front-report", "--app"),
        ("close-window", "instance"),
        ("find", "click id:<X>"),
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args([command, "--help"])
            .output()
            .unwrap();
        let help = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success());
        assert!(
            help.contains(needle),
            "{command} help missing {needle}: {help}"
        );
    }
}

#[test]
fn verify_closed_without_instance_still_decisive_on_dead_pid() {
    // ⑤ --instance 可选化:pid 消失 → 决定性通过(与 instance 无关);
    // 冒烟断言用必然不存在的 pid,离线可跑。
    let out = support::IsolatedCommand::new()
        .command
        .args(["verify-closed", "--hwnd", "12345", "--pid", "4294967290"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["data"]["verified"], true);
    assert_eq!(result["data"]["original_instance_gone"], true);
}
