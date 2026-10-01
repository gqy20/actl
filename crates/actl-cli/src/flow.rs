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

pub fn configure_task() -> Result<(), CtlError> {
    if let Some(path) = std::env::var_os("ACTL_TASK_CONFIG") {
        if actl_core::history::task_from_env()?.is_none() {
            return Err(CtlError::protocol(
                "ACTL_TASK_CONFIG requires a unique ACTL_TASK_ID",
            ));
        }
        let text = actl_flow::store::read_text(std::path::Path::new(&path))?;
        let config = actl_flow::task_config::parse(&text)?;
        actl_core::handoff::configure(config.handoff.approval);
    }
    Ok(())
}

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
            | Command::VerifyFile { .. }
            | Command::VerifyRows { .. }
            | Command::VerifyOwner { .. }
            | Command::VerifyClosed { .. }
            | Command::VerifyWindow { .. }
            | Command::VerifyClipboard { .. }
            | Command::Wait { .. }
            | Command::GetClipboard
    )
}

fn validate_assertion(command: &Command) -> Result<(), CtlError> {
    match command {
        Command::VerifyWindow { app, state } => actl_uia::window_check::validate(app, state)?,
        Command::VerifyOwner {
            program,
            token,
            hwnd,
            ..
        } => {
            if !std::path::Path::new(program).is_absolute()
                || !std::path::Path::new(token).is_absolute()
                || token.len() < 12
            {
                return Err(CtlError::protocol(
                    "owner verification requires absolute executable and task paths",
                ));
            }
            if let Some(hwnd) = hwnd {
                actl_uia::parse_hwnd(hwnd)?;
            }
        }
        Command::VerifyClosed { hwnd, pid, .. } => {
            actl_uia::parse_hwnd(hwnd)?;
            if *pid == 0 {
                return Err(CtlError::protocol("process ID must be positive"));
            }
        }
        Command::VerifyRows {
            expected, prefix, ..
        } => {
            crate::file_check::rows_expected(prefix, expected)?;
        }
        Command::VerifyFile { expected, path, .. } => {
            if !std::path::Path::new(path).is_absolute() {
                return Err(CtlError::protocol(
                    "persisted file checks require an absolute path",
                ));
            }
            crate::file_check::expectation(expected)?;
        }
        Command::Wait {
            element,
            window,
            gone,
            idle,
            property,
            ..
        } => {
            // --idle(桌面空闲门)是仅有的无目标等待形态:整批无人值守前置,
            // 只读且不定位任何元素/窗口,与三种目标等待互斥。
            if [
                element.is_some(),
                window.is_some(),
                gone.is_some(),
                idle.is_some(),
            ]
            .into_iter()
            .filter(|v| *v)
            .count()
                != 1
            {
                return Err(CtlError::protocol(
                    "flow wait requires exactly one of --element/--window/--gone/--idle",
                ));
            }
            if idle.is_some() {
                return Ok(());
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
            url_query,
            ..
        } => {
            if url_query.as_ref().is_some_and(String::is_empty) {
                return Err(CtlError::protocol("URL query key must not be empty"));
            }
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
        actl_flow::branch::validate(step)?;
        if let Some(otherwise) = &step.otherwise {
            let mut child = step.clone();
            child.run = otherwise.clone();
            child.otherwise = None;
            validate(&Plan {
                version: plan.version,
                name: plan.name.clone(),
                handoff: plan.handoff,
                auto_resume: None,
                steps: vec![child],
            })?;
        }
        if let Some(calls) = actl_flow::sequence::decode(&step.run)? {
            if step.verify.is_none() || !step.outputs.is_empty() || step.retry == Retry::ReadOnly {
                return Err(CtlError::protocol(
                    "sequence requires result verification, no outputs or read-only retry",
                ));
            }
            for call in calls {
                let mut child = step.clone();
                child.run = call;
                child.otherwise = None;
                validate(&Plan {
                    version: plan.version,
                    name: plan.name.clone(),
                    handoff: plan.handoff,
                    auto_resume: None,
                    steps: vec![child],
                })?;
            }
            continue;
        }
        let context = |field: &str, e: CtlError| {
            CtlError::with_evidence(
                e.code,
                format!("step {} / {field}: {}", step.id, e.message),
                json!({"step_id":step.id,"step_index":index,"field":format!("steps[{index}].{field}")}),
            )
        };
        for (field, call) in [
            ("run", Some(&step.run)),
            ("when", step.when.as_ref()),
            ("unless", step.unless.as_ref()),
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
            if field != "run"
                && !matches!(
                    command,
                    Command::Verify { .. }
                        | Command::Wait { .. }
                        | Command::VerifyFile { .. }
                        | Command::VerifyRows { .. }
                        | Command::VerifyOwner { .. }
                        | Command::VerifyClosed { .. }
                        | Command::VerifyWindow { .. }
                        | Command::VerifyClipboard { .. }
                )
            {
                return Err(context(
                    field,
                    CtlError::protocol("checks require verify or wait"),
                ));
            }
            if matches!(field, "when" | "unless") && matches!(command, Command::Wait { .. }) {
                return Err(context(
                    field,
                    CtlError::protocol("conditions require immediate assertions, not wait"),
                ));
            }
            if matches!(field, "when" | "unless")
                && let Command::Verify {
                    exists: Some(target),
                    app,
                    ..
                } = &command
                && (app.as_ref().is_none_or(|app| app.trim().is_empty())
                    || !matches!(actl_core::parse_target(target)?, actl_core::Target::Uia(_)))
            {
                return Err(context(
                    field,
                    CtlError::protocol(
                        "element existence conditions require explicit --app and a strict uia: selector",
                    ),
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
    fn recovery_event(&mut self, step_id: &str, event: &actl_flow::bounded::Event) {
        actl_core::signal_session::flow_event(
            "workflow_recovery",
            json!({"step_id":step_id,"observation":event}),
        );
    }
    fn capture_failure(&mut self, step_id: &str, phase: &str, call: Option<&Invocation>) {
        // 失败瞬间三路留证到 flow-resources/<id>/evidence/:归因依赖现场
        // (docs/68 实证:截图给「人看到的」,窗口清单给「原生实况」,
        // UIA 快照给「自动化看到的」——Zotero 空库树缺失、X:/.xlsx 对话框
        // 原文都靠后者定位)。诊断只吞错,绝不影响流程;文件名按 步骤-相位
        // 固定,同步重试覆盖旧文件,不无限堆积。
        let stem = format!("{}-{}", step_id.replace(['/', '\\', ':'], "_"), phase);
        let Some(root) = self
            .store
            .dir
            .parent()
            .and_then(|flows| flows.parent())
            .map(|state| state.join("flow-resources"))
        else {
            return;
        };
        let Some(id) = self.store.dir.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let dir = root.join(id).join("evidence");
        let shot = std::fs::create_dir_all(&dir)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                actl_uia::screenshot_screen(Some(
                    &dir.join(format!("{stem}.png")).to_string_lossy(),
                ))
                .map(|_| ())
                .map_err(|e| e.to_string())
            });
        if let Err(error) = shot {
            eprintln!("INTERNAL: failure screenshot unavailable for {step_id}/{phase}: {error}");
        }
        // 原生窗口清单:锁屏/隐藏壳/歧义窗口/对话框归属一眼可见。
        let windows = actl_uia::native_windows().map(|rows| {
            let items: Vec<Value> = rows
                .iter()
                .map(|w| {
                    json!({
                        "title": w.title, "class": w.class, "pid": w.pid,
                        "hwnd": w.hwnd, "visible": w.visible, "foreground": w.foreground,
                        "minimized": w.minimized, "topmost": w.topmost,
                    })
                })
                .collect();
            json!({"windows": items})
        });
        match windows {
            Ok(value) => {
                if let Err(error) = std::fs::write(
                    dir.join(format!("{stem}.windows.json")),
                    serde_json::to_vec(&value).unwrap_or_default(),
                ) {
                    eprintln!("INTERNAL: window inventory write failed: {error}");
                }
            }
            Err(error) => {
                eprintln!("INTERNAL: window inventory unavailable for {step_id}/{phase}: {error}");
            }
        }
        // UIA 快照:按失败调用的 --app 定向;窗口垂死/缺失时如实跳过
        // (截屏与窗口清单已兜底)。
        if let Some(app) = call.and_then(app_context) {
            let command =
                crate::cli::Cli::try_parse_from(["actl", "snapshot", "--app", app.as_str()])
                    .map_err(|e| e.to_string())
                    .and_then(|parsed| match parsed.command {
                        crate::cli::Command::Snapshot { .. } => Ok(parsed.command),
                        _ => Err("unexpected parse".to_string()),
                    });
            match command {
                Ok(command) => match crate::dispatch(&command) {
                    Ok(output) => {
                        if let Err(error) = std::fs::write(
                            dir.join(format!("{stem}.uia.json")),
                            serde_json::to_vec(&output.data).unwrap_or_default(),
                        ) {
                            eprintln!("INTERNAL: UIA snapshot write failed: {error}");
                        }
                    }
                    Err(error) => {
                        // 失败现场本来就可能是窗口垂死/缺失——如实记录错误码即可。
                        eprintln!(
                            "INTERNAL: UIA snapshot unavailable for {step_id}/{phase}: {error}"
                        );
                    }
                },
                Err(error) => {
                    eprintln!("INTERNAL: snapshot command build failed: {error}");
                }
            }
        }
    }
    fn condition(&mut self, call: &Invocation) -> Result<bool, CtlError> {
        let element_exists = matches!(
            parse(call)?,
            Command::Verify {
                exists: Some(_),
                ..
            }
        );
        crate::condition::outcome(self.execute(call), element_exists)
    }
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
            handoff: self.plan.handoff,
            auto_resume: None,
            name: self.plan.name.clone(),
            steps: vec![step.clone()],
        })
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
        if let Some(calls) = actl_flow::sequence::decode(call)? {
            let mut started = false;
            for (index, child) in calls.iter().enumerate() {
                actl_core::stop::check_current()?;
                if self.store.pause_requested() {
                    return Err(CtlError::with_evidence(
                        ErrorCode::NotActionable,
                        "sequence paused",
                        json!({"stage":"handoff","reason":"user_paused","action_started":started,"sequence_index":index}),
                    ));
                }
                let result = self.execute_output(child);
                actl_core::signal_session::flow_event(
                    "workflow_sequence",
                    json!({"index":index,"command":child.command,"ok":result.is_ok(),"error_code":result.as_ref().err().map(|error|error.code)}),
                );
                if let Err(error) = result {
                    if self.store.pause_requested() && !self.stopped() {
                        let uncertain = started
                            || (!self.read_only(child)
                                && error
                                    .evidence
                                    .as_ref()
                                    .is_none_or(|e| e["action_started"] != false));
                        return Err(CtlError::with_evidence(
                            ErrorCode::NotActionable,
                            "sequence paused",
                            json!({"stage":"handoff","reason":"user_paused","action_started":uncertain,"sequence_index":index}),
                        ));
                    }
                    return Err(actl_flow::sequence::failure(error, started, index));
                }
                started |= !self.read_only(child);
            }
            return Ok(json!({"executed":calls.len()}));
        }
        let handoff_policy = self.plan.handoff.unwrap_or_default();
        // 始终信任此流程:standing 声明 → 指纹跟随请求(持久信任匹配,
        // 模板变更自动失效);非 standing 不带指纹(不影响其他信任通道)。
        if handoff_policy.approval == actl_core::handoff::Approval::Standing {
            let hash = actl_core::handoff::plan_fingerprint(
                &serde_json::to_value(self.plan).unwrap_or(serde_json::Value::Null),
            );
            actl_uia::set_plan_hash(Some(hash));
        }
        actl_core::handoff::configure(handoff_policy.approval);
        let command = parse(call)?;
        // 批次2②:暂停观察点从 Wait 轮询扩展到全部命令——type 的分段间隙等
        // 注入边界也能在亚秒内响应暂停(序列子调用间隙本就观察)。
        let _scope = actl_core::wait_control::Scope::enter(self.store.dir.join("pause-requested"));
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
/// 模板文件字节指纹(FNV-1a 双通道,与 handoff::plan_fingerprint 同算法族):
/// flow-status 陈旧计划提示用——重读模板重算,与冻结时记录不一致即提示
/// "模板已改,重试仍跑旧路线"。防对冻结计划验证新修复(ROADMAP 实录废过两次)。
fn bytes_fingerprint(bytes: &[u8]) -> String {
    let (mut h1, mut h2) = (0xcbf2_9ce4_8422_2325u64, 0x9e37_79b9_7f4a_7c15u64);
    for &b in bytes {
        h1 = (h1 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        h2 = (h2 ^ u64::from(b).rotate_left(7)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h1:016x}{h2:016x}")
}
/// flow-run 时旁车记录源模板(路径+字节指纹);失败不阻断——提示性特征。
fn record_source(store: &Store, file: &str) {
    let Ok(template) = std::path::Path::new(file).canonicalize() else {
        return;
    };
    let Ok(bytes) = std::fs::read(&template) else {
        return;
    };
    let record = json!({
        "path": template.to_string_lossy(),
        "fingerprint": bytes_fingerprint(&bytes),
        "recorded_ms": actl_core::state::unix_ms(),
    });
    if let Ok(text) = serde_json::to_vec(&record) {
        let _ = std::fs::write(store.dir.join("source.json"), text);
    }
}
/// 陈旧计划提示:模板字节指纹与冻结记录比对。旧流程无 source.json 时返回
/// null(零噪音);模板不可读时 changed=null 并说明原因,不臆断。
fn template_hint(store: &Store) -> Value {
    let Some(record) = std::fs::read(store.dir.join("source.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    else {
        return Value::Null;
    };
    let path = record["path"].as_str().unwrap_or_default().to_owned();
    let frozen = record["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    match std::fs::read(&path) {
        Ok(bytes) => {
            let changed = frozen != bytes_fingerprint(&bytes);
            json!({
                "source_path": path,
                "changed": changed,
                "hint": if changed { "模板已改,重试仍跑冻结的旧路线;验证新修复需新任务" } else { "" },
            })
        }
        Err(_) => json!({"source_path": path, "changed": null, "hint": "模板不可读,无法比对"}),
    }
}
fn state(store: &Store) -> Result<Value, CtlError> {
    let (_, run) = store.load()?;
    let active = store.active()?;
    let interrupted = run.status == Status::Running && !active;
    Ok(
        json!({"run":run,"runner_active":active,"interrupted":interrupted,"pause_requested":store.pause_requested(),"template":template_hint(store)}),
    )
}
fn load_plan(
    file: &str,
    inputs: &Option<String>,
    root: &std::path::Path,
    id: &str,
) -> Result<(Plan, actl_flow::resources::Prepared), CtlError> {
    let text = actl_flow::store::read_text(std::path::Path::new(file))?;
    let prepared = actl_flow::resources::prepare(
        &text,
        std::path::Path::new(file),
        &root.join("flow-resources").join(id),
        id,
    )?;
    let values = inputs
        .as_ref()
        .map(|file| actl_flow::store::read_text(std::path::Path::new(file)))
        .transpose()?;
    Ok((
        actl_flow::inputs::resolve(&prepared.text, values.as_deref())?,
        prepared,
    ))
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
        FlowCommand::Validate {
            file,
            inputs,
            emit_plan,
        } => {
            let (plan, resources) = load_plan(file, inputs, &root, "validation-preview")?;
            validate(&plan)?;
            let mut result =
                json!({"valid":true,"version":1,"name":plan.name,"steps":plan.steps.len()});
            if resources.enabled() {
                result["resources"] = resources.summary();
            }
            if *emit_plan {
                result["plan"] =
                    serde_json::to_value(plan).map_err(|e| CtlError::internal(e.to_string()))?;
            }
            Ok(result)
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
        FlowCommand::Run {
            file,
            inputs,
            id,
            paused,
        } => {
            let (plan, resources) = load_plan(file, inputs, &root, id)?;
            validate(&plan)?;
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            if store.dir.exists() {
                return Err(CtlError::protocol(
                    "run ID already exists; use flow-resume or a new ID",
                ));
            }
            actl_core::stop::check_current()?;
            resources.create()?;
            // 无资源声明时 ${run.dir} 仍可能被 input default 引用(screenshot
            // 输出等)——确保 run.dir 存在,否则写文件报路径不存在。
            let run_dir = actl_core::state::SignalPaths::default()
                .dir
                .join("flow-resources")
                .join(id);
            if !run_dir.exists() {
                std::fs::create_dir_all(&run_dir)
                    .map_err(|e| CtlError::internal(format!("cannot create run dir: {e}")))?;
            }
            let mut run = store.create(&plan, id)?;
            record_source(&store, file);
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
        FlowCommand::Clean {
            older_than_minutes,
            dry_run,
        } => {
            let flows_dir = root.join("flows");
            let archive_dir = root.join("flows-archived");
            if !*dry_run {
                std::fs::create_dir_all(&archive_dir)
                    .map_err(|e| CtlError::internal(format!("cannot create archive dir: {e}")))?;
            }
            let cutoff_ms = actl_core::state::unix_ms()
                .saturating_sub(older_than_minutes.saturating_mul(60_000));
            let mut moved = vec![];
            let mut kept = vec![];
            let mut errors = vec![];
            if let Ok(entries) = std::fs::read_dir(&flows_dir) {
                for entry in entries.flatten() {
                    let id = entry.file_name().to_string_lossy().into_owned();
                    let state_path = entry.path().join("state.json");
                    let Ok(text) = std::fs::read_to_string(&state_path) else {
                        continue; // 非 flow 目录或损坏状态,不动
                    };
                    let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    let status = state["status"].as_str().unwrap_or("");
                    if status == "completed" {
                        continue; // 已完成不动(保留在 flows/ 里作保留窗口内的证据)
                    }
                    let updated = state["updated_ms"].as_u64().unwrap_or(0);
                    if updated >= cutoff_ms {
                        kept.push(json!({"id":id,"status":status,"reason":"recent"}));
                        continue;
                    }
                    // active runner 在跑的不动:running.lock 存 PID,进程死了视为过期锁
                    if let Ok(lock) = std::fs::read_to_string(entry.path().join("running.lock"))
                        && let Some(pid) = lock.trim().parse::<u32>().ok()
                        && actl_uia::evidence::process(pid, false)
                            .ok()
                            .and_then(|p| p["state"].as_str().map(str::to_owned))
                            .is_some_and(|s| s != "absent")
                    {
                        kept.push(json!({"id":id,"status":status,"reason":"runner_active"}));
                        continue;
                    }
                    if *dry_run {
                        moved.push(json!({"id":id,"status":status,"would_archive":true}));
                        continue;
                    }
                    let dest = archive_dir.join(&id);
                    match std::fs::rename(entry.path(), &dest) {
                        Ok(()) => moved.push(json!({"id":id,"status":status,"archived":true})),
                        Err(e) => errors.push(json!({"id":id,"error":e.to_string()})),
                    }
                }
            }
            Ok(json!({
                "archived": moved.len(),
                "kept": kept.len(),
                "errors": errors,
                "detail": moved,
                "dry_run": dry_run,
                "older_than_minutes": older_than_minutes,
            }))
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

/// 从失败调用里提取 --app 窗口上下文,供失败瞬间的定向 UIA 快照。
/// 覆盖两种形态:顶层参数("--app", 值)与序列 JSON 内嵌("\"--app\",\"值\"")。
fn app_context(call: &Invocation) -> Option<String> {
    for (index, arg) in call.args.iter().enumerate() {
        if arg == "--app" {
            return call.args.get(index + 1).cloned().filter(|v| !v.is_empty());
        }
        if arg.contains("\"--app\"") {
            // 序列参数是子调用的 JSON 串;取 "--app" 后的下一个 JSON 字符串值。
            let tail = arg.split("\"--app\"").nth(1)?;
            let value = tail.trim_start().trim_start_matches(',').trim_start();
            let value = value.strip_prefix('"')?;
            let end = value.find(['"', '\\'])?;
            let text = &value[..end];
            if !text.is_empty() && !text.starts_with("--") {
                return Some(text.replace("\\\\", "\\"));
            }
        }
    }
    None
}

#[cfg(test)]
mod failure_evidence_tests {
    use super::*;

    fn call(args: &[&str]) -> Invocation {
        Invocation {
            command: "workflow-sequence".into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            bind: Default::default(),
        }
    }

    #[test]
    fn app_context_reads_top_level_and_embedded_sequence_forms() {
        assert_eq!(
            app_context(&call(&["--app", "我的文库 - Zotero"])),
            Some("我的文库 - Zotero".into())
        );
        let embedded =
            call(&["{\"args\":[\"--app\",\"apps-verify1-excel-table\"],\"command\":\"wait\"}"]);
        assert_eq!(
            app_context(&embedded),
            Some("apps-verify1-excel-table".into())
        );
        assert_eq!(app_context(&call(&["--value", "x"])), None);
        // 内嵌值本身是旗标形态(--app 后紧跟另一个旗)时不误取。
        let flag_next = call(&["{\"args\":[\"--app\",\"--timeout\"],\"command\":\"wait\"}"]);
        assert_eq!(app_context(&flag_next), None);
    }
}
