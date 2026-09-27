//! Display-only action facts. No input contents, platform handles or execution policy.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardEffect {
    #[default]
    Untouched,
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputEffects {
    pub keyboard: bool,
    pub pointer: bool,
    pub clipboard: ClipboardEffect,
    pub focus: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionPhase {
    Preparing,
    Executing,
    Delivered,
    Failed,
    Cancelled,
    Partial,
}

/// Physical virtual-desktop pixels; a rectangle is [left, top, right, bottom].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTarget {
    pub window: Option<String>,
    pub point: Option<[i32; 2]>,
    pub bounds: Option<[i32; 4]>,
    pub destination: Option<[i32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionState {
    pub id: u64,
    pub seq: u64,
    pub kind: String,
    pub phase: ActionPhase,
    pub effects: InputEffects,
    pub target: ActionTarget,
    pub ts_ms: u64,
    pub expires_ms: u64,
}

impl ActionState {
    pub fn active(&self) -> bool {
        matches!(self.phase, ActionPhase::Preparing | ActionPhase::Executing)
    }
}

/// A consumer cursor starts at launch time: reconnects never replay old clicks.
#[derive(Default)]
pub struct ActionCursor {
    since_ms: u64,
    seen: std::collections::HashMap<String, u64>,
}

impl ActionCursor {
    pub fn new(since_ms: u64) -> Self {
        Self {
            since_ms,
            seen: Default::default(),
        }
    }

    pub fn take(&mut self, states: &[crate::state::SessionState], now: u64) -> Vec<ActionState> {
        let mut out = Vec::new();
        self.seen
            .retain(|id, _| states.iter().any(|s| &s.call_id == id));
        for state in states {
            let seq = self.seen.entry(state.call_id.clone()).or_default();
            for action in &state.recent_actions {
                if action.seq > *seq
                    && action.ts_ms >= self.since_ms
                    && action.expires_ms > now
                    && action.phase == ActionPhase::Delivered
                {
                    out.push(action.clone());
                }
                *seq = (*seq).max(action.seq);
            }
        }
        out.sort_by_key(|a| a.ts_ms);
        out
    }
}
