//! Optional task-header cache. A cache miss always falls back to the raw journal.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, io::Read, path::Path, time::UNIX_EPOCH};

const FILE: &str = "cache/task-headers.json";
const LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    size: u64,
    modified_ns: u64,
    task: Option<String>,
}
pub(crate) struct Index {
    original: BTreeMap<String, Entry>,
    current: BTreeMap<String, Entry>,
}
impl Index {
    pub(crate) fn load(dir: &Path) -> Self {
        let original = (|| -> Option<BTreeMap<String, Entry>> {
            let mut bytes = Vec::new();
            fs::File::open(dir.join(FILE))
                .ok()?
                .take(LIMIT + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() as u64 > LIMIT {
                return None;
            }
            let entries: BTreeMap<String, Entry> = serde_json::from_slice(&bytes).ok()?;
            entries
                .iter()
                .all(|(name, entry)| {
                    name.strip_suffix(".jsonl")
                        .is_some_and(crate::history::valid_id)
                        && entry.task.as_deref().is_none_or(crate::history::valid_id)
                })
                .then_some(entries)
        })()
        .unwrap_or_default();
        Self {
            original,
            current: BTreeMap::new(),
        }
    }
    pub(crate) fn stamp(metadata: &fs::Metadata) -> Option<(u64, u64)> {
        let nanos = metadata
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some((metadata.len(), u64::try_from(nanos).ok()?))
    }
    pub(crate) fn foreign(&mut self, name: &str, stamp: Option<(u64, u64)>, task: &str) -> bool {
        let Some(entry) = self
            .original
            .get(name)
            .filter(|e| stamp == Some((e.size, e.modified_ns)))
        else {
            return false;
        };
        self.current.insert(name.into(), entry.clone());
        entry.task.as_deref() != Some(task)
    }
    pub(crate) fn remember(
        &mut self,
        name: &str,
        stamp: Option<(u64, u64)>,
        first: &Value,
    ) -> bool {
        let Some((size, modified_ns)) = stamp else {
            return false;
        };
        let valid_task = first.get("task_id").is_some_and(|task| {
            task.is_null() || task.as_str().is_some_and(crate::history::valid_id)
        });
        if !name
            .strip_suffix(".jsonl")
            .is_some_and(crate::history::valid_id)
            || !valid_task
            || first["version"] != 1
            || first["seq"] != 1
            || first["event"] != "call_started"
            || first["call_id"].as_str() != name.strip_suffix(".jsonl")
        {
            self.current.remove(name);
            return false;
        }
        self.current.insert(
            name.into(),
            Entry {
                size,
                modified_ns,
                task: first["task_id"].as_str().map(str::to_owned),
            },
        );
        true
    }
    pub(crate) fn save(&self, dir: &Path) {
        if self.original == self.current {
            return;
        }
        // This is only an accelerator. Invalid/missing/unwritable cache does not
        // change raw events or report execution failure. Never cache unknown headers.
        let result = (|| -> std::io::Result<()> {
            let bytes = serde_json::to_vec(&self.current)?;
            if bytes.len() as u64 > LIMIT {
                return Ok(());
            }
            fs::create_dir_all(dir.join("cache"))?;
            let tmp = dir.join(format!("cache/task-headers-{}.tmp", std::process::id()));
            fs::write(&tmp, bytes)?;
            fs::rename(tmp, dir.join(FILE))
        })();
        if let Err(error) = result {
            eprintln!("[actl-history] optional task-header cache unavailable: {error}");
        }
    }
}
