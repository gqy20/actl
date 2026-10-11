pub mod capture;
pub mod checks;
pub mod diagnostics;
pub mod fixtures;
pub mod install;
pub mod package;
pub mod regress;
pub mod timing;
use actl_core::{CtlError, ErrorCode};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, CtlError>;
pub fn io(e: impl std::fmt::Display) -> CtlError {
    CtlError::internal(e.to_string())
}
pub fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(CtlError::new(ErrorCode::AssertionFailed, message))
    }
}
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value).map_err(io)?).map_err(io)
}
pub fn id(prefix: &str) -> String {
    format!("{prefix}-{}", actl_core::snapshot::new_snapshot_id())
}
pub struct Args {
    pub values: BTreeMap<String, String>,
    pub flags: Vec<String>,
}
impl Args {
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut values = BTreeMap::new();
        let mut flags = vec![];
        let mut i = 0;
        while i < args.len() {
            let key = args[i]
                .strip_prefix("--")
                .ok_or_else(|| CtlError::protocol("expected --option"))?;
            if ["list", "unattended", "physical", "release"].contains(&key) {
                if flags.iter().any(|f| f == key) {
                    return Err(CtlError::protocol("duplicate flag"));
                }
                flags.push(key.into());
                i += 1;
            } else {
                let value = args
                    .get(i + 1)
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| CtlError::protocol("option requires value"))?;
                if values.insert(key.into(), value.clone()).is_some() {
                    return Err(CtlError::protocol("duplicate option"));
                }
                i += 2;
            }
        }
        Ok(Self { values, flags })
    }
    pub fn get(&self, key: &str) -> Result<&str> {
        self.values
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| CtlError::protocol(format!("missing --{key}")))
    }
    pub fn value<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.values.get(key).map(String::as_str).unwrap_or(default)
    }
    pub fn number(&self, key: &str, default: u64, min: u64, max: u64) -> Result<u64> {
        let n = self
            .values
            .get(key)
            .map(|v| v.parse::<u64>().map_err(io))
            .transpose()?
            .unwrap_or(default);
        require(n >= min && n <= max, "numeric option out of range")?;
        Ok(n)
    }
    pub fn check(&self, keys: &[&str], flags: &[&str]) -> Result<()> {
        if self.values.keys().any(|k| !keys.contains(&k.as_str()))
            || self.flags.iter().any(|k| !flags.contains(&k.as_str()))
        {
            Err(CtlError::protocol("unknown option"))
        } else {
            Ok(())
        }
    }
}
