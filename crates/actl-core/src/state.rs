//! 显示信号协议(docs/11 §10 / docs/13 §4.1):actl 侧状态源。
//!
//! 原语四件套:①`state.json` 命令级状态快照(开始/结束/batch 步间写入);
//! ②`stop-requested` 当前停止记录 + `stop-generation` 调用取消代次;
//! ③输入占用锁(actl-uia 的命名互斥体,"正在操控"的权威信号,本模块只约定
//! 不实现);④时间戳心跳(消费端判失联)。
//! 消费端(actl-signal 等)全部拉取、零订阅——"事件当触发器,拉取当真相源"
//! (docs/13 的事故价目表:常驻事件订阅路线不可取)。
//! 路径可注入(`SignalPaths::at`)以便测试;`default()` 用 %LOCALAPPDATA%。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 命令生命周期相位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// 命令开始(batch:第 i/N 步)
    Running,
    /// 命令成功结束
    Done,
    /// 命令失败结束
    Error,
    /// 用户停止中止
    Stopped,
}

/// 一次命令执行的状态快照(单写者:当前 actl 进程;多读者)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowProgress>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub started_ms: u64,
    #[serde(default)]
    pub finished_ms: Option<u64>,
    #[serde(default)]
    pub action: Option<crate::activity::ActionState>,
    #[serde(default)]
    pub recent_actions: Vec<crate::activity::ActionState>,
    #[serde(default)]
    pub error_code: Option<String>,
    pub pid: u32,
    pub command: String,
    /// 目标窗口摘要(--app 原样;无则 None)
    pub app: Option<String>,
    /// batch 步序 "i/N";单命令 None
    pub step: Option<String>,
    pub phase: Phase,
    /// unix 毫秒时间戳(心跳)
    pub ts_ms: u64,
}

impl SessionState {
    pub fn now(
        pid: u32,
        command: &str,
        app: Option<&str>,
        step: Option<String>,
        phase: Phase,
    ) -> Self {
        Self {
            handoff_reason: None,
            workflow: None,
            version: 1,
            call_id: String::new(),
            seq: 0,
            started_ms: unix_ms(),
            finished_ms: None,
            action: None,
            recent_actions: Vec::new(),
            error_code: None,
            pid,
            command: command.to_string(),
            app: app.map(str::to_string),
            step,
            phase,
            ts_ms: unix_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowProgress {
    pub run_id: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<crate::ErrorCode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
}

pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 信号目录/文件的解析。默认 %LOCALAPPDATA%\actl\。
#[derive(Debug, Clone)]
pub struct SignalPaths {
    pub dir: PathBuf,
}

impl Default for SignalPaths {
    fn default() -> Self {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("actl");
        Self { dir }
    }
}

impl SignalPaths {
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn state_file(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    pub fn stop_file(&self) -> PathBuf {
        self.dir.join("stop-requested")
    }

    pub fn preauth_file(&self) -> PathBuf {
        self.dir.join("preauth.json")
    }
    /// 常驻预授权开关:开启后桌面写入免出卡(Consent 以 Standing 策略授予)。
    /// 写入方只应是显示端托盘的用户点击;读取失败按关闭处理(fail-closed)。
    pub fn preauthorized(&self) -> bool {
        std::fs::read(self.preauth_file())
            .ok()
            .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
            .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
            .unwrap_or(false)
    }
    pub fn set_preauth(&self, enabled: bool) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let payload = serde_json::json!({"enabled": enabled, "ts_ms": unix_ms()});
        let tmp = self.dir.join("preauth.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&payload)?)?;
        std::fs::rename(&tmp, self.preauth_file())
    }

    /// 状态快照落盘；诊断故障不改变业务结果，但写入独立健康记录。
    pub fn write_state(&self, st: &SessionState) {
        if let Err(error) = self.try_write_state(st) {
            crate::log_health::failure(
                &self.dir,
                &st.call_id,
                st.workflow.as_ref().map(|w| w.run_id.as_str()),
                "state_write",
                0,
                &error,
            );
            eprintln!("[actl-state] INTERNAL: state publication failed: {error}");
        }
    }
    fn try_write_state(&self, st: &SessionState) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        if st.version >= 2 && !st.call_id.is_empty() {
            let dir = self.dir.join("calls");
            std::fs::create_dir_all(&dir)?;
            // Each invocation owns its file: concurrent commands cannot overwrite it.
            let file = dir.join(format!("{}.json", st.call_id));
            Self::atomic_write(&file, st)?;
        }
        Self::atomic_write(&self.state_file(), st)
    }

    fn atomic_write(file: &std::path::Path, st: &SessionState) -> std::io::Result<()> {
        let tmp = file.with_extension(format!("{}.{}.tmp", st.pid, st.call_id));
        let result = (|| {
            std::fs::write(&tmp, serde_json::to_vec(st)?)?;
            std::fs::rename(&tmp, file)
        })();
        let _ = std::fs::remove_file(tmp);
        result
    }

    /// Recent call snapshots, bounded retention; active calls are never evicted.
    pub fn read_calls(&self) -> Vec<SessionState> {
        let now = unix_ms();
        let mut states = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.dir.join("calls")) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "json") {
                    continue;
                }
                let Some(st) = std::fs::read(&path)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<SessionState>(&b).ok())
                else {
                    continue;
                };
                if now.saturating_sub(st.ts_ms) > 60_000 {
                    let _ = std::fs::remove_file(path);
                } else {
                    states.push(st);
                }
            }
        }
        if states.is_empty()
            && let Some(st) = self.read_state()
        {
            states.push(st);
        }
        states.sort_by_key(|s| s.ts_ms);
        let terminal_count = states.iter().filter(|s| s.phase != Phase::Running).count();
        let mut excess = terminal_count.saturating_sub(128);
        states.retain(|s| {
            if excess > 0 && s.phase != Phase::Running && !s.call_id.is_empty() {
                // File names are enumerated/generated locally; never trust a serialized path.
                if s.call_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                {
                    let _ = std::fs::remove_file(
                        self.dir.join("calls").join(format!("{}.json", s.call_id)),
                    );
                }
                excess -= 1;
                false
            } else {
                true
            }
        });
        states
    }

    pub fn read_state(&self) -> Option<SessionState> {
        let text = std::fs::read_to_string(self.state_file()).ok()?;
        serde_json::from_str(&text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> SignalPaths {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("actl-state-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        SignalPaths::at(&dir)
    }

    #[test]
    fn state_roundtrip_and_fields() {
        let p = tmp();
        let st = SessionState::now(
            42,
            "press",
            Some("记事本"),
            Some("3/8".into()),
            Phase::Running,
        );
        p.write_state(&st);
        let back = p.read_state().expect("read back");
        assert_eq!(back.command, "press");
        assert_eq!(back.app.as_deref(), Some("记事本"));
        assert_eq!(back.step.as_deref(), Some("3/8"));
        assert_eq!(back.phase, Phase::Running);
        assert!(back.ts_ms > 0);
        let _ = std::fs::remove_dir_all(&p.dir);
    }

    #[test]
    fn stop_flag_is_sticky_until_cleared() {
        let p = tmp();
        assert!(!p.stop_requested());
        p.request_stop().unwrap();
        assert!(p.stop_requested());
        p.recover_execution().unwrap();
        assert!(!p.stop_requested());
        let _ = std::fs::remove_dir_all(&p.dir);
    }

    #[test]
    fn phase_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&Phase::Stopped).unwrap(),
            r#""stopped""#
        );
    }
}
