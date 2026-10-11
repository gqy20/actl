//! CLI adapter: preflight every invocation, then execute through the existing dispatcher.
mod validate;

pub use validate::validate;

use crate::cli::{Cli, Command, FlowCommand};
use actl_core::{CtlError, ErrorCode};
use actl_flow::{
    Invocation, Plan,
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
            | Command::BrowserList
            | Command::BrowserRead { .. }
            | Command::BrowserVerify { .. }
    )
}

pub(super) fn validate_assertion(command: &Command) -> Result<(), CtlError> {
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
        Command::VerifyFile {
            expected,
            expected_file,
            path,
            ..
        } => {
            if !std::path::Path::new(path).is_absolute() {
                return Err(CtlError::protocol(
                    "persisted file checks require an absolute path",
                ));
            }
            let text = match (expected, expected_file) {
                (Some(_), Some(_)) => {
                    return Err(CtlError::protocol(
                        "--expected and --expected-file are mutually exclusive",
                    ));
                }
                (Some(text), None) => text.clone(),
                (None, Some(file)) => std::fs::read_to_string(file)
                    .map_err(|e| {
                        CtlError::protocol(format!("cannot read --expected-file {file:?}: {e}"))
                    })?
                    .trim_end_matches(['\r', '\n'])
                    .to_string(),
                (None, None) => {
                    return Err(CtlError::protocol(
                        "verify-file requires --expected or --expected-file",
                    ));
                }
            };
            crate::file_check::expectation(&text)?;
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
            // 弹层快照(运行时治理第一轮):失败瞬间该应用的前台引导层全文
            // 与分类——弹窗滞留类事故的一手证据,front-report 只读不加锁。
            if let Ok(front) = actl_uia::front_report(&app)
                && let Err(error) = std::fs::write(
                    dir.join(format!("{stem}.front.json")),
                    serde_json::to_vec(&front).unwrap_or_default(),
                )
            {
                eprintln!("INTERNAL: front layer write failed: {error}");
            }
        }
        // 采集清单(bundle manifest):本相位实际归集了哪些工件——flow-evidence
        // 的索引依据;缺失工件如实不列,不伪造。
        let mut artifacts = Vec::new();
        for (kind, extension) in [
            ("screenshot", "png"),
            ("windows", "windows.json"),
            ("uia", "uia.json"),
            ("front", "front.json"),
        ] {
            let path = dir.join(format!("{stem}.{extension}"));
            if let Ok(meta) = std::fs::metadata(&path) {
                artifacts.push(json!({
                    "kind": kind,
                    "file": path.file_name().and_then(|n| n.to_str()),
                    "bytes": meta.len(),
                }));
            }
        }
        let manifest = json!({
            "step_id": step_id,
            "phase": phase,
            "captured_ms": actl_core::state::unix_ms(),
            "artifacts": artifacts,
        });
        if let Err(error) = std::fs::write(
            dir.join(format!("{stem}.bundle.json")),
            serde_json::to_vec(&manifest).unwrap_or_default(),
        ) {
            eprintln!("INTERNAL: bundle manifest write failed: {error}");
        }
    }
    fn front_layer(&mut self, app: &str) -> Result<engine::FrontLayer, CtlError> {
        // 观察失败(窗口垂死/锁屏等)原样上抛——引擎按"阳性检测"放行。
        let report = actl_uia::front_report(app)?;
        let mut surfaces = Vec::new();
        for node in report["in_tree"].as_array().unwrap_or(&Vec::new()) {
            surfaces.push(engine::Surface {
                classification: node["classification"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                name: node["name"].as_str().unwrap_or_default().to_string(),
                text: node["text"].as_str().unwrap_or_default().to_string(),
                buttons: node["buttons"]
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter_map(|b| b.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
                hwnd: None,
            });
        }
        // owned 原生弹窗无文本可分类:一律 unknown,宁可上抛(门禁语义)。
        for node in report["owned"].as_array().unwrap_or(&Vec::new()) {
            surfaces.push(engine::Surface {
                classification: "unknown".into(),
                name: node["title"].as_str().unwrap_or_default().to_string(),
                text: String::new(),
                buttons: Vec::new(),
                hwnd: node["hwnd"].as_i64().map(|h| h as isize),
            });
        }
        Ok(engine::FrontLayer {
            clear: report["front_clear"].as_bool().unwrap_or(true),
            surfaces,
        })
    }
    fn popup_event(&mut self, step_id: &str, event: Value) {
        actl_core::signal_session::flow_event(
            "workflow_popup",
            json!({"step_id": step_id, "handled": event}),
        );
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
    /// 快照通道:捕获 envelope 顶层的 snapshot_id(帧内 ref 的身份锚点,
    /// 探索引擎 ref 绑定动作用)。快照为只读,跳过 handoff 配置;保留
    /// 暂停观察 scope 与既有 parse/护栏路径。
    fn execute_snapshot(&mut self, args: Vec<String>) -> Result<(String, Value), CtlError> {
        let call = Invocation {
            command: "snapshot".into(),
            args,
            bind: Default::default(),
        };
        let command = parse(&call)?;
        let _scope = actl_core::wait_control::Scope::enter(self.store.dir.join("pause-requested"));
        let output = crate::dispatch(&command)?;
        let snapshot_id = output.snapshot_id.clone().unwrap_or_default();
        Ok((snapshot_id, output.data))
    }
    fn validate_step(&self, step: &actl_flow::Step) -> Result<(), CtlError> {
        validate(&Plan {
            version: 1,
            handoff: self.plan.handoff,
            auto_resume: None,
            explore: None,
            name: self.plan.name.clone(),
            popup_handlers: Vec::new(),
            popup_ignores: Vec::new(),
            steps: vec![step.clone()],
        })
    }
    fn execute_output(&mut self, call: &Invocation) -> Result<Value, CtlError> {
        if let Some(each) = actl_flow::for_each::decode(call)? {
            // 列表来自前置步骤保存输出;超 max = PROTOCOL(不静默截断)
            let items = actl_flow::data::for_each_list(
                &self.store.load()?.1,
                &each.spec.from,
                each.spec.max,
            )?;
            for (item_index, item) in items.iter().enumerate() {
                actl_core::watchdog::reset();
                let text = actl_flow::for_each::item_text(item, each.spec.field.as_deref())?;
                for (child_index, child) in each.body.iter().enumerate() {
                    actl_core::stop::check_current()?;
                    if self.store.pause_requested() {
                        return Err(CtlError::with_evidence(
                            ErrorCode::NotActionable,
                            "for_each paused",
                            json!({
                                "stage":"handoff","reason":"user_paused","action_started":false,
                                "for_each_item":item_index,"for_each_child":child_index
                            }),
                        ));
                    }
                    let injected = actl_flow::data::for_each_inject(child, &text)?;
                    let result = self.execute_output(&injected);
                    actl_core::signal_session::flow_event(
                        "workflow_for_each",
                        json!({
                            "item": item_index,
                            "items": items.len(),
                            "index": child_index,
                            "command": child.command,
                            "ok": result.is_ok(),
                            "error_code": result.as_ref().err().map(|error| error.code),
                        }),
                    );
                    if let Err(error) = result {
                        // 项内失败:位置随错误携带(迭代粒度幂等——已完成项
                        // 不重放,--retry-step 自失败步整体续跑由引擎既有
                        // 语义接管;此处位置证据支撑人工判读)
                        let started = !self.read_only(child);
                        return Err(actl_flow::for_each::failure(
                            error,
                            item_index,
                            child_index,
                            started,
                        ));
                    }
                }
            }
            return Ok(json!({"executed": items.len()}));
        }
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
        // 探索运行 steps 恒空:发布任务一句话与探索状态,不做步骤索引。
        let (title, step_id, command, error_code) = match self.plan.steps.split_first() {
            Some(_) => {
                let current = run
                    .steps
                    .iter()
                    .position(|s| s.status != engine::StepStatus::Completed)
                    .unwrap_or(run.steps.len().saturating_sub(1));
                (
                    self.plan.steps[current].title.clone(),
                    Some(run.steps[current].id.clone()),
                    self.plan.steps[current].run.command.clone(),
                    run.steps[current].error_code,
                )
            }
            None => (
                self.plan
                    .explore
                    .as_ref()
                    .map(|e| e.task.clone())
                    .unwrap_or_else(|| self.plan.name.clone()),
                None,
                "explore".to_owned(),
                None,
            ),
        };
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
            // 静态流程 needs_review 沿用 recheck;探索的决策应答走
            // flow-resume --decision(agents/脚本),显示端入口是 resume。
            Status::NeedsReview if self.plan.steps.is_empty() => vec!["resume"],
            Status::NeedsReview => vec!["recheck"],
            Status::Stopped => vec!["allow_then_resume"],
            Status::Completed => vec![],
        };
        actl_core::signal_session::workflow(actl_core::state::WorkflowProgress {
            run_id: run.id.clone(),
            title,
            status: status.into(),
            reason: run.reason.clone(),
            step_id,
            error_code,
            actions: actions.into_iter().map(str::to_owned).collect(),
        });
        actl_core::signal_session::step(
            &command,
            None,
            run.steps.len().min(1),
            run.steps.len().max(1),
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
    let (plan, run) = store.load()?;
    let active = store.active()?;
    let interrupted = run.status == Status::Running && !active;
    let mut value = json!({"run":run,"runner_active":active,"interrupted":interrupted,"pause_requested":store.pause_requested(),"template":template_hint(store)});
    // 静态 needs_review:uncertain 写步在裸 resume 下不会重跑(引擎只允许
    // --retry-step 显式授权)——裸 resume 两度被误读成"执行了没生效"
    // (docs/80 改写重放轮 007),按 DX 结构化为可复制的重试命令。
    if let Some(hint) = retry_hint(&run) {
        value["retry_hint"] = hint;
    }
    // 弹层门禁上抛(运行时治理第一轮):popup_pending 不是步骤失败,是
    // "前台有未知弹窗,按纪律先判读"——给出判读入口与续跑命令。
    if run
        .reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with("popup_pending:"))
    {
        value["popup_hint"] = json!({
            "note": "unknown front-layer popup blocked the step; inspect before resuming",
            "inspect": "actl front-report <step --app>",
            "evidence": "actl flow-evidence --id <id>",
            "resume": format!("actl flow-resume --id {}", run.id),
        });
    }
    // AMBIGUOUS 目标串联(docs/63):候选即应答面——暂停记录里直接给
    // 候选清单与可复制的 --pick 命令,不强迫调用方再去读 state 文件。
    if let Some(ambiguous) = run
        .steps
        .iter()
        .find(|s| {
            s.status == actl_flow::engine::StepStatus::Pending
                && s.ambiguous.as_ref().is_some_and(|a| a.picked.is_none())
        })
        .and_then(|s| s.ambiguous.as_ref())
    {
        value["ambiguous_hint"] = json!({
            "step_target": ambiguous.target,
            "snapshot_id": ambiguous.snapshot_id,
            "candidates": ambiguous.candidates,
            "answer": format!(
                "actl flow-resume --id {} --pick <@eN>",
                run.id
            ),
        });
    }
    // 版本谱系(flow-revise,运行时治理第二轮):有修订史的 run 展示
    // 只追加的版本事件链(版本/指纹/理由/diff)——修订是显式操作,不是
    // "模板漂移告警"要防的作弊路径。
    let lineage = actl_flow::revise::lineage(&store.dir);
    if !lineage.is_empty() {
        value["revisions"] =
            serde_json::to_value(&lineage).map_err(|e| CtlError::internal(e.to_string()))?;
    }
    // 探索运行:附探索状态(动作/决策计数、token 审计)与悬挂决策卡。
    // 卡即应答入口(flow-resume --decision <id>);其余证据按省略纪律不进协议面。
    if plan.explore.is_some()
        && let Some(state) = actl_flow::explore::load(store)?
    {
        value["explore"] = json!({
            "task": state.spec.task,
            "app": state.spec.app,
            "actions": state.actions,
            "decisions": state.decisions,
            "observations": state.observations,
            "errors": state.errors,
            "budgets": {
                "actions": state.spec.budgets.actions,
                "decisions": state.spec.budgets.decisions,
                "wall_ms": state.spec.budgets.wall_ms,
            },
            "widened": state.widened,
            "frames_truncated": state.frames_truncated,
            "goal_retries": state.goal_retries,
            "goal_window": state.goal_window,
            "snapshot_id": state.snapshot_id,
            "anchor_transfers": state.anchor_transfers,
            "priors": state.spec.priors,
            "texts": state.spec.texts,
            "tried": state.tried,
            "visited_states": state.visited.len(),
            "goal_met": state.goal_met,
            "audit": {
                "observation_bytes": state.audit.observation_bytes,
                "card_bytes": state.audit.card_bytes,
            },
        });
        if let Some(pending) = &state.pending {
            value["decision_card"] = serde_json::to_value(&pending.card)
                .map_err(|e| CtlError::internal(e.to_string()))?;
        }
        if !state.queue.is_empty() {
            value["explore"]["queue"] = json!(state.queue);
        }
    }
    Ok(value)
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
    let mut plan = actl_flow::inputs::resolve(&prepared.text, values.as_deref())?;
    // pack 级常驻面板声明注入(降噪第二轮 pack 来源,2026-10-11):apps/*.yml
    // 的 popup_ignores 与模板级声明并集去重——逐模板抄窗格清单不可扩展,
    // pack 是应用知识持有者(锚形态列全是 pack 职责)。注入留 stderr 痕。
    crate::pack_inject::inject_pack_popup_ignores(&mut plan)?;
    // 持久 profile 声明治理(docs/60/72):flow 中 browser-launch --profile <name>
    // 只允许使用 resources.browser_profiles 已声明的名字——凭据资产显式归属
    validate_profile_declarations(&plan, prepared.browser_profiles())?;
    Ok((plan, prepared))
}

fn validate_profile_declarations(plan: &Plan, declared: &[String]) -> Result<(), CtlError> {
    for step in &plan.steps {
        let Ok(command) = parse(&step.run) else {
            continue;
        };
        if let Command::BrowserLaunch {
            profile: Some(name),
            ..
        } = command
            && !declared.contains(&name)
        {
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                format!(
                    "step {} uses persistent browser profile {name:?} without declaring it in resources.browser_profiles",
                    step.id
                ),
                json!({"step_id": step.id, "profile": name, "declared": declared}),
            ));
        }
    }
    Ok(())
}

pub fn dispatch(command: &FlowCommand) -> Result<Value, CtlError> {
    if matches!(
        command,
        FlowCommand::Run { .. } | FlowCommand::Resume { .. }
    ) {
        actl_core::stop::check_current()?;
    }
    let root = actl_core::state::SignalPaths::default().dir;
    if let FlowCommand::Writeback { id, out } = command {
        // 只读证据收集(docs/42 S3):片段 status 恒 unverified,verified 迁移
        // 走人工复核(PACKS.md);actl 不写 pack 文件,--out 只落调用方给的片段文件。
        let store = Store::at(&root, id)?;
        let (plan, _) = store.load()?;
        if plan.explore.is_none() {
            return Err(CtlError::protocol("run is not an exploration run"));
        }
        let state = actl_flow::explore::load(&store)?
            .ok_or_else(|| CtlError::protocol("exploration state missing"))?;
        let fragment = actl_flow::route::extract(&state, id)?;
        let yaml = actl_flow::route::render_yaml(&fragment);
        if let Some(path) = out {
            std::fs::write(path, yaml.as_bytes())
                .map_err(|e| CtlError::internal(format!("fragment write: {e}")))?;
        }
        let fragment =
            serde_json::to_value(&fragment).map_err(|e| CtlError::internal(e.to_string()))?;
        return Ok(json!({"route": fragment, "yaml": yaml}));
    }
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
            if let Some(explore) = &plan.explore {
                result["explore"] = json!({
                    "app": explore.app,
                    "keywords": explore.keywords,
                    "budgets": {
                        "actions": explore.budgets.actions,
                        "decisions": explore.budgets.decisions,
                        "wall_ms": explore.budgets.wall_ms,
                    },
                });
            }
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
        // 失败采集包索引(运行时治理第一轮):只读扫描 flow-resources/<id>/
        // evidence/ 的 *.bundle.json 清单;--step 过滤。工件缺失如实不列。
        FlowCommand::Evidence { id, step } => {
            let store = Store::at(&root, id)?;
            let evidence_dir = store
                .dir
                .parent()
                .and_then(|flows| flows.parent())
                .map(|state| state.join("flow-resources").join(id).join("evidence"));
            let Some(dir) = evidence_dir else {
                return Err(CtlError::internal("evidence root unresolved"));
            };
            let mut bundles = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else { continue };
                    if !name.ends_with(".bundle.json") {
                        continue;
                    }
                    let Ok(bytes) = std::fs::read(entry.path()) else {
                        continue;
                    };
                    let Ok(mut manifest) = serde_json::from_slice::<Value>(&bytes) else {
                        continue;
                    };
                    manifest["file"] = Value::String(name.to_string());
                    if let Some(want) = &step
                        && manifest["step_id"].as_str() != Some(want.as_str())
                    {
                        continue;
                    }
                    bundles.push(manifest);
                }
            }
            bundles.sort_by_key(|bundle| {
                (
                    bundle["step_id"].as_str().unwrap_or_default().to_string(),
                    bundle["captured_ms"].as_u64().unwrap_or_default(),
                )
            });
            if bundles.is_empty() {
                return Err(CtlError::protocol(format!(
                    "no failure bundles for id {id}{} (clean run or pre-governance run)",
                    step.as_deref()
                        .map(|s| format!(" step {s}"))
                        .unwrap_or_default()
                )));
            }
            Ok(json!({"id": id, "evidence_dir": dir.to_string_lossy(), "bundles": bundles}))
        }
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
            resources.create_at(Some(
                &actl_browser::session::Paths::default().profiles_dir(),
            ))?;
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
            } else if let Some(spec) = &plan.explore {
                actl_flow::explore::execute(spec, &mut run, &store, &mut host, None, None)?;
            } else {
                engine::execute(&plan, &mut run, &store, &mut host, None, None)?;
            }
            drop(active);
            outcome(&store, &run)
        }
        FlowCommand::Resume {
            id,
            retry_step,
            decision,
            confidence,
            pick,
        } => {
            let source = actl_core::control_log::source();
            actl_core::signal_session::flow_event(
                "flow_resume_requested",
                json!({"run_id":id,"request_id":actl_core::control_log::request_id(),"source":source,"source_verified":false,"retry_step":retry_step,"decision":decision,"pick":pick}),
            );
            let store = Store::at(&root, id)?;
            let _global = store.lock(true)?;
            let (plan, mut run) = store.load()?;
            // Revalidate saved schema and commands before trusting disk state.
            let plan = actl_flow::parse(
                &serde_json::to_string(&plan).map_err(|e| CtlError::internal(e.to_string()))?,
            )?;
            validate(&plan)?;
            if let Some(spec) = &plan.explore {
                if retry_step.is_some() {
                    return Err(CtlError::protocol(
                        "exploration runs do not support --retry-step",
                    ));
                }
                if pick.is_some() {
                    return Err(CtlError::protocol(
                        "exploration runs do not support --pick; use --decision",
                    ));
                }
                let active = store.lock(false)?;
                actl_core::stop::check_current()?;
                store.clear_pause()?;
                actl_flow::explore::execute(
                    spec,
                    &mut run,
                    &store,
                    &mut Adapter {
                        plan: &plan,
                        store: &store,
                    },
                    decision.as_deref(),
                    confidence.as_deref(),
                )?;
                drop(active);
                return outcome(&store, &run);
            }
            if decision.is_some() {
                return Err(CtlError::protocol(
                    "--decision answers an exploration run; this run is a static flow",
                ));
            }
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
                pick.as_deref(),
            )?;
            drop(active);
            outcome(&store, &run)
        }
        // flow-revise(运行时治理第二轮):版本化修订——旧计划字节归档、
        // 版本谱系只追加、完成写步规格变更需显式 --accept-rerun、修订后
        // 默认不自动续跑(--resume 才链式恢复,与显式恢复纪律一致)。
        FlowCommand::Revise {
            id,
            file,
            inputs,
            message,
            accept_rerun,
            resume,
        } => {
            let text = std::fs::read_to_string(file.as_str())
                .map_err(|e| CtlError::internal(format!("revision file unreadable: {e}")))?;
            // input 引用解析(2026-10-11 工具链缺口①收口):修订文件与 flow-run
            // 同形——resources 块经 prepare 剥离,{"input":...} 引用经 declared
            // defaults(或 --inputs 值文件)解析为具体值后再 schema 校验。
            // 此前直接 parse,引用/资源形态必报 PROTOCOL(VS Code 轮被迫
            // emit-plan 具体化+带外手术的根源)。资源重新 prepare 到同
            // run 目录(幂等,不创建新 run)。
            let prepared = actl_flow::resources::prepare(
                &text,
                std::path::Path::new(file.as_str()),
                &root.join("flow-resources").join(id.as_str()),
                id.as_str(),
            )?;
            let values = inputs
                .as_ref()
                .map(|f| actl_flow::store::read_text(std::path::Path::new(f)))
                .transpose()?;
            let mut new_plan = actl_flow::inputs::resolve(&prepared.text, values.as_deref())?;
            // pack 级常驻面板声明注入(与 load_plan 同口径):修订计划同样
            // 注入,否则与冻结计划 diff 时 ignores 差异误报为规格变化。
            crate::pack_inject::inject_pack_popup_ignores(&mut new_plan)?;
            if new_plan.explore.is_some() {
                return Err(CtlError::protocol(
                    "revision file declares an exploration plan; flow-revise is static-flow only",
                ));
            }
            let store = Store::at(&root, id)?;
            if !store.dir.join("plan.json").exists() {
                return Err(CtlError::protocol(format!("unknown id {id}")));
            }
            {
                let _global = store.lock(true)?;
                let (old_plan, mut run) = store.load()?;
                if old_plan.explore.is_some() {
                    return Err(CtlError::protocol(
                        "exploration runs do not support flow-revise",
                    ));
                }
                if store.active()? {
                    return Err(CtlError::new(
                        actl_core::ErrorCode::NotActionable,
                        "executor busy; revise after the run pauses or completes",
                    ));
                }
                if message.chars().count() > 200 {
                    return Err(CtlError::protocol(
                        "revision message exceeds 200 characters",
                    ));
                }
                let adapter = Adapter {
                    plan: &new_plan,
                    store: &store,
                };
                let plan_bytes =
                    serde_json::to_vec(&new_plan).map_err(|e| CtlError::internal(e.to_string()))?;
                let fingerprint = actl_flow::revise::plan_fingerprint(&plan_bytes);
                let outcome = actl_flow::revise::apply_to_run(
                    &old_plan,
                    &new_plan,
                    &mut run,
                    |step| !adapter.read_only(&step.run),
                    accept_rerun,
                    message,
                    &fingerprint,
                )?;
                let old_bytes = std::fs::read(store.dir.join("plan.json"))
                    .map_err(|e| CtlError::internal(format!("plan archive read: {e}")))?;
                run.status = Status::Paused;
                run.reason = None;
                actl_flow::revise::persist(
                    &store.dir,
                    &old_bytes,
                    &new_plan,
                    &run,
                    &outcome.revision,
                )?;
            }
            if *resume {
                let (plan, mut run) = store.load()?;
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
                    None,
                    None,
                )?;
                drop(active);
            }
            let mut value = state(&store)?;
            value["next"] = json!(format!(
                "actl flow-resume --id {id}{}",
                if *resume { " (already resumed)" } else { "" }
            ));
            Ok(value)
        }
        // 防御分支:writeback 在 match 前已提前返回;走到这里说明分发序被破坏。
        FlowCommand::Writeback { .. } => {
            Err(CtlError::internal("writeback dispatch ordering invariant"))
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

/// needs_review 运行的 uncertain 步重试提示:裸 resume 不重跑写步,
/// 命令必须显式给到(误诊教训见 docs/80 改写重放轮 007)。
fn retry_hint(run: &Run) -> Option<Value> {
    if run.status != Status::NeedsReview {
        return None;
    }
    let step = run
        .steps
        .iter()
        .find(|s| s.status == engine::StepStatus::Uncertain)?;
    Some(json!({
        "step": step.id,
        "command": format!("actl flow-resume --id {} --retry-step {}", run.id, step.id),
        "note": "plain resume does not re-execute the uncertain write step",
    }))
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

    fn record(id: &str, status: engine::StepStatus) -> engine::StepRecord {
        engine::StepRecord {
            id: id.into(),
            status,
            attempts: 1,
            checkpoint_passed: false,
            verified: false,
            error_code: None,
            error_evidence: None,
            attempt_errors: vec![],
            outputs: Default::default(),
            outputs_captured_ms: None,
            observation_retries: vec![],
            branch: None,
            recovery: None,
            plan_version: None,
            started_ms: None,
            finished_ms: None,
            ambiguous: None,
        }
    }

    #[test]
    fn retry_hint_names_uncertain_step_and_command() {
        let run = Run {
            version: 1,
            id: "demo-rewrite-007".into(),
            status: Status::NeedsReview,
            updated_ms: 0,
            reason: Some("result_unconfirmed".into()),
            plan_version: 1,
            steps: vec![
                record("s1", engine::StepStatus::Completed),
                record("s6", engine::StepStatus::Uncertain),
                record("s7", engine::StepStatus::Pending),
            ],
        };
        let hint = retry_hint(&run).expect("hint");
        assert_eq!(hint["step"], "s6");
        assert_eq!(
            hint["command"],
            "actl flow-resume --id demo-rewrite-007 --retry-step s6"
        );
    }

    #[test]
    fn retry_hint_absent_without_uncertain_step_or_other_status() {
        let mut run = Run {
            version: 1,
            id: "r".into(),
            status: Status::NeedsReview,
            updated_ms: 0,
            reason: None,
            plan_version: 1,
            steps: vec![record("s1", engine::StepStatus::Completed)],
        };
        assert!(retry_hint(&run).is_none());
        run.status = Status::Paused;
        run.steps.push(record("s2", engine::StepStatus::Uncertain));
        assert!(retry_hint(&run).is_none());
    }

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
