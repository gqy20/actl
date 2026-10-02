//! History cleanup instrumentation. Recovery directories are never traversed.
use serde::Serialize;
use std::{fs, io, path::Path, time::Instant};

#[derive(Default, Serialize)]
struct Metrics {
    enumerated: u64,
    records: u64,
    metadata_reads: u64,
    activity_reads: u64,
    sorted_records: usize,
    deleted_records: u64,
    deleted_bytes: u64,
    remaining_bytes: Option<u64>,
    enumeration_us: u64,
    metadata_us: u64,
    activity_us: u64,
    sorting_us: u64,
    deletion_us: u64,
    #[serde(skip)]
    timed: bool,
}

impl Metrics {
    fn start(&self) -> Option<Instant> {
        self.timed.then(Instant::now)
    }
}

fn elapsed(start: Option<Instant>) -> u64 {
    start.map_or(0, |s| s.elapsed().as_micros() as u64)
}

pub fn cleanup(dir: &Path, age: u64, budget: u64) -> io::Result<()> {
    let mut metrics = Metrics {
        timed: crate::trace::enabled(),
        ..Metrics::default()
    };
    let started = metrics.start();
    let result = run(
        dir,
        age,
        budget,
        crate::state::unix_ms(),
        &mut metrics,
        &mut active,
        &mut |path| fs::remove_file(path),
    );
    if metrics.timed {
        crate::trace::detail(
            "history.cleanup",
            &serde_json::json!({"outcome":if result.is_ok(){"succeeded"}else{"failed"},
            "total_us":elapsed(started),"metrics":metrics,
            "error_kind":result.as_ref().err().map(|e|format!("{:?}",e.kind()))}),
        );
    }
    result
}

fn active(path: &Path, now: u64) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|id| {
            let state = path
                .parent()?
                .parent()?
                .join("calls")
                .join(format!("{id}.json"));
            serde_json::from_slice::<crate::state::SessionState>(&fs::read(state).ok()?).ok()
        })
        .is_some_and(|s| {
            s.phase == crate::state::Phase::Running && now.saturating_sub(s.ts_ms) < 10_000
        })
}

fn is_active(
    path: &Path,
    now: u64,
    metrics: &mut Metrics,
    check: &mut impl FnMut(&Path, u64) -> bool,
) -> bool {
    let started = metrics.start();
    metrics.activity_reads += 1;
    let active = check(path, now);
    metrics.activity_us += elapsed(started);
    active
}

fn remove(
    path: &Path,
    size: u64,
    metrics: &mut Metrics,
    delete: &mut impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let started = metrics.start();
    let result = delete(path);
    metrics.deletion_us += elapsed(started);
    result?;
    metrics.deleted_records += 1;
    metrics.deleted_bytes += size;
    Ok(())
}

fn run(
    dir: &Path,
    age: u64,
    budget: u64,
    now: u64,
    metrics: &mut Metrics,
    check: &mut impl FnMut(&Path, u64) -> bool,
    delete: &mut impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut files = Vec::new();
    let started = metrics.start();
    let mut entries = fs::read_dir(dir)?;
    metrics.enumeration_us += elapsed(started);
    loop {
        let started = metrics.start();
        let entry = entries.next();
        metrics.enumeration_us += elapsed(started);
        let Some(entry) = entry else { break };
        let entry = entry?;
        metrics.enumerated += 1;
        let started = metrics.start();
        let path = entry.path();
        let regular = entry.file_type()?.is_file();
        let eligible = regular
            && matches!(
                path.extension().and_then(|s| s.to_str()),
                Some("json" | "jsonl")
            );
        metrics.metadata_us += elapsed(started);
        if !eligible {
            continue;
        }
        let started = metrics.start();
        metrics.metadata_reads += 1;
        let meta = entry.metadata()?;
        let ts = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        metrics.metadata_us += elapsed(started);
        metrics.records += 1;
        if now.saturating_sub(ts) > age && !is_active(&path, now, metrics, check) {
            remove(&path, meta.len(), metrics, delete)?;
        } else {
            files.push((ts, meta.len(), path));
        }
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    metrics.remaining_bytes = Some(total);
    if total <= budget {
        return Ok(());
    }
    let started = metrics.start();
    metrics.sorted_records = files.len();
    files.sort_by_key(|f| f.0);
    metrics.sorting_us += elapsed(started);
    for (ts, size, path) in files {
        if total <= budget {
            break;
        }
        if now.saturating_sub(ts) < 3_600_000 || is_active(&path, now, metrics, check) {
            continue;
        }
        remove(&path, size, metrics, delete)?;
        total = total.saturating_sub(size);
        metrics.remaining_bytes = Some(total);
    }
    if total > budget {
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "history capacity reached; recent records protected",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
