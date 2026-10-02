//! Workflow-level lifecycle facts; individual actions and observations do not notify.
use actl_core::state::SessionState;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Started,
    Completed,
    Failed,
    Stopped,
    Paused,
    Attention,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Started => "任务开始",
            Self::Completed => "任务完成",
            Self::Failed => "任务失败",
            Self::Stopped => "任务已停止",
            Self::Paused => "任务已暂停",
            Self::Attention => "任务待核对",
        }
    }
    pub fn color(self) -> u32 {
        match self {
            Self::Started => 0x93B4CA,
            Self::Completed => 0x8AB698,
            Self::Failed => 0xE59188,
            Self::Stopped | Self::Paused | Self::Attention => 0xD4B879,
        }
    }
}
#[derive(Default)]
struct Seen {
    started: bool,
    finished: bool,
    timestamp: u64,
    status: String,
}
pub struct Cursor {
    since: u64,
    seen: HashMap<String, Seen>,
}
impl Cursor {
    pub fn new(since: u64) -> Self {
        Self {
            since,
            seen: HashMap::new(),
        }
    }
    pub fn take(&mut self, states: &[SessionState]) -> Vec<Kind> {
        let mut states = states
            .iter()
            .filter(|s| s.workflow.is_some())
            .collect::<Vec<_>>();
        states.sort_by_key(|s| s.ts_ms);
        let mut events = Vec::new();
        for state in states {
            let Some(flow) = &state.workflow else {
                continue;
            };
            let seen = self.seen.entry(flow.run_id.clone()).or_default();
            if state.ts_ms < seen.timestamp {
                continue;
            }
            seen.timestamp = state.ts_ms;
            let changed = seen.status != flow.status;
            seen.status.clone_from(&flow.status);
            // A display restart must not announce historical or already running tasks.
            if state.started_ms < self.since || state.pid == 0 {
                seen.started = true;
                seen.finished |= matches!(flow.status.as_str(), "completed" | "failed" | "stopped");
                continue;
            }
            let end = match flow.status.as_str() {
                "completed" => Some(Kind::Completed),
                "failed" => Some(Kind::Failed),
                "stopped" => Some(Kind::Stopped),
                _ => None,
            };
            if !seen.started && (flow.status == "running" || end.is_some()) {
                events.push(Kind::Started);
                seen.started = true;
            }
            if !seen.finished
                && let Some(end) = end
            {
                events.push(end);
                seen.finished = true;
            }
            if changed && flow.status == "running" {
                seen.finished = false;
            }
            if changed && end.is_none() {
                match flow.status.as_str() {
                    "paused"
                        if !matches!(
                            flow.reason.as_deref(),
                            Some("checkpoint" | "start_paused")
                        ) =>
                    {
                        events.push(Kind::Paused)
                    }
                    "needs_review" => events.push(Kind::Attention),
                    _ => (),
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actl_core::state::{Phase, WorkflowProgress};
    fn state(id: &str, status: &str, time: u64) -> SessionState {
        let mut s = SessionState::now(1, "click", None, None, Phase::Running);
        s.started_ms = 110;
        s.ts_ms = time;
        s.workflow = Some(WorkflowProgress {
            run_id: id.into(),
            title: "step".into(),
            status: status.into(),
            reason: None,
            step_id: Some("step".into()),
            error_code: None,
            actions: vec![],
        });
        s
    }
    #[test]
    fn step_updates_pause_resume_and_new_calls_only_notify_once_per_task() {
        let mut cursor = Cursor::new(100);
        assert_eq!(
            cursor.take(&[state("task", "running", 120)]),
            vec![Kind::Started]
        );
        for (i, status) in ["running", "paused", "running", "needs_review", "running"]
            .into_iter()
            .enumerate()
        {
            let expected = match status {
                "paused" => vec![Kind::Paused],
                "needs_review" => vec![Kind::Attention],
                _ => vec![],
            };
            assert_eq!(
                cursor.take(&[state("task", status, 130 + i as u64 * 10)]),
                expected
            );
            assert!(
                cursor
                    .take(&[state("task", status, 131 + i as u64 * 10)])
                    .is_empty()
            );
        }
        assert_eq!(
            cursor.take(&[state("task", "completed", 190)]),
            vec![Kind::Completed]
        );
        assert!(cursor.take(&[state("task", "completed", 200)]).is_empty());
        let raw = SessionState::now(1, "browser-click", None, None, Phase::Running);
        assert!(cursor.take(&[raw]).is_empty());
    }
    #[test]
    fn fast_tasks_keep_both_boundaries_and_old_snapshots_do_not_replay() {
        let mut cursor = Cursor::new(100);
        assert_eq!(
            cursor.take(&[state("fast", "completed", 120)]),
            vec![Kind::Started, Kind::Completed]
        );
        assert!(cursor.take(&[state("fast", "running", 115)]).is_empty());
        let mut old = state("old", "completed", 130);
        old.started_ms = 90;
        assert!(cursor.take(&[old]).is_empty());
        assert_eq!(
            cursor.take(&[state("failed", "failed", 140)]),
            vec![Kind::Started, Kind::Failed]
        );
    }
    #[test]
    fn paused_task_can_start_later_and_stop_is_not_a_success() {
        let mut cursor = Cursor::new(100);
        let mut paused = state("task", "paused", 120);
        paused.workflow.as_mut().unwrap().reason = Some("start_paused".into());
        assert!(cursor.take(&[paused]).is_empty());
        assert_eq!(
            cursor.take(&[state("task", "running", 130)]),
            vec![Kind::Started]
        );
        assert_eq!(
            cursor.take(&[state("task", "stopped", 140)]),
            vec![Kind::Stopped]
        );
    }
}
