//! CLI adapter: preflight every invocation, then execute through the existing dispatcher.
use crate::cli::{Cli, Command, DesktopCommand, FlowCommand};
use actl_core::{CtlError, ErrorCode};
use actl_flow::{
    Invocation, Plan, Retry,
    engine::{self, Host, Run, Status},
    store::Store,
};
use clap::Parser;
use serde_json::{Value, json};

fn parse(call: &Invocation) -> Result<Command, CtlError> {
    Cli::try_parse_from(
        std::iter::once("actl")
            .chain(std::iter::once(call.command.as_str()))
            .chain(call.args.iter().map(String::as_str)),
    )
    .map(|c| c.command)
    .map_err(|e| CtlError::protocol(format!("invalid flow command: {e}")))
}
fn read_only(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Status
            | Command::ListWindows { .. }
            | Command::Snapshot { .. }
            | Command::Find { .. }
            | Command::Get { .. }
            | Command::Extract { .. }
            | Command::Verify { .. }
            | Command::Wait { .. }
            | Command::GetClipboard
    )
}

fn validate_assertion(command: &Command) -> Result<(), CtlError> {
    match command {
        Command::Wait {
            element,
            window,
            gone,
            property,
            ..
        } => {
            if [element.is_some(), window.is_some(), gone.is_some()]
                .into_iter()
                .filter(|v| *v)
                .count()
                != 1
            {
                return Err(CtlError::protocol(
                    "flow wait requires exactly one of --element/--window/--gone",
                ));
            }
            for target in element.iter().chain(gone.iter()) {
                actl_core::parse_target(target)?;
            }
            if let Some(predicate) = property {
                actl_uia::WaitPredicate::parse(predicate)?;
            }
        }
        Command::Verify {
            exists,
            value,
            checked,
            ..
        } => {
            if [exists.is_some(), value.is_some(), checked.is_some()]
                .into_iter()
                .filter(|v| *v)
                .count()
                != 1
            {
                return Err(CtlError::protocol(
                    "flow verify requires exactly one assertion",
                ));
            }
            for target in exists.iter().chain(value.iter()).chain(checked.iter()) {
                actl_core::parse_target(target)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub fn validate(plan: &Plan) -> Result<(), CtlError> {
    for (index, step) in plan.steps.iter().enumerate() {
        let context = |field: &str, e: CtlError| {
            CtlError::with_evidence(
                e.code,
                format!("step {} / {field}: {}", step.id, e.message),
                json!({"step_id":step.id,"step_index":index,"field":format!("steps[{index}].{field}")}),
            )
        };
        for (field, call) in [
            ("run", Some(&step.run)),
            ("before", step.before.as_ref()),
            ("verify", step.verify.as_ref()),
            ("resume_check", step.resume_check.as_ref()),
        ] {
            let Some(call) = call else {
                continue;
            };
            let command = parse(call).map_err(|e| context(&format!("{field}.args"), e))?;
            if field == "run" && !step.outputs.is_empty() && !read_only(&command) {
                return Err(context(
                    "outputs",
                    CtlError::protocol("outputs require a read-only producer"),
                ));
            }
            crate::selector_input::validate(&command)
                .map_err(|e| context(&format!("{field}.args"), e))?;
            validate_assertion(&command).map_err(|e| context(&format!("{field}.args"), e))?;
            if field != "run" && !matches!(command, Command::Verify { .. } | Command::Wait { .. }) {
                return Err(context(
                    field,
                    CtlError::protocol("checks require verify or wait"),
                ));
            }
            if matches!(
                command,
                Command::Batch { .. }
                    | Command::Preview { .. }
                    | Command::Desktop(
                        DesktopCommand::ExecutionStatus | DesktopCommand::ExecutionRecover
                    )
                    | Command::Flow(_)
                    | Command::Desktop(_)
                    | Command::Commands
                    | Command::KeyDown { .. }
                    | Command::KeyUp { .. }
            ) {
                return Err(context(
                    field,
                    CtlError::protocol(
                        "nested flows and management/held-key commands are not allowed",
                    ),
                ));
            }
            if field == "run"
                && !read_only(&command)
                && (step.verify.is_none() || step.retry == Retry::ReadOnly)
            {
                return Err(context(
                    "verify",
                    CtlError::protocol(
                        "side effects require verify; retry=read_only cannot replay writes",
                    ),
                ));
            }
            if call.args.iter().any(|a| {
                a == "--snapshot"
                    || a.strip_prefix("@e")
                        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
            }) {
                return Err(context(
                    &format!("{field}.args"),
                    CtlError::protocol("persisted flows require semantic selectors"),
                ));
            }
        }
    }
    Ok(())
}
struct Adapter<'a> {
    plan: &'a Plan,
    store: &'a Store,
}
impl Host for Adapter<'_> {
    fn observation_event(
        &mut self,
        phase: &str,
        attempt: u32,
        outcome: &str,
        error: Option<ErrorCode>,
    ) {
        actl_core::signal_session::flow_event(
            "workflow_invocation",
            json!({"phase":phase,"attempt":attempt,"outcome":outcome,"error_code":error}),
        );
    }
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.execute_output(call).map(|_| ())
    }
    fn validate_step(&self, step: &actl_flow::Step) -> Result<(), CtlError> {
        validate(&Plan {
            version: 1,
            name: self.plan.name.clone(),
            steps: vec![step.clone()],
        })
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
        let command = parse(call)?;
        let _scope = matches!(command, Command::Wait { .. })
            .then(|| actl_core::wait_control::Scope::enter(self.store.dir.join("pause-requested")));
        crate::dispatch(&command).map(|output| output.data)
    }
    fn read_only(&self, call: &Invocation) -> bool {
        parse(call).is_ok_and(|c| read_only(&c))
    }
    fn stopped(&self) -> bool {
        actl_core::stop::check_current().is_err()
    }
    fn publish(&mut self, run: &Run) {
        let current = run
            .steps
            .iter()
            .position(|s| s.status != engine::StepStatus::Completed)
            .unwrap_or(run.steps.len().saturating_sub(1));
        let status = match run.status {
            Status::Ready => "ready",
            Status::Running => "running",
            Status::Paused => "paused",
            Status::NeedsReview => "needs_review",
            Status::Stopped => "stopped",
            Status::Completed => "completed",
            Status::Failed => "failed",
        };
        let actions = match run.status {
            Status::Running => vec!["pause", "stop"],
            Status::Paused | Status::Ready | Status::Failed => vec!["resume"],
            Status::NeedsReview => vec!["recheck"],
            Status::Stopped => vec!["allow_then_resume"],
            Status::Completed => vec![],
        };
        actl_core::signal_session::workflow(actl_core::state::WorkflowProgress {
            run_id: run.id.clone(),
            title: self.plan.steps[current].title.clone(),
            status: status.into(),
            reason: run.reason.clone(),
            step_id: Some(run.steps[current].id.clone()),
            error_code: run.steps[current].error_code,
            actions: actions.into_iter().map(str::to_owned).collect(),
        });
        actl_core::signal_session::step(
            &self.plan.steps[current].run.command,
            None,
            current + 1,
            run.steps.len(),
        );
    }
}
fn state(store: &Store) -> Result<Value, CtlError> {
    let (_, run) = store.load()?;
    let active = store.active()?;
    let interrupted = run.status == Status::Running && !active;
    Ok(
        json!({"run":run,"runner_active":active,"interrupted":interrupted,"pause_requested":store.pause_requested()}),
    )
}
pub fn dispatch(command: &FlowCommand) -> Result<Value, CtlError> {
    if matches!(
        command,
        FlowCommand::Run { .. } | FlowCommand::Resume { .. }
    ) {
        actl_core::stop::check_current()?;
    }
    let root = actl_core::state::SignalPaths::default().dir;
    match command {
        FlowCommand::Validate { file } => {
            let plan = actl_flow::parse(&actl_flow::store::read_text(std::path::Path::new(file))?)?;
            validate(&plan)?;
            Ok(json!({"valid":true,"version":1,"name":plan.name,"steps":plan.steps.len()}))
        }
        FlowCommand::Status { id } => state(&Store::at(&root, id)?),
        FlowCommand::Pause { id } => {
            let request = actl_core::control_log::Request::new(id, "pause");
            request.record("requested", None);
            let result = (|| {
                let store = Store::at(&root, id)?;
                store.pause_with_request(&request.request_id)?;
                state(&store)
            })();
            request.record(
                if result.is_ok() { "accepted" } else { "failed" },
                result.as_ref().err().map(|e: &CtlError| e.code),
            );
            result
        }
        FlowCommand::Run { file, id, paused } => {
            let plan = actl_flow::parse(&actl_flow::store::read_text(std::path::Path::new(file))?)?;
            validate(&plan)?;
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            let mut run = store.create(&plan, id)?;
            let active = store.lock(false)?;
            let mut host = Adapter {
                plan: &plan,
                store: &store,
            };
            if *paused {
                run.status = Status::Paused;
                run.reason = Some("start_paused".into());
                store.save(&run)?;
                host.publish(&run);
            } else {
                engine::execute(&plan, &mut run, &store, &mut host, None)?;
            }
            drop(active);
            outcome(&store, &run)
        }
        FlowCommand::Resume { id, retry_step } => {
            let source = actl_core::control_log::source();
            actl_core::signal_session::flow_event(
                "flow_resume_requested",
                json!({"run_id":id,"request_id":actl_core::control_log::request_id(),"source":source,"source_verified":false,"retry_step":retry_step}),
            );
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            let (plan, mut run) = store.load()?;
            // Revalidate saved schema and commands before trusting disk state.
            let plan = actl_flow::parse(
                &serde_json::to_string(&plan).map_err(|e| CtlError::internal(e.to_string()))?,
            )?;
            validate(&plan)?;
            let active = store.lock(false)?;
            actl_core::stop::check_current()?;
            store.clear_pause()?;
            engine::execute(
                &plan,
                &mut run,
                &store,
                &mut Adapter {
                    plan: &plan,
                    store: &store,
                },
                retry_step.as_deref(),
            )?;
            drop(active);
            outcome(&store, &run)
        }
    }
}
fn outcome(store: &Store, run: &Run) -> Result<Value, CtlError> {
    let value = state(store)?;
    if matches!(run.status, Status::Failed | Status::Stopped) {
        let code = if run.status == Status::Stopped {
            ErrorCode::Aborted
        } else {
            run.steps
                .iter()
                .find_map(|s| s.error_code)
                .unwrap_or(ErrorCode::Internal)
        };
        Err(CtlError::with_evidence(code, "workflow halted", value))
    } else {
        Ok(value)
    }
}
