//! 浏览器会话记录:跨调用状态落 actl 数据目录(浏览器进程独立于 CLI 进程存活,
//! 会话文件是唯一衔接)。原子写(tmp+rename),路径可注入以便测试。

use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) const SESSION_VERSION: u32 = 1;

/// 浏览器数据根(`%LOCALAPPDATA%\actl\browser`;`at` 注入用于测试)。
pub struct Paths {
    pub root: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        let dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("actl")
            .join("browser");
        Self { root: dir }
    }
}

impl Paths {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.root.join("profiles")
    }

    fn session_file(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.json"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    Ephemeral,
    Persistent { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BrowserSession {
    pub version: u32,
    pub session_id: String,
    pub browser_executable: String,
    pub profile_dir: String,
    pub mode: Mode,
    pub headless: bool,
    pub pid: u32,
    pub devtools_port: u16,
    pub browser_ws_path: String,
    pub target_id: String,
    pub bound_url: String,
    pub created_unix: u64,
}

/// 会话 id 仅允许小写字母数字(生成自 new_snapshot_id,先验拒绝路径注入)。
fn validate_session_id(session_id: &str) -> Result<(), CtlError> {
    let ok = !session_id.is_empty()
        && session_id.len() <= 64
        && session_id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(CtlError::new(
            ErrorCode::Protocol,
            format!(
                "invalid session id (expected lowercase alphanumeric from browser-launch): {session_id}"
            ),
        ))
    }
}

/// 持久 profile 名:1-64 个 [A-Za-z0-9_-],防目录逃逸。
pub(crate) fn validate_profile_name(name: &str) -> Result<(), CtlError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(CtlError::new(
            ErrorCode::Protocol,
            format!("invalid profile name (1-64 of [A-Za-z0-9_-]): {name}"),
        ))
    }
}

pub(crate) fn save(paths: &Paths, session: &BrowserSession) -> Result<(), CtlError> {
    std::fs::create_dir_all(paths.sessions_dir()).map_err(io)?;
    let data = serde_json::to_vec(session)
        .map_err(|e| CtlError::internal(format!("cannot serialize browser session: {e}")))?;
    let tmp = paths
        .session_file(&session.session_id)
        .with_extension("tmp");
    std::fs::write(&tmp, &data).map_err(io)?;
    std::fs::rename(&tmp, paths.session_file(&session.session_id))
        .map_err(|e| CtlError::internal(format!("cannot persist browser session: {e}")))
}

pub(crate) fn load(paths: &Paths, session_id: &str) -> Result<BrowserSession, CtlError> {
    validate_session_id(session_id)?;
    let file = paths.session_file(session_id);
    let data = std::fs::read(&file).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => CtlError::new(
            ErrorCode::NotFound,
            format!("unknown browser session {session_id}; run browser-launch to create one"),
        ),
        _ => CtlError::internal(format!("cannot read browser session file: {e}")),
    })?;
    let session: BrowserSession = serde_json::from_slice(&data)
        .map_err(|e| CtlError::internal(format!("corrupt browser session file: {e}")))?;
    if session.version != SESSION_VERSION || session.session_id != session_id {
        return Err(CtlError::internal(format!(
            "browser session file does not match expected id/version: {session_id}"
        )));
    }
    Ok(session)
}

pub(crate) fn remove(paths: &Paths, session_id: &str) -> Result<(), CtlError> {
    validate_session_id(session_id)?;
    match std::fs::remove_file(paths.session_file(session_id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(CtlError::internal(format!(
            "cannot remove browser session file: {e}"
        ))),
    }
}

fn io(e: std::io::Error) -> CtlError {
    CtlError::internal(format!("browser session directory error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> BrowserSession {
        BrowserSession {
            version: SESSION_VERSION,
            session_id: id.to_string(),
            browser_executable: "C:/edge/msedge.exe".into(),
            profile_dir: "C:/tmp/profile".into(),
            mode: Mode::Ephemeral,
            headless: true,
            pid: 42,
            devtools_port: 9222,
            browser_ws_path: "/devtools/browser/UUID".into(),
            target_id: "T1".into(),
            bound_url: "about:blank".into(),
            created_unix: 1790000000,
        }
    }

    #[test]
    fn save_load_remove_roundtrip() {
        let dir = std::env::temp_dir().join(format!("actl-browser-test-{}", std::process::id()));
        let paths = Paths::at(&dir);
        save(&paths, &sample("abc123")).unwrap();
        let loaded = load(&paths, "abc123").unwrap();
        assert_eq!(loaded.pid, 42);
        assert_eq!(loaded.mode, Mode::Ephemeral);
        remove(&paths, "abc123").unwrap();
        assert!(load(&paths, "abc123").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_path_traversal_session_id() {
        let paths = Paths::at("C:/nowhere");
        let err = load(&paths, "..\\evil").unwrap_err();
        assert_eq!(err.code, ErrorCode::Protocol);
        let err = load(&paths, "UPPER").unwrap_err();
        assert_eq!(err.code, ErrorCode::Protocol);
    }

    #[test]
    fn unknown_session_maps_to_not_found() {
        let paths = Paths::at("C:/nowhere");
        let err = load(&paths, "nope123").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn profile_name_validation() {
        assert!(validate_profile_name("work-mail").is_ok());
        assert!(validate_profile_name("../escape").is_err());
        assert!(validate_profile_name("").is_err());
    }
}
