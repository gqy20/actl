//! CLI adapter: preflight every invocation, then execute through the existing dispatcher.
use crate::cli::{Cli, Command, FlowCommand};
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
            | Command::ListWindows
            | Command::Snapshot { .. }
            | Command::Find { .. }
            | Command::Get { .. }
            | Command::Extract { .. }
            | Command::Verify { .. }
            | Command::Wait { .. }
            | Command::GetClipboard
    )
}
pub fn validate(plan: &Plan) -> Result<(), CtlError> {
    for step in &plan.steps {
        let command = parse(&step.run)?;
        if matches!(
            command,
            Command::Batch { .. }
                | Command::Flow(_)
                | Command::Desktop(_)
                | Command::Commands
                | Command::KeyDown { .. }
                | Command::KeyUp { .. }
        ) {
            return Err(CtlError::protocol(
                "flows cannot contain batch, flow, management or held-key commands",
            ));
        }
        if !read_only(&command) && (step.verify.is_none() || step.retry != Retry::Never) {
            return Err(CtlError::protocol(
                "side-effect steps require verify and retry: never",
            ));
        }
        for call in std::iter::once(&step.run)
            .chain(step.before.iter())
            .chain(step.verify.iter())
        {
            // Persisted ordinal refs cannot survive process/desktop changes reliably.
            if call.args.iter().any(|a| {
                a.strip_prefix("@e")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
                    || a == "--snapshot"
            }) {
                return Err(CtlError::protocol(
                    "saved flows require semantic selectors; transient refs/snapshot IDs are unsupported",
                ));
            }
            parse(call)?;
        }
        for check in step.before.iter().chain(step.verify.iter()) {
            if !matches!(parse(check)?, Command::Verify { .. } | Command::Wait { .. }) {
                return Err(CtlError::protocol(
                    "before/verify must use verify or wait assertions",
                ));
            }
        }
    }
    Ok(())
}
struct Adapter<'a> {
    plan: &'a Plan,
}
impl Host for Adapter<'_> {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        let command = parse(call)?;
        crate::dispatch(&command).map(|_| ())
    }
    fn read_only(&self, call: &Invocation) -> bool {
        parse(call).is_ok_and(|c| read_only(&c))
    }
    fn stopped(&self) -> bool {
        actl_core::state::SignalPaths::default().stop_requested()
    }
    fn publish(&mut self, run: &Run) {
        let current = run
            .steps
            .iter()
            .position(|s| s.status != engine::StepStatus::Completed)
            .unwrap_or(run.steps.len().saturating_sub(1));
        actl_core::signal_session::step(
            &self.plan.steps[current].run.command,
            None,
            current + 1,
            run.steps.len(),
        );
        let status = match run.status {
            Status::Ready => "ready",
            Status::Running => "running",
            Status::Paused => "paused",
            Status::NeedsReview => "needs_review",
            Status::Stopped => "stopped",
            Status::Completed => "completed",
            Status::Failed => "failed",
        };
        actl_core::signal_session::workflow(&run.id, &self.plan.steps[current].title, status);
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
    let root = actl_core::state::SignalPaths::default().dir;
    match command {
        FlowCommand::Validate { file } => {
            let plan = actl_flow::parse(&actl_flow::store::read_text(std::path::Path::new(file))?)?;
            validate(&plan)?;
            Ok(json!({"valid":true,"version":1,"name":plan.name,"steps":plan.steps.len()}))
        }
        FlowCommand::Status { id } => state(&Store::at(&root, id)?),
        FlowCommand::Pause { id } => {
            let store = Store::at(&root, id)?;
            store.pause()?;
            state(&store)
        }
        FlowCommand::Run { file, id, paused } => {
            let plan = actl_flow::parse(&actl_flow::store::read_text(std::path::Path::new(file))?)?;
            validate(&plan)?;
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            let mut run = store.create(&plan, id)?;
            let active = store.lock(false)?;
            let mut host = Adapter { plan: &plan };
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
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            let (plan, mut run) = store.load()?;
            // Revalidate saved schema and commands before trusting disk state.
            let plan = actl_flow::parse(
                &serde_json::to_string(&plan).map_err(|e| CtlError::internal(e.to_string()))?,
            )?;
            validate(&plan)?;
            let active = store.lock(false)?;
            if actl_core::state::SignalPaths::default().stop_requested() {
                return Err(CtlError::new(
                    ErrorCode::Aborted,
                    "emergency stop remains set",
                ));
            }
            store.clear_pause()?;
            engine::execute(
                &plan,
                &mut run,
                &store,
                &mut Adapter { plan: &plan },
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
