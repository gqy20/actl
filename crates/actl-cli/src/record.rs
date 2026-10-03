//! GUI 操作录制命令族(docs/06「GUI 操作录制 R1」)。
//! start spawn 分离 worker(actl-uia 捕获层);worker 终态时在本层做机械编译
//! (actl-flow)并落 draft.yml;stop 写 stop-requested 标记并等终态(abandoned
//! 会话做数据收口——编译既有 trace,不复活进程);status 只读。
//! 会话目录 = %LOCALAPPDATA%\actl\records\<id>\{state.json, trace.jsonl,
//! stop-requested, draft.yml}。

use actl_core::{CtlError, state::unix_ms};
use actl_flow::record::{compile, render};
use actl_uia::record::{self as capture, SessionConfig};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use crate::cli::RecordCommand;

pub fn dispatch(command: &RecordCommand) -> Result<Value, CtlError> {
    match command {
        RecordCommand::Start { id, max_seconds } => start(id.as_deref(), *max_seconds),
        RecordCommand::Worker { id, max_seconds } => worker(id, *max_seconds),
        RecordCommand::Stop { id } => stop(id.as_deref()),
        RecordCommand::Status { id } => status(id.as_deref()),
    }
}

fn session_dir(id: &str) -> Result<PathBuf, CtlError> {
    if !actl_core::history::valid_id(id) {
        return Err(CtlError::protocol(
            "invalid session id (a-z0-9-_, <=80 chars)",
        ));
    }
    Ok(capture::records_dir().join(id))
}

fn read_state(dir: &Path) -> Option<Value> {
    fs::read(dir.join("state.json"))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
}

fn write_state_file(dir: &Path, state: &Value) -> Result<(), CtlError> {
    let tmp = dir.join("state.json.tmp");
    let bytes = serde_json::to_vec(state).map_err(|e| CtlError::internal(e.to_string()))?;
    fs::write(&tmp, bytes).map_err(|e| CtlError::internal(format!("state write: {e}")))?;
    fs::rename(&tmp, dir.join("state.json"))
        .map_err(|e| CtlError::internal(format!("state rename: {e}")))
}

/// 派生态:state=recording 而 pid 已死 → abandoned(如实报告,不修复进程)。
fn derived_status(state: &Value) -> String {
    match state["status"].as_str().unwrap_or_default() {
        "recording" => {
            let pid = state["pid"].as_u64().unwrap_or_default() as u32;
            if pid != 0 && !capture::pid_alive(pid) {
                "abandoned".into()
            } else {
                "recording".into()
            }
        }
        other => other.to_string(),
    }
}

fn active_ids() -> Vec<String> {
    let root = capture::records_dir();
    let Ok(entries) = fs::read_dir(&root) else {
        return vec![];
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().into_string().ok()?;
            let state = read_state(&root.join(&id))?;
            (derived_status(&state) == "recording").then_some(id)
        })
        .collect();
    ids.sort();
    ids
}

/// 无 --id 时按活跃会话发现;唯一才执行,歧义要求显式 --id(flow discover 同纪律)。
fn require_session(id: Option<&str>) -> Result<String, CtlError> {
    if let Some(id) = id {
        if read_state(&session_dir(id)?).is_none() {
            return Err(CtlError::protocol("unknown session id"));
        }
        return Ok(id.into());
    }
    let active = active_ids();
    match active.len() {
        0 => Err(CtlError::protocol("no active recording session")),
        1 => Ok(active[0].clone()),
        _ => Err(CtlError::protocol(
            "multiple active sessions — pass --id explicitly",
        )),
    }
}

fn start(id: Option<&str>, max_seconds: u64) -> Result<Value, CtlError> {
    if !active_ids().is_empty() {
        return Err(CtlError::protocol(
            "a recording session is already active (record-stop it first)",
        ));
    }
    let id = id
        .map(str::to_string)
        .unwrap_or_else(|| format!("rec-{}", unix_ms()));
    let dir = session_dir(&id)?;
    if read_state(&dir).is_some() {
        return Err(CtlError::protocol("session id already used"));
    }
    fs::create_dir_all(&dir).map_err(|e| CtlError::internal(format!("records dir: {e}")))?;
    let pid = capture::spawn_worker(&id, max_seconds)?;
    // 等 state.json 出现(≤3s)确认 worker 起来;此后异步失败由 record-status 如实暴露。
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut state = None;
    while Instant::now() < deadline {
        if let Some(found) = read_state(&dir) {
            state = Some(found);
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let state = state.ok_or_else(|| CtlError::internal("worker did not create session state"))?;
    if state["pid"].as_u64() != Some(pid as u64) {
        return Err(CtlError::internal("session state pid mismatch"));
    }
    Ok(json!({
        "id": id, "status": "recording", "pid": pid,
        "dir": format!("records/{id}"),
        "trace": format!("records/{id}/trace.jsonl"),
        "max_seconds": max_seconds,
    }))
}

/// 内部会话进程(record-start spawn;直跑同语义——契约测试/诊断通道)。
fn worker(id: &str, max_seconds: u64) -> Result<Value, CtlError> {
    let dir = session_dir(id)?;
    if read_state(&dir).is_some() {
        return Err(CtlError::protocol("session id already used"));
    }
    let outcome = capture::run_session(&SessionConfig {
        id: id.into(),
        dir: dir.clone(),
        max_seconds,
    })?;
    let draft = compile(&outcome.events);
    let yaml = render(
        &draft,
        &outcome.stop_reason,
        &format!("records/{id}/trace.jsonl"),
    );
    let draft_path = dir.join("draft.yml");
    fs::write(&draft_path, yaml.as_bytes())
        .map_err(|e| CtlError::internal(format!("draft write: {e}")))?;
    let prior = read_state(&dir).unwrap_or_else(|| json!({}));
    write_state_file(
        &dir,
        &json!({
            "id": id,
            "status": "completed",
            "pid": prior.get("pid").cloned().unwrap_or(json!(0)),
            "started_ms": prior.get("started_ms").cloned().unwrap_or(json!(0)),
            "stopped_ms": unix_ms(),
            "max_seconds": max_seconds,
            "events": draft.events,
            "skipped_own": outcome.skipped_own,
            "stop_reason": outcome.stop_reason,
            "steps": draft.steps.len(),
            "todos": draft.todos.len(),
        }),
    )?;
    Ok(json!({
        "id": id, "status": "completed", "stop_reason": outcome.stop_reason,
        "events": draft.events, "steps": draft.steps.len(), "todos": draft.todos.len(),
        "skipped_own": outcome.skipped_own,
        "yaml": yaml,
        "trace": format!("records/{id}/trace.jsonl"),
        "draft": format!("records/{id}/draft.yml"),
        "dir": format!("records/{id}"),
    }))
}

fn stop(id: Option<&str>) -> Result<Value, CtlError> {
    let id = require_session(id)?;
    let dir = session_dir(&id)?;
    let state = read_state(&dir).ok_or_else(|| CtlError::protocol("unknown session id"))?;
    match derived_status(&state).as_str() {
        "recording" => {
            fs::write(dir.join("stop-requested"), b"")
                .map_err(|e| CtlError::internal(format!("stop marker: {e}")))?;
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut final_state = None;
            while Instant::now() < deadline {
                if let Some(found) = read_state(&dir)
                    && matches!(found["status"].as_str(), Some("completed") | Some("failed"))
                {
                    final_state = Some(found);
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            if final_state.is_none() {
                return Err(CtlError::internal(
                    "stop wait timed out (worker did not finalize in 15s)",
                ));
            }
            summarize(&id, &dir, final_state)
        }
        "abandoned" => {
            // 数据收口(不复活进程):worker 已死但 trace 在,显式 stop 时编译既有事件。
            salvage(&id, &dir)
        }
        _ => summarize(&id, &dir, Some(state)),
    }
}

fn salvage(id: &str, dir: &Path) -> Result<Value, CtlError> {
    let prior = read_state(dir).ok_or_else(|| CtlError::protocol("unknown session id"))?;
    let text = fs::read_to_string(dir.join("trace.jsonl"))
        .map_err(|e| CtlError::internal(format!("trace read: {e}")))?;
    let trace: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let draft = compile(&trace);
    let yaml = render(&draft, "abandoned", &format!("records/{id}/trace.jsonl"));
    fs::write(dir.join("draft.yml"), yaml.as_bytes())
        .map_err(|e| CtlError::internal(format!("draft write: {e}")))?;
    write_state_file(
        dir,
        &json!({
            "id": id,
            "status": "completed",
            "pid": prior.get("pid").cloned().unwrap_or(json!(0)),
            "started_ms": prior.get("started_ms").cloned().unwrap_or(json!(0)),
            "stopped_ms": unix_ms(),
            "max_seconds": prior.get("max_seconds").cloned().unwrap_or(json!(0)),
            "events": draft.events,
            "skipped_own": prior.get("skipped_own").cloned().unwrap_or(json!(0)),
            "stop_reason": "abandoned",
            "steps": draft.steps.len(),
            "todos": draft.todos.len(),
        }),
    )?;
    Ok(json!({
        "id": id, "status": "completed", "stop_reason": "abandoned",
        "events": draft.events, "steps": draft.steps.len(), "todos": draft.todos.len(),
        "skipped_own": prior.get("skipped_own").cloned().unwrap_or(json!(0)),
        "yaml": yaml,
        "trace": format!("records/{id}/trace.jsonl"),
        "draft": format!("records/{id}/draft.yml"),
        "dir": format!("records/{id}"),
    }))
}

fn summarize(id: &str, dir: &Path, state: Option<Value>) -> Result<Value, CtlError> {
    let state = state
        .or_else(|| read_state(dir))
        .ok_or_else(|| CtlError::protocol("unknown session id"))?;
    let yaml = fs::read_to_string(dir.join("draft.yml")).ok();
    Ok(json!({
        "id": id,
        "status": derived_status(&state),
        "stop_reason": state.get("stop_reason").cloned().unwrap_or(Value::Null),
        "events": state.get("events").cloned().unwrap_or(json!(0)),
        "steps": state.get("steps").cloned().unwrap_or(json!(0)),
        "todos": state.get("todos").cloned().unwrap_or(json!(0)),
        "skipped_own": state.get("skipped_own").cloned().unwrap_or(json!(0)),
        "error": state.get("error").cloned().unwrap_or(Value::Null),
        "yaml": yaml,
        "trace": format!("records/{id}/trace.jsonl"),
        "draft": format!("records/{id}/draft.yml"),
        "dir": format!("records/{id}"),
    }))
}

fn trace_stats(dir: &Path) -> Value {
    let Ok(text) = fs::read_to_string(dir.join("trace.jsonl")) else {
        return json!({"lines": 0});
    };
    let (mut lines, mut clicks, mut keys, mut masked, mut injected) = (0, 0, 0, 0, 0);
    for line in text.lines() {
        lines += 1;
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            match event["kind"].as_str().unwrap_or_default() {
                "click" => clicks += 1,
                "key" => keys += 1,
                _ => {}
            }
            if event["injected"].as_bool() == Some(true) {
                injected += 1;
            }
            if event["masked"].as_bool() == Some(true) {
                masked += 1;
            }
        }
    }
    json!({"lines": lines, "clicks": clicks, "keys": keys, "masked": masked, "injected": injected})
}

fn status(id: Option<&str>) -> Result<Value, CtlError> {
    match id {
        Some(id) => {
            let dir = session_dir(id)?;
            let state = read_state(&dir).ok_or_else(|| CtlError::protocol("unknown session id"))?;
            let mut data = summarize(id, &dir, Some(state))?;
            if let Some(object) = data.as_object_mut() {
                object.insert("trace_stats".into(), trace_stats(&dir));
            }
            Ok(data)
        }
        None => {
            let root = capture::records_dir();
            let sessions = fs::read_dir(&root)
                .map(|entries| entries.flatten().count())
                .unwrap_or(0);
            Ok(json!({"active": active_ids(), "sessions": sessions}))
        }
    }
}
