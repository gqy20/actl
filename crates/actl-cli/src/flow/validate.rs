//! flow 计划校验(自 flow.rs 拆分,2026-10-11——文件行数 DENY 门禁)。
//! 语义单点:静态计划/探索计划的命令白名单、护栏与 sequence/for_each
//! 伪调用展开校验;入口 [`validate`]。
use crate::cli::{Command, DesktopCommand};
use actl_core::{CtlError, ErrorCode};
use actl_flow::{Plan, Retry};
use serde_json::json;

use super::{parse, read_only, validate_assertion};

pub fn validate(plan: &Plan) -> Result<(), CtlError> {
    // 探索计划(docs/42 S2):无静态步骤;校验目标是只读断言族命令。
    if let Some(explore) = &plan.explore {
        let command = parse(&explore.goal).map_err(|e| {
            CtlError::with_evidence(
                e.code,
                format!("explore.goal: {}", e.message),
                json!({"field": "explore.goal"}),
            )
        })?;
        if !read_only(&command) {
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                "explore goal must be a read-only assertion (verify family)",
                json!({"field": "explore.goal"}),
            ));
        }
        if !matches!(
            command,
            Command::Verify { .. }
                | Command::Wait { .. }
                | Command::VerifyFile { .. }
                | Command::VerifyRows { .. }
                | Command::VerifyOwner { .. }
                | Command::VerifyClosed { .. }
                | Command::VerifyWindow { .. }
                | Command::VerifyClipboard { .. }
                | Command::BrowserVerify { .. }
        ) {
            return Err(CtlError::with_evidence(
                ErrorCode::Protocol,
                "explore goal must be an assertion command (verify/wait family)",
                json!({"field": "explore.goal"}),
            ));
        }
        crate::selector_input::validate(&command)?;
        validate_assertion(&command)?;
        return Ok(());
    }
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
                explore: None,
                popup_handlers: Vec::new(),
                popup_ignores: Vec::new(),
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
                    explore: None,
                    popup_handlers: Vec::new(),
                    popup_ignores: Vec::new(),
                    steps: vec![child],
                })?;
            }
            continue;
        }
        if let Some(each) = actl_flow::for_each::decode(&step.run)? {
            // for_each 步约束:verify 必填(每轮后置核验)、禁 outputs 与
            // read_only retry(data.rs 静态校验同样覆盖;此处早拒语义更近)
            if step.verify.is_none() || !step.outputs.is_empty() || step.retry == Retry::ReadOnly {
                return Err(CtlError::protocol(
                    "for_each requires result verification, no outputs or read-only retry",
                ));
            }
            for call in &each.body {
                let mut child = step.clone();
                // 占位符以中性值参与 clap/护栏预检(运行期注入真实项)。
                // verify 保留(与 sequence 分支同形——迭代体的 run 是写步,
                // 单步校验要求 verify 在场;实际语义由外层步统一执行)。
                child.run = actl_flow::data::for_each_inject(call, "preview")?;
                child.otherwise = None;
                validate(&Plan {
                    version: plan.version,
                    name: plan.name.clone(),
                    handoff: plan.handoff,
                    auto_resume: None,
                    explore: None,
                    popup_handlers: Vec::new(),
                    popup_ignores: Vec::new(),
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
                        | Command::BrowserVerify { .. }
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
