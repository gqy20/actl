//! One publisher per invocation; a standard thread refreshes liveness even during COM calls.
//! Writes never contaminate stdout; execution-side acknowledgement gates detect missing records.
use crate::activity::{ActionPhase, ActionState, ActionTarget, InputEffects};
use crate::state::{Phase, SessionState, SignalPaths, unix_ms};
use std::cell::RefCell;
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::Duration;

struct Publisher {
    paths: SignalPaths,
    state: SessionState,
    journal: Option<crate::history::Journal>,
    task: Option<String>,
}
impl Publisher {
    fn event(&mut self, event: &str, data: serde_json::Value) {
        if let Some(journal) = &mut self.journal {
            journal.record_context(event, data, self.state.workflow.as_ref());
        }
    }
    fn action_event(&mut self) {
        if let Some(a) = &self.state.action {
            // Window titles, selectors, text and clipboard contents are deliberately omitted.
            let data = serde_json::json!({"action_id":a.id,"kind":a.kind,"phase":a.phase,"effects":a.effects});
            self.event("action", data);
        }
    }
    fn publish(&mut self) {
        self.state.ts_ms = unix_ms();
        self.state.seq += 1;
        self.paths.write_state(&self.state);
    }
}
thread_local! { static CURRENT: RefCell<Weak<Mutex<Publisher>>> = const { RefCell::new(Weak::new()) }; }

pub struct SignalSession {
    publisher: Arc<Mutex<Publisher>>,
    stop: mpsc::Sender<()>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl SignalSession {
    pub fn start(paths: SignalPaths, command: &str, app: Option<&str>) -> Self {
        Self::start_with_task(paths, command, app, None)
    }
    pub fn start_with_task(
        paths: SignalPaths,
        command: &str,
        app: Option<&str>,
        task: Option<String>,
    ) -> Self {
        let _trace = crate::trace::scope("session.start");
        let calls = paths.dir.join("calls");
        let maintenance = std::fs::create_dir_all(&calls).and_then(|()| {
            crate::maintenance::periodic_with_interval(&calls, unix_ms(), 1000, || {
                let _ = paths.read_calls();
                Ok(())
            })
        });
        if let Err(error) = maintenance {
            eprintln!("[actl-signal] INTERNAL: snapshot retention unavailable: {error}");
        }
        let mut state = SessionState::now(std::process::id(), command, app, None, Phase::Running);
        state.version = 2;
        state.call_id = format!(
            "{}-{}",
            std::process::id(),
            crate::snapshot::new_snapshot_id()
        );
        state.started_ms = state.ts_ms;
        let journal = if command == "history" {
            None
        } else {
            match crate::history::Journal::open(&paths.dir, &state.call_id, task.clone()) {
                Ok(journal) => Some(journal),
                Err(e) => {
                    eprintln!("[actl-history] INTERNAL: history unavailable: {e}");
                    None
                }
            }
        };
        let publisher = Arc::new(Mutex::new(Publisher {
            paths,
            state,
            journal,
            task,
        }));
        CURRENT.with(|c| *c.borrow_mut() = Arc::downgrade(&publisher));
        if let Ok(mut p) = publisher.lock() {
            p.event(
                "call_started",
                serde_json::json!({"command":command,"version":env!("CARGO_PKG_VERSION")}),
            );
            p.publish();
        }
        let (stop, rx) = mpsc::channel();
        let shared = publisher.clone();
        let worker = std::thread::Builder::new()
            .name("actl-signal-heartbeat".into())
            .spawn(move || {
                while rx.recv_timeout(Duration::from_millis(1000))
                    == Err(mpsc::RecvTimeoutError::Timeout)
                {
                    if let Ok(mut p) = shared.lock() {
                        p.publish();
                    }
                }
            })
            .ok();
        Self {
            publisher,
            stop,
            worker,
        }
    }

    pub fn finish(&mut self, phase: Phase, error: Option<&str>) {
        self.stop_worker();
        if let Ok(mut p) = self.publisher.lock() {
            p.state.phase = phase;
            p.state.finished_ms = Some(unix_ms());
            p.state.error_code = error.map(str::to_owned);
            let elapsed = unix_ms().saturating_sub(p.state.started_ms);
            let mut data =
                serde_json::json!({"phase":phase,"error_code":error,"duration_ms":elapsed});
            if let Some(flow) = &p.state.workflow {
                data["workflow_status"] = serde_json::json!(flow.status);
                data["workflow_completed"] = serde_json::json!(flow.status == "completed");
                data["workflow_reason"] = serde_json::json!(flow.reason);
                data["workflow_error_code"] = serde_json::json!(flow.error_code);
            }
            p.event("call_finished", data);
            p.publish();
        }
        CURRENT.with(|c| *c.borrow_mut() = Weak::new());
        if let Ok(p) = self.publisher.lock() {
            let _ = std::fs::remove_file(p.paths.dir.join(format!("ack-{}.json", p.state.call_id)));
        }
    }
    fn stop_worker(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for SignalSession {
    fn drop(&mut self) {
        self.stop_worker();
    }
}

pub fn step(command: &str, app: Option<&str>, index: usize, total: usize) {
    if let Some(shared) = CURRENT.with(|c| c.borrow().upgrade())
        && let Ok(mut p) = shared.lock()
    {
        p.state.command = command.into();
        p.state.app = app.map(str::to_owned);
        p.state.step = Some(format!("{index}/{total}"));
        p.state.action = None;
        p.event(
            "step",
            serde_json::json!({"command":command,"index":index,"total":total}),
        );
        p.publish();
    }
}

pub fn workflow(progress: crate::state::WorkflowProgress) {
    if let Some(shared) = CURRENT.with(|c| c.borrow().upgrade())
        && let Ok(mut p) = shared.lock()
    {
        p.state.workflow = Some(progress.clone());
        p.event(
            "workflow",
            serde_json::json!({"run_id":progress.run_id,"status":progress.status,
                "step_id":progress.step_id,"reason":progress.reason,"error_code":progress.error_code}),
        );
        p.state.workflow = Some(progress);
        p.publish();
    }
}

/// Content-free workflow diagnostics; not an authorization channel.
pub fn flow_event(event: &str, data: serde_json::Value) {
    if let Some(shared) = CURRENT.with(|c| c.borrow().upgrade())
        && let Ok(mut p) = shared.lock()
    {
        p.event(event, data);
    }
}

/// Guard owns exactly one resolved action. Nested helpers share their enclosing action.
pub struct ActionGuard {
    shared: Option<Arc<Mutex<Publisher>>>,
    id: u64,
    ended: bool,
}
impl ActionGuard {
    pub fn begin(kind: &str, effects: InputEffects, target: ActionTarget) -> Self {
        let shared = CURRENT.with(|c| c.borrow().upgrade());
        let mut id = 0;
        if let Some(shared) = &shared
            && let Ok(mut p) = shared.lock()
        {
            if p.state.action.as_ref().is_some_and(ActionState::active) {
                return Self {
                    shared: None,
                    id,
                    ended: false,
                };
            }
            id = p.state.seq + 1;
            let now = unix_ms();
            p.state.action = Some(ActionState {
                id,
                seq: id,
                kind: kind.into(),
                phase: ActionPhase::Preparing,
                effects,
                target,
                ts_ms: now,
                expires_ms: now + 6000,
            });
            p.action_event();
            p.publish();
        }
        Self {
            shared,
            id,
            ended: false,
        }
    }
    pub fn is_recording(&self) -> bool {
        self.shared.is_some()
    }
    pub fn identity(&self) -> Option<(String, u64, u64)> {
        let p = self.shared.as_ref()?.lock().ok()?;
        Some((
            p.state.call_id.clone(),
            self.id,
            p.state.action.as_ref()?.ts_ms,
        ))
    }
    pub fn executing(&mut self) {
        self.transition(ActionPhase::Executing);
    }
    pub fn delivered(&mut self) {
        self.transition(ActionPhase::Delivered);
        self.ended = true;
    }
    pub fn partial(&mut self) {
        self.transition(ActionPhase::Partial);
        self.ended = true;
    }
    fn transition(&mut self, phase: ActionPhase) {
        let Some(shared) = &self.shared else { return };
        let Ok(mut p) = shared.lock() else { return };
        let seq = p.state.seq + 1;
        let Some(action) = p.state.action.as_mut().filter(|a| a.id == self.id) else {
            return;
        };
        action.phase = phase;
        action.seq = seq;
        action.ts_ms = unix_ms();
        action.expires_ms = action.ts_ms + 2000;
        let copy = action.clone();
        if !copy.active() {
            p.state.recent_actions.push(copy);
            if p.state.recent_actions.len() > 32 {
                p.state.recent_actions.remove(0);
            }
        }
        p.action_event();
        p.publish();
    }
}
impl Drop for ActionGuard {
    fn drop(&mut self) {
        if !self.ended {
            let cancelled = self
                .shared
                .as_ref()
                .and_then(|s| s.lock().ok())
                .is_some_and(|p| p.paths.stop_requested());
            self.transition(if cancelled {
                ActionPhase::Cancelled
            } else {
                ActionPhase::Failed
            });
        }
    }
}

/// Identity shared by commands in ACTL_TASK_ID / one flow run.
pub fn handoff_identity() -> Option<(String, String)> {
    CURRENT.with(|c| {
        let p = c.borrow().upgrade()?;
        let p = p.lock().ok()?;
        Some((
            p.state.call_id.clone(),
            p.task.clone().unwrap_or_else(|| p.state.call_id.clone()),
        ))
    })
}

/// Content-free handoff transitions in the existing per-call journal.
pub fn handoff_event(reason: &str) {
    CURRENT.with(|c| {
        if let Some(shared) = c.borrow().upgrade()
            && let Ok(mut publisher) = shared.lock()
        {
            publisher.state.handoff_reason = (reason != "allowed").then(|| reason.to_owned());
            publisher.event("handoff", serde_json::json!({"reason":reason}));
            publisher.publish();
        }
    });
}
