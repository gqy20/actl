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
}
impl Publisher {
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
        let _ = paths.read_calls();
        let mut state = SessionState::now(std::process::id(), command, app, None, Phase::Running);
        state.version = 2;
        state.call_id = format!(
            "{}-{}",
            std::process::id(),
            crate::snapshot::new_snapshot_id()
        );
        state.started_ms = state.ts_ms;
        let publisher = Arc::new(Mutex::new(Publisher { paths, state }));
        CURRENT.with(|c| *c.borrow_mut() = Arc::downgrade(&publisher));
        if let Ok(mut p) = publisher.lock() {
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
    if let Some(shared) = CURRENT.with(|c| c.borrow().upgrade()) {
        if let Ok(mut p) = shared.lock() {
            p.state.command = command.into();
            p.state.app = app.map(str::to_owned);
            p.state.step = Some(format!("{index}/{total}"));
            p.state.action = None;
            p.publish();
        }
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
        if let Some(shared) = &shared {
            if let Ok(mut p) = shared.lock() {
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
                p.publish();
            }
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
