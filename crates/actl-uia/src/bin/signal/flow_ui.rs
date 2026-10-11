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
    let _trace = actl_core::trace::scope("signal.flow_discovery");
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
pub fn request(id: &str, resume: bool, generation: &str) -> Result<(), String> {
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
    let generation = generation.to_owned();
    std::thread::Builder::new()
        .name("signal-flow-request".into())
        .spawn(move || {
            child_request.record("dispatching", None);
            let result = execute(&run, resume, &child_request.request_id, &generation);
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
fn execute(id: &str, resume: bool, request_id: &str, generation: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()
        .map_err(|_| "INTERNAL：无法定位程序")?
        .with_file_name("actl.exe");
    // The executor's stage trace lands next to the journals so display-triggered
    // resumes keep the same evidence a CLI-invoked resume would leave behind.
    let trace_file = actl_core::state::SignalPaths::default()
        .dir
        .join("history")
        .join(format!("executor-{request_id}.trace"));
    let output = std::process::Command::new(exe)
        .env("ACTL_EXECUTION_GENERATION", generation)
        .env("ACTL_FLOW_REQUEST_SOURCE", "signal_control")
        .env("ACTL_FLOW_REQUEST_ID", request_id)
        .env("ACTL_TRACE_FILE", &trace_file)
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
        // run 级停止与执行级急停("急停已启用")是两个概念,文案刻意区分。
        "stopped" => "流程已停止",
        "completed" => "已完成",
        "ready" => "待开始",
        "pending" => "待执行",
        "verifying" => "核验中",
        "uncertain" => "待核对",
        "skipped" => "已跳过",
        _ => "状态未知",
    }
}
/// 详细信息分层(2026-09-30 重排):摘要一行(状态 · 流程 · 原因)→ 步骤统计一行
/// (完成步骤折叠为计数)→ 仅未完成/异常步骤逐条一行(错误码并进同行)→ 每步只留
/// 最近一条恢复观察、压缩为缩进一行 → 结尾一行继续指引。不再逐项等密度罗列。
pub fn details(paths: &SignalPaths, id: &str) -> String {
    let Some(state) = read(paths, id) else {
        return format!("{id} · 状态暂不可读，请稍候");
    };
    let status = state["status"].as_str().unwrap_or("");
    let reason = state["reason"].as_str().map(super::ui_text::reason);
    let mut lines = vec![match reason {
        Some(reason) => format!("{} · {id} · {reason}", status_name(status)),
        None => format!("{} · {id}", status_name(status)),
    }];
    if let Some(steps) = state["steps"].as_array().filter(|s| !s.is_empty()) {
        // 统计行:完成始终展示,其余仅非零项,尾附总数。
        let count = |name: &str| steps.iter().filter(|s| s["status"] == name).count();
        let completed = count("completed");
        let mut stats = format!("完成 {completed}");
        for (name, label) in [
            ("pending", "待执行"),
            ("running", "进行中"),
            ("verifying", "核验中"),
            ("uncertain", "待核对"),
            ("needs_review", "待核对"),
            ("failed", "失败"),
            ("skipped", "已跳过"),
        ] {
            let n = count(name);
            if n > 0 {
                stats.push_str(&format!(" · {label} {n}"));
            }
        }
        stats.push_str(&format!(" · 共 {}", steps.len()));
        lines.push(stats);
        for step in steps.iter().filter(|s| s["status"] != "completed") {
            let sid = step["id"].as_str().unwrap_or("—");
            let step_status = status_name(step["status"].as_str().unwrap_or(""));
            let code = step["error_code"].as_str();
            let attempts = step["attempts"].as_u64().unwrap_or(0);
            lines.push(format!(
                "▸ {sid} · {step_status}{}{}",
                code.map(|c| format!(" · {c}")).unwrap_or_default(),
                if attempts > 1 {
                    format!(" · 第 {attempts} 次尝试")
                } else {
                    String::new()
                }
            ));
            // 障碍提示:把错误码翻译成用户可在应用里核实的障碍(手动修复路径)。
            if let Some(code) = code
                && let Some(hint) = super::ui_text::obstacle_hint(code)
            {
                lines.push(format!("   {hint}"));
            }
            if let Some(event) = step["recovery"]["events"].as_array().and_then(|e| e.last()) {
                lines.push(format!(
                    "   最后观察 {} · 重试 {} · 用时 {} · 剩余 {}",
                    event["outcome"].as_str().unwrap_or("—"),
                    event["retries"],
                    fmt_secs(event["elapsed_ms"].as_u64()),
                    fmt_secs(event["remaining_ms"].as_u64()),
                ));
            }
        }
    }
    // 失败留证直达(三路:整屏截图/原生窗口清单/UIA 快照)。
    let evidence_dir = paths.dir.join("flow-resources").join(id).join("evidence");
    if evidence_dir.is_dir() {
        lines.push(format!(
            "失败留证：{}（整屏截图 · 窗口清单 · UIA 快照）",
            evidence_dir.display()
        ));
    }
    if matches!(status, "paused" | "needs_review") {
        if state["reason"].as_str() == Some("action_failed") {
            lines.push(
                "继续：将自动重发失败步骤的动作——适合你已清除障碍（关弹窗/修正状态）。\
                 若你已手动完成该步，请先在应用中核对结果再继续。"
                    .into(),
            );
        } else {
            lines.push(
                "继续：先核对上一步结果，再使用底部的继续或核对结果按钮。\n重放写步骤须显式 --retry-step。"
                    .into(),
            );
        }
    } else if status == "failed" {
        lines.push("失败：查看异常步骤的原因后修正路线；直接重试不会绕过失败。".into());
    } else if status == "stopped" {
        lines.push("流程已停止：旧调用不会自动恢复；可核对后继续此任务，或发起新任务。".into());
    }
    lines.join("\n")
}

/// 毫秒人性化:≥1s 显示一位小数秒,不足 1s 显示毫秒。
fn fmt_secs(ms: Option<u64>) -> String {
    match ms {
        Some(ms) if ms >= 1000 => format!("{:.1}s", ms as f64 / 1000.0),
        Some(ms) => format!("{ms}ms"),
        None => "—".into(),
    }
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
            ("resume_check_failed", "恢复核验未通过"),
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
    fn details_are_layered_and_completed_steps_collapse_into_stats() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let dir = paths.dir.join("flows").join("layered");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("state.json"),
            serde_json::json!({
                "status":"needs_review","reason":"action_failed",
                "steps":[
                    {"id":"open","status":"completed"},
                    {"id":"fill","status":"completed"},
                    {"id":"cell-readback","status":"uncertain","error_code":"NOT_FOUND","attempts":2,
                     "recovery":{"events":[{"outcome":"recovered","retries":3,"elapsed_ms":14700,"remaining_ms":300}]}},
                    {"id":"close-frame","status":"skipped"}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let text = details(&paths, "layered");
        let lines: Vec<&str> = text.lines().collect();
        // 摘要一行:状态 · 流程 · 原因三合一。
        assert_eq!(
            lines[0], "待核对 · layered · 操作未完成，请先核对已有结果。",
            "{text}"
        );
        // 统计行:完成折叠为计数,含总数。
        assert!(
            lines[1].contains("完成 2") && lines[1].contains("共 4"),
            "{text}"
        );
        // completed 步骤不再单独占行。
        assert!(
            !text.contains("\nopen ·") && !text.contains("fill · 已完成"),
            "{text}"
        );
        // 异常步骤一行合并错误码与尝试计数;障碍提示紧随;恢复观察压缩为一行且数字人性化。
        assert!(
            text.contains("▸ cell-readback · 待核对 · NOT_FOUND · 第 2 次尝试"),
            "{text}"
        );
        assert!(
            text.contains("障碍提示：目标可能尚未渲染或已变化"),
            "{text}"
        );
        assert!(
            text.contains("最后观察 recovered · 重试 3 · 用时 14.7s · 剩余 300ms"),
            "{text}"
        );
        // action_failed 上下文:指引明示"点继续将自动重发动作"与代做警示。
        assert!(text.contains("继续：将自动重发失败步骤的动作"), "{text}");
        assert!(text.contains("若你已手动完成该步"), "{text}");
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
    matches!(
        state["status"].as_str(),
        Some("paused" | "needs_review" | "stopped")
    ) && state["updated_ms"]
        .as_u64()
        .is_some_and(|t| now >= t && now - t >= 800)
}
/// Every native entry re-reads persisted state before dispatch; this never grants consent.
pub fn resume_guard(state: Option<&serde_json::Value>, now: u64) -> Result<(), String> {
    let state = state.ok_or("任务状态未确认，请稍后查看")?;
    if !matches!(
        state["status"].as_str(),
        Some("paused" | "needs_review" | "stopped")
    ) {
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
        s["status"] = "stopped".into();
        assert!(continue_ready(&s, 1000));
        for status in ["running", "completed", "failed"] {
            s["status"] = status.into();
            assert!(!continue_ready(&s, 1000));
        }
    }
    #[test]
    fn every_entry_rejects_unknown_stopped_and_unsettled_resume() {
        let state = serde_json::json!({"status":"paused", "updated_ms":100});
        assert!(resume_guard(Some(&state), 900).is_ok());
        assert!(resume_guard(Some(&state), 899).is_err());
        assert!(resume_guard(None, 900).is_err());
        for status in ["running", "failed", "completed"] {
            let state = serde_json::json!({"status":status, "updated_ms":100});
            assert!(resume_guard(Some(&state), 1000).is_err());
        }
    }
}
