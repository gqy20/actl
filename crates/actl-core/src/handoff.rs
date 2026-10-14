//! Task consent is volatile: restart, external input, or expiry revokes it.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    #[default]
    Expiring,
    OncePerRun,
    /// 常驻信任:不过期、用户输入只让行不撤销(安静即自动通过)。
    /// 授予来源只有两种,均为用户动作:显示端托盘的「预授权写入」开关
    /// (持久,对所有写入生效),或流程声明 `approval: standing` 后用户
    /// 点一次开始(会话内常驻)。声明本身永远不构成授权。
    Standing,
}
/// 误触自动恢复（pardon）：授权被外部输入撤销后，若撤销源与后续输入均为
/// "良性"（纯鼠标移动，无任何按键/点击/滚轮），静默窗内无新输入且前台未变，
/// 显示端可自动重新授权，预算内不需要用户点击。任何键盘事件永久不赦免。
/// 流程可在 handoff.pardon_input 覆盖全局默认（逐流程优先）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PardonPolicy {
    pub enabled: bool,
    /// 静默窗：最后一次外部输入后需安静的毫秒数（1000..=10000）。
    pub quiet_ms: u64,
    /// 每任务自动恢复次数（1..=5）；耗尽后回到显式确认。
    pub budget: u32,
    /// 恢复前要求前台窗口与出卡时一致。
    pub require_foreground: bool,
    /// 仅当待执行动作非键盘通道时赦免（type/press 等注入键盘的动作永不自动恢复）。
    pub keyboard_safe_only: bool,
}
impl Default for PardonPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            quiet_ms: 4000,
            budget: 2,
            require_foreground: true,
            keyboard_safe_only: true,
        }
    }
}
impl PardonPolicy {
    pub fn valid(&self) -> bool {
        (1000..=10000).contains(&self.quiet_ms) && (1..=5).contains(&self.budget)
    }
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PardonInputs {
    pub now_ms: u64,
    /// 撤销授权的那次输入是否为按键/点击/滚轮（硬输入）。
    pub revoker_hard: bool,
    /// 最后一次外部输入时间；无输入则 None。
    pub last_input_ms: Option<u64>,
    /// 撤销之后是否出现过硬输入；有则永不赦免。
    pub hard_input_after_revoke: bool,
    /// 前台窗口与出卡时是否一致。
    pub foreground_same: bool,
    /// 待执行动作是否为键盘通道。
    pub keyboard_action: bool,
    pub budget_left: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PardonDecision {
    /// 条件满足，自动重新授权。
    Pardon,
    /// 条件未满足但可继续等待（静默窗未满等）。
    Wait,
    /// 结构性不满足：出卡等显式确认，本任务不再自动恢复。
    Never,
}
pub fn pardon_decision(inputs: PardonInputs, policy: &PardonPolicy) -> PardonDecision {
    if !policy.enabled || inputs.revoker_hard || inputs.hard_input_after_revoke {
        return PardonDecision::Never;
    }
    if policy.keyboard_safe_only && inputs.keyboard_action {
        return PardonDecision::Never;
    }
    if inputs.budget_left == 0 {
        return PardonDecision::Never;
    }
    if policy.require_foreground && !inputs.foreground_same {
        return PardonDecision::Never;
    }
    match inputs.last_input_ms {
        None => PardonDecision::Wait,
        Some(last) if inputs.now_ms.saturating_sub(last) < policy.quiet_ms => PardonDecision::Wait,
        Some(_) => PardonDecision::Pardon,
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub approval: Approval,
    #[serde(default, skip_serializing_if = "PardonPolicy::is_default")]
    pub pardon_input: PardonPolicy,
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
pub const FOREGROUND_MESSAGE: u32 = 0x8000 + 51;
pub const PENDING: usize = 1;
pub const ALLOWED: usize = 2;
pub const DEFERRED: usize = 3;
pub const YIELDED: usize = 4;
pub const PANEL_UNAVAILABLE: usize = 5;
pub const MONITOR_UNAVAILABLE: usize = 6;
pub const USER_PAUSED: usize = 7;
pub const CANCELLED: usize = 8;
/// Standing 授权仍在、仅输入活跃(显示协议 v11):客户端应做有界等待安静,
/// 而非立即让行失败——预授权下误触不再直接打断任务(批次3)。
pub const YIELD_BUSY: usize = 9;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub approval: Approval,
    pub generation: String,
    pub call: String,
    pub task: String,
    pub target: String,
    pub ts_ms: u64,
    /// 动作种类（click/type/press…）；旧执行器请求缺失时按未知处理。
    #[serde(default)]
    pub action: String,
    /// 冻结计划的指纹（声明 standing 的流程首跑由执行器填入;哈希变化=模板已改,
    /// 信任集自动失效）。持久信任按此匹配,与全局预授权正交。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
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
            } else if self.approval == Approval::Standing {
                // 常驻信任下用户输入是预期状态:本边界让行但不撤销,
                // 重置基线——计数不再变化(安静)即自动通过,无需点击。
                self.reason = Some("input_active");
                *seq = input_seq;
                return false;
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
    error_at(&crate::state::SignalPaths::default(), reason)
}

/// 路径注入版(测试隔离):execution 态从给定目录尽力读。生产路径走
/// [`error`](fn.error.html)(机器级 %LOCALAPPDATA%)——golden 契约不得
/// 依赖本机真实代次。
pub fn error_at(paths: &crate::state::SignalPaths, reason: &str) -> crate::CtlError {
    crate::CtlError::with_evidence(
        crate::ErrorCode::NotActionable,
        "desktop handoff required; user must choose Start/Continue in actl, then re-observe before retrying interrupted work",
        serde_json::json!({
            "stage":"handoff", "reason":reason, "retry_input":false,
            // 状态联动(2026-10-06 基建②):拒绝自述 execution 态——暂停守护
            // 拒绝时调用方直接看到「是谁持有现场」,不必反查。无锁尽力读,
            // 错误路径不阻塞。
            "execution": paths.execution_status_best_effort(),
        }),
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
    fn standing_trust_never_expires_and_yields_without_revoking() {
        let mut c = Consent::default();
        c.allow_with_policy("a", 10, 100, Approval::Standing);
        // 不过期:远超 expiring 的 10 分钟总量与 60 秒闲置
        assert!(c.check("a", 10, 100 + 700_000));
        // 用户输入:让行(拒绝)但不撤销,基线重置后安静即自动通过
        assert!(!c.check("a", 11, 100 + 700_001));
        assert_eq!(c.reason(), "input_active");
        assert!(c.check("a", 11, 100 + 700_002));
        // 任务边界仍然生效
        assert!(!c.check("b", 11, 100 + 700_003));
        // 急停等显式撤销仍然杀死常驻信任
        c.revoke_with_reason("stopped");
        assert!(!c.check("a", 11, 100 + 700_004));
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

    fn inputs(now: u64, last: Option<u64>) -> PardonInputs {
        PardonInputs {
            now_ms: now,
            revoker_hard: false,
            last_input_ms: last,
            hard_input_after_revoke: false,
            foreground_same: true,
            keyboard_action: false,
            budget_left: 2,
        }
    }
    #[test]
    fn pardon_waits_for_quiet_then_recovers_within_budget() {
        let policy = PardonPolicy::default();
        assert_eq!(
            pardon_decision(inputs(10_000, Some(9_000)), &policy),
            PardonDecision::Wait
        );
        assert_eq!(
            pardon_decision(inputs(10_000, Some(10_000 - policy.quiet_ms)), &policy),
            PardonDecision::Pardon
        );
        let drained = PardonInputs {
            budget_left: 0,
            ..inputs(10_000, Some(0))
        };
        assert_eq!(pardon_decision(drained, &policy), PardonDecision::Never);
    }
    #[test]
    fn hard_input_keyboards_and_focus_change_never_pardon() {
        let policy = PardonPolicy::default();
        let hard_revoker = PardonInputs {
            revoker_hard: true,
            ..inputs(10_000, Some(0))
        };
        assert_eq!(
            pardon_decision(hard_revoker, &policy),
            PardonDecision::Never
        );
        let hard_after = PardonInputs {
            hard_input_after_revoke: true,
            ..inputs(10_000, Some(0))
        };
        assert_eq!(pardon_decision(hard_after, &policy), PardonDecision::Never);
        let typing = PardonInputs {
            keyboard_action: true,
            ..inputs(10_000, Some(0))
        };
        assert_eq!(pardon_decision(typing, &policy), PardonDecision::Never);
        let moved_focus = PardonInputs {
            foreground_same: false,
            ..inputs(10_000, Some(0))
        };
        assert_eq!(pardon_decision(moved_focus, &policy), PardonDecision::Never);
        let relaxed = PardonPolicy {
            require_foreground: false,
            keyboard_safe_only: false,
            ..policy
        };
        assert_eq!(
            pardon_decision(
                PardonInputs {
                    keyboard_action: true,
                    foreground_same: false,
                    ..inputs(10_000, Some(0))
                },
                &relaxed
            ),
            PardonDecision::Pardon
        );
    }
    #[test]
    fn disabled_policy_and_unknown_ranges_fail_closed() {
        assert!(
            !PardonPolicy {
                quiet_ms: 999,
                ..PardonPolicy::default()
            }
            .valid()
        );
        assert!(
            !PardonPolicy {
                budget: 0,
                ..PardonPolicy::default()
            }
            .valid()
        );
        let off = PardonPolicy {
            enabled: false,
            ..PardonPolicy::default()
        };
        assert_eq!(
            pardon_decision(inputs(10_000, Some(0)), &off),
            PardonDecision::Never
        );
        assert_eq!(
            serde_json::from_str::<Policy>("{\"approval\":\"once_per_run\"}").unwrap(),
            Policy {
                approval: Approval::OncePerRun,
                pardon_input: PardonPolicy::default()
            }
        );
    }
}

// ─── 始终信任此流程:指纹持久信任(与全局预授权正交) ─────────────────────

/// 冻结计划指纹:规范化 JSON → FNV-1a 双通道 128 位。非密码学(非对抗场景:
/// 用户自己的模板变更检测);serde_json 的 BTreeMap 保证键序确定。
pub fn plan_fingerprint(value: &serde_json::Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    let (mut h1, mut h2) = (0xcbf2_9ce4_8422_2325u64, 0x9e37_79b9_7f4a_7c15u64);
    for &b in &bytes {
        h1 = (h1 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        h2 = (h2 ^ u64::from(b).rotate_left(7)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h1:016x}{h2:016x}")
}

/// 结构指纹(2026-10-14 信任梯度补全):standing 信任的哈希面从全量冻结计划
/// 收窄为**结构面**——剔除 inputs(调用方参数,设计上就该换:timing-sweep/
/// 回归场景换参数不该击穿信任集)与 auto_resume(策略面,与结构无关)。
/// 步骤增删改/name/handoff/popup_handlers 变更仍在哈希面内(模板结构性
/// 变更自动失效的既有语义不变)。风险与预授权同档:信任的是"这份流程的
/// 结构",参数本来就是调用方权限内的自由度。
pub fn plan_structure_fingerprint(value: &serde_json::Value) -> String {
    let mut structural = value.clone();
    if let serde_json::Value::Object(map) = &mut structural {
        map.remove("inputs");
        map.remove("auto_resume");
    }
    plan_fingerprint(&structural)
}

/// 信任集条目:指纹 → 何时信任的何份计划。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustEntry {
    pub fingerprint: String,
    pub name: String,
    pub trusted_at_ms: u64,
}

/// 持久信任集(`%LOCALAPPDATA%\actl\trusted-flows.json`),由显示端 UI 独占写入。
/// 显示端重启不丢;模板变更 → 指纹变 → 自动失效(重新询问一次)。
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TrustStore {
    pub entries: Vec<TrustEntry>,
}
impl TrustStore {
    const MAX: usize = 128;

    pub fn trusted(&self, fingerprint: &str) -> bool {
        self.entries.iter().any(|e| e.fingerprint == fingerprint)
    }

    pub fn grant(&mut self, fingerprint: &str, name: &str, now_ms: u64) {
        if self.trusted(fingerprint) {
            return;
        }
        self.entries.push(TrustEntry {
            fingerprint: fingerprint.into(),
            name: name.into(),
            trusted_at_ms: now_ms,
        });
        // 有界:最旧先出
        if self.entries.len() > Self::MAX {
            self.entries.drain(..self.entries.len() - Self::MAX);
        }
    }

    pub fn revoke(&mut self, fingerprint: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.fingerprint != fingerprint);
        self.entries.len() != before
    }
}
impl crate::state::SignalPaths {
    pub fn trusted_flows_file(&self) -> std::path::PathBuf {
        self.dir.join("trusted-flows.json")
    }
    /// 读取失败按空集处理(fail-closed)。
    pub fn trust_store(&self) -> TrustStore {
        std::fs::read(self.trusted_flows_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save_trust_store(&self, store: &TrustStore) {
        use std::io::Write;
        let tmp = self.dir.join("trusted-flows.json.tmp");
        let saved = (|| {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&serde_json::to_vec_pretty(store).unwrap_or_default())?;
            std::fs::rename(&tmp, self.trusted_flows_file())
        })();
        if saved.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod trust_tests {
    use super::*;
    #[test]
    fn fingerprint_is_deterministic_and_change_sensitive() {
        let a = serde_json::json!({"steps":[{"id":"x","run":{"command":"find"}}]});
        let b = serde_json::json!({"steps":[{"id":"x","run":{"command":"find"}}]});
        let c = serde_json::json!({"steps":[{"id":"x","run":{"command":"get"}}]});
        let fa = plan_fingerprint(&a);
        assert_eq!(fa, plan_fingerprint(&b), "identical plans must match");
        assert_ne!(fa, plan_fingerprint(&c), "changed plan must differ");
        assert_eq!(fa.len(), 32, "128-bit hex: {fa}");
    }
    #[test]
    fn structure_fingerprint_ignores_params_but_not_steps() {
        // 2026-10-14 信任梯度补全:inputs(参数面)与 auto_resume(策略面)
        // 变更不击穿信任集;steps 结构变更必须失效
        let base = serde_json::json!({
            "name": "sweep", "steps": [{"click": ["name:微信"]}],
            "inputs": {"window": {"default": "w1"}},
            "auto_resume": "action_failed",
        });
        let mut param_changed = base.clone();
        param_changed["inputs"]["window"]["default"] = "w2".into();
        param_changed["auto_resume"] = serde_json::Value::Null;
        assert_eq!(
            plan_structure_fingerprint(&base),
            plan_structure_fingerprint(&param_changed),
            "param/strategy drift must not break the trust set"
        );
        let mut step_changed = base.clone();
        step_changed["steps"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"press": ["enter"]}));
        assert_ne!(
            plan_structure_fingerprint(&base),
            plan_structure_fingerprint(&step_changed),
            "structural change must invalidate"
        );
        // name 变更也属结构面(handoff 元数据)
        let mut renamed = base.clone();
        renamed["name"] = "sweep-v2".into();
        assert_ne!(
            plan_structure_fingerprint(&base),
            plan_structure_fingerprint(&renamed)
        );
    }
    #[test]
    fn trust_store_grant_revoke_and_bound() {
        let mut s = TrustStore::default();
        s.grant("aa", "flow-a", 100);
        assert!(s.trusted("aa"));
        assert!(!s.trusted("bb"));
        s.grant("aa", "flow-a", 200); // 幂等
        assert_eq!(s.entries.len(), 1);
        assert!(s.revoke("aa"));
        assert!(!s.trusted("aa"));
        assert!(!s.revoke("aa"));
        for i in 0..TrustStore::MAX + 10 {
            s.grant(&format!("f{i}"), "f", 1);
        }
        assert!(s.entries.len() <= TrustStore::MAX);
    }
}
