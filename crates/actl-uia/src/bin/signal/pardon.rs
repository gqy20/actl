//! 误触自动恢复：授权被纯鼠标移动类外部输入撤销时，静默窗内无进一步输入
//! （且无任何键盘/点击）即可在预算内自动重新授权，不需要用户点击。
//! 任何键盘事件、点击、滚轮都不赦免；策略按流程 handoff.pardon_input 覆盖
//! 全局默认；ACTL_HANDOFF_PARDON=0 可全局停用（紧急回退）。
use actl_core::{
    handoff::{PardonDecision, PardonInputs, PardonPolicy, Policy, pardon_decision},
    state::{SignalPaths, unix_ms},
};
use std::sync::atomic::{AtomicU64, Ordering};

static LAST_INPUT_MS: AtomicU64 = AtomicU64::new(0);
static LAST_HARD_MS: AtomicU64 = AtomicU64::new(0);

/// 键盘通道动作种类：keyboard_safe_only 下永不自动恢复；未知种类按键盘处理（fail-closed）。
const KEYBOARD_ACTIONS: [&str; 5] = ["type", "press", "key-down", "key-up", "paste"];

pub(super) fn note_input(hard: bool) {
    let now = unix_ms();
    LAST_INPUT_MS.store(now, Ordering::SeqCst);
    if hard {
        LAST_HARD_MS.store(now, Ordering::SeqCst);
    }
}
fn last_input() -> Option<u64> {
    match LAST_INPUT_MS.load(Ordering::SeqCst) {
        0 => None,
        ms => Some(ms),
    }
}
fn hard_input_since(ms: u64) -> bool {
    LAST_HARD_MS.load(Ordering::SeqCst) > ms
}
fn global_kill_switch_off() -> bool {
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OFF.get_or_init(|| std::env::var("ACTL_HANDOFF_PARDON").is_ok_and(|v| v == "0"))
}

/// 出卡时刻的现场快照与任务级预算。
#[derive(Default)]
pub(super) struct State {
    task: Option<String>,
    used: u32,
    policy: Option<Policy>,
    card: Option<Card>,
    exhausted: bool,
    last_resume_request_ms: u64,
}
#[derive(Clone, Copy)]
struct Card {
    ms: u64,
    revoker_hard: bool,
    foreground: isize,
}
impl State {
    fn policy_for(&mut self, paths: &SignalPaths, task: &str) -> PardonPolicy {
        if self.task.as_deref() != Some(task) {
            self.task = Some(task.into());
            self.used = 0;
            self.policy = None;
            self.exhausted = false;
        }
        let policy = self
            .policy
            .get_or_insert_with(|| match super::ui_text::plan(paths, task) {
                Some(plan) => serde_json::from_value::<Policy>(
                    plan.get("handoff").cloned().unwrap_or_default(),
                )
                .unwrap_or_default(),
                None => Policy::default(),
            });
        policy.pardon_input
    }
    /// 出卡时快照：最近一次外部输入即撤销源，其类别决定可否赦免。
    pub fn on_card(&mut self) {
        let hard = LAST_HARD_MS.load(Ordering::SeqCst);
        self.card = Some(Card {
            ms: unix_ms(),
            revoker_hard: last_input().is_some_and(|last| last == hard),
            foreground: unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as isize
            },
        });
        self.exhausted = false;
    }
    fn inputs(&self, keyboard_action: bool, foreground_same: bool) -> PardonInputs {
        let card = self.card.unwrap_or(Card {
            ms: 0,
            revoker_hard: true,
            foreground: 0,
        });
        PardonInputs {
            now_ms: unix_ms(),
            revoker_hard: card.revoker_hard,
            last_input_ms: last_input(),
            hard_input_after_revoke: hard_input_since(card.ms),
            foreground_same,
            keyboard_action,
            budget_left: self.budget_left(),
        }
    }
    fn budget_left(&self) -> u32 {
        self.policy
            .map_or(0, |p| p.pardon_input.budget.saturating_sub(self.used))
    }
}

fn keyboard_action(action: &str) -> bool {
    action.is_empty() || KEYBOARD_ACTIONS.contains(&action)
}

/// Path A：交接卡正为一个在途请求展示。返回是否应自动授权。
pub(super) fn consider_card(
    state: &mut State,
    paths: &SignalPaths,
    request: &actl_core::handoff::Request,
    foreground_same: impl FnOnce(isize) -> bool,
) -> bool {
    if global_kill_switch_off() || state.exhausted {
        return false;
    }
    let policy = state.policy_for(paths, &request.task);
    if !policy.valid() {
        state.exhausted = true;
        return false;
    }
    let same = state
        .card
        .is_some_and(|card| foreground_same(card.foreground));
    match pardon_decision(
        state.inputs(keyboard_action(&request.action), same),
        &policy,
    ) {
        PardonDecision::Pardon => true,
        PardonDecision::Wait => false,
        PardonDecision::Never => {
            state.exhausted = true;
            false
        }
    }
}

/// Path B：因交接暂停（reason=handoff_pending）且执行器已退出的流程自动续跑。
/// 仅触发续跑请求；实际授权仍由续跑执行器的交接卡走 Path A 判定（含预算）。
/// 用户暂停、待核对、检查点与停止状态永不自动继续。
pub(super) fn consider_flow_resume(state: &mut State, paths: &SignalPaths, generation: &str) {
    if global_kill_switch_off() {
        return;
    }
    let now = unix_ms();
    if now.saturating_sub(state.last_resume_request_ms) < 5_000 {
        return;
    }
    let pointer = paths.dir.join("visible-flow.json");
    let Some(id) = std::fs::read(&pointer)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| v["id"].as_str().map(str::to_owned))
    else {
        return;
    };
    let Some(flow) = super::flow_ui::read(paths, &id) else {
        return;
    };
    if flow["status"].as_str() != Some("paused")
        || flow["reason"].as_str() != Some("handoff_pending")
        || flow["runner_active"].as_bool() != Some(false)
    {
        return;
    }
    let Some(updated_ms) = flow["updated_ms"].as_u64() else {
        return;
    };
    let policy = state.policy_for(paths, &id);
    if !policy.enabled || state.budget_left() == 0 || !policy.valid() {
        return;
    }
    let quiet = match last_input() {
        Some(last) => now.saturating_sub(last),
        None => return,
    };
    if hard_input_since(updated_ms)
        || quiet < policy.quiet_ms
        || now.saturating_sub(updated_ms) < 2_000
    {
        return;
    }
    state.last_resume_request_ms = now;
    let _ = super::flow_ui::request(&id, true, generation);
}

/// 预算记账：一次自动授权消耗一次。
pub(super) fn mark_pardoned(state: &mut State) {
    state.used = state.used.saturating_add(1);
    state.card = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state_with_budget(paths: &SignalPaths, task: &str, policy: Policy) -> State {
        let mut state = State::default();
        std::fs::create_dir_all(&paths.dir).unwrap();
        std::fs::write(
            paths.dir.join("plan.json"),
            serde_json::to_vec(&serde_json::json!({"handoff": policy})).unwrap(),
        )
        .unwrap();
        state.policy_for(paths, task);
        state
    }
    #[test]
    fn keyboard_kinds_and_unknowns_fail_closed() {
        assert!(keyboard_action("type"));
        assert!(keyboard_action("press"));
        assert!(keyboard_action(""));
        assert!(!keyboard_action("click"));
        assert!(!keyboard_action("set-value"));
    }
    #[test]
    fn budget_is_per_task_and_policy_caches() {
        let dir = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
        let paths = SignalPaths::at(&dir);
        let mut state = state_with_budget(&paths, "task-a", Policy::default());
        assert_eq!(state.budget_left(), 2);
        mark_pardoned(&mut state);
        mark_pardoned(&mut state);
        assert_eq!(state.budget_left(), 0);
        // A different task resets accounting, not the cached policy lookup.
        state.policy_for(&paths, "task-b");
        assert_eq!(state.budget_left(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
