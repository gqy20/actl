use crate::{Failure, Invocation, Plan, PopupHandler, Retry, store::Store};
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
    /// 本记录派发时的计划版本(flow-revise 版本谱系的溯源标记;
    /// None = 修订机制引入前的 v1)。结果可归属到确切的步骤规格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_version: Option<u32>,
    /// 步骤最近一次尝试的起止 unix 毫秒(docs/81 §8:性能画像一等公民——
    /// 逐步耗时不再靠 outputs_captured_ms 差值与外围计时拼凑)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_ms: Option<u64>,
    /// AMBIGUOUS 目标串联(docs/63):run 相多命中时的候选暂停记录;
    /// `--pick` 应答后 picked 置位,派发参数以 ref+snapshot 重写。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ambiguous: Option<crate::AmbiguousTarget>,
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
    /// 当前计划版本(1 起;flow-revise 每次修订 +1)。旧状态文件缺省 = 1。
    #[serde(default = "default_plan_version")]
    pub plan_version: u32,
}
fn default_plan_version() -> u32 {
    1
}
impl Run {
    pub fn new(id: &str, plan: &Plan) -> Self {
        Self {
            version: 1,
            id: id.into(),
            status: Status::Ready,
            updated_ms: actl_core::state::unix_ms(),
            reason: None,
            plan_version: 1,
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
                    plan_version: None,
                    started_ms: None,
                    finished_ms: None,
                    ambiguous: None,
                })
                .collect(),
        }
    }
    pub(crate) fn save(&mut self, store: &Store, host: &mut impl Host) -> Result<(), CtlError> {
        self.updated_ms = actl_core::state::unix_ms();
        // 终态步骤补 finished_ms(幂等):一切落盘路径共用,单点收口
        for step in &mut self.steps {
            if step.started_ms.is_some()
                && step.finished_ms.is_none()
                && matches!(
                    step.status,
                    StepStatus::Completed | StepStatus::Uncertain | StepStatus::Pending
                )
                && step.status != StepStatus::Pending
            {
                step.finished_ms = Some(self.updated_ms);
            }
        }
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
    /// 面板归属的原生 HWND(十进制;无标题保存确认框/错误框实例,docs/81 §8
    /// 枚举盲区)——处置通道用 hwnd 精确直达,不依赖步骤 --app 锚能否覆盖
    /// 无标题弹窗宿主。None = 采集层未提供。
    pub hwnd: Option<isize>,
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
/// 可交互面板判定:unknown 且无按钮 = 对话框宿主壳/纯瞬态(无文本可判、
/// 无按钮可点)——门禁降噪跳过,不阻断(2026-10-04 门禁真机 E2E 发现:
/// Win11 内容对话框自带「弹出窗口」空壳,真按钮在分类面板上)。
fn actionable(surface: &Surface) -> bool {
    surface.classification != "unknown" || !surface.buttons.is_empty()
}

/// notice 处理器匹配(2026-10-11 spec,最窄形态):流程自产的 INFO 级表面
/// (如 VS Code「扩展已禁用」通知)在 naive 词表落 unknown——notice 声明
/// 按 name_contains(schema 层强制必填)认领 unknown 面板;未声明的
/// unknown 仍永远上抛,不存在「任意 unknown 一律放行」的宽形态。
fn classification_matches(handler: &PopupHandler, surface: &Surface) -> bool {
    if handler.classification == "notice" {
        surface.classification == "unknown"
    } else {
        handler.classification == surface.classification
    }
}

fn popup_gate(
    plan: &Plan,
    step: &crate::Step,
    host: &mut impl Host,
    app: &str,
    layer: &FrontLayer,
) -> Result<bool, CtlError> {
    let pending: Vec<&Surface> = layer.surfaces.iter().filter(|s| actionable(s)).collect();
    let mut handled = Vec::new();
    for surface in pending {
        let matched = plan.popup_handlers.iter().find(|handler| {
            app.contains(&handler.app)
                && classification_matches(handler, surface)
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
            // 处置锚(docs/81 §8):面板携带原生 HWND 时用 hwnd 精确直达——
            // 无标题保存确认框的宿主窗不在步骤 --app 锚的标题匹配范围内,
            // 引擎自动处置曾因此够不到按钮(004 实证,只能人工拆弹)。
            args: match surface.hwnd {
                Some(h) => vec![handler.click.clone(), "--app".into(), format!("hwnd:{h}")],
                None => vec![handler.click.clone(), "--app".into(), app.to_string()],
            },
            bind: Default::default(),
        };
        host.popup_event(
            step.id.as_str(),
            serde_json::json!({
                "classification": surface.classification,
                // 认领声明的分类(notice 承接 unknown 时审计面区分
                // 「面板本体分类」与「哪条白名单认领的」,2026-10-11)
                "handler": handler.classification,
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
        if recheck.surfaces.iter().any(actionable) {
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
/// `pick` = `flow-resume --pick @eN`(运行时候选纠正,docs/63):校验 ref 属于
/// 该步骤 AMBIGUOUS 记录的候选集后置位,派发参数以 ref+snapshot 重写。
pub fn execute(
    plan: &Plan,
    run: &mut Run,
    store: &Store,
    host: &mut impl Host,
    retry_step: Option<&str>,
    pick: Option<&str>,
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
            .chain(s.ensure.iter().map(|ensure| &ensure.check))
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
    if let Some(reference) = pick {
        // --pick 校验(运行时候选纠正):必须命中唯一待应答的 AMBIGUOUS
        // 暂停步骤,ref 必须属于候选集——PROTOCOL 先于任何派发。
        let pending: Vec<usize> = run
            .steps
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.status == StepStatus::Pending
                    && s.ambiguous.as_ref().is_some_and(|a| a.picked.is_none())
            })
            .map(|(i, _)| i)
            .collect();
        let [index] = pending[..] else {
            return Err(CtlError::protocol(
                "--pick requires exactly one step awaiting candidate selection",
            ));
        };
        let refs = run.steps[index]
            .ambiguous
            .as_ref()
            .expect("filtered above")
            .candidate_refs();
        if !refs.iter().any(|r| r == reference) {
            return Err(CtlError::protocol(format!(
                "--pick {reference:?} is not one of the offered candidates: {}",
                refs.join(", ")
            )));
        }
        run.steps[index]
            .ambiguous
            .as_mut()
            .expect("filtered above")
            .picked = Some(reference.to_owned());
    }
    let resuming = run.status != Status::Ready;
    let mut section_reason: Option<String> = None;
    run.status = Status::Running;
    run.reason = None;
    run.save(store, host)?;
    'steps: for (i, step) in plan.steps.iter().enumerate() {
        // 看门狗心跳(运行时治理第三轮):步边界 = 进展。长流程不受总时长
        // 限制,只有单步卡死超期限才会被进程级终止(interrupted 恢复语义接手)。
        actl_core::watchdog::reset();
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
        // --pick 应答后的参数重写(docs/63 运行时候选纠正):args[0] 换成
        // 选中 ref 并追加 --snapshot——身份核验交给既有 ReplayGuard(窗口
        // 重建/元素漂移即 STALE_REF 拒绝派发,不消费重写)。计划目标已变
        // (修订后的陈旧 pick)→ PROTOCOL,引导重新判读或走 flow-revise。
        if run.steps[i].status == StepStatus::Pending
            && let Some(ambiguous) = run.steps[i].ambiguous.clone()
            && let Some(reference) = ambiguous.picked
        {
            if step.run.args.first().is_none_or(|a| a != &ambiguous.target) {
                return Err(CtlError::protocol(
                    "step target changed since the ambiguous pause; re-answer with --pick or use flow-revise",
                ));
            }
            step.run.args[0] = reference;
            if !step.run.args.iter().any(|a| a == "--snapshot") {
                step.run.args.push("--snapshot".into());
                step.run.args.push(ambiguous.snapshot_id);
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
            // ensure: 幂等语义(运行时治理第二轮,docs/06):只读 check 通过 =
            // 步骤已满足,直接 Completed+verified,run 不派发——重跑/修订
            // 续跑的环境准备前缀自动短路。观察错误如实传播(与 when/unless
            // 的错误语义同族:不能判定满足就不动写步)。
            if let Some(ensure) = &step.ensure
                && matches!(run.steps[i].status, StepStatus::Pending)
                && host.condition(&ensure.check)?
            {
                run.steps[i].status = StepStatus::Completed;
                run.steps[i].verified = true;
                run.steps[i].plan_version = Some(run.plan_version);
                run.status = Status::Running;
                run.reason = None;
                run.save(store, host)?;
                continue;
            }
            // 步前弹层门禁(运行时治理第一轮,docs/06):写步动手前先听前台
            // 引导层——unknown 面板 = popup_pending,步不执行(保持 Pending,
            // 无副作用不需 uncertain);声明过的面板自动处置留痕后复查放行。
            // 只读步与无 --app 锚的步不设防;观察失败放行(写步守卫兜底)。
            if !host.read_only(&step.run)
                && let Some(app) = invocation_app(&step.run)
                && let Ok(layer) = host.front_layer(&app)
                && layer.surfaces.iter().any(actionable)
                && !popup_gate(plan, &step, host, &app, &layer)?
            {
                run.status = Status::NeedsReview;
                run.reason = Some(format!("popup_pending:{}", step.id));
                run.save(store, host)?;
                host.publish(run);
                return Ok(());
            }
            run.steps[i].plan_version = Some(run.plan_version);
            run.steps[i].status = StepStatus::Running;
            run.steps[i].started_ms = Some(actl_core::state::unix_ms());
            run.steps[i].finished_ms = None;
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
                // AMBIGUOUS 目标拦截(运行时候选纠正,docs/63):locate 多命中
                // =零派发(无"可能已生效"语义),引擎锚定一份骨架快照作候选
                // ref 的身份来源,候选证据**全量保留**(不走默认 redaction——
                // 候选是应答面,截断即失能),run 进 needs_review 等待
                // `flow-resume --pick`。步骤保持 Pending。仅在可识别形态下
                // 拦截:目标在 args[0]、有 --app 锚、证据含候选——其余形态
                // 走既有通用失败语义,不吞边界。
                if error.code == ErrorCode::Ambiguous
                    && step.run.args.first().is_some_and(|a| !a.starts_with('-'))
                    && let Some(app) = invocation_app(&step.run)
                    && let Some(candidates) = error
                        .evidence
                        .as_ref()
                        .and_then(|e| e["candidates"].as_array())
                    && !candidates.is_empty()
                {
                    let snapshot = host.execute_snapshot(vec![
                        "--app".into(),
                        app.clone(),
                        "--skeleton".into(),
                    ]);
                    let (snapshot_id, target) = match snapshot {
                        Ok((id, _)) if !id.is_empty() => (id, step.run.args[0].clone()),
                        // 快照失败:候选无法锚定身份,pick 通道不可用——
                        // 落回通用失败语义(uncertain/paused),不降级猜测。
                        _ => (String::new(), step.run.args[0].clone()),
                    };
                    record_attempt_error(run, i, error.code);
                    run.steps[i].error_code = Some(error.code);
                    if !snapshot_id.is_empty() {
                        run.steps[i].status = StepStatus::Pending;
                        run.steps[i].ambiguous = Some(crate::AmbiguousTarget {
                            command: step.run.command.clone(),
                            target,
                            app: Some(app),
                            snapshot_id,
                            candidates: candidates.clone(),
                            picked: None,
                        });
                        run.status = Status::NeedsReview;
                        run.reason = Some(format!("ambiguous_target:{}", step.id));
                        run.save(store, host)?;
                        host.publish(run);
                        return Ok(());
                    }
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
