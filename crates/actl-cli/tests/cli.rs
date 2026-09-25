//! bin 集成测试:对真实 `actl` 二进制断言 envelope 契约(06 §4)与退出码。
//! 这是命令输出形状的 golden 起点;协议变更(spec 提交)时同步更新断言。

use std::process::Command;

fn actl(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_actl"))
        .args(args)
        .output()
        .expect("spawn actl binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn parse_envelope(stdout: &str) -> serde_json::Value {
    serde_json::from_str(stdout).expect("stdout must be a single JSON envelope")
}

#[test]
fn status_emits_success_envelope() {
    let (code, stdout, _) = actl(&["status"]);
    assert_eq!(code, 0);
    let v = parse_envelope(&stdout);
    assert_eq!(v["version"], "1");
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "status");
    assert!(v["duration_ms"].is_u64());
    assert!(v["data"]["version"].is_string());
    assert_eq!(v["data"]["protocol"], "1");
    assert!(v.get("snapshot_id").is_none());
}

#[test]
fn commands_lists_the_command_surface() {
    let (code, stdout, _) = actl(&["commands"]);
    assert_eq!(code, 0);
    let v = parse_envelope(&stdout);
    let names: Vec<&str> = v["data"]["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(names.contains(&"status"), "missing status in {names:?}");
    assert!(names.contains(&"commands"), "missing commands in {names:?}");
    for c in v["data"]["commands"].as_array().unwrap() {
        assert!(
            c["about"].is_string(),
            "every command must carry about: {c}"
        );
    }
}

#[test]
fn unknown_command_exits_2_with_human_stderr() {
    let (code, _, stderr) = actl(&["definitely-not-a-command"]);
    assert_eq!(code, 2);
    assert!(!stderr.is_empty());
}

#[test]
fn version_is_the_documented_stdout_exception() {
    let (code, stdout, _) = actl(&["--version"]);
    assert_eq!(code, 0);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "--version must stay human text, not JSON"
    );
}
