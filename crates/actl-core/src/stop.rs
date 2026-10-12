//! Persistent cancellation generations. Recovery never revives an old invocation.
use crate::{
    CtlError, ErrorCode,
    state::{SignalPaths, unix_ms},
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    io::{ErrorKind, Write},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecord {
    pub version: u32,
    pub generation: String,
    pub requested_ms: u64,
    pub source: String,
    pub pid: u32,
    pub display_epoch: Option<String>,
}
/// 损坏态(坏格式/版本不匹配):execution-recover 修复路径——原语义。
fn corrupt(detail: impl ToString) -> CtlError {
    CtlError::with_evidence(
        ErrorCode::NotActionable,
        "execution state requires repair",
        serde_json::json!({
            "reason":"execution_state_invalid", "detail":detail.to_string(),
            "recovery":"execution-recover", "retry_input":false
        }),
    )
}
/// 基础设施 IO 故障(存储目录不可写/磁盘耗尽/锁不可得):INTERNAL——
/// 修复路径不适用(修 execution-recover 修不了磁盘),按硬规则 3 如实
/// 归基础设施故障码(2026-10-12 故障注入实证:不可写存储曾把
/// list-windows 误导成 NOT_ACTIONABLE"requires repair")。
fn infra(detail: impl ToString) -> CtlError {
    CtlError::with_evidence(
        ErrorCode::Internal,
        "execution state storage unavailable",
        serde_json::json!({
            "reason":"execution_state_io", "detail":detail.to_string(),
            "retry_input":false
        }),
    )
}

/// StopRecord 序列化失败是进程内逻辑错误(数据结构自产)——纯 INTERNAL,
/// 与存储 IO 故障(infra)和损坏态(corrupt)三分。
fn stop_record_json(error: serde_json::Error) -> CtlError {
    CtlError::internal(format!("stop record serialize: {error}"))
}
fn cancelled() -> CtlError {
    CtlError::with_evidence(
        ErrorCode::Aborted,
        "this invocation was stopped; start a new invocation and verify unfinished work",
        serde_json::json!({
            "reason":"execution_generation_changed", "retry_input":false
        }),
    )
}
impl SignalPaths {
    pub fn execution_status(&self) -> serde_json::Value {
        let snapshot = (|| -> Result<_, CtlError> {
            let _lock = self.control_lock()?;
            let generation = self.read_generation()?;
            let stop = self.read_stop(&generation)?;
            Ok((generation, stop))
        })();
        match snapshot {
            Ok((generation, stop)) => {
                serde_json::json!({"state":if stop.is_some(){"stopped"}else{"ready"},
                "generation":generation,"stop":stop,"new_task_requires_consent":true,"old_calls_resume":false})
            }
            Err(e) => {
                serde_json::json!({"state":"repair_required","error":e.evidence,"new_task_requires_consent":true,"old_calls_resume":false})
            }
        }
    }

    /// 错误路径专用(2026-10-06 基建②完整状态联动):**无锁**尽力读——错误
    /// 构造不得阻塞(控制锁可能被运行中的调用持有);读不出标 unavailable,
    /// 不影响错误语义。诊断面:拒绝错误自述 execution 态 + 检查指引。
    pub fn execution_status_best_effort(&self) -> serde_json::Value {
        let generation = fs::read_to_string(self.generation_file()).ok();
        let stop = generation
            .as_deref()
            .and_then(|g| self.read_stop(g).ok().flatten());
        serde_json::json!({
            "state": if stop.is_some() { "stopped" } else if generation.is_some() { "ready" } else { "unavailable" },
            "generation": generation,
            "paused_runs_hint": "actl execution-status / actl flow-status",
        })
    }
    fn generation_file(&self) -> std::path::PathBuf {
        self.dir.join("stop-generation")
    }
    fn control_lock(&self) -> Result<fs::File, CtlError> {
        fs::create_dir_all(&self.dir).map_err(infra)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join("stop-control.lock"))
            .map_err(infra)?;
        file.lock().map_err(infra)?;
        Ok(file)
    }
    fn read_generation(&self) -> Result<String, CtlError> {
        match fs::read_to_string(self.generation_file()) {
            Ok(s)
                if !s.is_empty()
                    && s.len() <= 128
                    && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') =>
            {
                Ok(s)
            }
            Ok(_) => Err(corrupt("invalid generation")),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok("initial".into()),
            Err(e) => Err(infra(e)),
        }
    }
    fn read_stop(&self, generation: &str) -> Result<Option<StopRecord>, CtlError> {
        let data = match fs::read(self.stop_file()) {
            Ok(data) => data,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(infra(e)),
        };
        let r: StopRecord = serde_json::from_slice(&data).map_err(corrupt)?;
        if r.version != 2 || r.generation != generation {
            return Err(corrupt("unsupported stop record or generation mismatch"));
        }
        Ok(Some(r))
    }
    pub fn execution_generation(&self) -> Result<String, CtlError> {
        let _lock = self.control_lock()?;
        let generation = self.read_generation()?;
        self.read_stop(&generation)?;
        Ok(generation)
    }
    pub fn stop_requested(&self) -> bool {
        match fs::symlink_metadata(self.stop_file()) {
            Ok(_) => true,
            Err(e) => e.kind() != ErrorKind::NotFound,
        }
    }
    fn write_control(&self, path: &std::path::Path, data: &[u8]) -> Result<(), CtlError> {
        let tmp = path.with_extension(crate::snapshot::new_snapshot_id());
        let result = (|| {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(infra)?;
            f.write_all(data)
                .and_then(|_| f.sync_all())
                .map_err(infra)?;
            fs::rename(&tmp, path).map_err(infra)
        })();
        if result.is_err() {
            let _ = fs::remove_file(tmp);
        }
        result
    }
    pub fn request_stop(&self) -> Result<(), CtlError> {
        self.request_stop_from("explicit_request", None)
    }
    pub fn request_stop_from(&self, source: &str, epoch: Option<&str>) -> Result<(), CtlError> {
        let _lock = self.control_lock()?;
        let generation = crate::snapshot::new_snapshot_id();
        self.write_control(&self.generation_file(), generation.as_bytes())?;
        let record = StopRecord {
            version: 2,
            generation,
            requested_ms: unix_ms(),
            source: source.into(),
            pid: std::process::id(),
            display_epoch: epoch.map(str::to_owned),
        };
        self.write_control(
            &self.stop_file(),
            &serde_json::to_vec(&record).map_err(stop_record_json)?,
        )
    }
    /// Explicit recovery: invalidate callers and archive raw state; no consent or resume.
    pub fn acknowledge_stop(&self, expected: &str) -> Result<(), CtlError> {
        let _lock = self.control_lock()?;
        let generation = self.read_generation()?;
        if generation != expected {
            return Err(cancelled());
        }
        self.read_stop(&generation)?;
        if self.stop_requested() {
            let archive = self.dir.join("stop-history").join(&generation);
            fs::create_dir_all(&archive).map_err(infra)?;
            fs::copy(self.stop_file(), archive.join("stop-requested")).map_err(infra)?;
        }
        match fs::remove_file(self.stop_file()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(infra(e)),
        }
    }
    /// Explicit recovery: invalidate callers and archive raw state; no consent or resume.
    pub fn recover_execution(&self) -> Result<serde_json::Value, CtlError> {
        let _lock = self.control_lock()?;
        let generation = crate::snapshot::new_snapshot_id();
        let archive = self.dir.join("stop-history").join(&generation);
        let mut archived = Vec::new();
        for path in [self.stop_file(), self.generation_file()] {
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
                    fs::create_dir_all(&archive).map_err(infra)?;
                    let name = path
                        .file_name()
                        .ok_or_else(|| corrupt("missing filename"))?;
                    let dest = archive.join(name);
                    fs::copy(&path, &dest).map_err(infra)?;
                    archived.push(dest.to_string_lossy().into_owned());
                }
                Ok(_) => {
                    return Err(corrupt(
                        "state path is not a regular file; no automatic removal",
                    ));
                }
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => return Err(infra(e)),
            }
        }
        self.write_control(&self.generation_file(), generation.as_bytes())?;
        match fs::remove_file(self.stop_file()) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(infra(e)),
        }
        Ok(
            serde_json::json!({"recovered":true,"generation":generation,"permission_granted":false,"resumed":false,"archived":archived}),
        )
    }
    pub fn stop_record(&self) -> Option<StopRecord> {
        let _lock = self.control_lock().ok()?;
        self.read_stop(&self.read_generation().ok()?).ok().flatten()
    }
    pub fn stop_timestamp(&self) -> Option<u64> {
        self.stop_requested()
            .then(|| self.stop_record().map_or(u64::MAX, |r| r.requested_ms))
    }
    pub fn stop_details(&self) -> String {
        if !self.stop_requested() {
            return String::new();
        }
        match self.stop_record() {
            Some(r) => format!("\n已停止上一轮执行（来源：{}）。新任务可在交接提示中点击“开始”；旧调用不会恢复。", r.source),
            None => "\n运行状态损坏或版本不支持。请使用“修复运行状态”或 execution-recover；原始记录会归档，旧任务不会恢复。".into()
        }
    }
}
#[derive(Clone)]
pub struct Invocation(Result<String, (ErrorCode, String, Option<serde_json::Value>)>);
impl Invocation {
    pub fn capture(paths: &SignalPaths) -> Self {
        Self(
            paths
                .execution_generation()
                .map_err(|e| (e.code, e.message, e.evidence)),
        )
    }
    pub fn generation(&self) -> Result<String, CtlError> {
        self.0
            .clone()
            .map_err(|(code, message, evidence)| CtlError {
                code,
                message,
                evidence,
                hint: None,
            })
    }
    pub fn check(&self, paths: &SignalPaths) -> Result<(), CtlError> {
        if self.generation()? != paths.execution_generation()? {
            return Err(cancelled());
        }
        Ok(())
    }
}
thread_local! { static CURRENT: RefCell<Option<Invocation>> = const { RefCell::new(None) }; }
pub struct Scope(Option<Invocation>);
impl Scope {
    pub fn enter() -> Self {
        Self(CURRENT.with(|c| c.replace(Some(Invocation::capture(&SignalPaths::default())))))
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        CURRENT.with(|c| c.replace(self.0.take()));
    }
}
pub fn generation() -> Result<String, CtlError> {
    CURRENT.with(|c| {
        c.borrow_mut()
            .get_or_insert_with(|| Invocation::capture(&SignalPaths::default()))
            .generation()
    })
}

/// Bind a queued child to the generation selected by its parent UI action.
pub fn bind_generation(expected: String) {
    CURRENT.with(|c| *c.borrow_mut() = Some(Invocation(Ok(expected))));
}
pub fn check_current() -> Result<(), CtlError> {
    CURRENT.with(|c| {
        c.borrow_mut()
            .get_or_insert_with(|| Invocation::capture(&SignalPaths::default()))
            .check(&SignalPaths::default())
    })
}
