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
    /// Isolated by a `skip_section` failure elsewhere in this section; never executed.
    Skipped,
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
    /// Error codes of the last failed attempts, oldest first (bounded to 8).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempt_errors: Vec<ErrorCode>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub outputs: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs_captured_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observation_retries: Vec<crate::recovery::ObservationRetry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<crate::branch::Branch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<crate::bounded::Progress>,
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
                    attempt_errors: Vec::new(),
                    outputs: Default::default(),
                    outputs_captured_ms: None,
                    observation_retries: vec![],
                    branch: None,
                    recovery: None,
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
/// 步前弹层门禁的观察结果(运行时治理第一轮,docs/06):Host 从前台引导层
/// 采集,引擎只做分类判定。观察失败由调用方决定放行——门禁是阳性检测,
/// 观察不可用不伪造阻塞,写步自身守卫兜底。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrontLayer {
    pub clear: bool,
    /// 未清空的前台面板:树内对话框(带分类)与 owned 原生弹窗(无文本
    /// 可判,一律 unknown——宁可上抛)。
    pub surfaces: Vec<Surface>,
}
/// 单个前台面板摘要。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    /// error/save/confirm/update/unknown(朴素分类,判定权在上抛侧)。
    pub classification: String,
    pub name: String,
    pub text: String,
    pub buttons: Vec<String>,
}
/// 提取调用的 `--app` 锚(门禁定位用):顶层参数或 sequence 子调用内嵌。
fn invocation_app(call: &Invocation) -> Option<String> {
    for (index, arg) in call.args.iter().enumerate() {
        if arg == "--app" {
            return call
                .args
                .get(index + 1)
                .cloned()
                .filter(|value| !value.is_empty() && !value.starts_with('-'));
        }
        if arg.starts_with('{')
            && let Ok(child) = serde_json::from_str::<Invocation>(arg)
            && let Some(app) = invocation_app(&child)
        {
            return Some(app);
        }
    }
    None
}
/// 步前弹层门禁(运行时治理第一轮,docs/06):写步动手前先听前台引导层。
/// 返回 Ok(true)=放行,false=popup_pending(unknown 面板存在或处置未清层,
/// 调用方置 NeedsReview 并保留 Pending——步未执行,无副作用需 uncertain 化)。
/// 匹配 handler 的面板经既有 click 通道处置并留痕(popup_event);处置后
/// 复查一次,清空才放行。观察失败在调用方已放行,不会进入本函数。
fn popup_gate(
    plan: &Plan,
    step: &crate::Step,
    host: &mut impl Host,
    app: &str,
    layer: &FrontLayer,
) -> Result<bool, CtlError> {
    let mut handled = Vec::new();
    for surface in &layer.surfaces {
        let matched = plan.popup_handlers.iter().find(|handler| {
            app.contains(&handler.app)
                && handler.classification == surface.classification
                && handler.name_contains.as_deref().is_none_or(|needle| {
                    surface.name.contains(needle) || surface.text.contains(needle)
                })
        });
        let Some(handler) = matched else {
            host.capture_failure(step.id.as_str(), "popup_pending", Some(&step.run));
            return Ok(false);
        };
        let action = Invocation {
            command: "click".into(),
            args: vec![handler.click.clone(), "--app".into(), app.to_string()],
            bind: Default::default(),
        };
        host.popup_event(
            step.id.as_str(),
            serde_json::json!({
                "classification": surface.classification,
                "surface": surface.name,
                "click": handler.click,
                "app": app,
            }),
        );
        // 处置走既有 click 全护栏;失败视为未处置(fail-safe 上抛)。
        host.execute(&action)?;
        handled.push(surface.name.clone());
    }
    if !handled.is_empty() {
        let recheck = host.front_layer(app)?;
        if !recheck.clear {
            host.capture_failure(step.id.as_str(), "popup_pending", Some(&step.run));
            return Ok(false);
        }
    }
    Ok(true)
}

pub trait Host {
    fn recovery_event(&mut self, _step_id: &str, _event: &crate::bounded::Event) {}
    /// 步骤失败留证钩子(默认无操作):实现方在失败瞬间截屏、记录窗口清单与
    /// UIA 快照等取证。只读诊断,失败不得影响流程走向;实现自行吞错并限频。
    /// call 携带失败的调用(含其 --app 上下文)供定向取证;bind 失败无调用。
    fn capture_failure(&mut self, _step_id: &str, _phase: &str, _call: Option<&Invocation>) {}
    fn sleep_ms(&mut self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
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
    /// 批量只读探测通道(docs/42 §4.1 的卡面收口):引擎用于卡候选与应答
    /// 回退队列的存活性预检。默认串行逐个执行;实现方可对独立只读调用
    /// 并行化(结果与输入同序,单个失败不影响其他调用)。调用方保证只传
    /// 只读探测命令;本层不解释结果语义。
    fn execute_probes(&mut self, calls: &[Invocation]) -> Vec<Result<(), CtlError>> {
        calls.iter().map(|call| self.execute(call)).collect()
    }
    /// 快照执行通道(发现 #6 的结构性收口):返回 (snapshot_id, data)。
    /// snapshot_id 是帧内 ref(@eN)的身份锚点——引擎的 ref 绑定动作
    /// (`click @eN --snapshot <id>`)依赖它走捕获身份解析。默认实现
    /// 从 execute_output 推导,snapshot_id 未知(空串)→ 引擎回退选择器
    /// 动作。实现方覆写以捕获 envelope 顶层的 snapshot_id。
    fn execute_snapshot(
        &mut self,
        args: Vec<String>,
    ) -> Result<(String, serde_json::Value), CtlError> {
        let call = Invocation {
            command: "snapshot".into(),
            args,
            bind: Default::default(),
        };
        self.execute_output(&call).map(|data| (String::new(), data))
    }
    fn validate_step(&self, _step: &crate::Step) -> Result<(), CtlError> {
        Ok(())
    }
    /// 步前弹层观察(运行时治理第一轮):返回锚窗的前台引导层摘要。
    /// 默认 clear——离线 Host/无显示环境不设防(门禁是阳性检测)。
    fn front_layer(&mut self, _app: &str) -> Result<FrontLayer, CtlError> {
        Ok(FrontLayer {
            clear: true,
            surfaces: Vec::new(),
        })
    }
    /// 弹层处置留痕(实现方转发到审计通道):门禁按声明处置面板时逐次上报。
    fn popup_event(&mut self, _step_id: &str, _event: serde_json::Value) {}
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
/// Steps sharing the failing step's section value are one isolation scope.
fn same_section(plan: &crate::Plan, index: usize, other: usize) -> bool {
    plan.steps[index]
        .section
        .as_deref()
        .is_some_and(|scope| plan.steps[other].section.as_deref() == Some(scope))
}
/// Mark the not-yet-started steps of the section as skipped; the run continues
/// with the first step outside it. Already-recorded results are never rewritten.
fn skip_section(plan: &crate::Plan, run: &mut Run, index: usize) {
    for other in (index + 1)..plan.steps.len() {
        if same_section(plan, index, other) && run.steps[other].status == StepStatus::Pending {
            run.steps[other].status = StepStatus::Skipped;
        }
    }
}
/// A later authorized retry that clears the uncertainty may run its section again.
fn restore_section(plan: &crate::Plan, run: &mut Run, index: usize) {
    for other in (index + 1)..plan.steps.len() {
        if same_section(plan, index, other) && run.steps[other].status == StepStatus::Skipped {
            run.steps[other].status = StepStatus::Pending;
        }
    }
}
fn record_attempt_error(run: &mut Run, index: usize, code: ErrorCode) {
    let record = &mut run.steps[index];
    record.attempt_errors.push(code);
    if record.attempt_errors.len() > 8 {
        record.attempt_errors.remove(0);
    }
}
fn failure_reason(base: &str, run: &Run, index: usize) -> String {
    if crate::route_defect(&run.steps[index]) {
        format!("{base};route_defect_suspected")
    } else {
        base.to_owned()
    }
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
    let mut section_reason: Option<String> = None;
    run.status = Status::Running;
    run.reason = None;
    run.save(store, host)?;
    'steps: for (i, step) in plan.steps.iter().enumerate() {
        if matches!(
            run.steps[i].status,
            StepStatus::Completed | StepStatus::Skipped
        ) {
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
                run.steps[i].error_evidence =
                    crate::step_failure_evidence(step, &error, "bind", "resolve");
                host.capture_failure(&step.id, "bind", None);
                record_attempt_error(run, i, error.code);
                if step.on_failure == crate::Failure::SkipSection {
                    skip_section(plan, run, i);
                    let reason = failure_reason(&format!("section_failure:{}", step.id), run, i);
                    section_reason = Some(reason.clone());
                    run.status = Status::Running;
                    run.reason = Some(reason);
                    run.save(store, host)?;
                    continue;
                }
                run.status = if step.on_failure == Failure::Fail {
                    Status::Failed
                } else {
                    Status::Paused
                };
                run.reason = Some(failure_reason("binding_validation_failed", run, i));
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
            run.steps[i].error_evidence =
                crate::step_failure_evidence(&step, &error, "resume_check", &check.command);
            host.capture_failure(&step.id, "resume_check", Some(check));
            record_attempt_error(run, i, error.code);
            if boundary(run, store, host)? {
                return Ok(());
            }
            if step.on_failure == crate::Failure::SkipSection {
                skip_section(plan, run, i);
                let reason = failure_reason(&format!("section_failure:{}", step.id), run, i);
                section_reason = Some(reason.clone());
                run.status = Status::Running;
                run.reason = Some(reason);
                run.save(store, host)?;
                continue;
            }
            run.status = Status::Paused;
            if step.on_failure == Failure::Fail {
                run.status = Status::Failed;
            }
            run.reason = Some(failure_reason("resume_check_failed", run, i));
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
                        restore_section(plan, run, i);
                        run.save(store, host)?;
                        continue;
                    }
                    Ok(_) => {} // A read completed but its output was not durably captured; re-read.
                    Err(error) => {
                        replay_confirmed = error.code == ErrorCode::AssertionFailed;
                        run.steps[i].error_evidence =
                            crate::step_failure_evidence(&step, &error, "verify", &verify.command);
                        host.capture_failure(&step.id, "verify", Some(verify));
                        run.steps[i].error_code = Some(error.code);
                        record_attempt_error(run, i, error.code);
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
                if step.on_failure == crate::Failure::SkipSection {
                    skip_section(plan, run, i);
                    let reason = failure_reason(&format!("section_failure:{}", step.id), run, i);
                    section_reason = Some(reason.clone());
                    run.status = Status::Running;
                    run.reason = Some(reason);
                    run.save(store, host)?;
                    continue;
                }
                run.status = Status::NeedsReview;
                run.reason = Some(failure_reason(
                    if replay_confirmed {
                        "result_unconfirmed; inspect before authorizing --retry-step"
                    } else {
                        "result_observation_failed"
                    },
                    run,
                    i,
                ));
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
            host.capture_failure(&step.id, "before", Some(before));
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
            match crate::branch::choose(plan, &mut step, i, run, store, host)? {
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
            // 步前弹层门禁(运行时治理第一轮,docs/06):写步动手前先听前台
            // 引导层——unknown 面板 = popup_pending,步不执行(保持 Pending,
            // 无副作用不需 uncertain);声明过的面板自动处置留痕后复查放行。
            // 只读步与无 --app 锚的步不设防;观察失败放行(写步守卫兜底)。
            if !host.read_only(&step.run)
                && let Some(app) = invocation_app(&step.run)
                && let Ok(layer) = host.front_layer(&app)
                && !layer.clear
                && !popup_gate(plan, &step, host, &app, &layer)?
            {
                run.status = Status::NeedsReview;
                run.reason = Some(format!("popup_pending:{}", step.id));
                run.save(store, host)?;
                host.publish(run);
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
                run.steps[i].error_evidence =
                    crate::step_failure_evidence(&step, &error, "run", &step.run.command);
                host.capture_failure(&step.id, "run", Some(&step.run));
                run.steps[i].error_code = Some(error.code);
                run.steps[i].status = if host.read_only(&step.run) {
                    StepStatus::Failed
                } else {
                    StepStatus::Uncertain
                };
                record_attempt_error(run, i, error.code);
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
                if step.on_failure == crate::Failure::SkipSection {
                    skip_section(plan, run, i);
                    let reason = failure_reason(&format!("section_failure:{}", step.id), run, i);
                    section_reason = Some(reason.clone());
                    run.status = Status::Running;
                    run.reason = Some(reason);
                    run.save(store, host)?;
                    continue 'steps;
                }
                run.status = if step.on_failure == Failure::Fail {
                    Status::Failed
                } else {
                    Status::Paused
                };
                run.reason = Some(failure_reason("action_failed", run, i));
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
                        restore_section(plan, run, i);
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
                            crate::step_failure_evidence(&step, &error, "verify", &verify.command);
                        host.capture_failure(&step.id, "verify", Some(verify));
                        run.steps[i].error_code = Some(error.code);
                        run.steps[i].status = StepStatus::Uncertain;
                        record_attempt_error(run, i, error.code);
                        if boundary(run, store, host)? {
                            return Ok(());
                        }
                        if step.on_failure == crate::Failure::SkipSection {
                            skip_section(plan, run, i);
                            let reason =
                                failure_reason(&format!("section_failure:{}", step.id), run, i);
                            section_reason = Some(reason.clone());
                            run.status = Status::Running;
                            run.reason = Some(reason);
                            run.save(store, host)?;
                            continue 'steps;
                        }
                        run.status = if step.on_failure == Failure::Fail {
                            Status::Failed
                        } else {
                            Status::NeedsReview
                        };
                        run.reason = Some(failure_reason("verification_failed", run, i));
                        run.save(store, host)?;
                        return Ok(());
                    }
                }
            }
            run.save(store, host)?;
            break;
        }
    }
    if let Some(reason) = section_reason.filter(|r| !r.is_empty()) {
        run.status = Status::NeedsReview;
        run.reason = Some(reason);
    } else {
        run.status = Status::Completed;
        run.reason = None;
    }
    run.save(store, host)
}
