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
fn file_discover_picks_newest_match_reports_name_and_catches_late_arrival() {
    let s = Sandbox::new();
    // 发现形态:同 glob 两个文件取 mtime 最新者断言,信封带 discovered 实名
    let dir = s.root.join("exports");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("stale.csl.json.bib"), "old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(60));
    std::fs::write(dir.join("fresh.csl.json.bib"), "new").unwrap();
    let args = [
        "verify-file",
        "--path",
        dir.to_str().unwrap(),
        "--kind",
        "text",
        "--discover",
        "*.csl.json*",
        "--expected",
        "\"new\"",
    ];
    let (code, mut result) = s.invoke(&args);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    assert_eq!(result["data"]["kind"], "text");
    assert_eq!(result["data"]["comparison"], "exact", "{result}");
    assert!(
        result["data"]["discovered"]
            .as_str()
            .unwrap()
            .ends_with("fresh.csl.json.bib"),
        "{result}"
    );
    // 晚到文件:首轮无匹配(ASSERTION_FAILED 触发 settle 重试),300ms 后落盘被等到
    let late_dir = s.root.join("late");
    std::fs::create_dir_all(&late_dir).unwrap();
    let spawn_dir = late_dir.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        std::fs::write(spawn_dir.join("late-export.csl.json.bib"), "late").unwrap();
    });
    let args = [
        "verify-file",
        "--path",
        late_dir.to_str().unwrap(),
        "--kind",
        "text",
        "--discover",
        "*.csl.json*",
        "--expected",
        "\"late\"",
    ];
    let (code, result) = s.invoke(&args);
    assert_eq!(code, 0, "{result}");
}

#[test]
fn reusable_workflows_preflight_without_desktop_actions() {
    let s = Sandbox::new();
    for (name, values) in [
        ("cloudmusic-playback", "app: 'hwnd:123'"),
        ("quark-search", "app: 'hwnd:123'"),
        (
            "tencent-meeting-draft",
            "app: 'hwnd:123'\ntopic: Test\nexpected_date: '2026/9/29'",
        ),
        (
            "tencent-meeting-draft-date",
            "app: 'hwnd:123'\ntopic: Test\nexpected_date: '2026/9/29'\ndate: '2026/9/30'",
        ),
        (
            "app-launch-observe",
            "program: 'C:/test/test.exe'\napp: Test",
        ),
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
                    if call.command == "click" && call.args[0].contains("ListItem") {
                        assert!(
                            call.args[0].contains("quickInput_list"),
                            "result clicks must be container scoped"
                        );
                    }
                    assert!(
                        !(call.command == "press"
                            && matches!(
                                call.args[0].as_str(),
                                "enter" | "f1" | "ctrl+g" | "ctrl+p"
                            )),
                        "use UIA navigation controls instead of Quick Input shortcuts"
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
                    // Element-level focus is required for keys that input into the
                    // focused control. Modifier chords (copy/save-style commands on a
                    // window-scoped surface, e.g. Excel's grid selection) keep the
                    // engine's physical foreground + focus-thread guard instead; grid
                    // cells do not expose element focus (docs/68 first round).
                    if call.command == "press"
                        && !call
                            .args
                            .first()
                            .is_some_and(|key| key.starts_with("ctrl+"))
                    {
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
fn url_query_assertion_contract() {
    let s = Sandbox::new();
    let (_, result) = s.invoke(&["commands"]);
    let command = result["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "verify")
        .unwrap();
    let names: Vec<_> = command["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].clone())
        .collect();
    assert_eq!(
        json!(names),
        serde_json::from_str::<Value>(include_str!("golden/verify_query_args.json")).unwrap()
    );
    for extra in [", --contains", ", --normalize-lines", ", --property, value"] {
        let file=s.file(&format!("version: 1\nname: Bad\nsteps:\n - id: check\n   title: Check\n   run: {{command: verify, args: [--value, 'id:route', expected, --url-query, keyword{extra}]}}\n"));
        let (code, value) = s.invoke(&["flow-validate", "--file", &file]);
        assert_ne!(code, 0);
        assert_eq!(value["error"]["code"], "PROTOCOL");
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
            .env_remove("ACTL_TASK_ID")
            .env_remove("ACTL_TASK_CONFIG");
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
fn flow_status_flags_stale_template_after_freeze() {
    // 陈旧计划提示(spec):flow-run 冻结计划时旁车记录源模板指纹;
    // 模板改写后 flow-status 须提示"重试仍跑旧路线"——防对冻结计划验证
    // 新修复(ROADMAP 实录废过两次重试)。旧流程无记录时字段为 null。
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "stale", "--paused"]);
    assert_eq!(code, 0, "{result}");
    let (code, result) = s.invoke(&["flow-status", "--id", "stale"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["template"]["changed"], false, "{result}");
    std::fs::write(&file, "version: 1\nname: Inspect v2\nsteps:\n  - id: one\n    title: System\n    run: {command: status}\n").unwrap();
    let (code, result) = s.invoke(&["flow-status", "--id", "stale"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["template"]["changed"], true, "{result}");
    let hint = result["data"]["template"]["hint"].as_str().unwrap();
    assert!(hint.contains("旧路线"), "hint explains frozen plan: {hint}");
}

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
    // 2026-10-12 维护轮:CLI resume 补齐 control_log 对称——journal 从
    // 3(run+两子调用)变 4(+resume control record),与 Pause 同形。
    assert_eq!(history["data"]["calls"].as_array().unwrap().len(), 4);
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
fn cli_resume_writes_control_log_requested_accepted_and_failed() {
    // control_log 对称(2026-10-12 维护轮):显示端 resume 记录
    // requested/dispatching/child_returned 在位,CLI 侧补齐同形 record——
    // 成功路径落 requested+accepted(source=cli),失败路径落 failed。
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "resumelog", "--paused"]);
    assert_eq!(code, 0, "{result}");
    let (code, result) = s.invoke(&["flow-resume", "--id", "resumelog"]);
    assert_eq!(code, 0, "{result}");
    let (_, journal) = s.invoke(&["history", "--task", "resumelog"]);
    let events: Vec<_> = journal["data"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["events"].as_array().unwrap())
        .collect();
    for stage in ["requested", "accepted"] {
        let hit = events.iter().find(|e| {
            e["event"] == "control_request"
                && e["data"]["action"] == "resume"
                && e["data"]["stage"] == stage
                && e["data"]["source"] == "cli"
        });
        assert!(
            hit.is_some(),
            "missing resume control_request stage {stage}"
        );
    }
    // 失败路径对称:unknown id 的 resume 也落 failed record(不静默)
    let (code, _) = s.invoke(&["flow-resume", "--id", "resumelog-ghost"]);
    assert_ne!(code, 0);
    let (_, journal) = s.invoke(&["history", "--task", "resumelog-ghost"]);
    let events: Vec<_> = journal["data"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["events"].as_array().unwrap())
        .collect();
    let failed = events.iter().find(|e| {
        e["event"] == "control_request"
            && e["data"]["action"] == "resume"
            && e["data"]["stage"] == "failed"
    });
    assert!(
        failed.is_some(),
        "missing failed control_request for ghost resume"
    );
}

#[test]
fn real_kill_midstep_reports_interrupted_and_resume_closes_honestly() {
    // 进程中断中段验收(2026-10-12 维护轮):真实 TerminateProcess(模拟断电/
    // 崩溃),非文件模拟——WAL 落盘的 running 态必须诚实报 interrupted,恢复
    // 路径闭合为诚实失败态而非 completed。
    let s = Sandbox::new();
    let file = s.file("version: 1\nname: Slow\nsteps:\n - id: slow\n   title: Slow\n   run: {command: wait, args: [--window, ACTL-NONEXISTENT-KILL-4711, --timeout, '3000']}\n");
    let mut child = s
        .command()
        .args(["flow-run", "--file", &file, "--id", "killed"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let path = s.root.join("actl/flows/killed/state.json");
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
    child.kill().unwrap();
    let _ = child.wait();
    let (_, status) = s.invoke(&["flow-status", "--id", "killed"]);
    assert_eq!(status["data"]["interrupted"], true, "{status}");
    let (_, result) = s.invoke(&["flow-resume", "--id", "killed"]);
    assert_ne!(result["data"]["run"]["status"], "completed", "{result}");
    assert!(
        !result["data"]["run"]["steps"][0]["error_code"].is_null()
            || result["data"]["run"]["status"] == "paused",
        "{result}"
    );
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

#[test]
fn flow_evidence_indexes_failure_bundles_with_step_filter() {
    // 运行时治理第一轮:失败采集包索引契约——清单来自 capture_failure 的
    // bundle.json;--step 过滤;无包如实 PROTOCOL。
    let s = Sandbox::new();
    let file = s.file(SIMPLE);
    let (code, _) = s.invoke(&["flow-run", "--file", &file, "--id", "ev1", "--paused"]);
    assert_eq!(code, 0);
    // 未知 id:store 不存在 → PROTOCOL
    let (code, result) = s.invoke(&["flow-evidence", "--id", "nope"]);
    assert_eq!(code, 2);
    assert_eq!(result["error"]["code"], "PROTOCOL");
    // 已注册但无采集:如实拒绝
    let (code, result) = s.invoke(&["flow-evidence", "--id", "ev1"]);
    assert_eq!(code, 2);
    assert_eq!(result["error"]["code"], "PROTOCOL");
    let dir = s
        .root
        .join("actl")
        .join("flow-resources")
        .join("ev1")
        .join("evidence");
    std::fs::create_dir_all(&dir).unwrap();
    for (stem, step) in [
        ("s6-verify", "s6"),
        ("s6-popup_pending", "s6"),
        ("s9-run", "s9"),
    ] {
        std::fs::write(
            dir.join(format!("{stem}.bundle.json")),
            json!({"step_id": step, "phase": stem.rsplit_once('-').unwrap().1,
                   "captured_ms": 1791090000000u64 + stem.len() as u64,
                   "artifacts": [{"kind": "screenshot", "file": format!("{stem}.png"), "bytes": 12}]}).to_string(),
        )
        .unwrap();
    }
    let (code, result) = s.invoke(&["flow-evidence", "--id", "ev1"]);
    assert_eq!(code, 0);
    assert_eq!(result["data"]["bundles"].as_array().unwrap().len(), 3);
    let (code, result) = s.invoke(&["flow-evidence", "--id", "ev1", "--step", "s6"]);
    assert_eq!(code, 0);
    let bundles = result["data"]["bundles"].as_array().unwrap();
    assert_eq!(bundles.len(), 2);
    assert!(
        bundles
            .iter()
            .all(|bundle| bundle["step_id"].as_str() == Some("s6"))
    );
    // 帮助四要素可发现
    let out = s
        .command()
        .args(["flow-evidence", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("Example") && help.contains("--step") && help.contains("Errors"));
}

#[test]
fn flow_revise_versions_plans_and_continues_in_place() {
    // 运行时治理第二轮离线 E2E:失败(type 打不存在窗口,定位阶段 NOT_FOUND,
    // 零注入)→ 修订(版本谱系+旧计划归档)→ 续跑原地完成,不重开新任务。
    let s = Sandbox::new();
    let v1 = s.file(
        "version: 1\nname: ReviseE2E\nsteps:\n  - id: s1\n    title: Read\n    run: {command: status}\n  - id: s2\n    title: Doomed write\n    run: {command: type, args: ['name:Nope', hi, --app, definitely-missing-window-xyz]}\n    verify: {command: verify-window, args: [--app, definitely-missing, --state, absent]}\n",
    );
    let (code, result) = s.invoke(&["flow-run", "--file", &v1, "--id", "rv1"]);
    assert_eq!(code, 0, "{result}");
    // 观察类失败的写步:paused 诚实停(不授权重放),步 uncertain
    assert_eq!(result["data"]["run"]["status"], "paused");
    assert_eq!(result["data"]["run"]["steps"][1]["status"], "uncertain");
    // s1 已完成——修订不得重置未变步骤
    assert_eq!(result["data"]["run"]["steps"][0]["status"], "completed");

    let v2 = s.file(
        "version: 1\nname: ReviseE2E\nsteps:\n  - id: s1\n    title: Read\n    run: {command: status}\n  - id: s2\n    title: Fixed read\n    run: {command: status}\n",
    );
    let (code, result) = s.invoke(&[
        "flow-revise",
        "--id",
        "rv1",
        "--file",
        &v2,
        "--message",
        "s2 target never exists; downgrade to read",
    ]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["plan_version"], 2);
    assert_eq!(result["data"]["run"]["steps"][0]["status"], "completed");
    assert_eq!(result["data"]["run"]["steps"][1]["status"], "pending");
    let revisions = result["data"]["revisions"].as_array().unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["version"], 2);
    assert_eq!(
        revisions[0]["diff"]["modified"].as_array().unwrap().len(),
        1
    );

    // 原地续跑:同一 id 完成,版本谱系保留
    let (code, result) = s.invoke(&["flow-resume", "--id", "rv1"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    assert_eq!(result["data"]["run"]["plan_version"], 2);
    assert_eq!(result["data"]["revisions"].as_array().unwrap().len(), 1);
    // 旧版本字节归档未覆盖
    let archived = s
        .root
        .join("actl")
        .join("flows")
        .join("rv1")
        .join("revisions")
        .join("v1.plan.json");
    assert!(archived.exists());

    // 探索计划拒绝修订;帮助四要素可发现
    let (code, result) = s.invoke(&["flow-revise", "--id", "nope", "--file", &v2]);
    assert_eq!(code, 2);
    assert_eq!(result["error"]["code"], "PROTOCOL");
    let out = s
        .command()
        .args(["flow-revise", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("--accept-rerun") && help.contains("Errors") && help.contains("Example"));
}

#[test]
fn ensure_steps_short_circuit_when_satisfied() {
    // ensure: 幂等语义离线验证——check 通过的写步不派发(无注入)。
    let s = Sandbox::new();
    let file = s.file(
        "version: 1\nname: EnsureE2E\nsteps:\n  - id: e1\n    title: Env\n    ensure: {check: {command: verify-window, args: [--app, definitely-missing, --state, absent]}}\n    run: {command: type, args: ['name:Nope', hi, --app, definitely-missing]}\n    verify: {command: verify-window, args: [--app, definitely-missing, --state, absent]}\n",
    );
    let (code, result) = s.invoke(&["flow-run", "--file", &file, "--id", "ens1"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    assert_eq!(result["data"]["run"]["steps"][0]["status"], "completed");
    assert_eq!(result["data"]["run"]["steps"][0]["verified"], true);
}

#[test]
fn for_each_plan_validates_and_bad_shapes_are_rejected() {
    // for_each 数据驱动迭代(2026-10-11 spec):合法计划过 validate;
    // 坏形态(无占位/前向引用/步内 outputs)在 validate 即拒,离线可测。
    let s = Sandbox::new();
    let good = "version: 1\nname: EachPlan\nsteps:\n  - id: collect-steps\n    title: C\n    run: {command: find, args: [--role, listitem, --app, X]}\n    outputs:\n      items: {pointer: /elements, type: array}\n  - id: act-on-each\n    title: L\n    run:\n      for_each: {from: collect-steps.items, field: name, max: 8}\n      body:\n        - {command: click, args: [\"name:@each\", --app, X]}\n    verify: {command: verify-window, args: [--app, X, --state, exists]}\n"
        .to_string();
    let file = s.file(&good);
    let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["valid"], true);
    for (bad, needle) in [
        (good.replace("\"name:@each\"", "\"name:static\""), "@each"),
        (good.replace("max: 8", "max: 0"), "max"),
        (
            good + "    outputs:\n      total: {pointer: /executed, type: number}\n",
            "outputs",
        ),
    ] {
        let file = s.file(&bad);
        let (code, result) = s.invoke(&["flow-validate", "--file", &file]);
        assert_eq!(code, 2, "{result}");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains(needle),
            "{result}"
        );
    }
}
