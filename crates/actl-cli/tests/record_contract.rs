//! GUI 操作录制 R1 契约(docs/06「GUI 操作录制 R1」spec 提交):会话协议
//! (start/status/stop/worker)、终态语义、abandoned 数据收口、编译产物
//! flow-validate 兼容。全部离线——ACTL_RECORD_DISABLE_HOOKS=1 测试缝跳过
//! 钩子线程(docs/06 测试缝节),不碰真桌面、不捕获真实输入;真机钩子
//! 验收另轮(docs/77 先例)。
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;

struct Sandbox {
    root: PathBuf,
}
impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-record-cli-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_actl"));
        cmd.env("LOCALAPPDATA", &self.root)
            .env("ACTL_RECORD_DISABLE_HOOKS", "1")
            .env_remove("ACTL_FLOW_REQUEST_SOURCE")
            .env_remove("ACTL_FLOW_REQUEST_ID")
            .env_remove("ACTL_TASK_ID")
            .env_remove("ACTL_TASK_CONFIG");
        cmd
    }
    fn invoke(&self, args: &[&str]) -> (i32, Value) {
        let out = self.command().args(args).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let data = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("envelope json expected, got: {text}"));
        (out.status.code().unwrap(), data)
    }
    /// 记录目录内写伪造会话现场(死亡 pid + 已有 trace),驱动 abandoned 收口路径。
    fn fabricate_session(&self, id: &str, trace: &str) {
        let dir = self.root.join("actl").join("records").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("trace.jsonl"), trace).unwrap();
        std::fs::write(
            dir.join("state.json"),
            serde_json::to_vec(&json!({
                "id": id, "status": "recording", "pid": 999999,
                "started_ms": 1, "max_seconds": 60, "events": 3, "skipped_own": 0,
            }))
            .unwrap(),
        )
        .unwrap();
    }
    /// 剔除易变字段后比对 golden(duration_ms + pid)。
    fn assert_golden(&self, result: Value, golden: &str) {
        let mut result = result;
        let mut object = result.as_object().cloned().unwrap();
        object.remove("duration_ms");
        result = Value::Object(object);
        let mut object = result["data"].as_object().cloned().unwrap();
        object.remove("pid");
        result["data"] = Value::Object(object);
        assert_eq!(
            result,
            serde_json::from_str::<Value>(golden).unwrap(),
            "golden mismatch"
        );
    }
}
impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn worker_finalizes_on_duration_cap() {
    let s = Sandbox::new();
    let (code, result) = s.invoke(&["record-worker", "--id", "w1", "--max-seconds", "1"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["ok"], true);
    s.assert_golden(result, include_str!("golden/record_worker.json"));
    // 空事件会话的草稿如实被 flow schema 拒绝(0 步仅限 explore)——空录制没有
    // 可运行内容,不是编译缺陷。
    let draft = s
        .root
        .join("actl")
        .join("records")
        .join("w1")
        .join("draft.yml");
    let (code, result) = s.invoke(&["flow-validate", "--file", &draft.to_string_lossy()]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert_eq!(
        result["error"]["message"],
        "expected 1-256 steps (empty only with explore)"
    );
}

#[test]
fn start_status_stop_cycle() {
    let s = Sandbox::new();
    let (code, result) = s.invoke(&["record-start", "--id", "s1", "--max-seconds", "60"]);
    assert_eq!(code, 0, "{result}");
    s.assert_golden(result, include_str!("golden/record_start.json"));

    let (code, result) = s.invoke(&["record-status", "--id", "s1"]);
    assert_eq!(code, 0, "{result}");
    s.assert_golden(result, include_str!("golden/record_status.json"));

    let (code, result) = s.invoke(&["record-stop", "--id", "s1"]);
    assert_eq!(code, 0, "{result}");
    s.assert_golden(result, include_str!("golden/record_stop.json"));

    // 停止后 idempotent:再次 stop 返回同一摘要,不报错。
    let (code, again) = s.invoke(&["record-stop", "--id", "s1"]);
    assert_eq!(code, 0, "{again}");
    s.assert_golden(again, include_str!("golden/record_stop.json"));

    let (code, result) = s.invoke(&["record-status"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["active"].as_array().map(Vec::len), Some(0));
    assert_eq!(result["data"]["sessions"], 1);
}

#[test]
fn double_start_is_rejected() {
    let s = Sandbox::new();
    let (code, _) = s.invoke(&["record-start", "--id", "a1", "--max-seconds", "60"]);
    assert_eq!(code, 0);
    let (code, result) = s.invoke(&["record-start", "--id", "a2", "--max-seconds", "60"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("already active")
    );
    let _ = s.invoke(&["record-stop", "--id", "a1"]);
}

#[test]
fn stop_requires_known_session() {
    let s = Sandbox::new();
    let (code, result) = s.invoke(&["record-stop", "--id", "missing"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown session id")
    );
    let (code, result) = s.invoke(&["record-stop"]);
    assert_eq!(code, 2, "{result}");
    assert_eq!(result["error"]["code"], "PROTOCOL");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no active recording session")
    );
}

#[test]
fn abandoned_session_gets_data_salvage() {
    let s = Sandbox::new();
    let trace = concat!(
        "{\"kind\":\"session\",\"id\":\"salv1\",\"started_ms\":1,\"max_seconds\":60,\"pid\":999999}\n",
        "{\"t\":10,\"kind\":\"click\",\"button\":\"left\",\"x\":1,\"y\":2,\"window\":{\"hwnd\":1,\"title\":\"记事本\",\"pid\":2,\"rect\":[0,0,10,10],\"dpi\":96},\"element\":{\"automation_id\":\"btn\",\"name\":\"保存\",\"control_type\":\"Button\",\"runtime_id\":[1],\"is_password\":false},\"lookup\":\"ok\"}\n",
        "{\"t\":20,\"kind\":\"key\",\"vk\":83,\"mods\":[\"ctrl\"],\"ime\":false,\"focus\":{\"hwnd\":1,\"title\":\"记事本\"},\"focus_element\":{\"automation_id\":\"edit\",\"name\":\"编辑\",\"control_type\":\"Edit\",\"runtime_id\":[2],\"is_password\":false}}\n",
    );
    s.fabricate_session("salv1", trace);
    let (code, result) = s.invoke(&["record-status", "--id", "salv1"]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["status"], "abandoned");

    let (code, result) = s.invoke(&["record-stop", "--id", "salv1"]);
    assert_eq!(code, 0, "{result}");
    s.assert_golden(result, include_str!("golden/record_salvage.json"));

    // 收口产物必须 flow-validate 通过(编译契约:非空草稿可验证)。
    let draft = s
        .root
        .join("actl")
        .join("records")
        .join("salv1")
        .join("draft.yml");
    let (code, result) = s.invoke(&["flow-validate", "--file", &draft.to_string_lossy()]);
    assert_eq!(code, 0, "{result}");
    assert_eq!(result["data"]["valid"], true);
    assert_eq!(result["data"]["steps"], 2);
}
