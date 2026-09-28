//! Bounded fresh semantic queries. Never retries a write or infers human activity.
use crate::{
    Invocation, Plan, Retry, Step,
    engine::{Host, Run},
    store::Store,
};
use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRetry {
    pub phase: String,
    pub error_code: ErrorCode,
    pub retry_number: u32,
}

pub(crate) fn observe(
    context: (&Plan, &Step, usize),
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
    phase: &str,
    call: &Invocation,
) -> Result<Value, CtlError> {
    let (plan, step, index) = context;
    let limit = if step.retry == Retry::StaleRead && host.read_only(call) {
        3
    } else {
        1
    };
    for attempt in 0..limit {
        if host.stopped() || store.pause_requested() {
            return Err(CtlError::new(ErrorCode::Aborted, "observation interrupted"));
        }
        // A retry must not consume business values that expired while waiting.
        if attempt > 0 {
            crate::freshness::check(plan, &plan.steps[index], run, host.now_ms())?;
        }
        host.observation_event(phase, attempt + 1, "started", None);
        let result = if phase == "run" {
            host.execute_output(call)
        } else {
            host.execute(call).map(|()| Value::Null)
        };
        host.observation_event(
            phase,
            attempt + 1,
            if result.is_ok() {
                "succeeded"
            } else {
                "failed"
            },
            result.as_ref().err().map(|e| e.code),
        );
        match result {
            Err(error)
                if error.code == ErrorCode::StaleRef
                    && attempt + 1 < limit
                    && run.steps[index].observation_retries.len() < 32 =>
            {
                run.steps[index].observation_retries.push(ObservationRetry {
                    phase: phase.into(),
                    error_code: error.code,
                    retry_number: attempt + 1,
                });
                run.save(store, host)?;
                std::thread::sleep(std::time::Duration::from_millis(200));
                if host.stopped() || store.pause_requested() {
                    return Err(CtlError::new(
                        ErrorCode::Aborted,
                        "observation retry interrupted",
                    ));
                }
                crate::freshness::check(plan, &plan.steps[index], run, host.now_ms())?;
                if phase == "run" {
                    run.steps[index].attempts = run.steps[index]
                        .attempts
                        .checked_add(1)
                        .ok_or_else(|| CtlError::protocol("step attempt count overflow"))?;
                    run.save(store, host)?;
                }
            }
            other => return other,
        }
    }
    Err(CtlError::internal(
        "observation retry exhausted without a result",
    ))
}
