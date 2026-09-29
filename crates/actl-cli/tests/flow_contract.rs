use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn migrated_assertion_commands_have_a_stable_surface() {
    let s = Sandbox::new();
    let (code, result) = s.invoke(&["commands"]);
    assert_eq!(code, 0);
    let mut commands = serde_json::Map::new();
    for command in result["data"]["commands"].as_array().unwrap() {
        let name = command["name"].as_str().unwrap();
        if [
            "verify-file",
            "verify-rows",
            "verify-owner",
            "verify-closed",
            "verify-clipboard",
        ]
        .contains(&name)
        {
            commands.insert(
                name.into(),
                json!(
                    command["args"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|arg| arg["name"].clone())
                        .collect::<Vec<_>>()
                ),
            );
        }
    }
    assert_eq!(
        Value::Object(commands),
        serde_json::from_str::<Value>(include_str!("golden/workflow_assertions.json")).unwrap()
    );
}

#[test]
fn office_inspector_reads_synthetic_packages_without_office() {
    let s = Sandbox::new();
    let assets =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills/actl/workflows/assets");
    for ext in ["docx", "xlsx"] {
        std::fs::copy(
            assets.join(format!("seed.{ext}")),
            s.root.join(format!("test.{ext}")),
        )
        .unwrap();
    }
    for (file, kind, expected) in [
        ("test.docx", "docx", r#"{"text":"ACTL seed","bold":false}"#),
        (
            "test.xlsx",
            "xlsx",
            r#"{"A1":{"value":"ACTL seed","formula":""}}"#,
        ),
    ] {
        let path = s.root.join(file);
        let (code, result) = s.invoke(&[
            "verify-file",
            "--path",
            path.to_str().unwrap(),
            "--kind",
            kind,
            "--expected",
            expected,
        ]);
        assert_eq!(code, 0, "{result}");
        std::fs::write(&path, "broken package").unwrap();
        let (code, result) = s.invoke(&[
            "verify-file",
            "--path",
            path.to_str().unwrap(),
            "--kind",
            kind,
            "--expected",
            expected,
        ]);
        assert_ne!(code, 0);
        assert_eq!(result["error"]["code"], "NOT_ACTIONABLE");
    }
}

#[test]
fn sequences_preflight_every_child_and_never_allow_management() {
    let s = Sandbox::new();
    for child in ["execution-recover", "key-down", "batch", "flow-resume"] {
        let file=s.file(&format!("version: 1\nname: group\nsteps:\n - id: edit\n   title: Edit\n   run:\n     sequence:\n       - {{command: status}}\n       - {{command: {child}}}\n   verify: {{command: verify, args: [--exists, 'id:edit']}}\n"));
        assert_ne!(
            s.invoke(&["flow-validate", "--file", &file]).0,
            0,
            "{child}"
        );
    }
}

#[test]
fn file_check_contract_and_mismatch_are_read_only() {
    let s = Sandbox::new();
    let path = s.root.join("data.json");
    std::fs::write(&path, "{\"value\":\"saved\",\"extra\":1}").unwrap();
    let args = [
        "verify-file",
        "--path",
        path.to_str().unwrap(),
        "--kind",
        "json",
        "--expected",
        "{\"value\":\"saved\"}",
        "--subset",
    ];
    let (code, mut result) = s.invoke(&args);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/verify_file.json")).unwrap()
    );
    let (code, result) = s.invoke(&args[..args.len() - 1]);
    assert_ne!(code, 0);
    assert_eq!(result["error"]["code"], "ASSERTION_FAILED");
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "{\"value\":\"saved\",\"extra\":1}"
    );
}

#[test]
fn reusable_workflows_preflight_without_desktop_actions() {
    let s = Sandbox::new();
    for (name, values) in [
        (
            "word-edit",
            "program: 'C:/test/word.exe'\nfile: 'C:/test/task.docx'\napp: unique",
        ),
        (
            "excel-table",
            "program: 'C:/test/excel.exe'\nfile: 'C:/test/task.xlsx'\napp: unique",
        ),
        (
            "vscode-edit",
            "program: 'C:/test/Code.exe'\nprofile: 'C:/test/profile'\nfile: 'C:/test/task.txt'\napp: unique\neditor: 'id:editor'\nsecond: 'C:/test/second.txt'\nsecond_editor: 'id:second'\nfile_result: 'uia:{\"match\":{\"role\":\"ListItem\"},\"within\":{\"id\":\"quickInput_list\"}}'\nsecond_result: 'uia:{\"match\":{\"role\":\"ListItem\"},\"within\":{\"id\":\"quickInput_list\"}}'",
        ),
        (
            "zotero-search",
            "program: 'C:/test/zotero.exe'\nprofile: 'C:/test/profile'\ndata: 'C:/test/data'\napp: unique\nexport: 'C:/test/export.json'",
        ),
        (
            "vscode-resume",
            "program: 'C:/test/Code.exe'\nprofile: 'C:/test/profile'\nfile: 'C:/test/unique.txt'\napp: unique\ntarget: 'id:editor'",
        ),
        (
            "verify-text",
            "app: unique\ntarget: 'id:editor'\nexpected: text",
        ),
        (
            "edit-value",
            "app: unique\ntarget: 'id:editor'\nprevious: old\nvalue: new",
        ),
    ] {
        let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../skills/actl/workflows/{name}.yml"));
        let path = s.root.join("values.yml");
        std::fs::write(&path, values).unwrap();
        let (code, result) = s.invoke(&[
            "flow-validate",
            "--file",
            template.to_str().unwrap(),
            "--inputs",
            path.to_str().unwrap(),
            "--emit-plan",
        ]);
        assert_eq!(code, 0, "{name}: {result}");
        let plan: actl_flow::Plan = serde_json::from_value(result["data"]["plan"].clone()).unwrap();
        if name == "vscode-edit" {
            for step in plan.steps.iter().filter(|s| {
                matches!(
                    s.id.as_str(),
                    "goto-line" | "quick-open-second" | "quick-open-file"
                )
            }) {
                let calls = actl_flow::sequence::decode(&step.run).unwrap().unwrap();
                for call in calls {
                    if call.command == "type" {
                        assert_eq!(
                            call.args[0], "id:editor",
                            "Quick Input must use ValuePattern"
                        );
                    }
                    if call.command == "click" {
                        assert!(
                            call.args[0].contains("quickInput_list"),
                            "result clicks must be container scoped"
                        );
                    }
                    assert!(
                        !(call.command == "press" && call.args[0] == "enter"),
                        "activate the verified result instead of injecting into a virtual-focus input"
                    );
                }
            }
        }
        if matches!(name, "word-edit" | "excel-table") {
            for step in &plan.steps {
                let calls = actl_flow::sequence::decode(&step.run)
                    .unwrap()
                    .unwrap_or_else(|| vec![step.run.clone()]);
                for call in calls {
                    assert_ne!(call.command, "paste", "{name}: use a targeted input");
                    if call.command == "press" {
                        assert!(
                            call.args.iter().any(|arg| arg == "--target"),
                            "{name}: {} lacks focus assertion",
                            step.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn declared_inputs_validate_and_save_a_concrete_plan() {
    let s = Sandbox::new();
    let file = s.file("version: 1\nname: Inputs\ninputs: {app: {}}\nsteps:\n - id: read\n   title: Read\n   run: {command: snapshot, args: [--app, {input: app}]}\n");
    let path = s.root.join("inputs.yml");
    std::fs::write(&path, "app: unique-test-window").unwrap();
    let inputs = path.to_str().unwrap();
    let (code, mut result) = s.invoke(&["flow-validate", "--file", &file, "--inputs", inputs]);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/flow_inputs.json")).unwrap()
    );
    let (code, result) = s.invoke(&[
        "flow-run",
        "--file",
        &file,
        "--inputs",
        inputs,
        "--id",
        "inputs-test",
        "--paused",
    ]);
    assert_eq!(code, 0, "{result}");
    std::fs::write(&path, "app: changed").unwrap();
    let store = actl_flow::store::Store::at(&s.root.join("actl"), "inputs-test").unwrap();
    let (plan, _) = store.load().unwrap();
    assert_eq!(plan.steps[0].run.args, ["--app", "unique-test-window"]);
    assert_ne!(s.invoke(&["flow-validate", "--file", &file]).0, 0);
    std::fs::write(&path, "app: --physical").unwrap();
    assert_ne!(
        s.invoke(&["flow-validate", "--file", &file, "--inputs", inputs])
            .0,
        0
    );
}
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
            .env_remove("ACTL_FLOW_REQUEST_SOURCE")
            .env_remove("ACTL_FLOW_REQUEST_ID")
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
fn data_output_persists_across_cli_resume_and_revalidates_bound_args() {
    let s = Sandbox::new();
    let file = s.file("version: 1\nname: Bind\nsteps:\n - id: source\n   title: Read\n   run: {command: status}\n   outputs: {os: {pointer: /os, type: string}}\n - id: sink\n   title: Read again\n   run: {command: wait, args: [--window, ACTL-ABSENT, --timeout, '1'], bind: {'3': source.os}}\n");
    let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "bindings"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["reason"], "binding_validation_failed");
    assert_eq!(
        result["data"]["run"]["steps"][0]["outputs"]["os"],
        std::env::consts::OS
    );
    assert_eq!(result["data"]["run"]["steps"][1]["attempts"], 0);
    let (_, resumed) = s.invoke(&["flow-resume", "--id", "bindings"]);
    assert_eq!(resumed["data"]["run"]["steps"][0]["attempts"], 1);
    assert_eq!(resumed["data"]["run"]["steps"][1]["attempts"], 0);
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/flows/output-binding.yml"
    );
    let (code, _) = s.invoke(&["flow-validate", "--file", fixture]);
    assert_eq!(code, 0);
}

#[test]
fn bound_read_runs_across_two_cli_processes() {
    let s = Sandbox::new();
    // Protocol version "1" is used as a harmless nonexistent HWND to exercise real dispatch.
    let file=s.file("version: 1\nname: Bind\nsteps:\n - id: source\n   title: Read\n   run: {command: status}\n   outputs: {protocol: {pointer: /protocol, type: string}}\n - id: sink\n   title: Read handle\n   pause_before: true\n   run: {command: list-windows, args: [--native, --hwnd, '1'], bind: {'2': source.protocol}}\n");
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "read-bind"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "paused");
    let (code, result) = s.invoke(&["flow-resume", "--id", "read-bind"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    assert_eq!(result["data"]["run"]["steps"][0]["attempts"], 1);
    assert_eq!(result["data"]["run"]["steps"][1]["attempts"], 1);
}

#[test]
fn structured_targets_validate_before_any_step_and_survive_persistence() {
    let s = Sandbox::new();
    let file = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/flows/calculator-composite.yml"
    );
    let (code, mut result) = s.invoke(&["flow-validate", "--file", file]);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/flow_composite.json")).unwrap()
    );
    let (code, result) = s.invoke(&["flow-run", "--file", file, "--id", "composite", "--paused"]);
    assert_eq!(code, 0, "{result}");
    let plan: Value = serde_json::from_slice(
        &std::fs::read(s.root.join("actl/flows/composite/plan.json")).unwrap(),
    )
    .unwrap();
    assert!(plan.to_string().contains("clearButton"));
    for phase in ["run", "before", "verify"] {
        let step = if phase == "run" {
            "run: {command: get, args: ['uia:{}']}".to_string()
        } else {
            format!(
                "run: {{command: status}}\n   {phase}: {{command: verify, args: [--exists, 'uia:{{}}']}}"
            )
        };
        let file = s.file(&format!("version: 1\nname: Invalid\nsteps:\n - id: first\n   title: First\n   run: {{command: status}}\n - id: bad\n   title: Bad\n   {step}\n"));
        let (_, result) = s.invoke(&["flow-run", "--file", &file, "--id", "invalid-composite"]);
        assert_eq!(result["error"]["code"], "PROTOCOL", "{result}");
        assert_eq!(result["error"]["evidence"]["step_id"], "bad");
        assert!(!s.root.join("actl/flows/invalid-composite").exists());
    }
    let (_, mut result) = s.invoke(&["get", r#"uia:{"match":{}}"#, "--app", "ACTL-NOT-OPEN"]);
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/selector_error.json")).unwrap()
    );
}

#[test]
fn structural_and_command_errors_identify_the_step_and_field() {
    let s = Sandbox::new();
    for (call, field) in [
        ("{command: wait, args: [99]}", "steps[0].run.args[0]"),
        ("{command: wait}", "steps[0].run.args"),
    ] {
        let file = s.file(&format!(
            "version: 1\nname: Bad\nsteps:\n - id: target\n   title: Target\n   run: {call}\n"
        ));
        let (_, mut result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_eq!(result["error"]["evidence"]["step_id"], "target");
        assert_eq!(result["error"]["evidence"]["field"], field);
        if field.ends_with("[0]") {
            result.as_object_mut().unwrap().remove("duration_ms");
            assert_eq!(
                result,
                serde_json::from_str::<Value>(include_str!("golden/flow_field_error.json"))
                    .unwrap()
            );
        }
    }
    let (_, valid) = s.invoke(&[
        "flow-validate",
        "--file",
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/flows/calculator-verified.yml"
        ),
    ]);
    assert_eq!(valid["ok"], true, "{valid}");
}

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
    assert_eq!(result["error"]["code"], "NOT_ACTIONABLE");
    assert_eq!(
        result["error"]["evidence"]["reason"],
        "execution_state_invalid"
    );
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
    let (_, journal) = s.invoke(&["history", "--task", "live"]);
    let events: Vec<_> = journal["data"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["events"].as_array().unwrap())
        .collect();
    let accepted = events
        .iter()
        .find(|e| e["event"] == "control_request" && e["data"]["stage"] == "accepted")
        .unwrap();
    let observed = events
        .iter()
        .find(|e| e["event"] == "flow_pause_observed")
        .unwrap();
    assert_eq!(
        accepted["data"]["request_id"],
        observed["data"]["request_id"]
    );
    // Simulate a write-ahead record left by a dead process; status must report interruption.
    let mut stored: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    stored["status"] = json!("running");
    std::fs::write(path, stored.to_string()).unwrap();
    let (_, status) = s.invoke(&["flow-status", "--id", "live"]);
    assert_eq!(status["data"]["interrupted"], true);
}

#[test]
fn expired_output_is_reported_on_resume_without_dispatch() {
    let s = Sandbox::new();
    let file = s.file("version: 1\nname: Age\nsteps:\n - id: source\n   title: Source\n   run: {command: status}\n   outputs:\n     protocol: {pointer: /protocol, type: string, max_age_ms: 60000}\n - id: sink\n   title: Sink\n   pause_before: true\n   run:\n     command: wait\n     args: [--window, ACTL-NONEXISTENT-AGE, --timeout, '1']\n     bind: {'3': source.protocol}\n");
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "age"]);
    assert_eq!(code, 0, "{result}");
    assert!(result["data"]["run"]["steps"][0]["outputs_captured_ms"].is_u64());
    let path = s.root.join("actl/flows/age/state.json");
    let mut state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    state["steps"][0]["outputs_captured_ms"] = json!(1);
    std::fs::write(path, state.to_string()).unwrap();
    let (_, result) = s.invoke(&["flow-resume", "--id", "age"]);
    let run = &result["data"]["run"];
    assert_eq!(run["reason"], "output_freshness_failed", "{result}");
    assert_eq!(run["steps"][0]["attempts"], 1);
    assert_eq!(run["steps"][1]["attempts"], 0);
    assert_eq!(run["steps"][1]["error_code"], "PROTOCOL");
    assert_eq!(run["steps"][1]["error_evidence"]["reason"], "expired");
}

#[test]
fn recovery_checks_are_validated_and_never_allow_writes() {
    let s = Sandbox::new();
    for (check, expected) in [
        (
            "{command: verify, args: [--exists, id:result, --app, Calculator]}",
            0,
        ),
        ("{command: click, args: [id:result, --app, Calculator]}", 2),
        (
            "{command: verify, args: [--exists, '@e1', --app, Calculator]}",
            2,
        ),
        (
            "{command: verify, args: [--exists, id:result], bind: {'0': missing.value}}",
            2,
        ),
    ] {
        let file = s.file(&format!("version: 1\nname: Guard\nsteps:\n - id: a\n   title: A\n   retry: stale_read\n   run: {{command: status}}\n   resume_check: {check}\n"));
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_eq!(code, expected, "{result}");
    }
    let file = s.file("version: 1\nname: Write\nsteps:\n - id: a\n   title: A\n   retry: stale_read\n   run: {command: click, args: [id:result, --app, Calculator]}\n   verify: {command: verify, args: [--exists, id:result, --app, Calculator]}\n");
    let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
}

#[test]
fn resume_journal_has_source_guard_result_and_business_outcome() {
    for (marker, source) in [
        (None, "cli"),
        (Some("signal_control"), "signal_control"),
        (Some("private-untrusted-value"), "unknown"),
    ] {
        let s = Sandbox::new();
        let file = s.file("version: 1\nname: Log\nsteps:\n - id: a\n   title: A\n   pause_before: true\n   run: {command: status}\n   resume_check: {command: wait, args: [--window, ACTL-LOG-ABSENT-189235, --timeout, '1']}\n");
        assert_eq!(
            s.invoke(&["flow-run", "--file", &file, "--id", "logging"])
                .0,
            0
        );
        let mut cmd = s.command();
        if let Some(value) = marker {
            cmd.env("ACTL_FLOW_REQUEST_SOURCE", value);
        }
        let out = cmd
            .args(["flow-resume", "--id", "logging"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let (_, journal) = s.invoke(&["history", "--task", "logging"]);
        let events = journal["data"]["calls"][0]["events"].as_array().unwrap();
        let request = events
            .iter()
            .find(|e| e["event"] == "flow_resume_requested")
            .unwrap();
        assert_eq!(request["data"]["source"], source);
        assert_eq!(request["data"]["source_verified"], false);
        let phases: Vec<_> = events
            .iter()
            .filter(|e| e["event"] == "workflow_invocation")
            .collect();
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0]["data"]["phase"], "resume_check");
        assert_eq!(phases[0]["data"]["outcome"], "started");
        assert_eq!(phases[1]["data"]["outcome"], "failed");
        assert_eq!(phases[1]["step_id"], "a");
        let finish = &events.last().unwrap()["data"];
        assert_eq!(finish["phase"], "done");
        assert_eq!(finish["workflow_status"], "paused");
        assert_eq!(finish["workflow_completed"], false);
        assert_eq!(finish["workflow_reason"], "resume_check_failed");
        assert!(finish["workflow_error_code"].is_string());
        assert!(!journal.to_string().contains("private-untrusted-value"));
        assert!(!journal.to_string().contains("ACTL-LOG-ABSENT-189235"));
    }
}

#[test]
fn completed_resume_still_reports_business_completion() {
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    s.invoke(&["flow-run", "--file", &file, "--id", "done"]);
    for _ in 0..2 {
        s.invoke(&["flow-resume", "--id", "done"]);
        let (_, history) = s.invoke(&["history", "--task", "done"]);
        let events = history["data"]["calls"][0]["events"].as_array().unwrap();
        assert_eq!(events.last().unwrap()["data"]["workflow_completed"], true);
        assert_eq!(
            events.last().unwrap()["data"]["workflow_status"],
            "completed"
        );
    }
}

#[test]
fn pause_journal_does_not_overwrite_desktop_feedback() {
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    assert_eq!(
        s.invoke(&[
            "flow-run",
            "--file",
            &file,
            "--id",
            "pause-meta",
            "--paused"
        ])
        .0,
        0
    );
    let path = s.root.join("actl/state.json");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(s.invoke(&["flow-pause", "--id", "pause-meta"]).0, 0);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let request =
        std::fs::read_to_string(s.root.join("actl/flows/pause-meta/pause-requested")).unwrap();
    assert!(actl_core::history::valid_id(&request));
    let (_, history) = s.invoke(&["history", "--task", "pause-meta"]);
    let control = &history["data"]["calls"][0]["events"][1];
    assert_eq!(control["event"], "control_request");
    assert_eq!(control["data"]["stage"], "accepted");
    assert_eq!(control["data"]["request_id"], request);
    assert_eq!(control["data"]["source"], "cli");
}
