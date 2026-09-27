//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! 参数面在 `cli`(clap 树,help 文本与其 doc comment 同源);命令执行体在
//! `interact`(操作类)与 `query`(观察/编排类);本文件只做入口、分发与 envelope 落盘。

mod cli;
mod flow;
mod interact;
mod query;

use std::time::Instant;

use actl_core::{CtlError, Envelope, ErrorEnvelope};
use clap::FromArgMatches;
use serde_json::Value;

use crate::cli::{Cli, Command, DesktopCommand, app_of, command_name};
use crate::{interact::*, query::*};

/// 命令执行产物:data 进 envelope,`snapshot_id` 存在时附加到顶层(06 §4)。
pub(crate) struct CommandOutput {
    data: Value,
    snapshot_id: Option<String>,
}

impl From<Value> for CommandOutput {
    fn from(data: Value) -> Self {
        Self {
            data,
            snapshot_id: None,
        }
    }
}

fn main() {
    // clap 树注入 after_help 后再 parse(Example/Errors 进 -h;06 §3.5)
    let matches = crate::cli::build_cli().get_matches();
    let cli = Cli::from_arg_matches(&matches).expect("clap-validated matches");
    let command_name = command_name(&cli.command);
    let started = Instant::now();
    // Control/query commands must not overwrite the workflow's desktop feedback.
    if matches!(
        &cli.command,
        Command::Flow(
            crate::cli::FlowCommand::Validate { .. }
                | crate::cli::FlowCommand::Status { .. }
                | crate::cli::FlowCommand::Pause { .. }
        )
    ) {
        finish(
            command_name,
            dispatch(&cli.command),
            started.elapsed().as_millis() as u64,
        );
        return;
    }
    let task = match &cli.command {
        Command::Flow(
            crate::cli::FlowCommand::Run { id, .. } | crate::cli::FlowCommand::Resume { id, .. },
        ) => Ok(Some(id.clone())),
        _ => actl_core::history::task_from_env(),
    };
    let task = match task {
        Ok(task) => task,
        Err(error) => {
            finish(command_name, Err(error), 0);
            return;
        }
    };
    let mut signal = actl_core::signal_session::SignalSession::start_with_task(
        actl_core::state::SignalPaths::default(),
        command_name,
        app_of(&cli.command).as_deref(),
        task,
    );

    let result = dispatch(&cli.command);

    let duration_ms = started.elapsed().as_millis() as u64;
    // 停止标志存在且本命令失败 → 归因为 Stopped(诚实区分用户中止 vs 业务错误)
    let phase = match &result {
        Ok(_) => actl_core::state::Phase::Done,
        Err(e) if e.code == actl_core::ErrorCode::Aborted => actl_core::state::Phase::Stopped,
        Err(_) => actl_core::state::Phase::Error,
    };
    let error = result.as_ref().err().and_then(|e| {
        serde_json::to_value(e.code)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
    });
    signal.finish(phase, error.as_deref());
    finish(command_name, result, duration_ms);
}

/// 单命令执行体;batch 子命令复用同一入口(06 §3.7 唯一复合逃生舱)。
pub(crate) fn dispatch(command: &Command) -> Result<CommandOutput, CtlError> {
    match command {
        Command::Flow(command) => flow::dispatch(command).map(Into::into),
        Command::Commands => commands().map(Into::into),
        Command::Status => status().map(Into::into),
        Command::Desktop(DesktopCommand::History { task, limit }) => {
            actl_core::history::read_history(
                &actl_core::state::SignalPaths::default().dir,
                task.as_deref(),
                *limit,
            )
            .map(Into::into)
        }
        Command::Desktop(DesktopCommand::TaskArchive { file }) => {
            let path = actl_core::history::archive(
                &actl_core::state::SignalPaths::default().dir,
                std::path::Path::new(file),
            )?;
            Ok(serde_json::json!({"archived":true,"path":path,"source":"caller"}).into())
        }
        Command::Desktop(DesktopCommand::DisplayStatus) => {
            let paths = actl_core::state::SignalPaths::default();
            let lease = paths.display_lease();
            Ok(serde_json::json!({"ready":lease.as_ref().is_some_and(|l| l.ready(actl_core::state::unix_ms())),"lease":lease,"stop_requested":paths.stop_requested()}).into())
        }
        Command::Desktop(DesktopCommand::DisplayStart) => {
            let lease = actl_uia::display_gate::ensure()?;
            Ok(serde_json::json!({"ready":true,"lease":lease,"stop_requested":actl_core::state::SignalPaths::default().stop_requested()}).into())
        }
        Command::Desktop(DesktopCommand::MinimizeWindow { pattern, pid, hwnd })
        | Command::Desktop(DesktopCommand::RestoreWindow { pattern, pid, hwnd }) => {
            actl_uia::desktop::set_minimized(
                window_ref_of(pattern.as_deref(), *pid, hwnd.as_deref())?,
                matches!(
                    command,
                    Command::Desktop(DesktopCommand::MinimizeWindow { .. })
                ),
            )
            .map(Into::into)
        }
        Command::Desktop(DesktopCommand::DesktopPrepare { all, hwnd }) => {
            if !all && hwnd.is_empty() {
                return Err(CtlError::protocol(
                    "desktop-prepare requires --all or one or more --hwnd selectors",
                ));
            }
            let handles = hwnd
                .iter()
                .map(|s| actl_uia::parse_hwnd(s))
                .collect::<Result<Vec<_>, _>>()?;
            actl_uia::desktop::prepare(&handles).map(Into::into)
        }
        Command::Desktop(DesktopCommand::DesktopRestore { session }) => {
            actl_uia::desktop::restore(session).map(Into::into)
        }
        Command::Screenshot {
            app,
            hwnd,
            rect,
            element,
            sapp,
            snear,
            out,
        } => screenshot_cmd(
            app.as_deref(),
            hwnd.as_deref(),
            rect.as_deref(),
            element.as_deref(),
            sapp.as_deref(),
            snear.as_deref(),
            out.as_deref(),
        ),
        Command::Paste { app } => paste_cmd(app),
        Command::Snapshot {
            app,
            hwnd,
            skeleton,
            view,
        } => snapshot_cmd(app.as_deref(), hwnd.as_deref(), *skeleton, *view),
        Command::Click {
            target,
            app,
            near,
            snapshot,
            physical,
            with,
            expect,
            timeout,
        } => click(
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *physical,
            with.as_slice(),
            expect.as_deref(),
            *timeout,
        ),
        Command::Get {
            target,
            property,
            app,
            near,
            snapshot,
        } => get(
            target,
            property.as_deref(),
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
        ),
        Command::Launch { args } => launch(args),
        Command::Type {
            target,
            text,
            app,
            near,
            snapshot,
            paste,
        } => type_cmd(
            target,
            text,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *paste,
        ),
        Command::Press {
            keys,
            app,
            expect,
            timeout,
        } => press(keys, app.as_deref(), expect.as_deref(), *timeout),
        Command::SetValue {
            target,
            value,
            app,
            near,
            snapshot,
        } => set_value_cmd(
            target,
            value,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
        ),
        Command::RightClick { target, app, near } => pointer_cmd(
            "right-click",
            target,
            app.as_deref(),
            near.as_deref(),
            PointerKind::Right,
        ),
        Command::DoubleClick { target, app, near } => pointer_cmd(
            "double-click",
            target,
            app.as_deref(),
            near.as_deref(),
            PointerKind::Double,
        ),
        Command::MiddleClick { target, app, near } => pointer_cmd(
            "middle-click",
            target,
            app.as_deref(),
            near.as_deref(),
            PointerKind::Middle,
        ),
        Command::TripleClick { target, app, near } => pointer_cmd(
            "triple-click",
            target,
            app.as_deref(),
            near.as_deref(),
            PointerKind::Triple,
        ),
        Command::Hover { target, app, near } => pointer_cmd(
            "hover",
            target,
            app.as_deref(),
            near.as_deref(),
            PointerKind::Hover,
        ),
        Command::Drag {
            target,
            to,
            app,
            near,
            to_near,
        } => drag_cmd(
            target,
            to,
            app.as_deref(),
            near.as_deref(),
            to_near.as_deref(),
        ),
        Command::Scroll {
            target,
            down,
            up,
            app,
            near,
        } => scroll_cmd(target, *down, *up, app.as_deref(), near.as_deref()),
        Command::KeyDown { keys, app } => key_half_cmd("key-down", keys, app.as_deref(), true),
        Command::KeyUp { keys, app } => key_half_cmd("key-up", keys, app.as_deref(), false),
        Command::ResizeWindow {
            pattern,
            width,
            height,
        } => resize_window_cmd(pattern, *width, *height),
        Command::Verify {
            exists,
            value,
            expected,
            contains,
            checked,
            app,
            near,
        } => verify_cmd(
            exists.as_deref(),
            value.as_deref(),
            expected.as_deref(),
            *contains,
            checked.as_deref(),
            app.as_deref(),
            near.as_deref(),
        ),
        Command::Extract { target, app } => extract_cmd(target, app.as_deref()),
        Command::Select {
            target,
            name,
            app,
            near,
        } => select_cmd(target, name, app.as_deref(), near.as_deref()),
        Command::Toggle { target, app, near } => {
            toggle_cmd(target, app.as_deref(), near.as_deref())
        }
        Command::FocusWindow { pattern, pid, hwnd } => {
            focus_window_cmd(pattern.as_deref(), *pid, hwnd.as_deref())
        }
        Command::CloseWindow { pattern, pid, hwnd } => {
            close_window_cmd(pattern.as_deref(), *pid, hwnd.as_deref())
        }
        Command::ListWindows => list_windows_cmd(),
        Command::Find {
            name,
            role,
            id,
            app,
            limit,
        } => find_cmd(
            name.as_deref(),
            role.as_deref(),
            id.as_deref(),
            app.as_deref(),
            *limit,
        ),
        Command::GetClipboard => get_clipboard_cmd(),
        Command::SetClipboard { text } => set_clipboard_cmd(text),
        Command::Wait {
            element,
            window,
            gone,
            property,
            contains,
            stable,
            app,
            timeout,
        } => wait_cmd(
            element.as_deref(),
            window.as_deref(),
            gone.as_deref(),
            property.as_deref(),
            *contains,
            *stable,
            app.as_deref(),
            *timeout,
        ),
        Command::Batch { json } => batch_cmd(json),
    }
}

fn finish(command_name: &'static str, result: Result<CommandOutput, CtlError>, duration_ms: u64) {
    match result {
        Ok(out) => {
            let mut env = Envelope::success(command_name, Some(out.data), duration_ms);
            if let Some(id) = out.snapshot_id {
                env = env.with_snapshot_id(id);
            }
            println!(
                "{}",
                serde_json::to_string(&env).expect("envelope serializes")
            );
        }
        Err(err) => {
            let env = ErrorEnvelope::new(command_name, &err, duration_ms);
            println!(
                "{}",
                serde_json::to_string(&env).expect("envelope serializes")
            );
            // 退出码契约(06 §4):PROTOCOL 不可恢复 → 2;其余业务失败 → 1
            let code = if err.code == actl_core::ErrorCode::Protocol {
                2
            } else {
                1
            };
            std::process::exit(code);
        }
    }
}
