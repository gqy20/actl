//! Contract tests must not consume a user's sticky stop flag or publish desktop activity.
pub struct IsolatedCommand {
    pub command: std::process::Command,
    dir: std::path::PathBuf,
}
impl IsolatedCommand {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("actl-contract-{}-{n}", std::process::id()));
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_actl"));
        command.env("LOCALAPPDATA", &dir);
        Self { command, dir }
    }
}
impl Drop for IsolatedCommand {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
