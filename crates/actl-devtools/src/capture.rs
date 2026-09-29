//! Direct file redirection avoids pipe deadlocks and descendant-held pipe waits.
use crate::{Result, io, require, save};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub fn process(
    program: &Path,
    args: &[String],
    dir: &Path,
    prefix: &Path,
    env: &BTreeMap<String, String>,
    timeout: Duration,
) -> Result<Value> {
    let out = prefix.with_extension("stdout.json");
    let err = prefix.with_extension("stderr.log");
    let start = Instant::now();
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(dir)
        .env_remove("ACTL_TASK_ID")
        .env_remove("ACTL_TASK_CONFIG")
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(&out).map_err(io)?))
        .stderr(Stdio::from(File::create(&err).map_err(io)?));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(io)?;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(io)? {
            break status;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break child.wait().map_err(io)?;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let record = json!({"program":program,"args":args,"exit_code":status.code(),"elapsed_ms":start.elapsed().as_millis(),"stdout":out,"stderr":err,"status":if timed_out{"timeout"}else if status.success(){"exited"}else{"failed"},"capture":"direct_files"});
    save(&prefix.with_extension("capture.json"), &record)?;
    Ok(record)
}
pub fn read_json(path: &Path) -> Result<Value> {
    use std::io::Read;
    let mut data = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(io)?;
    require(
        data.len() <= 8 * 1024 * 1024,
        "diagnostic JSON exceeds 8 MiB",
    )?;
    serde_json::from_slice(&data)
        .map_err(|e| actl_core::CtlError::protocol(format!("invalid diagnostic JSON: {e}")))
}
pub struct Context {
    pub binary: PathBuf,
    pub dir: PathBuf,
    pub task: String,
    pub calls: Vec<Value>,
    pub uncertain: bool,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
}
impl Context {
    pub fn new(binary: PathBuf, dir: PathBuf) -> Result<Self> {
        let dir = std::path::absolute(dir).map_err(io)?;
        fs::create_dir_all(dir.parent().ok_or_else(|| io("output parent missing"))?).map_err(io)?;
        fs::create_dir(&dir).map_err(io)?;
        let binary = binary.canonicalize().map_err(io)?;
        let task = actl_core::history::task_from_env()?.unwrap_or_else(|| crate::id("fixture"));
        let mut env = BTreeMap::new();
        env.insert("ACTL_TASK_ID".into(), task.clone());
        let config = dir.join("handoff.yml");
        fs::write(&config, "version: 1\nhandoff: {approval: once_per_run}\n").map_err(io)?;
        env.insert(
            "ACTL_TASK_CONFIG".into(),
            std::env::var("ACTL_TASK_CONFIG").unwrap_or_else(|_| config.to_string_lossy().into()),
        );
        Ok(Self {
            binary,
            dir,
            task,
            calls: vec![],
            uncertain: false,
            env,
            timeout: Duration::from_secs(30),
        })
    }
    pub fn raw(&mut self, args: &[&str]) -> Result<Value> {
        let prefix = self.dir.join(format!("call-{:04}", self.calls.len() + 1));
        let args: Vec<_> = args.iter().map(|s| s.to_string()).collect();
        let record = process(
            &self.binary,
            &args,
            &self.dir,
            &prefix,
            &self.env,
            self.timeout,
        )?;
        self.calls.push(record.clone());
        if record["status"] == "timeout" {
            self.uncertain = true;
            return Err(io("CLI timeout; effects unknown, no replay or cleanup"));
        }
        let value = match read_json(&prefix.with_extension("stdout.json")) {
            Ok(v) => v,
            Err(e) => {
                self.uncertain = true;
                return Err(e);
            }
        };
        if !value["ok"].is_boolean() {
            self.uncertain = true;
            return Err(io("missing CLI envelope"));
        }
        require(
            value["ok"] != true || record["exit_code"] == 0,
            "successful envelope with failed exit",
        )?;
        if value["error"]["evidence"]["stage"] == "handoff" || value["error"]["code"] == "ABORTED" {
            self.uncertain = true;
        }
        Ok(value)
    }
    pub fn call(&mut self, args: &[&str]) -> Result<Value> {
        let v = self.raw(args)?;
        if v["ok"] != true {
            self.uncertain = true;
            let code = serde_json::from_value(v["error"]["code"].clone())
                .map_err(|_| actl_core::CtlError::protocol("unknown CLI error code"))?;
            return Err(actl_core::CtlError::with_evidence(
                code,
                format!(
                    "{}: {}",
                    args.first().copied().unwrap_or("call"),
                    v["error"]["message"]
                        .as_str()
                        .unwrap_or("missing error message")
                ),
                v["error"]["evidence"].clone(),
            ));
        }
        Ok(v["data"].clone())
    }
    pub fn expected(&mut self, args: &[&str], code: &str) -> Result<Value> {
        let value = self.raw(args)?;
        require(
            value["ok"] == false && value["error"]["code"] == code,
            "unexpected error classification",
        )?;
        Ok(value)
    }
    pub fn stopped(&mut self) -> Result<bool> {
        let state = self.call(&["execution-status"])?;
        match state["state"].as_str() {
            Some("ready") => Ok(false),
            Some("stopped" | "repair_required") => Ok(true),
            _ => Err(io("unknown execution state")),
        }
    }
    pub fn report(&self, path: &str, value: &mut Value) -> Result<()> {
        value["calls"] = json!(self.calls);
        value["environment"] = json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"machine":std::env::var("COMPUTERNAME").ok(),"processor":std::env::var("PROCESSOR_IDENTIFIER").ok(),"recorded_unix_ms":actl_core::state::unix_ms()});
        value["binary"] = json!(self.binary);
        value["binary_sha256"] = json!(actl_uia::evidence::sha256(
            &fs::read(&self.binary).map_err(io)?
        )?);
        value["task_id"] = json!(self.task);
        value["uncertain"] = json!(self.uncertain);
        save(&self.dir.join(path), value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn child() {
        let Ok(mode) = std::env::var("ACTL_CAPTURE_TEST") else {
            return;
        };
        if mode == "sleep" {
            std::thread::sleep(Duration::from_secs(2));
            std::process::exit(0);
        }
        if mode == "descendant" {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "capture::tests::child", "--nocapture"])
                .env("ACTL_CAPTURE_TEST", "sleep");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            // This Windows-only capture regression requires a descendant that
            // outlives its parent; it exits by itself after two seconds.
            #[allow(clippy::zombie_processes)]
            let _child = command.spawn().unwrap();
        }
        eprintln!("diagnostic:{}", "x".repeat(1024 * 1024));
        println!(
            "{}",
            serde_json::json!({"ok":true,"literal":std::env::var("ACTL_LITERAL").unwrap_or_default()})
        );
        std::process::exit(if mode == "skip" { 77 } else { 0 });
    }
    #[test]
    fn direct_capture_preserves_streams_and_bounds_waits() {
        let dir = std::env::temp_dir().join(crate::id("capture-test"));
        fs::create_dir(&dir).unwrap();
        let args = vec![
            "--exact".into(),
            "capture::tests::child".into(),
            "--nocapture".into(),
        ];
        for mode in ["normal", "skip", "sleep", "descendant"] {
            let env = BTreeMap::from([
                ("ACTL_CAPTURE_TEST".into(), mode.into()),
                ("ACTL_LITERAL".into(), "中文 \"quoted\" $()`".into()),
            ]);
            let result = process(
                &std::env::current_exe().unwrap(),
                &args,
                &dir,
                &dir.join(mode),
                &env,
                Duration::from_millis(if mode == "sleep" { 150 } else { 3000 }),
            )
            .unwrap();
            if mode == "sleep" {
                assert_eq!(result["status"], "timeout");
                continue;
            }
            assert_eq!(result["exit_code"], if mode == "skip" { 77 } else { 0 });
            assert!(
                fs::read_to_string(dir.join(format!("{mode}.stdout.json")))
                    .unwrap()
                    .contains("中文")
            );
            assert!(
                fs::metadata(dir.join(format!("{mode}.stderr.log")))
                    .unwrap()
                    .len()
                    > 1024 * 1024
            );
            if mode == "descendant" {
                assert!(result["elapsed_ms"].as_u64().unwrap() < 1500);
            }
        }
        let malformed = dir.join("invalid.json");
        fs::write(&malformed, "not JSON").unwrap();
        assert!(read_json(&malformed).is_err());
        // Descendant owns only its inherited output files and exits naturally.
    }
}
