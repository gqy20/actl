//! Cross-process throttling for history retention scans, separate from recovery data.
use std::io::{Read, Seek, Write};
use std::path::Path;

pub(crate) fn periodic(
    dir: &Path,
    now: u64,
    work: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    periodic_with_interval(dir, now, 60_000, work)
}

pub(crate) fn periodic_with_interval(
    dir: &Path,
    now: u64,
    interval: u64,
    work: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut marker = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("retention.lock"))?;
    match marker.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(()),
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    let mut bytes = [0u8; 8];
    if marker.read_exact(&mut bytes).is_ok() {
        let last = u64::from_le_bytes(bytes);
        if now >= last && now - last < interval {
            return Ok(());
        }
    }
    work()?;
    // A failed/interrupted scan never marks itself successful; a later call retries.
    marker.rewind()?;
    marker.write_all(&now.to_le_bytes())?;
    marker.set_len(8)?;
    marker.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_invocations_do_not_rescan_history() {
        let dir = std::env::temp_dir().join(crate::new_snapshot_id());
        std::fs::create_dir(&dir).unwrap();
        let calls = std::cell::Cell::new(0);
        for now in [100_000, 100_001] {
            periodic(&dir, now, || {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn failed_scan_clock_rollback_and_expiry_retry() {
        let dir = std::env::temp_dir().join(crate::new_snapshot_id());
        std::fs::create_dir(&dir).unwrap();
        assert!(periodic(&dir, 100_000, || Err(std::io::Error::other("failed"))).is_err());
        let calls = std::cell::Cell::new(0);
        for now in [100_001, 99_000, 159_000] {
            periodic(&dir, now, || {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(calls.get(), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_caller_does_not_block_on_retention() {
        let dir = std::env::temp_dir().join(crate::new_snapshot_id());
        std::fs::create_dir(&dir).unwrap();
        periodic(&dir, 100_000, || {
            periodic(&dir, 100_001, || panic!("second cleanup must not run"))
        })
        .unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
