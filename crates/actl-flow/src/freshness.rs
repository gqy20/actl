//! Wall-clock freshness for explicitly bounded business outputs.
use crate::{
    Failure, Plan, Step,
    engine::{Host, Run, Status},
    store::Store,
};
use actl_core::{CtlError, ErrorCode};
use serde_json::json;

pub fn check(plan: &Plan, step: &Step, run: &Run, now_ms: u64) -> Result<(), CtlError> {
    for call in std::iter::once(&step.run)
        .chain(step.when.iter())
        .chain(step.unless.iter())
        .chain(step.otherwise.iter())
        .chain(step.before.iter())
        .chain(step.verify.iter())
        .chain(step.resume_check.iter())
    {
        for reference in call.bind.values() {
            let Some((producer, name)) = reference.split_once('.') else {
                continue;
            };
            let Some(spec) = plan
                .steps
                .iter()
                .find(|s| s.id == producer)
                .and_then(|s| s.outputs.get(name))
            else {
                continue;
            };
            let Some(max_age_ms) = spec.max_age_ms else {
                continue;
            };
            let captured = run
                .steps
                .iter()
                .find(|s| s.id == producer)
                .and_then(|s| s.outputs_captured_ms);
            let age = captured.and_then(|t| now_ms.checked_sub(t));
            let reason = match age {
                Some(age) if age <= max_age_ms => continue,
                Some(_) => "expired",
                None if captured.is_some() => "clock_moved_backwards",
                None => "capture_time_missing",
            };
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                "bound output freshness cannot be established; review and prepare a new plan",
                json!({"stage":"output_freshness", "reason":reason, "source_step":producer,
                    "output":name, "max_age_ms":max_age_ms, "captured_ms":captured,
                    "checked_ms":now_ms, "age_ms":age}),
            ));
        }
    }
    Ok(())
}

pub(crate) fn guard(
    plan: &Plan,
    step: &Step,
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
) -> Result<bool, CtlError> {
    if let Err(error) = check(plan, step, run, host.now_ms()) {
        let record = run
            .steps
            .iter_mut()
            .find(|s| s.id == step.id)
            .ok_or_else(|| CtlError::protocol("step missing from flow state"))?;
        record.error_code = Some(error.code);
        // These diagnostics contain identifiers and times, never business values.
        record.error_evidence = error.evidence;
        run.status = if step.on_failure == Failure::Fail {
            Status::Failed
        } else {
            Status::Paused
        };
        run.reason = Some("output_freshness_failed".into());
        run.save(store, host)?;
        return Ok(false);
    }
    Ok(true)
}
