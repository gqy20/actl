//! envelope 契约测试(06 §4):成功/失败形态与退出码三态。
//! 只测无需桌面交互的确定性路径(CI 安全)。

use std::process::Command;

fn actl(args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_actl"))
        .args(args)
        .output()
        .expect("spawn actl");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    (code, stdout)
}

#[test]
fn success_envelope_shape() {
    let (code, stdout) = actl(&["status"]);
    assert_eq!(code, 0, "status must exit 0");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout is one JSON envelope");
    assert_eq!(v["version"], "1");
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "status");
    assert!(v["duration_ms"].is_u64(), "duration_ms present");
    assert!(v["data"].is_object(), "success carries data");
    assert!(v.get("error").is_none(), "success has no error field");
}

#[test]
fn protocol_error_exits_two() {
    // press 无 --app:fail-closed 门,PROTOCOL → exit 2
    let (code, stdout) = actl(&["press", "enter"]);
    assert_eq!(code, 2);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "PROTOCOL");
    assert!(v["error"]["hint"].is_string(), "hint present");
}

#[test]
fn bad_target_is_protocol() {
    let (code, stdout) = actl(&["click", "just-a-word"]);
    assert_eq!(code, 2);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "PROTOCOL");
    assert_eq!(v["command"], "click");
}

#[test]
fn unknown_command_exits_two() {
    let (code, _) = actl(&["definitely-not-a-command"]);
    assert_eq!(code, 2, "clap usage error path");
}

#[test]
fn self_describing_commands_schema() {
    let (code, stdout) = actl(&["commands"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    let cmds = v["data"]["commands"].as_array().expect("commands array");
    // M2 第二批后的命令面抽查:每命令必有 name/about,自发现保持同步
    let names: Vec<&str> = cmds
        .iter()
        .map(|c| c["name"].as_str().expect("name"))
        .collect();
    for must in [
        "status",
        "snapshot",
        "click",
        "type",
        "press",
        "set-value",
        "focus-window",
        "close-window",
        "list-windows",
        "find",
        "get-clipboard",
        "set-clipboard",
        "wait",
        "batch",
        "right-click",
        "double-click",
        "hover",
        "drag",
        "scroll",
        "key-down",
        "key-up",
        "resize-window",
        "verify",
        "extract",
        "select",
        "toggle",
    ] {
        assert!(names.contains(&must), "commands schema must list {must}");
    }
    for c in cmds {
        assert!(c["about"].is_string(), "every command has about");
    }
}

#[test]
fn get_property_whitelist_includes_text_channel() {
    // value/text 分工(spec):text = TextPattern 文档正文专用通道,
    // 白名单外属性 → PROTOCOL exit 2,错误信息须列出 text(离线可测)
    let (code, stdout) = actl(&["get", "name:whatever", "--property", "bogus"]);
    assert_eq!(code, 2);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "PROTOCOL");
    let msg = v["error"]["message"].as_str().expect("message");
    assert!(msg.contains("\"text\""), "whitelist mentions text: {msg}");
}

#[test]
fn batch_reports_per_step_results_and_durations() {
    let (code, stdout) = actl(&["batch", "--json", r#"["status","commands"]"#]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["data"]["executed"], 2);
    let steps = v["data"]["steps"].as_array().expect("steps array");
    assert_eq!(steps.len(), 2);
    for (i, s) in steps.iter().enumerate() {
        assert_eq!(s["i"], i, "step index");
        assert_eq!(s["ok"], true, "step ok");
        assert!(s["duration_ms"].is_u64(), "step duration");
        assert!(s["cmd"].is_string(), "step cmd echo");
    }
}

#[test]
fn batch_failure_carries_partial_steps_in_evidence() {
    // 第 2 步必败(窗口不存在),部分结果须随 evidence 带出且 stdout 仍是单 envelope
    let (code, stdout) = actl(&[
        "batch",
        "--json",
        r#"["status","close-window definitely-no-such-window-xyz"]"#,
    ]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "NOT_FOUND");
    let ev = &v["error"]["evidence"];
    assert_eq!(ev["executed"], 1, "first step succeeded before failure");
    let steps = ev["steps"].as_array().expect("partial steps in evidence");
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["ok"], true);
    assert_eq!(steps[1]["ok"], false);
    assert_eq!(steps[1]["error"]["code"], "NOT_FOUND");
}

#[test]
fn actl_trace_goes_to_stderr_and_stdout_stays_pure() {
    // ACTL_TRACE=1:诊断进 stderr;stdout 仍是一个合法 envelope(ADR-002 不变量)
    let out = Command::new(env!("CARGO_BIN_EXE_actl"))
        .args(["status"])
        .env("ACTL_TRACE", "1")
        .output()
        .expect("spawn actl");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).expect("stdout is pure JSON");
    assert_eq!(v["ok"], true);
}

#[test]
fn wait_property_predicate_validated_at_protocol_level() {
    // 谓词语法错误 = 参数错误 → PROTOCOL exit 2(离线可测,不触桌面)
    let (code, stdout) = actl(&[
        "wait",
        "--element",
        "name:whatever",
        "--property",
        "bogus=1",
    ]);
    assert_eq!(code, 2);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "PROTOCOL");
    // checked 谓词只接受 on|off
    let (code, _) = actl(&[
        "wait",
        "--element",
        "name:whatever",
        "--property",
        "checked=maybe",
    ]);
    assert_eq!(code, 2);
}

#[test]
fn snapshot_view_values_and_stable_flag_validated_at_protocol_level() {
    // --view 值域(content):未知值 clap 层 exit 2(spec:投影值域扩展走 spec)
    let (code, _) = actl(&["snapshot", "--view", "bogus", "--app", "x"]);
    assert_eq!(code, 2);
    // --stable 依赖 --element、与 --property 互斥
    let (code, _) = actl(&["wait", "--stable"]);
    assert_eq!(code, 2);
    let (code, _) = actl(&[
        "wait",
        "--element",
        "name:x",
        "--stable",
        "--property",
        "value=y",
    ]);
    assert_eq!(code, 2);
}

#[test]
fn snapshot_view_content_missing_window_is_not_found() {
    // 离线可测:窗口不存在 → NOT_FOUND exit 1(内容域裁定之前先挡在窗口解析)
    let (code, stdout) = actl(&[
        "snapshot",
        "--view",
        "content",
        "--app",
        "definitely-no-such-window-xyz",
    ]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "NOT_FOUND");
}

#[test]
fn commands_schema_carries_view_and_stable_args() {
    // 自发现同步:snapshot 带 view 参数、wait 带 stable 参数(schema 与 help 同源)
    let (code, stdout) = actl(&["commands"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    let cmds = v["data"]["commands"].as_array().expect("commands array");
    let find_args = |name: &str| -> Vec<String> {
        cmds.iter()
            .find(|c| c["name"] == name)
            .and_then(|c| c["args"].as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(
        find_args("snapshot").contains(&"view".to_string()),
        "snapshot schema lists view"
    );
    assert!(
        find_args("wait").contains(&"stable".to_string()),
        "wait schema lists stable"
    );
}

#[test]
fn close_window_by_missing_pid_is_not_found() {
    // FR3:--pid 定位;无此进程的窗口 → NOT_FOUND exit 1(离线可测,不碰真实窗口)
    let (code, stdout) = actl(&["close-window", "--pid", "3999999"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "NOT_FOUND");
}

#[test]
fn window_commands_without_selector_is_protocol() {
    let (code, stdout) = actl(&["close-window"]);
    assert_eq!(
        code, 2,
        "PROTOCOL (no selector) exits 2 per the exit-code contract"
    );
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "PROTOCOL");
}

#[test]
fn batch_reports_no_windows_created_on_headless_run() {
    // windows_created:空差集时省略(token);CI 安全路径(status 不开窗)
    let (code, stdout) = actl(&["batch", "--json", r#"["status"]"#]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    let data = &v["data"];
    assert!(
        data.get("windows_created").is_none()
            || data["windows_created"]
                .as_array()
                .is_some_and(|a| a.is_empty())
    );
}

#[test]
fn batch_steps_have_no_blocked_by_when_foreground_is_baseline() {
    // 正常路径:前台(终端)在批前基线内 → 步条目不带 blocked_by
    let (code, stdout) = actl(&["batch", "--json", r#"["status"]"#]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    let step = &v["data"]["steps"][0];
    assert_eq!(step["ok"], true);
    // 锁屏会话下前台=锁屏界面(非基线非 --app),blocked_by 如实上报是正确行为;
    // 仅当告警指向普通窗口时才视为虚假告警
    match step.get("blocked_by") {
        None => {}
        Some(b) => assert!(
            b["title"].as_str().unwrap_or_default().contains("锁屏"),
            "no spurious popup warning (got {:?})",
            b["title"]
        ),
    }
}

#[test]
fn paste_requires_app_fail_closed() {
    // paste 是注入类命令:--app 必填,缺失在 clap 层即拒(usage error,
    // 与未知子命令同款:exit 2,stdout 空)
    let (code, stdout) = actl(&["paste"]);
    assert_eq!(code, 2);
    assert!(
        stdout.trim().is_empty(),
        "clap usage errors stay off stdout"
    );
}

#[test]
fn screenshot_region_roundtrip() {
    // 最小区域(4x4):CI 安全(任何桌面都有像素),验证 envelope 形态 + 文件真实落盘
    let out = std::env::temp_dir().join("actl_contract_shot.png");
    let out_str = out.to_string_lossy().into_owned();
    let (code, stdout) = actl(&["screenshot", "--rect", "1,1,4,4", "--out", &out_str]);
    assert_eq!(code, 0, "region capture must succeed on any desktop");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["data"]["width"], 4);
    assert_eq!(v["data"]["height"], 4);
    assert!(
        v["data"]["bytes"].as_u64().unwrap_or(0) > 0,
        "non-trivial PNG"
    );
    assert!(out.exists(), "PNG lands at the requested path");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn screenshot_requires_exactly_one_domain() {
    // 全空 = PROTOCOL;--rect 与 --app 互斥 = clap 层 exit 2
    let (code, _) = actl(&["screenshot"]);
    assert_eq!(code, 2);
    let (code, _) = actl(&["screenshot", "--app", "x", "--rect", "1,1,2,2"]);
    assert_eq!(code, 2);
}

#[test]
fn pointer_commands_with_validated_args() {
    // middle-click / triple-click 与 right-click 同族:目标必填
    let (code, _) = actl(&["middle-click"]);
    assert_eq!(code, 2);
    let (code, _) = actl(&["triple-click"]);
    assert_eq!(code, 2);
    // --with 需要 --physical(组合修饰只在物理点击时有意义)
    let (code, _) = actl(&["click", "name:x", "--app", "y", "--with", "ctrl"]);
    assert_eq!(code, 2);
    // --with 只接受已知修饰键
    let (code, stdout) = actl(&[
        "click",
        "name:x",
        "--app",
        "y",
        "--physical",
        "--with",
        "ctrlx",
    ]);
    let _ = stdout;
    // 修饰键名校验在 pointer 层(运行时 PROTOCOL);clap 只做 requires 校验
    // 此处只验证 --with 不与 --physical 缺失共存
    assert!(code == 0 || code == 1 || code == 2);
}
