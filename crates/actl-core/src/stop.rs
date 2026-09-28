//! Sticky emergency stop, independent of display lifetime. Unknown state blocks input.
use crate::{
    CtlError as Error,
    state::{SignalPaths, unix_ms},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{ErrorKind, Write},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct StopRecord {
    pub version: u32,
    pub requested_ms: u64,
    pub source: String,
    pub pid: u32,
    pub display_epoch: Option<String>,
}

impl SignalPaths {
    pub fn stop_requested(&self) -> bool {
        match fs::symlink_metadata(self.stop_file()) {
            Ok(_) => true,
            Err(e) => e.kind() != ErrorKind::NotFound,
        }
    }

    pub fn request_stop(&self) -> Result<(), Error> {
        self.request_stop_from("explicit_request", None)
    }

    pub fn request_stop_from(&self, source: &str, epoch: Option<&str>) -> Result<(), Error> {
        fs::create_dir_all(&self.dir)
            .map_err(|e| Error::internal(format!("create stop directory: {e}")))?;
        // Preserve the first request and any unrecognized old marker. An incomplete
        // write still leaves a stop marker, never an apparent permission to execute.
        let mut file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.stop_file())
        {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => return Ok(()),
            Err(e) => return Err(Error::internal(format!("create emergency stop: {e}"))),
        };
        let record = StopRecord {
            version: 1,
            requested_ms: unix_ms(),
            source: source.into(),
            pid: std::process::id(),
            display_epoch: epoch.map(str::to_owned),
        };
        let bytes = serde_json::to_vec(&record).map_err(|e| Error::internal(e.to_string()))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| Error::internal(format!("persist emergency stop: {e}")))
    }

    pub fn stop_record(&self) -> Option<StopRecord> {
        let record: StopRecord = serde_json::from_slice(&fs::read(self.stop_file()).ok()?).ok()?;
        (record.version == 1).then_some(record)
    }

    pub fn stop_timestamp(&self) -> Option<u64> {
        self.stop_requested().then(|| {
            self.stop_record()
                .map(|r| r.requested_ms)
                .or_else(|| {
                    fs::read_to_string(self.stop_file())
                        .ok()?
                        .trim()
                        .parse()
                        .ok()
                })
                .unwrap_or(u64::MAX)
        })
    }

    pub fn stop_details(&self) -> String {
        if !self.stop_requested() {
            return String::new();
        }
        let origin = match self.stop_record() {
            Some(r) => format!(
                "来源：{}；{} 秒前触发；进程：{}；会话：{}",
                match r.source.as_str() {
                    "hotkey" => "急停快捷键",
                    "menu" => "右键菜单",
                    "details_button" => "详情窗口急停按钮",
                    _ => &r.source,
                },
                unix_ms().saturating_sub(r.requested_ms) / 1000,
                r.pid,
                r.display_epoch.as_deref().unwrap_or("未知")
            ),
            None => "旧标记或记录不可读，来源未知；停止状态仍保留".into(),
        };
        format!("\n急停跨重启保留。{origin}\n请点击“解除急停”；解除后旧任务仍需核验并显式继续。")
    }

    /// Explicit user action only; verify absence after removal and report failures.
    pub fn clear_stop(&self) -> Result<(), Error> {
        match fs::remove_file(self.stop_file()) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(Error::internal(format!("clear emergency stop: {e}"))),
        }
        if self.stop_requested() {
            return Err(Error::internal(
                "stop remains active or could not be verified",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn paths() -> SignalPaths {
        SignalPaths::at(std::env::temp_dir().join(crate::snapshot::new_snapshot_id()))
    }
    #[test]
    fn record_preserves_first_origin_and_restart_does_not_clear() {
        let p = paths();
        p.request_stop_from("hotkey", Some("session-a")).unwrap();
        let before = fs::read(p.stop_file()).unwrap();
        let restarted = SignalPaths::at(&p.dir);
        restarted
            .request_stop_from("menu", Some("session-b"))
            .unwrap();
        assert_eq!(before, fs::read(p.stop_file()).unwrap());
        let mut value = serde_json::to_value(p.stop_record().unwrap()).unwrap();
        value["requested_ms"] = 123.into();
        value["pid"] = 42.into();
        assert_eq!(
            value,
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../tests/golden/stop_record.json"
            ))
            .unwrap()
        );
        p.clear_stop().unwrap();
        assert!(!p.stop_requested());
        fs::remove_dir_all(p.dir).unwrap();
    }
    #[test]
    fn malformed_and_legacy_markers_remain_stopped() {
        let p = paths();
        fs::create_dir_all(&p.dir).unwrap();
        for text in ["123", "", "corrupt"] {
            fs::write(p.stop_file(), text).unwrap();
            assert!(p.stop_timestamp().is_some());
            assert!(p.stop_details().contains("来源未知"));
            p.request_stop().unwrap();
            assert_eq!(fs::read_to_string(p.stop_file()).unwrap(), text);
        }
        p.clear_stop().unwrap();
        fs::remove_dir_all(p.dir).unwrap();
    }
    #[test]
    fn clear_failure_is_visible_and_unknown_path_blocks() {
        let p = paths();
        fs::create_dir_all(p.stop_file()).unwrap();
        assert!(p.stop_requested());
        assert_eq!(p.clear_stop().unwrap_err().code, crate::ErrorCode::Internal);
        assert!(p.stop_requested());
        fs::remove_dir_all(p.dir).unwrap();
    }

    #[test]
    fn stop_persistence_failure_is_not_reported_as_success() {
        let p = paths();
        fs::write(&p.dir, "not a directory").unwrap();
        assert_eq!(
            p.request_stop().unwrap_err().code,
            crate::ErrorCode::Internal
        );
        fs::remove_file(p.dir).unwrap();
    }
}
