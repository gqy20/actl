//! Task consent is volatile: restart, external input, or expiry revokes it.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    #[default]
    Expiring,
    OncePerRun,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub approval: Approval,
}
thread_local! { static POLICY: std::cell::Cell<Approval> = const { std::cell::Cell::new(Approval::Expiring) }; }
pub fn configure(policy: Approval) {
    POLICY.with(|p| p.set(policy));
}
pub fn configured() -> Approval {
    POLICY.with(|p| p.get())
}

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
pub const CANCELLED: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub approval: Approval,
    pub generation: String,
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
            && !self.generation.is_empty()
    }
}

#[derive(Default)]
pub struct Consent {
    grant: Option<(String, u64, u64, u64)>,
    approval: Approval,
    reason: Option<&'static str>,
}
/// 授权动作自身余韵的宽限窗:授权(点击"开始"或快捷键)后的物理输入在
/// 此窗内视为 settle(按键/鼠标抬起、手部余动),刷新基线而非撤销。
/// 没有它,严格相等判定会让授权点击自己的 mouse-up 必然杀死授权
/// (实测:human_takeover_or_grant_expired 于授权后第一次 check 即现)。
pub const GRACE_MS: u64 = 750;
impl Consent {
    /// Called only by the companion's explicit user Start/Continue action
    /// (button click or Ctrl+Alt+Y hotkey).
    pub fn allow(&mut self, task: &str, input_seq: u64, now: u64) {
        self.allow_with_policy(task, input_seq, now, Approval::Expiring);
    }
    pub fn allow_with_policy(&mut self, task: &str, input_seq: u64, now: u64, approval: Approval) {
        self.approval = approval;
        self.grant = Some((task.into(), input_seq, now, now));
        self.reason = Some("allowed");
    }
    pub fn revoke(&mut self) {
        self.revoke_with_reason("revoked");
    }
    pub fn revoke_with_reason(&mut self, reason: &'static str) {
        if self.grant.take().is_some() || self.reason.is_none() {
            self.reason = Some(reason);
        }
    }
    pub fn reason(&self) -> &'static str {
        self.reason.unwrap_or("no_grant")
    }
    pub fn check(&mut self, task: &str, input_seq: u64, now: u64) -> bool {
        let Some((owner, seq, started, touched)) = self.grant.as_mut() else {
            return false;
        };
        let expired = if now < *touched {
            Some("clock_reversed")
        } else if self.approval == Approval::Expiring && now - *started >= 600_000 {
            Some("duration_expired")
        } else if self.approval == Approval::Expiring && now - *touched >= 60_000 {
            Some("idle_expired")
        } else {
            None
        };
        if let Some(reason) = expired {
            self.revoke_with_reason(reason);
            return false;
        }
        if *seq != input_seq {
            if now - *started < GRACE_MS {
                // 宽限窗内:授权动作的余韵,重设基线继续放行
                *seq = input_seq;
            } else {
                self.revoke_with_reason("external_input");
                return false;
            }
        }
        if owner != task {
            self.reason = Some("different_task");
            return false;
        }
        *touched = now;
        true
    }
    pub fn check_policy(
        &mut self,
        task: &str,
        input_seq: u64,
        now: u64,
        approval: Approval,
    ) -> bool {
        if self.grant.is_some() && self.approval != approval {
            self.reason = Some("policy_changed");
            return false;
        }
        self.check(task, input_seq, now)
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
        c.allow("a", 10, 10_000); // 宽限窗之外的时间原点
        assert!(!c.check("a", 11, 10_000 + GRACE_MS + 1));
        assert!(!c.check("a", 10, 10_000 + GRACE_MS + 2));
        c.allow("a", 12, 10_000 + GRACE_MS + 3);
        assert!(c.check("a", 12, 10_000 + GRACE_MS + 4));
    }

    #[test]
    fn grant_settle_grace_absorbs_then_goes_strict() {
        let mut c = Consent::default();
        c.allow("a", 10, 1_000);
        // 授权 mouse-up 与手部余动(宽限窗内):刷新基线,不撤销
        assert!(c.check("a", 12, 1_050));
        assert!(c.check("a", 15, 1_700));
        // 窗外的新输入 = 外部接管,撤销
        assert!(!c.check("a", 16, 1_000 + GRACE_MS + 5));
        // 撤销后不可复活
        assert!(!c.check("a", 16, 1_000 + GRACE_MS + 10));
    }

    #[test]
    fn grace_does_not_change_task_scope_or_expiry() {
        let mut c = Consent::default();
        c.allow("a", 10, 1_000);
        assert!(!c.check("b", 11, 1_100), "余韵不改任务归属");
        assert!(c.check("a", 11, 1_100));
        // idle/expiry 上限照旧
        assert!(!c.check("a", 11, 1_100 + 60_000));
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
