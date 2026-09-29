//! Simple assertion branches; observation errors never mean false.
use crate::{
    Failure, Step,
    engine::{Host, Run, Status, StepRecord, StepStatus},
    store::Store,
};
use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    Run,
    Otherwise,
    Skip,
}
pub(crate) enum Selection {
    Execute,
    Skip,
    Halt,
}
pub fn validate(step: &Step) -> Result<(), CtlError> {
    if step.when.is_some() && step.unless.is_some() {
        return Err(CtlError::protocol("when and unless are mutually exclusive"));
    }
    if step.otherwise.is_some() && step.when.is_none() && step.unless.is_none() {
        return Err(CtlError::protocol("otherwise requires when or unless"));
    }
    if (step.when.is_some() || step.unless.is_some()) && !step.outputs.is_empty() {
        return Err(CtlError::protocol(
            "conditional steps cannot declare outputs; use a following read step",
        ));
    }
    Ok(())
}
pub fn check_saved(step: &Step, record: &StepRecord) -> Result<(), CtlError> {
    let in_flight = matches!(
        record.status,
        StepStatus::Running | StepStatus::Verifying | StepStatus::Uncertain
    );
    let conditional = step.when.is_some() || step.unless.is_some();
    if (record.branch.is_some() && !conditional)
        || (conditional
            && (in_flight || record.status == StepStatus::Completed)
            && record.branch.is_none())
        || (record.branch == Some(Branch::Otherwise) && step.otherwise.is_none())
        || (record.branch == Some(Branch::Skip)
            && (record.status != StepStatus::Completed
                || record.verified
                || step.otherwise.is_some()))
    {
        return Err(CtlError::protocol("inconsistent saved branch decision"));
    }
    Ok(())
}
pub(crate) fn restore(step: &mut Step, record: &StepRecord) -> Result<(), CtlError> {
    check_saved(step, record)?;
    if matches!(
        record.status,
        StepStatus::Running | StepStatus::Verifying | StepStatus::Uncertain
    ) && record.branch == Some(Branch::Otherwise)
    {
        step.run = step
            .otherwise
            .clone()
            .ok_or_else(|| CtlError::protocol("saved otherwise branch missing"))?;
    }
    Ok(())
}
pub(crate) fn choose(
    step: &mut Step,
    index: usize,
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
) -> Result<Selection, CtlError> {
    let Some(condition) = step.when.as_ref().or(step.unless.as_ref()) else {
        return Ok(Selection::Execute);
    };
    let phase = if step.unless.is_some() {
        "unless"
    } else {
        "when"
    };
    // Pending actions re-observe on resume. No side effect has been dispatched yet.
    run.steps[index].branch = None;
    host.observation_event(phase, 1, "started", None);
    let observed = host.condition(condition);
    host.observation_event(
        phase,
        1,
        match &observed {
            Ok(true) => "true",
            Ok(false) => "false",
            Err(_) => "unknown",
        },
        observed.as_ref().err().map(|e| e.code),
    );
    if host.stopped() || store.pause_requested() {
        run.status = if host.stopped() {
            Status::Stopped
        } else {
            Status::Paused
        };
        run.reason = Some(
            if host.stopped() {
                "stop_requested"
            } else {
                "pause_requested"
            }
            .into(),
        );
        run.save(store, host)?;
        return Ok(Selection::Halt);
    }
    let matched = match observed {
        Ok(value) => value,
        Err(error) => {
            run.steps[index].error_code = Some(error.code);
            run.steps[index].error_evidence = crate::failure_evidence(step.evidence, &error);
            run.status = if step.on_failure == Failure::Fail {
                Status::Failed
            } else {
                Status::Paused
            };
            run.reason = Some("condition_observation_failed".into());
            run.save(store, host)?;
            return Ok(Selection::Halt);
        }
    };
    let selected = if step.unless.is_some() {
        !matched
    } else {
        matched
    };
    let branch = if selected {
        Branch::Run
    } else if step.otherwise.is_some() {
        Branch::Otherwise
    } else {
        Branch::Skip
    };
    run.steps[index].branch = Some(branch);
    run.steps[index].error_code = None;
    run.steps[index].error_evidence = None;
    if branch == Branch::Skip {
        run.steps[index].status = StepStatus::Completed;
        run.steps[index].verified = false;
    }
    // The selected route is durable before any side effect can begin.
    run.save(store, host)?;
    match branch {
        Branch::Run => Ok(Selection::Execute),
        Branch::Otherwise => {
            step.run = step
                .otherwise
                .clone()
                .ok_or_else(|| CtlError::new(ErrorCode::Protocol, "otherwise missing"))?;
            Ok(Selection::Execute)
        }
        Branch::Skip => Ok(Selection::Skip),
    }
}
