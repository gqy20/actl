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
        "browser-list",
        "browser-verify",
    ] {
        assert!(names.contains(&cmd), "commands schema lacks {cmd}");
    }
}

#[test]
fn browser_verify_requires_exactly_one_mode() {
    // 零模式:clap 放行,handler 出 PROTOCOL envelope
    let (code, stdout) = actl(&["browser-verify"]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
    // 组合冲突/缺依赖:clap 层拒绝(仅 stderr,无 envelope),断言退出码即可
    assert_eq!(actl(&["browser-verify", "--none", "--newest-alive"]).0, 2);
    assert_eq!(actl(&["browser-verify", "--alive"]).0, 2);
    assert_eq!(
        actl(&["browser-verify", "--gone", "--session", "x", "--alive"]).0,
        2
    );
}

#[test]
fn browser_list_reports_empty_without_sessions() {
    let (code, stdout) = actl(&["browser-list"]);
    assert_eq!(code, 0);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["data"]["sessions"].as_array().map(Vec::len), Some(0));
    assert_eq!(v["data"]["total"], 0);
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

#[test]
fn verify_selector_mode_requires_session() {
    // clap 层:--css 依赖 --session;组合互斥仅断言退出码(stderr 无 envelope)
    assert_eq!(actl(&["browser-verify", "--css", "#avatar"]).0, 2);
    assert_eq!(
        actl(&[
            "browser-verify",
            "--session",
            "x",
            "--css",
            "a",
            "--xpath",
            "b"
        ])
        .0,
        2
    );
    assert_eq!(actl(&["browser-verify", "--contains", "me"]).0, 2);
}

#[test]
fn flow_rejects_undeclared_persistent_profile() {
    let dir = std::env::temp_dir().join(format!("actl-browser-flow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let undeclared = dir.join("undeclared.yml");
    std::fs::write(
        &undeclared,
        "version: 1\nname: undeclared\nsteps:\n  - id: launch\n    title: t\n    run:\n      command: browser-launch\n      args: ['--browser-executable', 'C:/e/msedge.exe', '--profile', 'sneaky']\n    verify:\n      command: browser-verify\n      args: ['--newest-alive']\n",
    )
    .unwrap();
    let (code, stdout) = actl(&["flow-validate", "--file", undeclared.to_str().unwrap()]);
    assert_eq!(code, 2, "{stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
    assert!(v["error"]["message"].as_str().unwrap().contains("sneaky"));

    let declared = dir.join("declared.yml");
    std::fs::write(
        &declared,
        "version: 1\nname: declared\nresources:\n  browser_profiles:\n    - name: ok-profile\nsteps:\n  - id: launch\n    title: t\n    run:\n      command: browser-launch\n      args: ['--browser-executable', 'C:/e/msedge.exe', '--profile', 'ok-profile']\n    verify:\n      command: browser-verify\n      args: ['--newest-alive']\n",
    )
    .unwrap();
    let (code, stdout) = actl(&["flow-validate", "--file", declared.to_str().unwrap()]);
    assert_eq!(code, 0, "{stdout}");
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["data"]["resources"]["browser_profiles"][0], "ok-profile");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_rejects_unknown_property_offline() {
    let (code, stdout) = actl(&[
        "browser-read",
        "--session",
        "nope123",
        "--css",
        "#x",
        "--property",
        "innerhtml",
    ]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
    assert!(v["error"]["message"].as_str().unwrap().contains("allowed"));
}

#[test]
fn type_arg_surface_contract() {
    // clap 层:--text 必填、选择器互斥
    assert_eq!(
        actl(&["browser-type", "--session", "x", "--css", "#i"]).0,
        2
    );
    assert_eq!(
        actl(&[
            "browser-type",
            "--session",
            "x",
            "--css",
            "#i",
            "--xpath",
            "//i",
            "--text",
            "t"
        ])
        .0,
        2
    );
    // handler 层:缺选择器 PROTOCOL envelope
    let (code, stdout) = actl(&["browser-type", "--session", "nope123", "--text", "t"]);
    assert_eq!(code, 2);
    let v: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["error"]["code"], "PROTOCOL");
}

#[test]
fn snapshot_keywords_arg_contract() {
    // clap 层:--keywords 与 --view 互斥
    assert_eq!(
        actl(&["snapshot", "--keywords", "save", "--view", "content"]).0,
        2
    );
    // handler 层校验发生在窗口捕获前?——keywords 解析在 capture 之后、finish 之前,
    // 但空/超限词表在校验首步,无桌面依赖路径不可离线断言;此处只锁互斥面
}

#[test]
fn all_flag_documents_list_semantics_in_help() {
    // --all 枚举形态(2026-10-11 spec,docs/73 round7 痛点收口):零命中=
    // 诚实空集(非 NOT_FOUND)、--limit 裁返回面保真 total——进 help 四要素。
    let (code, stdout) = actl(&["browser-read", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--all"), "{stdout}");
    assert!(stdout.contains("empty set, not NOT_FOUND"), "{stdout}");
    assert!(stdout.contains("--limit"), "{stdout}");
    assert!(stdout.contains("Errors:"), "{stdout}");
}

#[test]
fn limit_requires_all_is_clap_rejected() {
    // --limit 是 --all 的从属参数:单独出现走 clap usage error(exit 2)
    assert_eq!(
        actl(&[
            "browser-read",
            "--session",
            "x",
            "--css",
            "div",
            "--limit",
            "10"
        ])
        .0,
        2
    );
}

#[test]
fn deep_flag_documents_shadow_piercing_semantics() {
    // --deep 穿透开放 shadow DOM(2026-10-11 spec,B 站 <bili-comments> 实证
    // 驱动):开 shadow 递归、闭 shadow 如实边界、预算触顶 traversal_capped
    // ——进 help 四要素;--limit 不从属于 --deep(可与唯一读组合)。
    let (code, stdout) = actl(&["browser-read", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--deep"), "{stdout}");
    assert!(stdout.contains("OPEN shadow roots"), "{stdout}");
    assert!(stdout.contains("traversal_capped"), "{stdout}");
    assert!(stdout.contains("closed shadow roots"), "{stdout}");
    // --deep 单独合法(唯一读穿透),不要求 --all
    let (code, _) = actl(&["browser-read", "--session", "x", "--css", "div", "--deep"]);
    assert_eq!(code, 1, "unknown session is NOT_FOUND(1), not clap error");
}

#[test]
fn no_popups_flag_documents_popup_predicate_semantics() {
    // --no-popups 页内弹层断言(2026-10-11 spec):默认选择器/可见性口径/
    // ASSERTION_FAILED→false 条件映射/--popup-selector 收窄——进 help 四要素。
    let (code, stdout) = actl(&["browser-verify", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--no-popups"), "{stdout}");
    assert!(stdout.contains("dialog[open]"), "{stdout}");
    assert!(stdout.contains("FALSE condition"), "{stdout}");
    assert!(stdout.contains("--popup-selector"), "{stdout}");
    assert!(stdout.contains("Errors:"), "{stdout}");
    // --popup-selector 从属于 --no-popups(requires 链,clap 拒绝孤立使用)
    let (code2, _) = actl(&[
        "browser-verify",
        "--session",
        "x",
        "--popup-selector",
        "div",
    ]);
    assert_eq!(code2, 2, "orphan --popup-selector is a clap usage error");
}

#[test]
fn no_popups_unknown_session_is_not_found() {
    // 会话不存在 → NOT_FOUND(1),不是 clap 错误(形态与 --css 谓词同规)
    let (code, stdout) = actl(&["browser-verify", "--session", "nope", "--no-popups"]);
    assert_eq!(code, 1, "{stdout}");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("envelope json");
    assert_eq!(v["error"]["code"], "NOT_FOUND", "{v}");
}

#[test]
fn wait_for_flag_documents_settle_predicate() {
    // --wait-for N 沉降谓词(2026-10-11 spec):命中数≥N 才返回,内建 500ms
    // 轮询,超时 TIMEOUT 带实测命中数——异步渲染页面的读段自稳原语。
    // 与 --all/唯一读通用(无 requires 约束)。
    let (code, stdout) = actl(&["browser-read", "--help"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("--wait-for"), "{stdout}");
    assert!(stdout.contains("N matches"), "{stdout}");
    assert!(stdout.contains("settle"), "{stdout}");
}
