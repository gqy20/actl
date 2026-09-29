//! Versioned display lease and per-action presentation acknowledgements.
use crate::state::{SignalPaths, unix_ms};
use serde::{Deserialize, Serialize};

pub const DISPLAY_PROTOCOL: u32 = 8;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisplayLease {
    pub protocol: u32,
    pub build: String,
    pub pid: u32,
    pub epoch: String,
    pub state: String,
    pub ts_ms: u64,
}
impl DisplayLease {
    pub fn compatible(&self, now: u64) -> bool {
        self.protocol == DISPLAY_PROTOCOL
            && self.build == env!("CARGO_PKG_VERSION")
            && self.ts_ms <= now
            && now - self.ts_ms < 1500
    }
    pub fn ready(&self, now: u64) -> bool {
        self.compatible(now) && self.state == "ready"
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisplayAck {
    pub epoch: String,
    pub call_id: String,
    pub action_id: u64,
    pub ts_ms: u64,
}
impl SignalPaths {
    pub fn display_lease(&self) -> Option<DisplayLease> {
        serde_json::from_slice(&std::fs::read(self.dir.join("display.json")).ok()?).ok()
    }
    pub fn write_display<T: Serialize>(&self, name: &str, value: &T) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let target = self.dir.join(name);
        let tmp = target.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&tmp, serde_json::to_vec(value)?)?;
        std::fs::rename(tmp, target)
    }
    pub fn acknowledged(&self, call: &str, action: u64, epoch: &str, since: u64) -> bool {
        let ack = std::fs::read(self.dir.join(format!("ack-{call}.json")))
            .ok()
            .and_then(|v| serde_json::from_slice::<DisplayAck>(&v).ok());
        ack.is_some_and(|a| {
            a.call_id == call
                && a.action_id == action
                && a.epoch == epoch
                && a.ts_ms >= since
                && a.ts_ms <= unix_ms()
                && unix_ms().saturating_sub(a.ts_ms) < 1500
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_requires_current_version_fresh_visible_frame() {
        let mut l = DisplayLease {
            protocol: DISPLAY_PROTOCOL,
            build: env!("CARGO_PKG_VERSION").into(),
            pid: 1,
            epoch: "test".into(),
            state: "ready".into(),
            ts_ms: 100,
        };
        assert!(l.ready(101));
        assert!(!l.ready(99));
        assert!(!l.ready(1600));
        l.state = "capture".into();
        assert!(!l.ready(101));
        assert!(l.compatible(101));
        l.state = "closed".into();
        assert!(!l.ready(101));
        l.protocol = 2;
        assert!(!l.compatible(101));
    }
}
