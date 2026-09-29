use serde_json::{Value, json};
use std::{path::PathBuf, process::Command};

struct Sandbox(PathBuf);
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-resources-cli-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn invoke(&self, args: &[&str]) -> (i32, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_actl"))
            .current_dir(&self.0)
            .args(args)
            .env("LOCALAPPDATA", &self.0)
            .env_remove("ACTL_TASK_CONFIG")
            .env_remove("ACTL_TASK_ID")
            .output()
            .unwrap();
        (
            out.status.code().unwrap(),
            serde_json::from_slice(&out.stdout).unwrap_or_else(|_| panic!("{out:?}")),
        )
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn template() -> Value {
    json!({"version":1,"name":"Seed verification","resources":{"files":[{"path":"${run.id}.txt","text":"seed"}]},"inputs":{"file":{"default":"${run.dir}/${run.id}.txt"}},"steps":[{"id":"verify","title":"Verify seed","run":{"command":"verify-file","args":["--path",{"input":"file"},"--kind","text","--expected","\"seed\""]}}]})
}

#[test]
fn validate_is_read_only_run_initializes_and_resume_never_reseeds() {
    let s = Sandbox::new();
    let file = s.0.join("flow.yml");
    std::fs::write(&file, template().to_string()).unwrap();
    let file = file.to_str().unwrap();
    let (code, mut result) = s.invoke(&["flow-validate", "--file", file]);
    assert_eq!(code, 0, "{result}");
    result.as_object_mut().unwrap().remove("duration_ms");
    result["data"]["resources"]["root"] = json!("<resources>/validation-preview");
    assert_eq!(
        result,
        serde_json::from_str::<Value>(include_str!("golden/flow_resources.json")).unwrap()
    );
    assert!(!s.0.join("actl/flow-resources").exists());
    assert!(!s.0.join("actl/flows").exists());
    let (code, result) = s.invoke(&["flow-run", "--file", file, "--id", "fresh"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
    let seed = s.0.join("actl/flow-resources/fresh/fresh.txt");
    assert_eq!(std::fs::read_to_string(&seed).unwrap(), "seed");
    std::fs::write(&seed, "user edit").unwrap();
    std::fs::write(file, "broken source").unwrap();
    let (code, result) = s.invoke(&["flow-resume", "--id", "fresh"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(std::fs::read_to_string(&seed).unwrap(), "user edit");
    std::fs::write(file, template().to_string()).unwrap();
    assert_ne!(
        s.invoke(&["flow-run", "--file", file, "--id", "fresh"]).0,
        0
    );
    assert_eq!(std::fs::read_to_string(seed).unwrap(), "user edit");
}

#[test]
fn invalid_command_is_rejected_before_resource_initialization() {
    let s = Sandbox::new();
    let file = s.0.join("flow.yml");
    let mut plan = template();
    plan["steps"][0]["run"]["command"] = json!("nonexistent-command");
    std::fs::write(&file, plan.to_string()).unwrap();
    assert_ne!(
        s.invoke(&["flow-run", "--file", file.to_str().unwrap(), "--id", "bad"])
            .0,
        0
    );
    assert!(!s.0.join("actl/flow-resources/bad").exists());
    assert!(!s.0.join("actl/flows/bad").exists());
}

#[test]
fn basename_template_copies_relative_assets() {
    let s = Sandbox::new();
    let mut plan = template();
    plan["resources"]["files"][0] = json!({"path":"${run.id}.txt","copy":"seed.txt"});
    std::fs::write(s.0.join("seed.txt"), "seed").unwrap();
    std::fs::write(s.0.join("flow.yml"), plan.to_string()).unwrap();
    let (code, result) = s.invoke(&["flow-run", "--file", "flow.yml", "--id", "relative"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["run"]["status"], "completed");
}

#[test]
fn all_application_templates_validate_without_input_files() {
    // 与 flow_contract::reusable_workflows_preflight 的分工:那边带具体 --inputs
    // 验证解析后的计划;这里验证模板仅靠声明默认值即可通过预检(应用模板
    // 开箱可验),步骤数 golden 兼作模板变更探测器。两条件不同,勿合并。
    let s = Sandbox::new();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/actl/workflows");
    let mut counts = serde_json::Map::new();
    for name in [
        "vscode-edit",
        "word-edit",
        "excel-table",
        "zotero-search",
        "verify-applications",
        "vscode-resume",
    ] {
        let path = base.join(format!("{name}.yml"));
        let (code, result) = s.invoke(&["flow-validate", "--file", path.to_str().unwrap()]);
        assert_eq!(code, 0, "{name}: {result}");
        counts.insert(name.into(), result["data"]["steps"].clone());
    }
    assert_eq!(
        Value::Object(counts),
        json!({"vscode-edit":26,"word-edit":20,"excel-table":13,"zotero-search":9,"verify-applications":68,"vscode-resume":3})
    );
    assert!(!s.0.join("actl/flow-resources").exists());
}

#[test]
fn combined_template_matches_individual_steps_and_resources() {
    fn bindings(value: &mut Value, prefix: &str) {
        match value {
            Value::Object(map) => {
                if map.len() == 1
                    && let Some(Value::String(input)) = map.get_mut("input")
                {
                    *input = format!("{prefix}-{input}");
                }
                if let Some(Value::Object(bind)) = map.get_mut("bind") {
                    for source in bind.values_mut() {
                        *source = json!(format!("{prefix}-{}", source.as_str().unwrap()));
                    }
                }
                for child in map.values_mut() {
                    bindings(child, prefix);
                }
            }
            Value::Array(array) => {
                for child in array {
                    bindings(child, prefix);
                }
            }
            _ => {}
        }
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills/actl/workflows");
    let mut expected = json!({"version":1,"name":"Application verification","handoff":{"approval":"once_per_run"},"inputs":{},"resources":{"directories":[],"files":[]},"steps":[]});
    for name in ["vscode-edit", "excel-table", "word-edit", "zotero-search"] {
        let mut raw: Value = serde_json::from_str(
            &std::fs::read_to_string(base.join(format!("{name}.yml"))).unwrap(),
        )
        .unwrap();
        for (key, decl) in raw["inputs"].as_object().unwrap() {
            let mut decl = decl.clone();
            decl["default"] = json!(
                decl["default"]
                    .as_str()
                    .unwrap()
                    .replace("${run.dir}", &format!("${{run.dir}}/{name}"))
                    .replace("${run.id}", &format!("${{run.id}}-{name}"))
            );
            expected["inputs"][format!("{name}-{key}")] = decl;
        }
        for dir in raw["resources"]["directories"].as_array().unwrap() {
            expected["resources"]["directories"]
                .as_array_mut()
                .unwrap()
                .push(json!(format!("{name}/{}", dir.as_str().unwrap())));
        }
        for file in raw["resources"]["files"].as_array().unwrap() {
            let mut file = file.clone();
            file["path"] = json!(format!(
                "{name}/{}",
                file["path"]
                    .as_str()
                    .unwrap()
                    .replace("${run.id}", &format!("${{run.id}}-{name}"))
            ));
            expected["resources"]["files"]
                .as_array_mut()
                .unwrap()
                .push(file);
        }
        for step in raw["steps"].as_array_mut().unwrap() {
            step["id"] = json!(format!("{name}-{}", step["id"].as_str().unwrap()));
            bindings(step, name);
            // The combined batch isolates each application as one section so a
            // deterministic failure skips its own section only (docs/68).
            step["section"] = json!(name);
            step["on_failure"] = json!("skip_section");
            expected["steps"].as_array_mut().unwrap().push(step.clone());
        }
    }
    let actual: Value = serde_json::from_str(
        &std::fs::read_to_string(base.join("verify-applications.yml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        actual, expected,
        "update the combined template when an application workflow changes"
    );
}
