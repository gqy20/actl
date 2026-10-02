//! Bounded, content-minimal local history. Display snapshots remain ephemeral.
use crate::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const DAY: u64 = 86_400_000;
const CALL_LIMIT: u64 = 1024 * 1024;
const RETENTION_BUDGET: u64 = 3 * 1024 * 1024 * 1024;
pub const RETENTION_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(15 * 24 * 60 * 60);

#[cfg(test)]
mod retention_policy_tests {
    use super::*;

    #[test]
    fn default_retention_policy_matches_contract() {
        let actual = serde_json::json!({
            "age_ms": 7 * DAY,
            "budget_bytes": RETENTION_BUDGET,
            "call_limit_bytes": CALL_LIMIT,
            "interval_ms": RETENTION_INTERVAL.as_millis() as u64,
        });
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/golden/history_retention_policy.json"
        ))
        .unwrap();
        assert_eq!(actual, expected);
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn task_from_env() -> Result<Option<String>, CtlError> {
    match std::env::var("ACTL_TASK_ID") {
        Ok(id) if valid_id(&id) => Ok(Some(id)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        _ => Err(CtlError::protocol(
            "ACTL_TASK_ID must contain 1-80 ASCII letters, digits, '-' or '_'",
        )),
    }
}

fn io_error(e: impl std::fmt::Display) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("history: {e}"))
}

/// Each invocation owns a file; no shared append stream or heartbeat traffic.
pub struct Journal {
    file: File,
    bytes: u64,
    failed: bool,
    call: String,
    task: Option<String>,
    seq: u64,
    root: PathBuf,
    dropped: u64,
    failure_error: Option<std::io::Error>,
}
impl Journal {
    pub fn open(root: &Path, call: &str, task: Option<String>) -> std::io::Result<Self> {
        Self::open_with_retention(root, call, task, true)
    }
    /// For latency-sensitive callers that schedule retention on a separate worker.
    pub fn open_without_retention(
        root: &Path,
        call: &str,
        task: Option<String>,
    ) -> std::io::Result<Self> {
        Self::open_with_retention(root, call, task, false)
    }
    fn open_with_retention(
        root: &Path,
        call: &str,
        task: Option<String>,
        retain: bool,
    ) -> std::io::Result<Self> {
        let result = (|| {
            if retain {
                maintain(root)?;
            }
            Self::open_inner(root, call, task.clone())
        })();
        if let Err(error) = &result {
            crate::log_health::failure(root, call, task.as_deref(), "journal_open", 0, error);
        }
        result
    }
    fn open_inner(root: &Path, call: &str, task: Option<String>) -> std::io::Result<Self> {
        if !valid_id(call) {
            return Err(std::io::Error::other("invalid call id"));
        }
        let dir = root.join("history");
        fs::create_dir_all(&dir)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(format!("{call}.jsonl")))?;
        Ok(Self {
            file,
            bytes: 0,
            failed: false,
            call: call.into(),
            task,
            seq: 0,
            root: root.into(),
            dropped: 0,
            failure_error: None,
        })
    }

    pub fn record(&mut self, event: &str, data: serde_json::Value) {
        self.record_context(event, data, None);
    }
    pub fn record_context(
        &mut self,
        event: &str,
        data: serde_json::Value,
        context: Option<&crate::state::WorkflowProgress>,
    ) {
        if self.failed {
            self.dropped += 1;
            if let Some(error) = &self.failure_error {
                crate::log_health::failure(
                    &self.root,
                    &self.call,
                    self.task.as_deref(),
                    "journal_write",
                    self.dropped,
                    error,
                );
            }
            return;
        }
        self.seq += 1;
        let mut value = serde_json::json!({"version":1,"call_id":self.call,"task_id":self.task,
            "seq":self.seq,"ts_ms":crate::state::unix_ms(),"event":event,"data":data});
        if let Some(context) = context {
            value["run_id"] = serde_json::json!(context.run_id);
            value["step_id"] = serde_json::json!(context.step_id);
        }
        let result = (|| -> std::io::Result<()> {
            let mut line = serde_json::to_vec(&value)?;
            line.push(b'\n');
            if self.bytes + line.len() as u64 > CALL_LIMIT {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::FileTooLarge,
                    "per-call limit reached",
                ));
            }
            self.file.write_all(&line)?;
            self.file.flush()?;
            self.bytes += line.len() as u64;
            Ok(())
        })();
        if let Err(e) = result {
            self.failed = true;
            self.dropped += 1;
            crate::log_health::failure(
                &self.root,
                &self.call,
                self.task.as_deref(),
                "journal_write",
                self.dropped,
                &e,
            );
            eprintln!("[actl-history] INTERNAL: record incomplete: {e}");
            self.failure_error = Some(e);
        }
    }
}

/// Run the existing cross-process-throttled retention policy outside UI callbacks.
pub fn maintain(root: &Path) -> std::io::Result<()> {
    let dir = root.join("history");
    fs::create_dir_all(&dir)?;
    let _trace = crate::trace::scope("history.retention");
    crate::maintenance::periodic(&dir, crate::state::unix_ms(), || {
        cleanup(&dir, 7 * DAY, RETENTION_BUDGET)
    })
}

/// Only this directory's regular files are eligible. Desktop recovery records are separate.
/// Files touched in the last hour are protected against concurrent writers.
pub use crate::history_retention::cleanup;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskReport {
    pub task_id: String,
    pub outcome: Outcome,
    pub summary: String,
    pub verification: String,
    pub call_ids: Vec<String>,
    pub feedback: Vec<Feedback>,
    /// References only: archiving does not copy or open screenshots or documents.
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<crate::reports::EvidenceRef>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Partial,
    Stopped,
    Failed,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feedback {
    pub source: FeedbackSource,
    pub text: String,
    pub verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<crate::reports::StatementKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_ids: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackSource {
    Agent,
    User,
}

/// Bounded newest-call query. Incomplete last lines are reported, never silently accepted.
pub fn read_history(
    root: &Path,
    task: Option<&str>,
    limit: usize,
) -> Result<serde_json::Value, CtlError> {
    read_filtered(root, task, None, limit)
}

/// One time-ordered stream of every recorded event, across calls and files.
/// The whole-task view that reconstructing a stop otherwise assembles by hand.
pub fn read_timeline(
    root: &Path,
    task: Option<&str>,
    limit: usize,
) -> Result<serde_json::Value, CtlError> {
    if !(1..=2000).contains(&limit) || task.is_some_and(|id| !valid_id(id)) {
        return Err(CtlError::protocol(
            "timeline requires limit 1-2000 and a valid task ID",
        ));
    }
    let dir = root.join("history");
    if !dir.exists() {
        return Ok(
            serde_json::json!({"events":[],"total":0,"truncated":false,"incomplete_files":0}),
        );
    }
    let mut events: Vec<(u64, u64, serde_json::Value)> = Vec::new();
    let mut incomplete = 0u32;
    let mut index = crate::history_index::Index::load(&dir);
    for entry in fs::read_dir(&dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if !entry.file_type().map_err(io_error)?.is_file()
            || !entry.path().extension().is_some_and(|s| s == "jsonl")
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let stamp = crate::history_index::Index::stamp(&entry.metadata().map_err(io_error)?);
        if task.is_some_and(|id| index.foreign(&name, stamp, id)) {
            continue;
        }
        use std::io::{BufRead, BufReader, Read};
        let file = File::open(entry.path()).map_err(io_error)?;
        for (line_index, line) in BufReader::new(file.take(CALL_LIMIT + 1))
            .lines()
            .enumerate()
        {
            let Ok(line) = line else {
                incomplete += 1;
                break;
            };
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(event) => {
                    let known_header = line_index == 0 && index.remember(&name, stamp, &event);
                    if task.is_some_and(|id| event["task_id"].as_str() != Some(id)) {
                        if known_header {
                            break;
                        }
                        continue;
                    }
                    let ts = event["ts_ms"].as_u64().unwrap_or(0);
                    let seq = event["seq"].as_u64().unwrap_or(0);
                    events.push((ts, seq, event));
                }
                Err(_) => {
                    incomplete += 1;
                    break;
                }
            }
        }
    }
    index.save(&dir);
    events.sort_by_key(|(ts, seq, _)| (*ts, *seq));
    let total = events.len();
    let truncated = total > limit;
    if truncated {
        events.drain(..total - limit);
    }
    Ok(serde_json::json!({
        "events": events.into_iter().map(|(_, _, event)| event).collect::<Vec<_>>(),
        "total": total,
        "truncated": truncated,
        "incomplete_files": incomplete
    }))
}
fn read_filtered(
    root: &Path,
    task: Option<&str>,
    step: Option<&str>,
    limit: usize,
) -> Result<serde_json::Value, CtlError> {
    if !(1..=100).contains(&limit) || task.is_some_and(|id| !valid_id(id)) {
        return Err(CtlError::protocol(
            "history requires limit 1-100 and a valid task ID",
        ));
    }
    let dir = root.join("history");
    if !dir.exists() {
        return Ok(serde_json::json!({"calls":[],"incomplete_files":0}));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if entry.file_type().map_err(io_error)?.is_file()
            && entry.path().extension().is_some_and(|s| s == "jsonl")
        {
            files.push((
                entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .map_err(io_error)?,
                entry.path(),
            ));
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut calls = Vec::new();
    let mut incomplete = 0;
    use std::io::{BufRead, BufReader, Read};
    for (_, path) in files {
        let file = File::open(path).map_err(io_error)?;
        let mut lines = BufReader::new(file.take(CALL_LIMIT + 1)).lines();
        let Some(first) = lines.next() else {
            incomplete += 1;
            continue;
        };
        let first = first.map_err(io_error)?;
        let Ok(first) = serde_json::from_str::<serde_json::Value>(&first) else {
            incomplete += 1;
            continue;
        };
        if task.is_some_and(|id| first["task_id"].as_str() != Some(id)) {
            continue;
        }
        let mut events = vec![first];
        let mut valid = true;
        for line in lines {
            match serde_json::from_str::<serde_json::Value>(&line.map_err(io_error)?) {
                Ok(event) => events.push(event),
                Err(_) => {
                    valid = false;
                    break;
                }
            }
        }
        if step.is_some_and(|id| !events.iter().any(|e| e["step_id"].as_str() == Some(id))) {
            continue;
        }
        let integrity = crate::log_integrity::check(&events, valid);
        let finished = integrity == "complete";
        if let Some(id) = step {
            events.retain(|e| {
                e["step_id"].as_str() == Some(id)
                    || matches!(e["event"].as_str(), Some("call_started" | "call_finished"))
            });
        }
        if !valid || !finished {
            incomplete += 1;
        }
        calls.push(serde_json::json!({"complete":finished,"integrity":integrity,"events":events}));
        if calls.len() == limit {
            break;
        }
    }
    Ok(serde_json::json!({"calls":calls,"incomplete_files":incomplete}))
}

/// Reports are caller assertions, not inferred task success. Unique immutable revisions.
pub fn archive(root: &Path, input: &Path) -> Result<PathBuf, CtlError> {
    let file = File::open(input).map_err(io_error)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(65_537)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > 65_536 {
        return Err(CtlError::protocol("task report exceeds 64 KiB"));
    }
    let report: TaskReport = serde_json::from_slice(&bytes)
        .map_err(|e| CtlError::protocol(format!("invalid task report: {e}")))?;
    crate::reports::validate(&report)?;
    if !valid_id(&report.task_id)
        || report.call_ids.iter().any(|id| !valid_id(id))
        || report.summary.trim().is_empty()
        || report.verification.trim().is_empty()
    {
        return Err(CtlError::protocol(
            "task report needs valid IDs, summary and verification",
        ));
    }
    let dir = root.join("tasks");
    fs::create_dir_all(&dir).map_err(io_error)?;
    cleanup(&dir, 30 * DAY, 32 * 1024 * 1024).map_err(io_error)?;
    let path = dir.join(format!(
        "{}-{}.json",
        report.task_id,
        crate::snapshot::new_snapshot_id()
    ));
    let doc = serde_json::json!({"version":1,"archived_ms":crate::state::unix_ms(),"source":"caller","report":report});
    let bytes = serde_json::to_vec_pretty(&doc).map_err(io_error)?;
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(io_error)?;
    if let Err(error) = out.write_all(&bytes).and_then(|_| out.sync_all()) {
        drop(out);
        let _ = fs::remove_file(&path);
        return Err(io_error(error));
    }
    Ok(path)
}

pub fn query(
    root: &Path,
    task: Option<&str>,
    step: Option<&str>,
    limit: usize,
    reports: bool,
) -> Result<serde_json::Value, CtlError> {
    if step.is_some_and(|s| !valid_id(s)) || ((step.is_some() || reports) && task.is_none()) {
        return Err(CtlError::protocol(
            "step/report queries require a task and valid step ID",
        ));
    }
    let mut result = read_filtered(root, task, step, limit).map_err(|mut error| {
        error.evidence =
            Some(serde_json::json!({"logging_health":crate::log_health::read(root, task)}));
        error
    })?;
    result["logging_health"] = crate::log_health::read(root, task);
    if reports {
        result["reports"] = crate::reports::read(root, task.unwrap_or_default(), limit)?;
    }
    Ok(result)
}
