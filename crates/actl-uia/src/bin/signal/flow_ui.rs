//! UI-only workflow binding and non-blocking request supervision.
use actl_core::state::SignalPaths;
use std::sync::{Mutex, mpsc};

pub fn read(paths: &SignalPaths, id: &str) -> Option<serde_json::Value> {
    if !actl_core::history::valid_id(id) {
        return None;
    }
    serde_json::from_slice(
        &std::fs::read(paths.dir.join("flows").join(id).join("state.json")).ok()?,
    )
    .ok()
}
// Bind once when a window opens. Unrelated CLI calls must never retarget its buttons.
pub fn discover(paths: &SignalPaths) -> Option<String> {
    let states = paths.read_calls();
    if let Some(id) = states
        .iter()
        .filter_map(|s| s.workflow.as_ref().map(|f| (s.ts_ms, &f.run_id)))
        .max_by_key(|(ts, _)| *ts)
        .map(|(_, id)| id.clone())
        && read(paths, &id).is_some()
    {
        return Some(id);
    }
    let entries = std::fs::read_dir(paths.dir.join("flows")).ok()?;
    let mut ids = entries.flatten().filter_map(|entry| {
        let id = entry.file_name().into_string().ok()?;
        let state = read(paths, &id)?;
        (state["status"].as_str()? != "completed").then_some(id)
    });
    let first = ids.next()?;
    // Multiple archived unfinished runs need an explicit choice, never a guess.
    ids.next().is_none().then_some(first)
}

struct Job {
    id: String,
    resume: bool,
    receiver: mpsc::Receiver<Result<(), String>>,
}
static JOBS: Mutex<Vec<Job>> = Mutex::new(Vec::new());
static MESSAGE: Mutex<String> = Mutex::new(String::new());
pub fn pending(id: &str, resume: bool) -> bool {
    JOBS.lock()
        .ok()
        .is_some_and(|jobs| jobs.iter().any(|j| j.id == id && j.resume == resume))
}
pub fn message() -> String {
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.retain(|job| match job.receiver.try_recv() {
            Ok(result) => {
                if let Ok(mut message) = MESSAGE.lock() {
                    *message = result.err().unwrap_or_else(|| {
                        if job.resume {
                            "流程状态已更新".into()
                        } else {
                            "已请求暂停，等待当前操作结束".into()
                        }
                    });
                }
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Ok(mut message) = MESSAGE.lock() {
                    *message = "INTERNAL：操作线程已退出".into();
                }
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
        });
    }
    MESSAGE.lock().map(|s| s.clone()).unwrap_or_default()
}
pub fn clear_message() {
    if let Ok(mut m) = MESSAGE.lock() {
        m.clear();
    }
}
pub fn request(id: &str, resume: bool) -> Result<(), String> {
    if !actl_core::history::valid_id(id) {
        return Err("PROTOCOL：无效的流程标识".into());
    }
    let mut jobs = JOBS.lock().map_err(|_| "INTERNAL：无法读取操作状态")?;
    if jobs.iter().any(|j| j.id == id && j.resume == resume) {
        return Ok(());
    }
    let (tx, receiver) = mpsc::channel();
    let mut request =
        actl_core::control_log::Request::new(id, if resume { "resume" } else { "pause" });
    request.request_id = actl_core::snapshot::new_snapshot_id();
    request.source = "signal_control".into();
    request.record("requested", None);
    let child_request = request.clone();
    let run = id.to_owned();
    std::thread::Builder::new()
        .name("signal-flow-request".into())
        .spawn(move || {
            child_request.record("dispatching", None);
            let result = execute(&run, resume, &child_request.request_id);
            child_request.record(
                if result.is_ok() {
                    "child_returned"
                } else {
                    "failed"
                },
                result.as_ref().err().map(|message| {
                    serde_json::from_value(serde_json::Value::String(
                        message.split('：').next().unwrap_or("").into(),
                    ))
                    .unwrap_or(actl_core::ErrorCode::Internal)
                }),
            );
            let _ = tx.send(result);
        })
        .map_err(|_| {
            request.record("thread_start_failed", Some(actl_core::ErrorCode::Internal));
            "INTERNAL：无法启动操作线程"
        })?;
    jobs.push(Job {
        id: id.into(),
        resume,
        receiver,
    });
    if let Ok(mut message) = MESSAGE.lock() {
        *message = if resume {
            "正在继续流程"
        } else {
            "正在请求暂停"
        }
        .into();
    }
    Ok(())
}
fn execute(id: &str, resume: bool, request_id: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()
        .map_err(|_| "INTERNAL：无法定位程序")?
        .with_file_name("actl.exe");
    let output = std::process::Command::new(exe)
        .env("ACTL_FLOW_REQUEST_SOURCE", "signal_control")
        .env("ACTL_FLOW_REQUEST_ID", request_id)
        .args([
            if resume { "flow-resume" } else { "flow-pause" },
            "--id",
            id,
        ])
        .stdin(std::process::Stdio::null())
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| "INTERNAL：无法启动流程执行器")?;
    outcome(output.status.success(), &output.stdout)
}
fn outcome(success: bool, bytes: &[u8]) -> Result<(), String> {
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "INTERNAL：执行器未返回有效结果")?;
    if success && body["ok"] == true {
        return Ok(());
    }
    Err(format!(
        "{}：{}",
        body["error"]["code"].as_str().unwrap_or("INTERNAL"),
        body["error"]["message"]
            .as_str()
            .unwrap_or("流程操作未完成")
    ))
}
pub fn status_name(status: &str) -> &str {
    match status {
        "running" => "运行中",
        "paused" => "已暂停",
        "needs_review" => "待核对",
        "failed" => "失败",
        "stopped" => "已停止",
        "completed" => "已完成",
        "ready" => "待开始",
        "pending" => "待执行",
        "verifying" => "核对中",
        "uncertain" => "待核对",
        _ => "状态未知",
    }
}
pub fn details(paths: &SignalPaths, id: &str) -> String {
    let Some(state) = read(paths, id) else {
        return format!("流程 · {id}\n状态暂不可读，请稍候");
    };
    let status = state["status"].as_str().unwrap_or("");
    let mut text = format!("流程 · {id}\n{}", status_name(status));
    if let Some(reason) = state["reason"].as_str() {
        let reason = super::ui_text::reason(reason);
        text.push_str(&format!(
            "\n{reason}\n原因标识：{}",
            state["reason"].as_str().unwrap_or("")
        ));
    }
    if let Some(steps) = state["steps"].as_array() {
        for step in steps {
            text.push_str(&format!(
                "\n{} · {}",
                step["id"].as_str().unwrap_or("—"),
                status_name(step["status"].as_str().unwrap_or(""))
            ));
            if let Some(code) = step["error_code"].as_str() {
                text.push_str(&format!(" · {code}"));
            }
        }
    }
    text
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_failures_have_readable_reasons() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let dir = paths.dir.join("flows").join("review");
        std::fs::create_dir_all(&dir).unwrap();
        for (reason, expected) in [
            ("resume_check_failed", "恢复检查未通过"),
            ("binding_validation_failed", "步骤输入未通过检查"),
        ] {
            std::fs::write(
                dir.join("state.json"),
                serde_json::to_vec(&serde_json::json!({"status":"paused", "reason":reason}))
                    .unwrap(),
            )
            .unwrap();
            let text = details(&paths, "review");
            assert!(text.contains(expected), "{text}");
        }
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
    #[test]
    fn executor_failure_is_not_reported_as_success() {
        assert!(
            outcome(
                false,
                br#"{"ok":false,"error":{"code":"ABORTED","message":"stop"}}"#
            )
            .unwrap_err()
            .contains("ABORTED")
        );
        assert!(outcome(true, b"").is_err());
        assert!(outcome(false, br#"{"ok":true}"#).is_err());
        assert!(outcome(true, br#"{"ok":true}"#).is_ok());
    }
    #[test]
    fn paused_run_survives_missing_call_snapshot_without_ambiguous_selection() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let dir = paths.dir.join("flows").join("first");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), br#"{"status":"paused"}"#).unwrap();
        assert_eq!(discover(&paths).as_deref(), Some("first"));
        let dir = paths.dir.join("flows").join("second");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), br#"{"status":"paused"}"#).unwrap();
        assert!(discover(&paths).is_none());
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
}

/// A click aimed at Pause must not instantly reverse a just-detected takeover.
pub fn continue_ready(state: &serde_json::Value, now: u64) -> bool {
    matches!(state["status"].as_str(), Some("paused" | "needs_review"))
        && state["updated_ms"]
            .as_u64()
            .is_some_and(|t| now >= t && now - t >= 800)
}
/// Every native entry re-reads persisted state before dispatch; this never grants consent.
pub fn resume_guard(
    state: Option<&serde_json::Value>,
    stopped: bool,
    now: u64,
) -> Result<(), String> {
    if stopped {
        return Err("请先解除急停，任务不会自动继续".into());
    }
    let state = state.ok_or("任务状态未确认，请稍后查看")?;
    if !matches!(state["status"].as_str(), Some("paused" | "needs_review")) {
        return Err("当前任务不可继续，请查看详细信息".into());
    }
    if !continue_ready(state, now) {
        return Err("正在确认暂停状态，请稍候".into());
    }
    Ok(())
}
#[cfg(test)]
mod continue_tests {
    use super::*;
    #[test]
    fn continuation_requires_settled_paused_state() {
        let mut s = serde_json::json!({"status":"paused","updated_ms":100});
        assert!(!continue_ready(&s, 99));
        assert!(!continue_ready(&s, 899));
        assert!(continue_ready(&s, 900));
        for status in ["running", "completed", "stopped", "failed"] {
            s["status"] = status.into();
            assert!(!continue_ready(&s, 1000));
        }
    }
    #[test]
    fn every_entry_rejects_unknown_stopped_and_unsettled_resume() {
        let state = serde_json::json!({"status":"paused", "updated_ms":100});
        assert!(resume_guard(Some(&state), false, 900).is_ok());
        assert!(resume_guard(Some(&state), true, 900).is_err());
        assert!(resume_guard(Some(&state), false, 899).is_err());
        assert!(resume_guard(None, false, 900).is_err());
        for status in ["running", "failed", "completed", "stopped"] {
            let state = serde_json::json!({"status":status, "updated_ms":100});
            assert!(resume_guard(Some(&state), false, 1000).is_err());
        }
    }
}
