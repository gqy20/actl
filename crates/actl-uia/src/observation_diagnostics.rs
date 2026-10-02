//! Bounded phase diagnostics, including evidence before a blocking provider call.
//! Correlation is not proof that input caused a failure, and never grants permission.
use actl_core::{
    CtlError, signal_session,
    state::{SignalPaths, unix_ms},
};
use serde::Serialize;
use serde_json::json;
use std::{cell::RefCell, time::Instant};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
};

const START_LIMIT: u64 = 32;
const LATE_LIMIT: u64 = 8;
const SLOW_MS: u64 = 250;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Stage {
    WindowInventory,
    AutomationInitialize,
    DesktopRoot,
    Children,
    WindowHandle,
    WindowRole,
    WindowProperties,
}

#[derive(Default)]
struct Budget {
    call: String,
    span: u64,
    late: u64,
}
impl Budget {
    fn next(&mut self, call: &str) -> (u64, bool) {
        if self.call != call {
            *self = Self {
                call: call.into(),
                ..Default::default()
            };
        }
        self.span += 1;
        (self.span, self.span <= START_LIMIT)
    }
    fn retain_late(&mut self, duration_ms: u64, failed: bool) -> bool {
        if (failed || duration_ms >= SLOW_MS) && self.late < LATE_LIMIT {
            self.late += 1;
            true
        } else {
            false
        }
    }
}
thread_local! { static BUDGET: RefCell<Budget> = RefCell::new(Budget::default()); }

#[derive(Serialize)]
struct Endpoint {
    ts_ms: u64,
    foreground_hwnd: Option<isize>,
    foreground_pid: Option<u32>,
    last_input_tick: Option<u32>,
    input: Option<crate::observation_metadata::SampleRead>,
}
fn endpoint(paths: &SignalPaths, sample: bool) -> Endpoint {
    let ts_ms = unix_ms();
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: native metadata reads only; no provider calls or input injection.
    let (foreground_hwnd, foreground_pid, last_input_tick) = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (
            (!hwnd.0.is_null()).then_some(hwnd.0 as isize),
            (thread != 0 && pid != 0).then_some(pid),
            GetLastInputInfo(&mut info).as_bool().then_some(info.dwTime),
        )
    };
    Endpoint {
        ts_ms,
        foreground_hwnd,
        foreground_pid,
        last_input_tick,
        input: sample.then(|| crate::observation_metadata::read(paths, ts_ms)),
    }
}

pub(crate) fn observe<T>(
    stage: Stage,
    target_hwnd: Option<isize>,
    operation: impl FnOnce() -> Result<T, CtlError>,
) -> Result<T, CtlError> {
    let Some((paths, call)) = signal_session::diagnostic_context() else {
        return operation();
    };
    let (span, retained) = BUDGET.with(|b| b.borrow_mut().next(&call));
    if span == START_LIMIT + 1 {
        signal_session::flow_event(
            "observation_budget",
            json!({
                "started_limit":START_LIMIT,"slow_failure_limit":LATE_LIMIT,"slow_ms":SLOW_MS
            }),
        );
    }
    let before = endpoint(&paths, retained);
    let target_pid = target_hwnd.and_then(|hwnd| {
        let mut pid = 0;
        // SAFETY: resolve an already known handle without probing UIA or activating it.
        let thread = unsafe {
            GetWindowThreadProcessId(
                windows::Win32::Foundation::HWND(hwnd as *mut _),
                Some(&mut pid),
            )
        };
        (thread != 0 && pid != 0).then_some(pid)
    });
    if retained {
        signal_session::flow_event(
            "observation_started",
            json!({
                "span_id":span,"stage":stage,"target_hwnd":target_hwnd,"target_pid":target_pid,
                "before":before
            }),
        );
    }
    // Exclude diagnostic sampling and journal I/O from the provider phase duration.
    let started = Instant::now();
    let mut result = operation();
    let duration_ms = started.elapsed().as_millis() as u64;
    let log_finish =
        retained || BUDGET.with(|b| b.borrow_mut().retain_late(duration_ms, result.is_err()));
    if log_finish {
        let after = endpoint(&paths, true);
        let error = result.as_ref().err();
        signal_session::flow_event(
            "observation_finished",
            json!({
                "span_id":span,"stage":stage,"target_hwnd":target_hwnd,"target_pid":target_pid,
                "started_recorded":retained,"duration_ms":duration_ms,
                "outcome":if error.is_some() { "failed" } else { "succeeded" },
                "error_code":error.map(|e| e.code),
            "native_code":error.and_then(|e| e.evidence.as_ref()).and_then(|e| e["native_code"].as_i64()),
            "sampled_input_delta":crate::observation_metadata::delta(before.input.as_ref(), after.input.as_ref()),
                "session_input_changed":before.last_input_tick.zip(after.last_input_tick).map(|(a,b)| a != b),
                "foreground_changed":before.foreground_hwnd.zip(after.foreground_hwnd).map(|(a,b)| a != b),
                "before":before,"after":after
            }),
        );
    }
    if let Err(error) = &mut result {
        let evidence = error.evidence.get_or_insert_with(|| json!({}));
        if let Some(map) = evidence.as_object_mut() {
            // Keep the innermost failing phase when an outer observation propagates it.
            map.entry("observation_stage")
                .or_insert_with(|| json!(stage));
            map.entry("observation_span_id")
                .or_insert_with(|| json!(span));
            map.entry("observation_duration_ms")
                .or_insert_with(|| json!(duration_ms));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budgets_reset_per_call_and_reserve_late_failures() {
        let mut budget = Budget::default();
        for _ in 0..START_LIMIT {
            assert!(budget.next("a").1);
        }
        assert!(!budget.next("a").1);
        assert!(!budget.retain_late(0, false));
        assert!(budget.retain_late(SLOW_MS, false));
        for _ in 1..LATE_LIMIT {
            assert!(budget.retain_late(0, true));
        }
        assert!(!budget.retain_late(0, true));
        assert_eq!(budget.next("b"), (1, true));
        assert!(budget.retain_late(0, true));
    }
    #[test]
    fn timeout_phase_and_control_request_share_a_content_free_timeline() {
        use actl_core::{
            ErrorCode,
            history::{Journal, read_timeline},
            signal_session::SignalSession,
            state::Phase,
        };
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let mut session =
            SignalSession::start_with_task(paths.clone(), "flow-resume", None, Some("run".into()));
        signal_session::workflow(
            serde_json::from_value(json!({
                "run_id":"run","step_id":"windows","title":"PRIVATE TITLE","status":"running"
            }))
            .unwrap(),
        );
        let error = observe(Stage::WindowInventory, None, || {
            observe(Stage::Children, None, || {
                Err::<(), _>(crate::read_channel::failure(
                    "children",
                    uiautomation::errors::Error::new(-2146233083, "PRIVATE ERROR"),
                ))
            })
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert_eq!(
            error.evidence.as_ref().unwrap()["observation_stage"],
            "children"
        );
        session.finish(Phase::Error, Some("TIMEOUT"));
        let mut control = Journal::open(&paths.dir, "control", Some("run".into())).unwrap();
        control.record("call_started", json!({"command":"flow-pause"}));
        control.record(
            "control_request",
            json!({"action":"pause","stage":"accepted","request_id":"request"}),
        );
        control.record("call_finished", json!({}));
        let result = read_timeline(&paths.dir, Some("run"), 500).unwrap();
        let events = result["events"].as_array().unwrap();
        let phases: Vec<_> = events
            .iter()
            .filter(|e| e["event"] == "observation_finished")
            .collect();
        assert_eq!(phases.len(), 2);
        let first = &phases[0];
        assert_eq!(first["step_id"], "windows");
        assert_eq!(first["data"]["stage"], "children");
        assert_eq!(first["data"]["native_code"], -2146233083i64);
        assert_eq!(first["data"]["before"]["input"]["status"], "missing");
        assert!(first["data"]["duration_ms"].is_u64());
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/observation_failure.json")).unwrap();
        let actual = json!({
            "stage":first["data"]["stage"],
            "outcome":first["data"]["outcome"],
            "error_code":first["data"]["error_code"],
            "native_code":first["data"]["native_code"],
            "started_recorded":first["data"]["started_recorded"],
            "run_id":first["run_id"],"step_id":first["step_id"],
            "input_status":first["data"]["before"]["input"]["status"]
        });
        assert_eq!(actual, expected);
        assert!(events.iter().any(|e| e["event"] == "control_request"));
        assert_eq!(result["truncated"], false);
        assert!(!result.to_string().contains("PRIVATE"));
        drop(session);
        drop(control);
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
}
