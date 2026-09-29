use actl_flow::resources::prepare;
use std::path::PathBuf;

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "actl-resource-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn resources_are_isolated_once_and_do_not_evaluate_shell_or_supplied_inputs() {
    let temp = Temp::new();
    let root = temp.0.join("run");
    let text = r#"{"version":1,"name":"resources","inputs":{"file":{"default":"${run.dir}/${run.id}.txt"}},"resources":{"directories":["profile"],"files":[{"path":"${run.id}.txt","text":"中文\n$(whoami)"}]},"steps":[{"id":"read","title":"read","run":{"command":"get","args":[{"input":"file"}]}}]}"#;
    let result = prepare(text, &temp.0.join("flow.yml"), &root, "task").unwrap();
    assert!(!root.exists());
    let plan = actl_flow::inputs::resolve(&result.text, None).unwrap();
    assert_eq!(
        plan.steps[0].run.args[0],
        root.join("task.txt").to_string_lossy().replace('\\', "/")
    );
    let override_plan =
        actl_flow::inputs::resolve(&result.text, Some("file: '${run.id}'")).unwrap();
    assert_eq!(override_plan.steps[0].run.args[0], "${run.id}");
    result.create().unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("task.txt")).unwrap(),
        "中文\n$(whoami)"
    );
    std::fs::write(root.join("task.txt"), "edited").unwrap();
    assert!(result.create().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("task.txt")).unwrap(),
        "edited"
    );
}

#[test]
fn invalid_resources_never_create_files() {
    let temp = Temp::new();
    let root = temp.0.join("run");
    for path in [
        "../escape",
        "C:/escape",
        "/escape",
        "a/../b",
        "a\\b",
        "NUL.txt",
        "x:stream",
        "a.",
        "a//b",
    ] {
        let text =
            serde_json::json!({"resources":{"files":[{"path":path,"text":"seed"}]}}).to_string();
        assert!(
            prepare(&text, &temp.0.join("flow.yml"), &root, "task").is_err(),
            "{path}"
        );
        assert!(!root.exists());
    }
    for resources in [
        serde_json::json!({"files":[{"path":"a","text":"a","copy":"b"}]}),
        serde_json::json!({"files":[{"path":"a"}]}),
        serde_json::json!({"files":[{"path":"a","text":"a"},{"path":"A","text":"b"}]}),
        serde_json::json!({"directories":["a/b"],"files":[{"path":"a","text":"a"}]}),
        serde_json::json!({"files":[{"path":"a","text":"${unknown}"}]}),
        serde_json::json!({"command":"powershell"}),
    ] {
        assert!(
            prepare(
                &serde_json::json!({"resources":resources}).to_string(),
                &temp.0.join("flow.yml"),
                &root,
                "task"
            )
            .is_err()
        );
    }
}

#[test]
fn copy_is_frozen_at_preflight_and_source_must_be_inside_template() {
    let temp = Temp::new();
    let root = temp.0.join("run");
    std::fs::write(temp.0.join("seed.bin"), [0, 1, 255]).unwrap();
    let text = r#"{"resources":{"files":[{"path":"nested/out.bin","copy":"seed.bin"}]}}"#;
    let prepared = prepare(text, &temp.0.join("flow.yml"), &root, "task").unwrap();
    std::fs::write(temp.0.join("seed.bin"), "changed").unwrap();
    prepared.create().unwrap();
    assert_eq!(
        std::fs::read(root.join("nested/out.bin")).unwrap(),
        [0, 1, 255]
    );
    assert!(
        prepare(
            &text.replace("seed.bin", "../outside"),
            &temp.0.join("flow.yml"),
            &root,
            "task"
        )
        .is_err()
    );
}
