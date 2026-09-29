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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_evidence: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub outputs: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs_captured_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observation_retries: Vec<crate::recovery::ObservationRetry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<crate::branch::Branch>,
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
                    error_evidence: None,
                    outputs: Default::default(),
                    outputs_captured_ms: None,
                    observation_retries: vec![],
                    branch: None,
                })
                .collect(),
        }
    }
    pub(crate) fn save(&mut self, store: &Store, host: &mut impl Host) -> Result<(), CtlError> {
        self.updated_ms = actl_core::state::unix_ms();
        store.save(self)?;
        host.publish(self);
        Ok(())
    }
}
pub trait Host {
    fn condition(&mut self, call: &Invocation) -> Result<bool, CtlError> {
        match self.execute(call) {
            Ok(()) => Ok(true),
            Err(error) if error.code == ErrorCode::AssertionFailed => Ok(false),
            Err(error) => Err(error),
        }
    }
    fn observation_event(
        &mut self,
        _phase: &str,
        _attempt: u32,
        _outcome: &str,
        _error: Option<ErrorCode>,
    ) {
    }
    fn now_ms(&self) -> u64 {
        actl_core::state::unix_ms()
    }
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError>;
    fn execute_output(&mut self, call: &Invocation) -> Result<serde_json::Value, CtlError> {
        self.execute(call).map(|()| serde_json::Value::Null)
    }
    fn validate_step(&self, _step: &crate::Step) -> Result<(), CtlError> {
        Ok(())
    }
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
    if run.reason.as_deref() == Some("pause_requested") {
        actl_core::signal_session::flow_event(
            "flow_pause_observed",
            serde_json::json!({"run_id":run.id,"request_id":store.pause_request_id()}),
        );
    }
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
    crate::data::validate(plan)?;
    crate::data::check_saved(plan, run)?;
    if plan.steps.iter().any(|s| {
        s.before
            .iter()
            .chain(s.verify.iter())
            .chain(s.resume_check.iter())
            .chain(s.when.iter())
            .chain(s.unless.iter())
            .any(|c| !host.read_only(c))
    }) {
        return Err(CtlError::protocol("flow checks must be read-only"));
    }
    if plan
        .steps
        .iter()
        .any(|s| !s.outputs.is_empty() && !host.read_only(&s.run))
    {
        return Err(CtlError::protocol("outputs require a read-only producer"));
    }
    if run.status == Status::Completed {
        host.publish(run);
        return Ok(());
    }
    if plan.steps.iter().any(|s| {
        s.retry == Retry::ReadOnly
            && std::iter::once(&s.run)
                .chain(s.otherwise.iter())
                .any(|call| !host.read_only(call))
    }) {
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
    let resuming = run.status != Status::Ready;
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
        if !crate::freshness::guard(plan, step, run, store, host)? {
            return Ok(());
        }
        let resolved = crate::data::resolve(step, run).and_then(|s| {
            host.validate_step(&s)?;
            Ok(s)
        });
        let mut step = match resolved {
            Ok(step) => step,
            Err(error) => {
                run.steps[i].error_code = Some(error.code);
                run.steps[i].error_evidence = crate::failure_evidence(step.evidence, &error);
                run.status = if step.on_failure == Failure::Fail {
                    Status::Failed
                } else {
                    Status::Paused
                };
                run.reason = Some("binding_validation_failed".into());
                run.save(store, host)?;
                return Ok(());
            }
        };
        crate::branch::restore(&mut step, &run.steps[i])?;
        let reuse_scene_check = resuming
            && run.steps[i].status == StepStatus::Pending
            && step.resume_check.is_some()
            && step.resume_check == step.before;
        if resuming
            && let Some(check) = &step.resume_check
            && let Err(error) =
                crate::recovery::observe((plan, &step, i), run, store, host, "resume_check", check)
        {
            run.steps[i].error_code = Some(error.code);
            run.steps[i].error_evidence = crate::failure_evidence(step.evidence, &error);
            if boundary(run, store, host)? {
                return Ok(());
            }
            run.status = Status::Paused;
            if step.on_failure == Failure::Fail {
                run.status = Status::Failed;
            }
            run.reason = Some("resume_check_failed".into());
            run.save(store, host)?;
            return Ok(());
        }
        if boundary(run, store, host)? {
            return Ok(());
        }
        if matches!(
            run.steps[i].status,
            StepStatus::Uncertain | StepStatus::Running | StepStatus::Verifying
        ) {
            // An interrupted write may have succeeded. Re-read its postcondition before any replay.
            let mut replay_confirmed = false;
            if let Some(verify) = &step.verify {
                run.steps[i].status = StepStatus::Verifying;
                run.save(store, host)?;
                match crate::recovery::observe((plan, &step, i), run, store, host, "verify", verify)
                {
                    Ok(_) if step.outputs.len() == run.steps[i].outputs.len() => {
                        run.steps[i].status = StepStatus::Completed;
                        run.steps[i].verified = true;
                        run.steps[i].error_code = None;
                        run.steps[i].error_evidence = None;
                        run.save(store, host)?;
                        continue;
                    }
                    Ok(_) => {} // A read completed but its output was not durably captured; re-read.
                    Err(error) => {
                        replay_confirmed = error.code == ErrorCode::AssertionFailed;
                        run.steps[i].error_evidence =
                            crate::failure_evidence(step.evidence, &error);
                        run.steps[i].error_code = Some(error.code);
                    }
                }
            }
            if boundary(run, store, host)? {
                return Ok(());
            }
            if !host.read_only(&step.run)
                && (retry_step != Some(step.id.as_str()) || !replay_confirmed)
            {
                run.steps[i].status = StepStatus::Uncertain;
                run.status = Status::NeedsReview;
                run.reason = Some(
                    if replay_confirmed {
                        "result_unconfirmed; inspect before authorizing --retry-step"
                    } else {
                        "result_observation_failed"
                    }
                    .into(),
                );
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
        if !reuse_scene_check
            && let Some(before) = &step.before
            && let Err(error) =
                crate::recovery::observe((plan, &step, i), run, store, host, "before", before)
        {
            if error.code == ErrorCode::Aborted
                && !host.stopped()
                && store.pause_requested()
                && boundary(run, store, host)?
            {
                return Ok(());
            }
            run.steps[i].error_evidence = crate::failure_evidence(step.evidence, &error);
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
        if !matches!(
            run.steps[i].status,
            StepStatus::Running | StepStatus::Verifying | StepStatus::Uncertain
        ) {
            match crate::branch::choose(&mut step, i, run, store, host)? {
                crate::branch::Selection::Skip => continue,
                crate::branch::Selection::Halt => return Ok(()),
                crate::branch::Selection::Execute => {}
            }
        }
        if boundary(run, store, host)? {
            return Ok(());
        }
        for attempt in 0..attempts {
            // Preconditions and retries may take time; check again before dispatch.
            if !crate::freshness::guard(plan, &plan.steps[i], run, store, host)? {
                return Ok(());
            }
            run.steps[i].status = StepStatus::Running;
            run.steps[i].attempts = run.steps[i]
                .attempts
                .checked_add(1)
                .ok_or_else(|| CtlError::protocol("step attempt count overflow"))?;
            run.steps[i].error_code = None;
            run.steps[i].error_evidence = None;
            run.steps[i].outputs.clear();
            run.steps[i].outputs_captured_ms = None;
            // Write-ahead record is required: failure to persist prevents the side effect.
            run.save(store, host)?;
            let captured_ms = host.now_ms();
            let result =
                crate::recovery::observe((plan, &step, i), run, store, host, "run", &step.run)
                    .and_then(|value| {
                        let outputs = crate::data::extract(&step, &value)?;
                        run.steps[i].outputs = outputs;
                        run.steps[i].outputs_captured_ms =
                            (!step.outputs.is_empty()).then_some(captured_ms);
                        if let Err(error) = crate::data::check_saved(plan, run) {
                            run.steps[i].outputs.clear();
                            run.steps[i].outputs_captured_ms = None;
                            return Err(error);
                        }
                        Ok(())
                    });
            if let Err(error) = result {
                if error.code == ErrorCode::Aborted
                    && host.read_only(&step.run)
                    && (store.pause_requested() || host.stopped())
                {
                    run.steps[i].status = StepStatus::Pending;
                    run.steps[i].error_code = None;
                    run.steps[i].error_evidence = None;
                    if boundary(run, store, host)? {
                        return Ok(());
                    }
                }
                if let Some(evidence) = error.evidence.as_ref()
                    && evidence["stage"] == "handoff"
                {
                    run.steps[i].error_evidence = crate::failure_evidence(step.evidence, &error);
                    run.steps[i].error_code = Some(error.code);
                    run.steps[i].status = if evidence["action_started"] == false {
                        StepStatus::Pending
                    } else {
                        StepStatus::Uncertain
                    };
                    run.status = Status::Paused;
                    run.reason = evidence["reason"].as_str().map(str::to_owned);
                    run.save(store, host)?;
                    return Ok(());
                }
                run.steps[i].error_evidence = crate::failure_evidence(step.evidence, &error);
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
                match crate::recovery::observe((plan, &step, i), run, store, host, "verify", verify)
                {
                    Ok(_) => {
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
                        run.steps[i].error_evidence =
                            crate::failure_evidence(step.evidence, &error);
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
