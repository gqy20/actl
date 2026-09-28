//! Task consent is volatile: restart, external input, or expiry revokes it.
use serde::{Deserialize, Serialize};

pub const INPUT_TAG: usize = 0x4143544c;
pub const REQUEST_MESSAGE: u32 = 0x8000 + 46;
pub const CHECK_MESSAGE: u32 = 0x8000 + 47;
pub const RECOVER_MESSAGE: u32 = 0x8000 + 48;
pub const PENDING: usize = 1;
pub const ALLOWED: usize = 2;
pub const DEFERRED: usize = 3;
pub const YIELDED: usize = 4;
pub const PANEL_UNAVAILABLE: usize = 5;
pub const MONITOR_UNAVAILABLE: usize = 6;
pub const USER_PAUSED: usize = 7;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub call: String,
    pub task: String,
    pub target: String,
    pub ts_ms: u64,
}
impl Request {
    pub fn fresh(&self, now: u64) -> bool {
        self.ts_ms <= now
            && now - self.ts_ms < 2000
            && !self.call.is_empty()
            && !self.task.is_empty()
    }
}

#[derive(Default)]
pub struct Consent {
    grant: Option<(String, u64, u64, u64)>,
}
impl Consent {
    /// Called only by the companion's explicit user Start/Continue button.
    pub fn allow(&mut self, task: &str, input_seq: u64, now: u64) {
        self.grant = Some((task.into(), input_seq, now, now));
    }
    pub fn revoke(&mut self) {
        self.grant = None;
    }
    pub fn check(&mut self, task: &str, input_seq: u64, now: u64) -> bool {
        let Some((owner, seq, started, touched)) = self.grant.as_mut() else {
            return false;
        };
        if *seq != input_seq
            || now < *touched
            || now - *touched >= 60_000
            || now - *started >= 600_000
        {
            self.revoke();
            return false;
        }
        if owner != task {
            return false;
        }
        *touched = now;
        true
    }
}

/// Ignore only events tagged by this injector and marked injected by Windows.
pub fn external_input(injected: bool, tag: usize) -> bool {
    !injected || tag != INPUT_TAG
}

pub fn error(reason: &str) -> crate::CtlError {
    crate::CtlError::with_evidence(
        crate::ErrorCode::NotActionable,
        "desktop handoff required; user must choose Start/Continue in actl, then re-observe before retrying interrupted work",
        serde_json::json!({"stage":"handoff", "reason":reason, "retry_input":false}),
    )
}

pub fn foreground_error(expected: &str, actual: &str) -> crate::CtlError {
    crate::CtlError::with_evidence(
        crate::ErrorCode::NotActionable,
        "physical foreground does not match the target; re-observe the window before retrying",
        serde_json::json!({"stage":"foreground_verify", "reason":"foreground_mismatch", "expected":expected, "actual":actual}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_events_from_other_tools_still_yield() {
        assert!(!external_input(true, INPUT_TAG));
        assert!(external_input(false, INPUT_TAG));
        assert!(external_input(false, 0));
        assert!(external_input(true, 0));
    }
    #[test]
    fn foreground_mismatch_never_recommends_elevation() {
        let e = foreground_error("target", "");
        assert_eq!(e.code, crate::ErrorCode::NotActionable);
        assert!(!e.code.recovery_hint().contains("elevated"));
    }
    #[test]
    fn consent_is_explicit_and_task_scoped() {
        let mut c = Consent::default();
        assert!(!c.check("a", 10, 100));
        c.allow("a", 10, 100);
        assert!(c.check("a", 10, 101));
        assert!(!c.check("b", 10, 102));
        assert!(c.check("a", 10, 103));
    }
    #[test]
    fn same_field_input_revokes_even_without_focus_change() {
        let mut c = Consent::default();
        c.allow("a", 10, 100);
        assert!(!c.check("a", 11, 101));
        assert!(!c.check("a", 10, 102));
        c.allow("a", 12, 103);
        assert!(c.check("a", 12, 104));
    }
    #[test]
    fn expiry_restart_and_clock_reversal_fail_closed() {
        let mut c = Consent::default();
        c.allow("a", 0, 100);
        assert!(!c.check("a", 0, 60_100));
        c.allow("a", 0, 100);
        assert!(!c.check("a", 0, 99));
        c.allow("a", 0, 100);
        for t in (101..600_100).step_by(1000) {
            assert!(c.check("a", 0, t));
        }
        assert!(!c.check("a", 0, 600_100));
        assert!(!Consent::default().check("a", 0, 101));
    }
}
