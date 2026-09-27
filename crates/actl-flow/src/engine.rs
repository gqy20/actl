use crate::{Failure, Invocation, Plan, Retry, store::Store};
use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ready,
    Running,
    Paused,
    NeedsReview,
    Stopped,
    Completed,
    Failed,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Verifying,
    Completed,
    Uncertain,
    Failed,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepRecord {
    pub id: String,
    pub status: StepStatus,
    pub attempts: u32,
    pub checkpoint_passed: bool,
    pub verified: bool,
    pub error_code: Option<ErrorCode>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub version: u32,
    pub id: String,
    pub status: Status,
    pub updated_ms: u64,
    pub steps: Vec<StepRecord>,
    pub reason: Option<String>,
}
impl Run {
    pub fn new(id: &str, plan: &Plan) -> Self {
        Self {
            version: 1,
            id: id.into(),
            status: Status::Ready,
            updated_ms: actl_core::state::unix_ms(),
            reason: None,
            steps: plan
                .steps
                .iter()
                .map(|s| StepRecord {
                    id: s.id.clone(),
                    status: StepStatus::Pending,
                    attempts: 0,
                    checkpoint_passed: false,
                    verified: false,
                    error_code: None,
                })
                .collect(),
        }
    }
    fn save(&mut self, store: &Store, host: &mut impl Host) -> Result<(), CtlError> {
        self.updated_ms = actl_core::state::unix_ms();
        store.save(self)?;
        host.publish(self);
        Ok(())
    }
}
pub trait Host {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError>;
    fn read_only(&self, call: &Invocation) -> bool;
    fn stopped(&self) -> bool;
    fn publish(&mut self, run: &Run);
}
fn boundary(run: &mut Run, store: &Store, host: &mut impl Host) -> Result<bool, CtlError> {
    if host.stopped() {
        run.status = Status::Stopped;
        run.reason = Some("stop_requested".into());
    } else if store.pause_requested() {
        run.status = Status::Paused;
        run.reason = Some("pause_requested".into());
    } else {
        return Ok(false);
    }
    run.save(store, host)?;
    Ok(true)
}
/// Host must hold both execution locks; resumption never changes the saved plan.
pub fn execute(
    plan: &Plan,
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
    retry_step: Option<&str>,
) -> Result<(), CtlError> {
    if run.status == Status::Completed {
        return Ok(());
    }
    if plan
        .steps
        .iter()
        .any(|s| s.retry == Retry::ReadOnly && !host.read_only(&s.run))
    {
        return Err(CtlError::protocol(
            "automatic retry is only allowed for read-only commands",
        ));
    }
    if let Some(id) = retry_step
        && !run.steps.iter().any(|s| {
            s.id == id
                && matches!(
                    s.status,
                    StepStatus::Uncertain | StepStatus::Running | StepStatus::Verifying
                )
        })
    {
        return Err(CtlError::protocol(
            "--retry-step must name an uncertain step",
        ));
    }
    run.status = Status::Running;
    run.reason = None;
    run.save(store, host)?;
    for (i, step) in plan.steps.iter().enumerate() {
        if run.steps[i].status == StepStatus::Completed {
            continue;
        }
        if boundary(run, store, host)? {
            return Ok(());
        }
        if matches!(
            run.steps[i].status,
            StepStatus::Uncertain | StepStatus::Running | StepStatus::Verifying
        ) {
            // An interrupted write may have succeeded. Re-read its postcondition before any replay.
            if let Some(verify) = &step.verify {
                run.steps[i].status = StepStatus::Verifying;
                run.save(store, host)?;
                if host.execute(verify).is_ok() {
                    run.steps[i].status = StepStatus::Completed;
                    run.steps[i].verified = true;
                    run.steps[i].error_code = None;
                    run.save(store, host)?;
                    continue;
                }
            }
            if boundary(run, store, host)? {
                return Ok(());
            }
            if !host.read_only(&step.run) && retry_step != Some(step.id.as_str()) {
                run.steps[i].status = StepStatus::Uncertain;
                run.status = Status::NeedsReview;
                run.reason =
                    Some("result_unconfirmed; inspect before authorizing --retry-step".into());
                run.save(store, host)?;
                return Ok(());
            }
        }
        if step.pause_before && !run.steps[i].checkpoint_passed {
            run.steps[i].checkpoint_passed = true;
            run.status = Status::Paused;
            run.reason = Some("checkpoint".into());
            run.save(store, host)?;
            return Ok(());
        }
        if let Some(before) = &step.before
            && let Err(error) = host.execute(before)
        {
            if error.code == ErrorCode::Aborted
                && !host.stopped()
                && store.pause_requested()
                && boundary(run, store, host)?
            {
                return Ok(());
            }
            run.steps[i].error_code = Some(error.code);
            if boundary(run, store, host)? {
                return Ok(());
            }
            run.status = Status::Paused;
            run.reason = Some("precondition_failed".into());
            run.save(store, host)?;
            return Ok(());
        }
        if boundary(run, store, host)? {
            return Ok(());
        }
        let attempts = if step.retry == Retry::ReadOnly { 3 } else { 1 };
        for attempt in 0..attempts {
            run.steps[i].status = StepStatus::Running;
            run.steps[i].attempts = run.steps[i]
                .attempts
                .checked_add(1)
                .ok_or_else(|| CtlError::protocol("step attempt count overflow"))?;
            run.steps[i].error_code = None;
            // Write-ahead record is required: failure to persist prevents the side effect.
            run.save(store, host)?;
            let result = host.execute(&step.run);
            if let Err(error) = result {
                if error.code == ErrorCode::Aborted
                    && host.read_only(&step.run)
                    && (store.pause_requested() || host.stopped())
                {
                    run.steps[i].status = StepStatus::Pending;
                    run.steps[i].error_code = None;
                    if boundary(run, store, host)? {
                        return Ok(());
                    }
                }
                run.steps[i].error_code = Some(error.code);
                run.steps[i].status = if host.read_only(&step.run) {
                    StepStatus::Failed
                } else {
                    StepStatus::Uncertain
                };
                run.save(store, host)?;
                if boundary(run, store, host)? {
                    return Ok(());
                }
                if attempt + 1 < attempts {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if boundary(run, store, host)? {
                        return Ok(());
                    }
                    continue;
                }
                run.status = if step.on_failure == Failure::Fail {
                    Status::Failed
                } else {
                    Status::Paused
                };
                run.reason = Some("action_failed".into());
                run.save(store, host)?;
                return Ok(());
            }
            // Successful run without a postcondition is a completed read, not a verified task.
            if step.verify.is_none() {
                run.steps[i].status = StepStatus::Completed;
                run.save(store, host)?;
                break;
            }
            run.steps[i].status = StepStatus::Verifying;
            run.save(store, host)?;
            if boundary(run, store, host)? {
                return Ok(());
            }
            if let Some(verify) = &step.verify {
                match host.execute(verify) {
                    Ok(()) => {
                        run.steps[i].status = StepStatus::Completed;
                        run.steps[i].verified = true;
                    }
                    Err(error) => {
                        if error.code == ErrorCode::Aborted
                            && !host.stopped()
                            && store.pause_requested()
                            && boundary(run, store, host)?
                        {
                            return Ok(());
                        }
                        run.steps[i].error_code = Some(error.code);
                        run.steps[i].status = StepStatus::Uncertain;
                        if boundary(run, store, host)? {
                            return Ok(());
                        }
                        run.status = if step.on_failure == Failure::Fail {
                            Status::Failed
                        } else {
                            Status::NeedsReview
                        };
                        run.reason = Some("verification_failed".into());
                        run.save(store, host)?;
                        return Ok(());
                    }
                }
            }
            run.save(store, host)?;
            break;
        }
    }
    run.status = Status::Completed;
    run.reason = None;
    run.save(store, host)
}
