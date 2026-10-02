use super::*;
use std::{
    collections::BTreeSet,
    path::PathBuf,
    time::{Duration, SystemTime},
};

struct Fixture {
    root: PathBuf,
    now: u64,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(crate::snapshot::new_snapshot_id());
        fs::create_dir_all(root.join("history")).unwrap();
        Self {
            root,
            now: crate::state::unix_ms(),
        }
    }
    fn dir(&self) -> PathBuf {
        self.root.join("history")
    }
    fn put(&self, name: &str, ago: u64) -> PathBuf {
        let path = self.dir().join(name);
        fs::write(&path, "1234").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + Duration::from_millis(self.now - ago)),
            )
            .unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn retained_records_do_not_read_activity_or_sort() {
    let f = Fixture::new();
    f.put("call-a.jsonl", 86_400_000);
    f.put("call-b.jsonl", 4_000_000);
    let mut metrics = Metrics::default();
    run(
        &f.dir(),
        7 * 86_400_000,
        100,
        f.now,
        &mut metrics,
        &mut |_, _| panic!("retained history must not read calls"),
        &mut |_| panic!("retained history must not be deleted"),
    )
    .unwrap();
    assert_eq!(metrics.activity_reads, 0);
    assert_eq!(metrics.sorted_records, 0);
    assert_eq!(metrics.remaining_bytes, Some(8));
}

#[test]
fn expiry_preserves_fresh_running_call_and_recovery_data() {
    let f = Fixture::new();
    let expired = 8 * 86_400_000;
    f.put("old.jsonl", expired);
    let live = f.put("live.jsonl", expired);
    f.put("stale.jsonl", expired);
    let mut state =
        crate::state::SessionState::now(1, "wait", None, None, crate::state::Phase::Running);
    state.call_id = "live".into();
    state.version = 2;
    state.ts_ms = f.now;
    crate::state::SignalPaths::at(&f.root).write_state(&state);
    state.call_id = "stale".into();
    state.ts_ms = f.now - 10_000;
    crate::state::SignalPaths::at(&f.root).write_state(&state);
    fs::create_dir(f.root.join("desktop")).unwrap();
    fs::write(f.root.join("desktop/recovery.json"), "keep").unwrap();
    let mut metrics = Metrics::default();
    run(
        &f.dir(),
        7 * 86_400_000,
        100,
        f.now,
        &mut metrics,
        &mut active,
        &mut |path| fs::remove_file(path),
    )
    .unwrap();
    assert!(live.exists());
    assert!(!f.dir().join("old.jsonl").exists());
    assert!(!f.dir().join("stale.jsonl").exists());
    assert!(f.root.join("desktop/recovery.json").exists());
    assert_eq!(metrics.activity_reads, 3);
    assert_eq!(metrics.sorted_records, 0);
}

#[test]
fn budget_removes_oldest_eligible_and_protects_recent_and_active() {
    let f = Fixture::new();
    f.put("live.jsonl", 4 * 86_400_000);
    f.put("old.jsonl", 3 * 86_400_000);
    f.put("newer.jsonl", 2 * 86_400_000);
    f.put("recent.jsonl", 100);
    let mut metrics = Metrics::default();
    let mut deleted = BTreeSet::new();
    run(
        &f.dir(),
        7 * 86_400_000,
        12,
        f.now,
        &mut metrics,
        &mut |path, _| path.file_name().unwrap() == "live.jsonl",
        &mut |path| {
            deleted.insert(path.file_name().unwrap().to_owned());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(deleted, BTreeSet::from(["old.jsonl".into()]));
    assert_eq!(metrics.activity_reads, 2);
    assert_eq!(metrics.sorted_records, 4);
    assert_eq!(metrics.remaining_bytes, Some(12));
}

#[test]
fn protected_capacity_is_failure_and_not_partial_success() {
    let f = Fixture::new();
    f.put("recent.jsonl", 100);
    let mut metrics = Metrics::default();
    let error = run(
        &f.dir(),
        7 * 86_400_000,
        0,
        f.now,
        &mut metrics,
        &mut |_, _| panic!("recent file does not need activity lookup"),
        &mut |_| panic!("recent file must not be deleted"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::StorageFull);
    assert_eq!(metrics.activity_reads, 0);
    assert_eq!(metrics.remaining_bytes, Some(4));
}

#[test]
fn delete_failure_and_directory_failure_propagate() {
    let f = Fixture::new();
    let not_dir = f.put("old.jsonl", 8 * 86_400_000);
    let mut metrics = Metrics::default();
    let error = run(
        &f.dir(),
        7 * 86_400_000,
        100,
        f.now,
        &mut metrics,
        &mut |_, _| false,
        &mut |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(metrics.deleted_records, 0);
    assert!(
        run(
            &not_dir,
            7 * 86_400_000,
            100,
            f.now,
            &mut Metrics::default(),
            &mut |_, _| false,
            &mut |_| Ok(())
        )
        .is_err()
    );
}

#[test]
fn activity_is_rechecked_at_capacity_deletion_boundary() {
    let f = Fixture::new();
    f.put("live.jsonl", 8 * 86_400_000);
    let mut checks = 0;
    let mut metrics = Metrics::default();
    let error = run(
        &f.dir(),
        7 * 86_400_000,
        0,
        f.now,
        &mut metrics,
        &mut |_, _| {
            checks += 1;
            true
        },
        &mut |_| panic!("active call must survive both cleanup phases"),
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::StorageFull);
    assert_eq!(checks, 2);
    assert_eq!(metrics.activity_reads, 2);
}

#[test]
fn unrelated_files_and_nested_directories_are_never_touched() {
    let f = Fixture::new();
    f.put("notes.txt", 8 * 86_400_000);
    fs::create_dir(f.dir().join("nested.jsonl")).unwrap();
    fs::write(f.dir().join("nested.jsonl/keep.jsonl"), "keep").unwrap();
    let mut metrics = Metrics::default();
    run(
        &f.dir(),
        7 * 86_400_000,
        0,
        f.now,
        &mut metrics,
        &mut |_, _| panic!("unrelated entries must not read calls"),
        &mut |_| panic!("unrelated entries must not be removed"),
    )
    .unwrap();
    assert_eq!(metrics.records, 0);
    assert_eq!(metrics.metadata_reads, 0);
    assert_eq!(metrics.sorted_records, 0);
    assert!(f.dir().join("notes.txt").exists());
    assert!(f.dir().join("nested.jsonl/keep.jsonl").exists());
}

fn inventory(dir: &Path) -> io::Result<Vec<(std::ffi::OsString, u64, SystemTime)>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            entries.push((entry.file_name(), meta.len(), meta.modified()?));
        }
    }
    entries.sort();
    Ok(entries)
}

#[test]
#[ignore = "manual read-only live history capture; set ACTL_RETENTION_ROOT/OUTPUT/LABEL"]
fn readonly_history_profile() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::fs::canonicalize(std::env::var("ACTL_RETENTION_ROOT")?)?;
    let output = std::path::absolute(std::env::var("ACTL_RETENTION_OUTPUT")?)?;
    let output = output
        .parent()
        .ok_or("profile output requires a parent")?
        .canonicalize()?
        .join(
            output
                .file_name()
                .ok_or("profile output requires a filename")?,
        );
    if output.starts_with(&dir) {
        return Err("profile evidence must be outside the history directory".into());
    }
    let mut file = fs::File::create_new(&output)?;
    let before = inventory(&dir)?;
    let mut metrics = Metrics {
        timed: true,
        ..Metrics::default()
    };
    let started = Instant::now();
    let mut planned = 0u64;
    // Read the production directory, but replace every deletion with a counter.
    // No retention marker, journal, flow or recovery data is written.
    let result = run(
        &dir,
        7 * 86_400_000,
        3 * 1024 * 1024 * 1024,
        crate::state::unix_ms(),
        &mut metrics,
        &mut active,
        &mut |_| {
            planned += 1;
            Ok(())
        },
    );
    let total_us = started.elapsed().as_micros();
    let after = inventory(&dir)?;
    let record = serde_json::json!({"label":std::env::var("ACTL_RETENTION_LABEL")?,
        "root":dir,"binary":std::env::current_exe()?,"os":std::env::consts::OS,
        "arch":std::env::consts::ARCH,"ts_ms":crate::state::unix_ms(),
        "total_us":total_us,"metrics":metrics,"planned_deletions":planned,
        "inventory_unchanged":before==after,"files_deleted":false,"marker_changed":false,
        "status":if result.is_err(){"failed"}else if before!=after{"inconclusive"}else{"observed"},
        "error_kind":result.as_ref().err().map(|e|format!("{:?}", e.kind()))});
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(&record)?)?;
    result?;
    if before != after {
        return Err("history inventory changed during profiling".into());
    }
    Ok(())
}
