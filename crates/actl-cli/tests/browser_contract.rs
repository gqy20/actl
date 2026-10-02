//! browser-* 命令族契约测试(docs/72 §9 spec 面)。
//! 只测无需浏览器的确定性路径(CI 安全):help 四要素、参数校验、
//! 未知会话/坏可执行文件的错误 envelope golden。真机行为走场景验收轨(docs/72 §10)。

mod support;

use serde_json::Value;

fn actl(args: &[&str]) -> (i32, String) {
    let out = support::IsolatedCommand::new()
        .command
        .args(args)
        .output()
        .expect("spawn actl");
    let code = out.status.code().unwrap_or(-1);
    (code, String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 去掉 duration_ms 后与 golden 全量比对(与 cli_contract 同纪律)。
fn normalize(mut v: Value) -> Value {
    v.as_object_mut().unwrap().remove("duration_ms");
    v
}

#[test]
fn help_has_four_elements() {
    for cmd in [
        "browser-launch",
        "browser-navigate",
        "browser-read",
        "browser-click",
        "browser-close",
    ] {
        let out = support::IsolatedCommand::new()
            .command
            .args([cmd, "--help"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{cmd} --help must exit 0");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("Usage:"), "{cmd} help lacks Usage");
        assert!(text.contains("Example:"), "{cmd} help lacks Example");
        assert!(text.contains("Errors:"), "{cmd} help lacks Errors");
    }
}

#[test]
fn commands_schema_contains_browser_family() {
    let (code, stdout) = actl(&["commands"]);
    assert_eq!(code, 0);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    let names: Vec<&str> = v["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for cmd in [
        "browser-launch",
        "browser-navigate",
        "browser-read",
        "browser-click",
        "browser-close",
    ] {
        assert!(names.contains(&cmd), "commands schema lacks {cmd}");
    }
}

#[test]
fn unknown_session_maps_to_not_found() {
    let (code, stdout) = actl(&["browser-read", "--session", "nope123", "--css", "div"]);
    assert_eq!(code, 1);
    assert_eq!(
        normalize(serde_json::from_str(&stdout).unwrap()),
        serde_json::from_str::<Value>(include_str!("golden/browser_unknown_session.json")).unwrap()
    );
}

#[test]
fn missing_selector_is_protocol() {
    // 06 §4 退出码三态:PROTOCOL 走 2(参数/协议错误),envelope 仍在 stdout
    let (code, stdout) = actl(&["browser-read", "--session", "nope123"]);
    assert_eq!(code, 2);
    assert_eq!(
        normalize(serde_json::from_str(&stdout).unwrap()),
        serde_json::from_str::<Value>(include_str!("golden/browser_missing_selector.json"))
            .unwrap()
    );
}

#[test]
fn mutually_exclusive_selectors_rejected_by_clap() {
    assert_eq!(
        actl(&[
            "browser-read",
            "--session",
            "x",
            "--css",
            "div",
            "--xpath",
            "//div"
        ])
        .0,
        2
    );
    assert_eq!(
        actl(&[
            "browser-click",
            "--session",
            "x",
            "--css",
            "div",
            "--xpath",
            "//div"
        ])
        .0,
        2
    );
}

#[test]
fn launch_rejects_missing_or_relative_executable() {
    let (code, stdout) = actl(&[
        "browser-launch",
        "--browser-executable",
        "Z:/definitely/not/here/msedge.exe",
    ]);
    assert_eq!(code, 2);
    assert_eq!(
        normalize(serde_json::from_str(&stdout).unwrap()),
        serde_json::from_str::<Value>(include_str!("golden/browser_launch_bad_exe.json")).unwrap()
    );
    let (code, _) = actl(&["browser-launch", "--browser-executable", "msedge.exe"]);
    assert_eq!(code, 2); // 相对路径同样 PROTOCOL
}

#[test]
fn launch_rejects_bad_profile_name_and_url() {
    let exe = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
    let (code, stdout) = actl(&[
        "browser-launch",
        "--browser-executable",
        exe,
        "--profile",
        "../evil",
    ]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
    let (code, stdout) = actl(&[
        "browser-launch",
        "--browser-executable",
        exe,
        "--url",
        "ftp://bad",
    ]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
}

#[test]
fn launch_rejects_non_bare_proxy_values() {
    let exe = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
    for bad in [
        "http://127.0.0.1:7890",
        "127.0.0.1",
        "127.0.0.1:0",
        "127.0.0.1:99999",
    ] {
        let (code, stdout) = actl(&[
            "browser-launch",
            "--browser-executable",
            exe,
            "--proxy",
            bad,
        ]);
        assert_eq!(code, 2, "proxy {bad} must be rejected");
        let v: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(v["error"]["code"], "PROTOCOL");
    }
}

#[test]
fn navigate_rejects_bad_url_before_touching_the_browser() {
    let (code, stdout) = actl(&[
        "browser-navigate",
        "--session",
        "nope123",
        "--url",
        "javascript:alert(1)",
    ]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
}
