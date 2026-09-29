//! Persisted budgets for automatic read-only retries; explicit resume gets one fresh check.
use crate::{
    Invocation, Plan, Step,
    engine::{Host, Run, StepRecord},
    store::Store,
};
use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub max_retries: u32,
    pub timeout_ms: u64,
    pub interval_ms: u64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    pub retries: u32,
    pub started_ms: Option<u64>,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub phase: String,
    pub outcome: String,
    pub at_ms: u64,
    pub retries: u32,
    pub elapsed_ms: u64,
    pub remaining_ms: u64,
    pub error_code: Option<ErrorCode>,
}
pub fn validate(step: &Step) -> Result<(), CtlError> {
    if let Some(p) = &step.recovery {
        if !(1..=10).contains(&p.max_retries)
            || !(1..=60000).contains(&p.timeout_ms)
            || p.interval_ms == 0
            || p.interval_ms > p.timeout_ms
        {
            return Err(CtlError::protocol(
                "recovery requires max_retries 1..10, timeout_ms 1..60000 and interval_ms 1..timeout_ms",
            ));
        }
        if step.retry != crate::Retry::Never {
            return Err(CtlError::protocol("recovery cannot be combined with retry"));
        }
    }
    Ok(())
}
pub fn check_saved(step: &Step, record: &StepRecord) -> Result<(), CtlError> {
    if let Some(r) = &record.recovery {
        let Some(p) = &step.recovery else {
            return Err(CtlError::protocol("saved recovery has no policy"));
        };
        if r.retries > p.max_retries
            || (r.retries > 0 && r.started_ms.is_none())
            || r.events.len() > 64
        {
            return Err(CtlError::protocol("invalid saved recovery budget"));
        }
    }
    Ok(())
}
fn retryable(error: &CtlError, call: &Invocation) -> bool {
    if call.args.iter().any(|v| v.starts_with('@')) {
        return false;
    }
    if error
        .evidence
        .as_ref()
        .is_some_and(|e| e["truncated"] == true || e["stage"] == "handoff")
    {
        return false;
    }
    // Internal 与 StaleRef/Timeout/NotFound 同属观察通道瞬态(docs/65):
    // 冷启动窗口/垂死窗口上的 UIA COM 查询会抖出 INTERNAL,动作实际已生效。
    // 有界重试(默认 3 次/15 秒)内不吞真错误——超限仍如实失败。
    matches!(
        error.code,
        ErrorCode::StaleRef | ErrorCode::Timeout | ErrorCode::NotFound | ErrorCode::Internal
    )
}
fn elapsed(progress: &Progress, now: u64) -> u64 {
    progress
        .started_ms
        .map_or(0, |start| now.checked_sub(start).unwrap_or(u64::MAX))
}
fn record(
    run: &mut Run,
    index: usize,
    policy: &Policy,
    phase: &str,
    outcome: &str,
    error: Option<ErrorCode>,
    host: &mut impl Host,
) {
    let now = host.now_ms();
    let p = run.steps[index]
        .recovery
        .get_or_insert_with(Progress::default);
    let elapsed_ms = elapsed(p, now);
    let event = Event {
        phase: phase.into(),
        outcome: outcome.into(),
        at_ms: now,
        retries: p.retries,
        elapsed_ms,
        remaining_ms: policy.timeout_ms.saturating_sub(elapsed_ms),
        error_code: error,
    };
    if p.events.len() == 64 {
        p.events.remove(0);
    }
    p.events.push(event.clone());
    host.recovery_event(&run.steps[index].id, &event);
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
    let p = step
        .recovery
        .as_ref()
        .ok_or_else(|| CtlError::protocol("missing recovery policy"))?;
    if !host.read_only(call) {
        return Err(CtlError::protocol(
            "recovery only observes read-only commands",
        ));
    }
    loop {
        if host.stopped() || store.pause_requested() {
            return Err(CtlError::new(ErrorCode::Aborted, "observation interrupted"));
        }
        crate::freshness::check(plan, &plan.steps[index], run, host.now_ms())?;
        record(run, index, p, phase, "observing", None, host);
        run.save(store, host)?;
        let result = match phase {
            "when" | "unless" => host.condition(call).map(Value::Bool),
            "run" => host.execute_output(call),
            _ => host.execute(call).map(|()| Value::Null),
        };
        record(
            run,
            index,
            p,
            phase,
            if result.is_ok() { "observed" } else { "failed" },
            result.as_ref().err().map(|e| e.code),
            host,
        );
        run.save(store, host)?;
        let error = match result {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if host.stopped() || store.pause_requested() {
            return Err(CtlError::new(ErrorCode::Aborted, "observation interrupted"));
        }
        if !retryable(&error, call) {
            record(
                run,
                index,
                p,
                phase,
                "requires_review",
                Some(error.code),
                host,
            );
            run.save(store, host)?;
            return Err(error);
        }
        let progress = run.steps[index]
            .recovery
            .get_or_insert_with(Progress::default);
        progress.started_ms.get_or_insert(host.now_ms());
        if progress.retries >= p.max_retries
            || elapsed(progress, host.now_ms()).saturating_add(p.interval_ms) >= p.timeout_ms
        {
            record(
                run,
                index,
                p,
                phase,
                "budget_exhausted",
                Some(error.code),
                host,
            );
            run.save(store, host)?;
            return Err(error);
        }
        // Reserve before sleeping/dispatch so a crash cannot refund an automatic retry.
        progress.retries += 1;
        record(
            run,
            index,
            p,
            phase,
            "retry_reserved",
            Some(error.code),
            host,
        );
        run.save(store, host)?;
        let until = std::time::Instant::now() + std::time::Duration::from_millis(p.interval_ms);
        while std::time::Instant::now() < until {
            if host.stopped() || store.pause_requested() {
                return Err(CtlError::new(
                    ErrorCode::Aborted,
                    "observation retry interrupted",
                ));
            }
            host.sleep_ms(
                25.min(
                    until
                        .saturating_duration_since(std::time::Instant::now())
                        .as_millis() as u64,
                )
                .max(1),
            );
        }
        let progress = run.steps[index]
            .recovery
            .as_ref()
            .ok_or_else(|| CtlError::internal("recovery progress missing"))?;
        if elapsed(progress, host.now_ms()) >= p.timeout_ms {
            record(
                run,
                index,
                p,
                phase,
                "budget_exhausted",
                Some(error.code),
                host,
            );
            run.save(store, host)?;
            return Err(error);
        }
        if phase == "run" {
            run.steps[index].attempts = run.steps[index]
                .attempts
                .checked_add(1)
                .ok_or_else(|| CtlError::protocol("attempt count overflow"))?;
        }
    }
}

pub fn diagnostic(record: &StepRecord) -> Value {
    let last = record.recovery.as_ref().and_then(|p| p.events.last());
    json!({"step_id":record.id,"branch":record.branch,"error_code":record.error_code,
        "retries":record.recovery.as_ref().map_or(0,|p|p.retries),"last_observation":last})
}
