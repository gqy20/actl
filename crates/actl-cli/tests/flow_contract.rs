use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};
struct Sandbox {
    root: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-flow-cli-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_actl"));
        cmd.env("LOCALAPPDATA", &self.root)
            .env_remove("ACTL_TASK_ID");
        cmd
    }
    fn invoke(&self, args: &[&str]) -> (i32, Value) {
        let out = self.command().args(args).output().unwrap();
        let data = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{:?}", out));
        (out.status.code().unwrap(), data)
    }
    fn file(&self, text: &str) -> String {
        let path = self.root.join("flow.yml");
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const SIMPLE: &str = "version: 1\nname: Inspect\nsteps:\n  - id: one\n    title: System\n    run: {command: status}\n  - id: two\n    title: System again\n    pause_before: true\n    run: {command: status}\n";

#[test]
fn preflight_rejects_missing_or_invalid_assertions_before_creating_run() {
    let s = Sandbox::new();
    for call in [
        "{command: wait}",
        "{command: verify}",
        "{command: verify, args: [--exists, invalid-selector]}",
        "{command: wait, args: [--element, 'name:x', --property, invalid-predicate]}",
    ] {
        let file=s.file(&format!("version: 1\nname: Invalid assertion\nsteps:\n - id: first\n   title: First\n   run: {{command: status}}\n - id: bad\n   title: Bad\n   run: {call}\n"));
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_eq!(code, 2, "{result}");
        assert_eq!(
            s.invoke(&["flow-run", "--file", &file, "--id", "invalid"])
                .0,
            2
        );
        assert!(!s.root.join("actl/flows/invalid").exists());
    }
}
#[test]
fn validate_checkpoint_resume_and_saved_plan_contract() {
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    let (code, mut result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0);
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/flow_validate.json")).unwrap()
    );
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "test"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "paused");
    assert_eq!(result["data"]["runner_active"], false);
    assert_eq!(result["data"]["run"]["steps"][0]["status"], "completed");
    // Editing the source must not silently replace a running plan.
    s.file("invalid YAML");
    let (code, result) = s.invoke(&["flow-resume", "--id", "test"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    assert_eq!(result["data"]["run"]["steps"][0]["attempts"], 1);
    let (_, history) = s.invoke(&["history", "--task", "test"]);
    assert_eq!(history["data"]["calls"].as_array().unwrap().len(), 2);
}
#[test]
fn validation_rejects_side_effect_retries_and_nested_control() {
    let s = Sandbox::new();
    for run in [
        "{command: batch, args: ['--json', '[\"status\"]']}",
        "{command: key-down, args: [shift, --app, Notepad]}",
        "{command: flow-status, args: [--id, test]}",
        "{command: set-clipboard, args: [secret]}",
    ] {
        let file = s.file(&format!(
            "version: 1\nname: Invalid\nsteps:\n - id: a\n   title: A\n   run: {run}\n"
        ));
        let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "test"]);
        assert_eq!(code, 2, "{result}");
        assert!(!s.root.join("actl/flows/test").exists());
    }
}
#[test]
fn stop_is_sticky_and_pause_is_a_boundary_request() {
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    assert_eq!(
        s.invoke(&["flow-run", "--file", &file, "--id", "test", "--paused"])
            .0,
        0
    );
    assert_eq!(
        s.invoke(&["flow-pause", "--id", "test"]).1["data"]["pause_requested"],
        true
    );
    std::fs::write(s.root.join("actl/stop-requested"), "stop").unwrap();
    let (code, result) = s.invoke(&["flow-resume", "--id", "test"]);
    assert_eq!(code, 1);
    assert_eq!(result["error"]["code"], "ABORTED");
    assert!(s.root.join("actl/stop-requested").exists());
}
#[test]
fn live_pause_and_process_death_are_distinguished() {
    let s = Sandbox::new();
    let file=s.file("version: 1\nname: Wait\nsteps:\n - id: wait\n   title: Wait\n   run: {command: wait, args: [--window, ACTL-NONEXISTENT-FLOW-947312, --timeout, '60000']}\n - id: next\n   title: Next\n   run: {command: status}\n");
    let mut child = s
        .command()
        .args(["flow-run", "--file", &file, "--id", "live"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let path = s.root.join("actl/flows/live/state.json");
    let started = std::time::Instant::now();
    loop {
        let state = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        if state
            .as_ref()
            .is_some_and(|v| v["steps"][0]["status"] == "running")
        {
            break;
        }
        if started.elapsed().as_secs() > 10 {
            let _ = child.kill();
            panic!("runner did not start");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let (_, status) = s.invoke(&["flow-pause", "--id", "live"]);
    assert_eq!(status["data"]["pause_requested"], true);
    let requested = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if requested.elapsed().as_secs() >= 5 {
            let _ = child.kill();
            let _ = child.wait();
            panic!("60-second wait did not cooperate with pause within 5 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["data"]["run"]["status"], "paused");
    assert_eq!(result["data"]["run"]["steps"][1]["attempts"], 0);
    assert_eq!(result["data"]["run"]["reason"], "pause_requested");
    assert_eq!(result["data"]["run"]["steps"][0]["status"], "pending");
    assert!(result["data"]["run"]["steps"][0]["error_code"].is_null());
    // Simulate a write-ahead record left by a dead process; status must report interruption.
    let mut stored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stored["status"] = json!("running");
    std::fs::write(path, stored.to_string()).unwrap();
    let (_, status) = s.invoke(&["flow-status", "--id", "live"]);
    assert_eq!(status["data"]["interrupted"], true);
}
