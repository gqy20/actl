//! Immutable plan plus durable, atomic state snapshots. The host holds the execution lock.
use crate::{Plan, engine::Run};
use actl_core::{CtlError, ErrorCode};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
}
impl Store {
    /// OS file locks are released on process death; stale lock files are harmless.
    pub fn lock(&self, global: bool) -> Result<std::fs::File, CtlError> {
        let parent = self
            .dir
            .parent()
            .ok_or_else(|| CtlError::internal("flow parent missing"))?;
        fs::create_dir_all(parent).map_err(io)?;
        let path = if global {
            parent.join("runner.lock")
        } else {
            self.dir.join("running.lock")
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(CtlError::new(ErrorCode::NotActionable, "flow runner busy"));
            }
            Err(error) => return Err(io(error)),
        }
        Ok(file)
    }
    pub fn active(&self) -> Result<bool, CtlError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.dir.join("running.lock"))
            .map_err(io)?;
        match file.try_lock() {
            Ok(()) => Ok(false),
            Err(std::fs::TryLockError::WouldBlock) => Ok(true),
            Err(e) => Err(io(e)),
        }
    }
    pub fn at(root: &Path, id: &str) -> Result<Self, CtlError> {
        if !actl_core::history::valid_id(id) {
            return Err(CtlError::protocol("invalid flow run ID"));
        }
        Ok(Self {
            dir: root.join("flows").join(id),
        })
    }
    pub fn create(&self, plan: &Plan, id: &str) -> Result<Run, CtlError> {
        fs::create_dir_all(
            self.dir
                .parent()
                .ok_or_else(|| CtlError::internal("flow path missing parent"))?,
        )
        .map_err(io)?;
        fs::create_dir(&self.dir).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                CtlError::protocol("run ID already exists; use flow-resume or a new ID")
            } else {
                io(e)
            }
        })?;
        write(&self.dir.join("plan.json"), plan)?;
        let state = Run::new(id, plan);
        self.save(&state)?;
        Ok(state)
    }
    pub fn load(&self) -> Result<(Plan, Run), CtlError> {
        let plan: Plan = read(&self.dir.join("plan.json"))?;
        let state: Run = read(&self.dir.join("state.json"))?;
        if state.version != 1
            || state.steps.len() != plan.steps.len()
            || self.dir.file_name().and_then(|s| s.to_str()) != Some(&state.id)
            || state
                .steps
                .iter()
                .zip(&plan.steps)
                .any(|(r, s)| r.id != s.id)
        {
            return Err(CtlError::protocol("inconsistent flow state"));
        }
        Ok((plan, state))
    }
    pub fn save(&self, state: &Run) -> Result<(), CtlError> {
        write(&self.dir.join("state.json"), state)
    }
    pub fn pause(&self) -> Result<(), CtlError> {
        self.load()?;
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(self.dir.join("pause-requested"))
            .map_err(io)?;
        file.sync_all().map_err(io)
    }
    pub fn pause_requested(&self) -> bool {
        self.dir.join("pause-requested").exists()
    }
    pub fn clear_pause(&self) -> Result<(), CtlError> {
        match fs::remove_file(self.dir.join("pause-requested")) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }
}
pub fn read_text(path: &Path) -> Result<String, CtlError> {
    let mut text = String::new();
    fs::File::open(path)
        .map_err(io)?
        .take(262_145)
        .read_to_string(&mut text)
        .map_err(io)?;
    if text.len() > 262_144 {
        return Err(CtlError::protocol("flow file exceeds 256 KiB"));
    }
    Ok(text)
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CtlError> {
    serde_json::from_str(&read_text(path)?)
        .map_err(|e| CtlError::protocol(format!("invalid saved flow: {e}")))
}
fn write<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), CtlError> {
    let bytes = serde_json::to_vec(value).map_err(io)?;
    let temp = path.with_extension("tmp");
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&temp)
        .map_err(io)?;
    f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(io)?;
    drop(f);
    fs::rename(temp, path).map_err(io)
}
fn io(e: impl std::fmt::Display) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("flow storage: {e}"))
}
