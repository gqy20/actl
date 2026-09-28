//! Best-effort independent failure markers. Missing markers never prove complete logging.
use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn failure(
    root: &Path,
    call: &str,
    task: Option<&str>,
    stage: &str,
    dropped: u64,
    error: &std::io::Error,
) {
    if !crate::history::valid_id(call) || !crate::history::valid_id(stage) {
        return;
    }
    let dir = root.join("log-health");
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(&dir)?;
        crate::history::cleanup(&dir, 7 * 86_400_000, 8 * 1024 * 1024)?;
        let value = json!({"version":1,"call_id":call,"task_id":task,"status":"degraded",
            "stage":stage,"dropped_events":dropped,"ts_ms":crate::state::unix_ms(),
            "error_kind":format!("{:?}",error.kind()),"native_code":error.raw_os_error()});
        let path = dir.join(format!("{call}-{stage}.json"));
        let temp = path.with_extension("tmp");
        fs::write(&temp, serde_json::to_vec(&value)?)?;
        fs::rename(temp, path)
    })();
    if let Err(e) = result {
        eprintln!("[actl-history] health marker unavailable: {e}");
    }
}

pub fn read(root: &Path, task: Option<&str>) -> Value {
    let dir = root.join("log-health");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return json!({"status":"no_recorded_failure","complete_logging_guaranteed":false,"failure_count":0,"truncated":false,"failures":[]});
        }
        Err(_) => {
            return json!({"status":"unknown","complete_logging_guaranteed":false,"failure_count":0,"truncated":false,"failures":[]});
        }
    };
    let mut failures = vec![];
    let mut unknown = false;
    for entry in entries {
        let Ok(entry) = entry else {
            unknown = true;
            continue;
        };
        if entry.path().extension().is_none_or(|e| e != "json") {
            continue;
        }
        let value = (|| -> std::io::Result<Vec<u8>> {
            use std::io::Read;
            let mut bytes = Vec::new();
            fs::File::open(entry.path())?
                .take(4097)
                .read_to_end(&mut bytes)?;
            Ok(bytes)
        })()
        .ok()
        .filter(|b| b.len() <= 4096)
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        match value {
            Some(v) if v["version"] == 1 && v["status"] == "degraded" => {
                if task.is_none_or(|t| v["task_id"].as_str() == Some(t)) {
                    failures.push(v);
                }
            }
            _ => unknown = true,
        }
    }
    failures.sort_by_key(|v| std::cmp::Reverse(v["ts_ms"].as_u64().unwrap_or(0)));
    let count = failures.len();
    failures.truncate(100);
    json!({"status":if unknown {"unknown"} else if count > 0 {"degraded"} else {"no_recorded_failure"},
        "complete_logging_guaranteed":false,"failure_count":count,"truncated":count>100,"failures":failures})
}
