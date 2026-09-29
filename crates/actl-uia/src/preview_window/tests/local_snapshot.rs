//! Reuse the owned native fixture; these tests inject no desktop input.
use super::*;
use serde_json::{Value, json};

#[test]
#[ignore = "interactive desktop; owned deep tree, no input"]
fn exact_container_survives_unrelated_deep_branch() {
    let host = OwnedHost::with_tree(true, true);
    let app = format!("hwnd:{}", host.hwnd);
    let full = crate::capture(Some(&app)).unwrap();
    assert!(
        full.truncated,
        "fixture must exceed the old DFS depth limit"
    );
    let result = crate::scoped_snapshot::capture(&app, r#"uia:{"match":{"id":"11"}}"#, 0, 1);
    assert!(result.is_ok(), "{:#?}", result.err());
    let (local, _) = result.unwrap();
    assert_eq!(local.nodes.len(), 1);
    assert_eq!(local.nodes[0].name.as_deref(), Some("Option B"));
    for selector in [
        r#"uia:{"match":{"name":"Option B"},"scope":"children"}"#,
        r#"uia:{"match":{"id":"11"},"within":{"name":"ACTL isolated preview target"},"scope":"children"}"#,
    ] {
        assert!(crate::scoped_snapshot::capture(&app, selector, 0, 1).is_ok());
    }
    for (selector, expected) in [
        (
            r#"uia:{"match":{"name":"deep"}}"#,
            actl_core::ErrorCode::Ambiguous,
        ),
        (
            r#"uia:{"match":{"id":"11","role":"Edit"}}"#,
            actl_core::ErrorCode::NotFound,
        ),
        (
            r#"uia:{"match":{"id":"11"},"within":{"id":"11"}}"#,
            actl_core::ErrorCode::NotFound,
        ),
    ] {
        assert_eq!(
            crate::scoped_snapshot::capture(&app, selector, 0, 1)
                .err()
                .unwrap()
                .code,
            expected
        );
    }
}

#[test]
#[ignore = "interactive desktop; ACTL_PREVIEW_CLI and pwsh required"]
fn local_observation_sampler_records_owned_window_without_input() {
    let binary = std::env::var("ACTL_PREVIEW_CLI").unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let host = OwnedHost::with_choices(true);
    let hwnd = HWND(host.hwnd as *mut _);
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = repo.join(format!(
        "target/local-observation-owned-{}",
        std::process::id()
    ));
    let out = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(repo.join("scenarios/local-observation.ps1"))
        .args([
            "-Actl",
            &binary,
            "-Hwnd",
            &host.hwnd.to_string(),
            "-Within",
            r#"uia:{"match":{"id":"11"}}"#,
            "-OutputDirectory",
        ])
        .arg(&dir)
        .output()
        .unwrap();
    std::fs::write(dir.join("runner.stderr.log"), &out.stderr).unwrap();
    let report: Value = serde_json::from_slice(&std::fs::read(dir.join("report.json")).unwrap())
        .unwrap_or_else(|_| {
            // PowerShell UTF-8 output may include a BOM on older hosts.
            let text = std::fs::read_to_string(dir.join("report.json")).unwrap();
            serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap()
        });
    drop(host);
    let closed = !unsafe { IsWindow(Some(hwnd)) }.as_bool();
    let foreground_preserved = unsafe { GetForegroundWindow() } == foreground;
    std::fs::write(
        dir.join("cleanup.json"),
        json!({"closed":closed,"foreground_preserved":foreground_preserved}).to_string(),
    )
    .unwrap();
    assert!(out.status.success(), "{report}");
    assert_eq!(report["status"], "measured");
    assert_eq!(report["replays"].as_array().unwrap().len(), 1);
    assert_eq!(report["measurement"]["local_nodes"], 1);
    assert!(report["measurement"]["full_nodes"].as_u64().unwrap() > 1);
    assert!(closed && foreground_preserved);
}

#[test]
#[ignore = "interactive desktop; ACTL_PREVIEW_CLI must point to built CLI"]
fn release_cli_local_snapshot_binds_refs_and_rejects_changed_scope() {
    let binary = std::env::var("ACTL_PREVIEW_CLI").unwrap();
    let foreground = unsafe { GetForegroundWindow() };
    let host = OwnedHost::with_choices(true);
    let hwnd = HWND(host.hwnd as *mut _);
    let app = format!("hwnd:{}", host.hwnd);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/local-snapshot-live")
        .join(format!("run-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut call = 0;
    let mut run = |args: &[&str]| -> Value {
        call += 1;
        let out = std::process::Command::new(&binary)
            .env("LOCALAPPDATA", dir.join("state"))
            .env_remove("ACTL_TASK_ID")
            .args(args)
            .output()
            .unwrap();
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        std::fs::write(dir.join(format!("{call}.json")), serde_json::to_vec_pretty(&json!({
            "binary":binary, "args":args, "exit":out.status.code(), "stdout_bytes":out.stdout.len(),
            "stderr":String::from_utf8_lossy(&out.stderr), "result":value,
        })).unwrap()).unwrap();
        value
    };
    let full = run(&["snapshot", "--app", &app]);
    assert_eq!(full["ok"], true, "{full}");
    let scope = r#"uia:{"match":{"id":"11"}}"#;
    let local = run(&[
        "snapshot", "--app", &app, "--within", scope, "--depth", "0", "--limit", "1",
    ]);
    assert_eq!(local["ok"], true, "{local}");
    assert_eq!(local["data"]["observed_nodes"], 1);
    assert_eq!(local["data"]["ref_scope"], "container");
    assert_eq!(local["data"]["truncated"], false);
    let mut normalized = local.clone();
    normalized.as_object_mut().unwrap().remove("duration_ms");
    normalized["snapshot_id"] = json!("s-test");
    normalized["data"]["window"]["pid"] = json!(1);
    normalized["data"]["scope"]["identity"]["pid"] = json!(1);
    normalized["data"]["scope"]["identity"]["runtime_id"] = json!([42, 1]);
    assert_eq!(
        normalized,
        serde_json::from_str::<Value>(include_str!("../../../tests/golden/local_snapshot.json"))
            .unwrap()
    );
    let elements = local["data"]["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 1);
    assert_eq!(elements[0]["ref"], "@e1");
    assert_eq!(elements[0]["name"], "Option B");
    assert_ne!(
        full["data"]["elements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == "Option B")
            .unwrap()["ref"],
        "@e1"
    );
    let id = local["snapshot_id"].as_str().unwrap();
    let get = run(&[
        "get",
        "@e1",
        "--app",
        &app,
        "--snapshot",
        id,
        "--property",
        "name",
    ]);
    assert_eq!(get["ok"], true, "{get}");
    assert!(get["data"].to_string().contains("Option B"), "{get}");
    // A depth cut is reported truthfully and does not fabricate a complete tree.
    let cut = run(&[
        "snapshot",
        "--app",
        &app,
        "--within",
        r#"uia:{"match":{"name":"ACTL isolated preview target"}}"#,
        "--depth",
        "0",
    ]);
    assert_eq!(cut["ok"], true, "{cut}");
    assert_eq!(cut["data"]["truncated"], true);
    assert!(
        cut["data"]["truncation_reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("depth_limit"))
    );
    let limited = run(&[
        "snapshot",
        "--app",
        &app,
        "--within",
        r#"uia:{"match":{"name":"ACTL isolated preview target"}}"#,
        "--limit",
        "2",
    ]);
    assert_eq!(limited["ok"], true, "{limited}");
    assert_eq!(limited["data"]["truncated"], true);
    let limited_ref = limited["data"]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["ref"].is_string())
        .unwrap();
    let replay = run(&[
        "get",
        limited_ref["ref"].as_str().unwrap(),
        "--app",
        &app,
        "--snapshot",
        limited["snapshot_id"].as_str().unwrap(),
        "--property",
        "name",
    ]);
    assert_eq!(replay["ok"], true, "{replay}");
    assert!(
        replay["data"]
            .to_string()
            .contains(limited_ref["name"].as_str().unwrap()),
        "{replay}"
    );
    // Change only the owned container label, leaving its locator stable.
    let button = unsafe { GetDlgItem(Some(hwnd), 11).unwrap() };
    unsafe { SetWindowTextW(button, w!("Changed B")).unwrap() };
    let stale = run(&[
        "get",
        "@e1",
        "--app",
        &app,
        "--snapshot",
        id,
        "--property",
        "name",
    ]);
    assert_eq!(stale["error"]["code"], "STALE_REF", "{stale}");
    let ambiguous = run(&[
        "snapshot",
        "--app",
        &app,
        "--within",
        r#"uia:{"match":{"role":"Button"}}"#,
    ]);
    assert_eq!(ambiguous["error"]["code"], "AMBIGUOUS", "{ambiguous}");
    drop(host);
    let closed = !unsafe { IsWindow(Some(hwnd)) }.as_bool();
    let foreground_preserved = unsafe { GetForegroundWindow() } == foreground;
    std::fs::write(dir.join("cleanup.json"), json!({"owned_hwnd":format!("{}", hwnd.0 as usize), "closed":closed, "foreground_preserved":foreground_preserved}).to_string()).unwrap();
    assert!(closed);
    assert!(foreground_preserved);
}
