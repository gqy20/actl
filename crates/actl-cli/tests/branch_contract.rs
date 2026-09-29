use serde_json::{Value, json};
use std::process::Command;
struct Sandbox(std::path::PathBuf);
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-branch-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn call(&self, args: &[&str]) -> (bool, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_actl"))
            .args(args)
            .env("LOCALAPPDATA", &self.0)
            .env_remove("ACTL_TASK_ID")
            .env_remove("ACTL_TASK_CONFIG")
            .output()
            .unwrap();
        (
            output.status.success(),
            serde_json::from_slice(&output.stdout).unwrap(),
        )
    }
    fn file(&self, value: &Value) -> String {
        let path = self.0.join("plan.yml");
        std::fs::write(&path, value.to_string()).unwrap();
        path.to_string_lossy().into_owned()
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn absent_window_assertion_has_stable_envelope_and_help() {
    let s = Sandbox::new();
    let name = format!("ACTL-ABSENT-{}", actl_core::snapshot::new_snapshot_id());
    let (ok, mut value) = s.call(&["verify-window", "--app", &name, "--state", "absent"]);
    assert!(ok, "{value}");
    value["duration_ms"] = 0.into();
    assert_eq!(
        value,
        serde_json::from_str::<Value>(include_str!("golden/verify_window.json")).unwrap()
    );
    let (ok, value) = s.call(&["verify-window", "--app", &name, "--state", "exists"]);
    assert!(!ok);
    assert_eq!(value["error"]["code"], "ASSERTION_FAILED");
    let help = Command::new(env!("CARGO_BIN_EXE_actl"))
        .args(["verify-window", "--help"])
        .output()
        .unwrap();
    let text = String::from_utf8(help.stdout).unwrap();
    for heading in ["Usage:", "--app", "--state", "Example:", "Errors:"] {
        assert!(text.contains(heading));
    }
}
#[test]
fn both_branches_validate_and_unknown_observation_does_not_fall_back() {
    let s = Sandbox::new();
    let mut plan = json!({"version":1,"name":"branch","steps":[{"id":"choose","title":"Choose","when":{"command":"verify-window","args":["--app","ACTL-ABSENT-UNIQUE-branch","--state","exists"]},"run":{"command":"status"},"otherwise":{"command":"status"}}]});
    let file = s.file(&plan);
    let (ok, result) = s.call(&["flow-run", "--file", &file, "--id", "negative"]);
    assert!(ok, "{result}");
    let (_, state) = s.call(&["flow-status", "--id", "negative"]);
    assert_eq!(state["data"]["run"]["steps"][0]["branch"], "otherwise");
    plan["steps"][0]["run"]["command"] = "does-not-exist".into();
    let (ok, result) = s.call(&["flow-validate", "--file", &s.file(&plan)]);
    assert!(!ok);
    assert_eq!(result["error"]["code"], "PROTOCOL");
    plan["steps"][0]["run"]["command"] = "status".into();
    plan["steps"][0]["otherwise"]["command"] = "does-not-exist".into();
    assert!(!s.call(&["flow-validate", "--file", &s.file(&plan)]).0);
    plan["steps"][0]["otherwise"]["command"] = "status".into();
    plan["steps"][0]["when"] = json!({"command":"verify","args":["--exists","uia:{\"match\":{\"role\":\"Button\",\"name\":\"Test\"}}","--app","hwnd:1"]});
    s.call(&["flow-run", "--file", &s.file(&plan), "--id", "unknown"]);
    let (_, state) = s.call(&["flow-status", "--id", "unknown"]);
    assert_eq!(
        state["data"]["run"]["reason"],
        "condition_observation_failed"
    );
    assert_eq!(state["data"]["run"]["steps"][0]["attempts"], 0);
}
#[test]
fn conditions_cannot_write_or_wait_or_use_fuzzy_element_absence() {
    let s = Sandbox::new();
    for predicate in [
        json!({"command":"click","args":["role:Button","--app","Test"]}),
        json!({"command":"wait","args":["--window","Test"]}),
        json!({"command":"verify","args":["--exists","name:Test","--app","Test"]}),
    ] {
        let plan = json!({"version":1,"name":"invalid","steps":[{"id":"choose","title":"Choose","when":predicate,"run":{"command":"status"}}]});
        assert!(!s.call(&["flow-validate", "--file", &s.file(&plan)]).0);
    }
}

#[test]
fn reusable_condition_templates_preflight_with_concrete_inputs() {
    let s = Sandbox::new();
    let inputs = s.0.join("inputs.yml");
    let selector = "uia:{\"match\":{\"role\":\"Edit\",\"id\":\"test-input\"}}";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/actl/workflows");
    for (name, values) in [
        (
            "ensure-app-window.yml",
            json!({"app":"Notepad","program":"notepad.exe","ready_target":selector}),
        ),
        (
            "dismiss-optional-control.yml",
            json!({"app":"Notepad","dialog":selector,"dismiss":selector}),
        ),
        (
            "ensure-element-value.yml",
            json!({"app":"Notepad","target":selector,"value":"test"}),
        ),
    ] {
        std::fs::write(&inputs, values.to_string()).unwrap();
        let file = root.join(name);
        let (ok, result) = s.call(&[
            "flow-validate",
            "--file",
            file.to_str().unwrap(),
            "--inputs",
            inputs.to_str().unwrap(),
        ]);
        assert!(ok, "{name}: {result}");
    }
}
