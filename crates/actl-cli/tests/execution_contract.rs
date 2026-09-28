mod support;
use actl_core::state::SignalPaths;

#[test]
fn a_running_cli_remains_cancelled_after_immediate_recovery() {
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let paths = SignalPaths::at(root.join("actl"));
    let mut cmd = support::IsolatedCommand::new();
    let mut child = cmd
        .command
        .env("LOCALAPPDATA", &root)
        .args([
            "wait",
            "--window",
            "ACTL-NO-SUCH-STOP-GENERATION-TEST-487251",
            "--timeout",
            "3000",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while paths.read_calls().iter().all(|s| s.pid != child.id())
        && std::time::Instant::now() < until
    {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let observed = paths.read_calls().iter().any(|s| s.pid == child.id());
    paths.request_stop().unwrap();
    paths.recover_execution().unwrap();
    let out = child.wait_with_output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(observed, "{v}");
    assert_eq!(v["error"]["code"], "ABORTED", "{v}");
    assert!(!paths.stop_requested());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn execution_status_is_a_stable_offline_contract() {
    let out = support::IsolatedCommand::new()
        .command
        .arg("execution-status")
        .output()
        .unwrap();
    assert!(out.status.success());
    let mut v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        v,
        serde_json::from_str::<serde_json::Value>(include_str!("golden/execution_status.json"))
            .unwrap()
    );
}

#[test]
fn queued_child_cannot_adopt_a_generation_created_after_its_parent_click() {
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let paths = SignalPaths::at(root.join("actl"));
    let generation = paths.execution_generation().unwrap();
    paths.request_stop().unwrap();
    paths.recover_execution().unwrap();
    let out = support::IsolatedCommand::new()
        .command
        .env("LOCALAPPDATA", &root)
        .env("ACTL_EXECUTION_GENERATION", generation)
        .args([
            "wait",
            "--window",
            "ACTL-NO-SUCH-QUEUED-CHILD-8712",
            "--timeout",
            "500",
        ])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["error"]["code"], "ABORTED", "{value}");
    assert_eq!(
        value["error"]["evidence"]["reason"],
        "execution_generation_changed"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recovery_is_explicit_preserves_evidence_and_grants_nothing() {
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let paths = SignalPaths::at(root.join("actl"));
    std::fs::create_dir_all(&paths.dir).unwrap();
    std::fs::write(paths.stop_file(), "obsolete-v1").unwrap();
    let mut command = support::IsolatedCommand::new();
    let out = command
        .command
        .env("LOCALAPPDATA", &root)
        .arg("execution-status")
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["state"], "repair_required");
    assert_eq!(
        std::fs::read_to_string(paths.stop_file()).unwrap(),
        "obsolete-v1"
    );
    // Core recovery is used here so an unrelated real companion cannot affect the test.
    let report = paths.recover_execution().unwrap();
    assert_eq!(report["permission_granted"], false);
    assert_eq!(report["resumed"], false);
    let archived = report["archived"][0].as_str().unwrap();
    assert_eq!(std::fs::read_to_string(archived).unwrap(), "obsolete-v1");
    let out = command.command.output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["state"], "ready");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn execution_commands_have_help_and_cannot_be_persisted_as_steps() {
    for name in ["execution-status", "execution-recover"] {
        let out = support::IsolatedCommand::new()
            .command
            .args([name, "--help"])
            .output()
            .unwrap();
        let help = String::from_utf8_lossy(&out.stdout);
        for needle in ["Usage:", "Arguments:", "Example:", "Errors:"] {
            assert!(help.contains(needle), "{help}");
        }
        let file =
            std::env::temp_dir().join(format!("{}.json", actl_core::snapshot::new_snapshot_id()));
        std::fs::write(&file,format!(r#"{{"version":1,"name":"bad","steps":[{{"id":"s","title":"s","run":{{"command":"{name}"}}}}]}}"#)).unwrap();
        let out = support::IsolatedCommand::new()
            .command
            .args(["flow-validate", "--file"])
            .arg(&file)
            .output()
            .unwrap();
        std::fs::remove_file(file).unwrap();
        assert!(!out.status.success());
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["error"]["code"], "PROTOCOL");
    }
}
