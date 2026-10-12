//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! 参数面在 `cli`(clap 树,help 文本与其 doc comment 同源);命令执行体在
//! `interact`(操作类)与 `query`(观察/编排类);本文件只做入口、分发与 envelope 落盘。

mod browser;
mod cli;
mod condition;
mod file_check;
mod flow;
mod help_text;
mod interact;
mod pack_inject;
mod query;
mod record;
mod selector_input;

use std::time::Instant;

use actl_core::{CtlError, Envelope, ErrorEnvelope};
use clap::FromArgMatches;
use serde_json::{Value, json};

use crate::cli::{Cli, Command, DesktopCommand, app_of, command_name};
use crate::{interact::*, query::*};

/// 命令执行产物:data 进 envelope,`snapshot_id` 存在时附加到顶层(06 §4)。
/// 字段 crate 可见:flow Adapter 的快照通道需要捕获 snapshot_id
/// (探索引擎 ref 绑定动作,docs/06 帧背书/ref 绑定增补)。
pub(crate) struct CommandOutput {
    pub(crate) data: Value,
    pub(crate) snapshot_id: Option<String>,
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
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let command_name = command_name(&cli.command);
    let started = Instant::now();
    // 进程看门狗(运行时治理第三轮,docs/06 §7.7):UIA 跨进程挂起族的
    // 进程级防护——语义是"无进展"(引擎按步、等待按轮询、录制按事件重置),
    // 不是总时长。ACTL_WATCHDOG_MS 可调,0=关。
    actl_core::watchdog::arm(actl_core::watchdog::env_timeout_ms());
    // 测试缝(同 ACTL_RECORD_DISABLE_HOOKS 先例):制造确定性的无进展挂起,
    // 供看门狗触发的契约测试使用;非测试环境不设此变量。
    if let Ok(hang_ms) = std::env::var("ACTL_TEST_HANG_MS")
        && let Ok(hang_ms) = hang_ms.parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(hang_ms));
        return;
    }
    if let Err(error) = flow::configure_task() {
        finish(command_name, Err(error), 0);
        return;
    }
    let _execution = actl_core::stop::Scope::enter();
    if let Ok(generation) = std::env::var("ACTL_EXECUTION_GENERATION") {
        actl_core::stop::bind_generation(generation);
    }
    // Control/query commands must not overwrite the workflow's desktop feedback.
    if matches!(
        &cli.command,
        Command::Flow(
            crate::cli::FlowCommand::Validate { .. }
                | crate::cli::FlowCommand::Status { .. }
                | crate::cli::FlowCommand::Evidence { .. }
                | crate::cli::FlowCommand::Pause { .. }
                | crate::cli::FlowCommand::Writeback { .. },
        ) | Command::Preview { .. }
            | Command::Desktop(DesktopCommand::ExecutionStatus | DesktopCommand::ExecutionRecover)
            | Command::Record(
                crate::cli::RecordCommand::Status { .. } | crate::cli::RecordCommand::Stop { .. },
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
    selector_input::validate(command)?;
    match command {
        Command::Preview {
            target,
            candidate,
            app,
            snapshot,
            message,
            duration,
        } => {
            let target = actl_core::parse_target(target)?;
            actl_uia::preview::validate(&target, snapshot.as_deref(), message, *duration)?;
            let _replay = replay_guard(snapshot.as_deref(), Some(app))?;
            if !candidate.is_empty() {
                let targets = actl_uia::preview::validate_candidates(
                    &target,
                    candidate,
                    snapshot.as_deref(),
                )?;
                return actl_uia::preview::choose(
                    app,
                    &targets,
                    snapshot.as_deref().unwrap_or_default(),
                    message,
                    *duration,
                )
                .map(Into::into);
            }
            actl_uia::preview::show(app, &target, message, *duration).map(Into::into)
        }
        Command::Flow(command) => flow::dispatch(command).map(Into::into),
        Command::Record(command) => record::dispatch(command).map(Into::into),
        Command::VerifyWindow { app, state } => {
            actl_uia::window_check::verify_window(app, state).map(Into::into)
        }
        Command::VerifyClosed {
            hwnd,
            pid,
            instance,
        } => file_check::closed(hwnd, *pid, instance).map(Into::into),
        Command::VerifyClipboard {
            expected,
            normalize_lines,
        } => file_check::clipboard(expected, *normalize_lines).map(Into::into),
        Command::VerifyOwner {
            app,
            program,
            token,
            instance,
            hwnd,
            pid,
        } => file_check::owner(
            app,
            program,
            token,
            instance.as_deref(),
            hwnd.as_deref(),
            *pid,
        )
        .map(Into::into),
        Command::VerifyRows {
            app,
            prefix,
            expected,
        } => file_check::rows(app, prefix, expected).map(Into::into),
        Command::VerifyFile {
            path,
            kind,
            expected,
            expected_file,
            subset,
        } => {
            // --expected-file:期望从 UTF-8 文件读——长/中文 JSON 经 shell 传参
            // 的引号/编码坑类消灭(docs/81 §8);与 --expected 互斥。
            let expectation = match (expected, expected_file) {
                (Some(_), Some(_)) => {
                    return Err(actl_core::CtlError::protocol(
                        "--expected and --expected-file are mutually exclusive",
                    ));
                }
                (Some(text), None) => text.clone(),
                (None, Some(file)) => std::fs::read_to_string(file)
                    .map_err(|e| {
                        actl_core::CtlError::new(
                            actl_core::ErrorCode::Protocol,
                            format!("cannot read --expected-file {file:?}: {e}"),
                        )
                    })?
                    .trim_end_matches(['\r', '\n'])
                    .to_string(),
                (None, None) => {
                    return Err(actl_core::CtlError::protocol(
                        "verify-file requires --expected or --expected-file",
                    ));
                }
            };
            file_check::verify(path, kind, &expectation, *subset).map(Into::into)
        }
        Command::Commands => commands().map(Into::into),
        Command::Status => status().map(Into::into),
        Command::Desktop(DesktopCommand::ExecutionStatus) => {
            Ok(actl_core::state::SignalPaths::default()
                .execution_status()
                .into())
        }
        Command::Desktop(DesktopCommand::FocusReport) => actl_uia::focus_report().map(Into::into),
        Command::Desktop(DesktopCommand::GetSelection {
            target,
            app,
            max_chars,
        }) => {
            let target = target.as_deref().unwrap_or("focused:");
            let parsed = actl_core::parse_target(target)?;
            let loc = actl_uia::locate(app.as_deref(), &parsed, None)?;
            let outcome = actl_uia::get_selection(&loc, max_chars.unwrap_or(8000))?;
            Ok(json!({
                "action": "get-selection",
                "target": parsed.describe(),
                "resolved_by": loc.resolved_by,
                "available": outcome.available,
                "reason": outcome.reason,
                "kind": outcome.kind,
                "via": outcome.via,
                "ranges": outcome.ranges,
                "items": outcome.items,
                "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
            })
            .into())
        }
        Command::Desktop(DesktopCommand::FrontReport { app }) => {
            actl_uia::front_report(app).map(Into::into)
        }
        Command::Desktop(DesktopCommand::ExecutionRecover) => {
            actl_uia::display_gate::recover_execution().map(Into::into)
        }
        Command::Desktop(DesktopCommand::History {
            task,
            step,
            reports,
            timeline,
            limit,
        }) => {
            let dir = &actl_core::state::SignalPaths::default().dir;
            if *timeline {
                actl_core::history::read_timeline(dir, task.as_deref(), *limit).map(Into::into)
            } else {
                actl_core::history::query(dir, task.as_deref(), step.as_deref(), *limit, *reports)
                    .map(Into::into)
            }
        }
        Command::Desktop(DesktopCommand::TaskArchive { file }) => {
            let path = actl_core::history::archive(
                &actl_core::state::SignalPaths::default().dir,
                std::path::Path::new(file),
            )?;
            Ok(serde_json::json!({"archived":true,"path":path,"source":"caller"}).into())
        }
        Command::Desktop(DesktopCommand::DisplayStatus) => {
            Ok(actl_uia::display_gate::diagnostics().into())
        }
        Command::Desktop(DesktopCommand::DisplayStart) => {
            let lease = actl_uia::display_gate::ensure()?;
            Ok(serde_json::json!({"ready":true,"lease":lease,"stop_requested":actl_core::state::SignalPaths::default().stop_requested()}).into())
        }
        Command::Desktop(DesktopCommand::DisplayStop { reason }) => {
            if reason.len() > 200 {
                return Err(CtlError::protocol(
                    "display-stop --reason exceeds 200 chars",
                ));
            }
            actl_uia::display_gate::stop(reason).map(Into::into)
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
        Command::FindImage {
            template,
            app,
            threshold,
            all,
            limit,
            scales,
        } => find_image_cmd(template, app.as_deref(), *threshold, *all, *limit, *scales),
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
            keywords,
            within,
            depth,
            limit,
        } => snapshot_cmd(crate::query::SnapshotArgs {
            app: app.as_deref(),
            hwnd: hwnd.as_deref(),
            skeleton: *skeleton,
            view: *view,
            keywords: keywords.as_deref(),
            within: within.as_deref(),
            depth: *depth,
            limit: *limit,
        }),
        Command::Click {
            target,
            app,
            near,
            snapshot,
            physical,
            with,
            no_activate,
            expect,
            timeout,
        } => click(
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *physical,
            with.as_slice(),
            *no_activate,
            expect.as_deref(),
            *timeout,
        ),
        Command::Focus {
            target,
            app,
            near,
            snapshot,
        } => focus(target, app.as_deref(), near.as_deref(), snapshot.as_deref()),
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
        Command::Launch {
            args,
            wait_window,
            timeout,
        } => launch(args, wait_window.as_deref(), *timeout),
        Command::Type {
            target,
            text,
            app,
            near,
            snapshot,
            paste,
            verify_value,
        } => type_cmd(
            target,
            text,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *paste,
            verify_value.as_deref(),
        ),
        Command::Press {
            keys,
            app,
            expect,
            timeout,
            target,
            snapshot,
        } => press(
            keys,
            app.as_deref(),
            expect.as_deref(),
            *timeout,
            target.as_deref(),
            snapshot.as_deref(),
        ),
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
        Command::RightClick {
            target,
            app,
            near,
            no_activate,
        } => pointer_cmd(
            "right-click",
            target,
            app.as_deref(),
            near.as_deref(),
            *no_activate,
            PointerKind::Right,
        ),
        Command::DoubleClick {
            target,
            app,
            near,
            no_activate,
        } => pointer_cmd(
            "double-click",
            target,
            app.as_deref(),
            near.as_deref(),
            *no_activate,
            PointerKind::Double,
        ),
        Command::MiddleClick {
            target,
            app,
            near,
            no_activate,
        } => pointer_cmd(
            "middle-click",
            target,
            app.as_deref(),
            near.as_deref(),
            *no_activate,
            PointerKind::Middle,
        ),
        Command::TripleClick {
            target,
            app,
            near,
            no_activate,
        } => pointer_cmd(
            "triple-click",
            target,
            app.as_deref(),
            near.as_deref(),
            *no_activate,
            PointerKind::Triple,
        ),
        Command::Hover {
            target,
            app,
            near,
            no_activate,
        } => pointer_cmd(
            "hover",
            target,
            app.as_deref(),
            near.as_deref(),
            *no_activate,
            PointerKind::Hover,
        ),
        Command::Drag {
            target,
            to,
            path,
            app,
            near,
            to_near,
        } => drag_cmd(
            target,
            to,
            path,
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
        Command::KeyDown {
            keys,
            duration,
            app,
        } => key_half_cmd("key-down", keys, app.as_deref(), *duration, true),
        Command::KeyUp { keys, app } => key_half_cmd("key-up", keys, app.as_deref(), None, false),
        Command::ResizeWindow {
            pattern,
            width,
            height,
        } => resize_window_cmd(pattern, *width, *height),
        Command::Verify {
            value,
            expected,
            url_query: Some(key),
            app,
            near,
            ..
        } => file_check::url_query(
            value.as_deref().unwrap_or_default(),
            expected.as_deref().unwrap_or_default(),
            key,
            app.as_deref(),
            near.as_deref(),
        )
        .map(Into::into),
        Command::Verify {
            value,
            property,
            expected,
            normalize_lines,
            app,
            near,
            ..
        } if *normalize_lines => file_check::element_text(
            value.as_deref().unwrap_or_default(),
            expected.as_deref().unwrap_or_default(),
            property.as_deref().unwrap_or("text"),
            app.as_deref(),
            near.as_deref(),
        )
        .map(Into::into),
        Command::Verify {
            exists,
            value,
            property,
            expected,
            contains,
            checked,
            app,
            near,
            ..
        } => verify_cmd(
            exists.as_deref(),
            value.as_deref(),
            property.as_deref(),
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
        Command::CloseWindow {
            pattern,
            pid,
            hwnd,
            system_close,
        } => close_window_cmd(pattern.as_deref(), *pid, hwnd.as_deref(), *system_close),
        Command::ListWindows { native, hwnd } => list_windows_cmd(*native, hwnd.as_deref()),
        Command::Find {
            target,
            name,
            role,
            id,
            capable,
            app,
            limit,
        } => find_cmd(
            target.as_deref(),
            name.as_deref(),
            role.as_deref(),
            id.as_deref(),
            capable.as_deref(),
            app.as_deref(),
            *limit,
        ),
        Command::Collect {
            target,
            role,
            rounds,
            max_items,
            app,
        } => collect_cmd(
            target.as_str(),
            role.as_deref(),
            *rounds,
            *max_items,
            app.as_deref(),
        ),
        Command::BrowserLaunch {
            browser_executable,
            url,
            profile,
            proxy,
            user_agent,
            headless,
            disable_extensions,
            timeout,
        } => browser::launch_cmd(
            browser_executable,
            url.as_deref(),
            profile.as_deref(),
            proxy.as_deref(),
            user_agent.as_deref(),
            *headless,
            *disable_extensions,
            *timeout,
        ),
        Command::BrowserNavigate {
            session,
            url,
            timeout,
        } => browser::navigate_cmd(session, url, *timeout),
        Command::BrowserRead {
            session,
            css,
            xpath,
            property,
            iframe,
            all,
            deep,
            limit,
            wait_for,
            timeout,
        } => browser::read_cmd(
            session,
            css.as_deref(),
            xpath.as_deref(),
            property.as_deref(),
            *iframe,
            *all,
            *deep,
            *limit,
            *wait_for,
            *timeout,
        ),
        Command::BrowserType {
            session,
            css,
            xpath,
            text,
            iframe,
            allow_popups,
            timeout,
        } => browser::type_cmd(
            session,
            css.as_deref(),
            xpath.as_deref(),
            text,
            *iframe,
            *timeout,
            *allow_popups,
        ),
        Command::BrowserClick {
            session,
            css,
            xpath,
            iframe,
            expect_text,
            allow_popups,
            timeout,
        } => browser::click_cmd(
            session,
            css.as_deref(),
            xpath.as_deref(),
            *iframe,
            expect_text.as_deref(),
            *timeout,
            *allow_popups,
        ),
        Command::BrowserClose { session, timeout } => browser::close_cmd(session, *timeout),
        Command::BrowserList => browser::list_cmd(),
        Command::AppRegister {
            name,
            path,
            pattern,
        } => {
            let result = if *pattern {
                actl_uia::app_registry::register_pattern(name, path)
            } else {
                actl_uia::app_registry::register(name, path, true)
            };
            result.map(|_| serde_json::json!({"registered": name}).into())
        }
        Command::AppList => actl_uia::app_registry::list().map(Into::into),
        Command::AppRemove { name } => actl_uia::app_registry::remove(name)
            .map(|_| serde_json::json!({"removed": name}).into()),
        Command::BrowserVerify {
            session,
            alive,
            gone,
            css,
            xpath,
            contains,
            no_popups,
            popup_selector,
            none,
            newest_alive,
            timeout,
        } => browser::verify_cmd(browser::VerifyArgs {
            session: session.as_deref(),
            alive: *alive,
            gone: *gone,
            css: css.as_deref(),
            xpath: xpath.as_deref(),
            contains: contains.as_deref(),
            no_popups: *no_popups,
            popup_selector: popup_selector.as_deref(),
            none: *none,
            newest_alive: *newest_alive,
            timeout: *timeout,
        }),
        Command::GetClipboard => get_clipboard_cmd(),
        Command::SetClipboard { text } => set_clipboard_cmd(text),
        Command::Wait {
            element,
            window,
            gone,
            idle,
            property,
            contains,
            stable,
            app,
            timeout,
        } => wait_cmd(
            element.as_deref(),
            window.as_deref(),
            gone.as_deref(),
            *idle,
            property.as_deref(),
            *contains,
            *stable,
            app.as_deref(),
            *timeout,
        ),
        Command::Batch { json } => batch_cmd(json),
        Command::BrowserCollect {
            session,
            css,
            xpath,
            property,
            deep,
            interval_ms,
            max_rounds,
            max_items,
            timeout,
        } => browser::collect_cmd(
            session,
            css.as_deref(),
            xpath.as_deref(),
            property.as_deref(),
            *deep,
            *interval_ms,
            *max_rounds,
            *max_items,
            *timeout,
        ),
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
            let _ = std::io::Write::flush(&mut std::io::stdout());
            // 成功路径硬退出(与下方错误路径对称):析构链上有无界阻塞面——
            // thread_local 定位缓存与栈上的 UIElement COM 代理,其 Drop 是
            // 对目标应用的跨进程 Release(提供程序不泵消息即无限阻塞,
            // 尾段挂起族根因,2026-10-06 定位);SignalSession 的 worker
            // join 同为无界。状态落盘不依赖析构(finish(phase) 已显式调用、
            // Journal 每事件即 flush、stop::Scope 仅清 TLS);进程死亡时
            // OS 断连清理 COM 通道是 providers 必须容忍的正常路径。
            // 2026-10-11 升级:process::exit 走 CRT 清理,13112 签名实证
            // 它到不了 OS(envelope 落盘后进程 CPU 恒零赖活数小时)——
            // envelope flush 后直接 TerminateProcess,尾段挂起族根杀。
            actl_uia::watchdog::hard_exit_after_envelope(0);
        }
        Err(mut err) => {
            // 单命令失败轻量采集(2026-10-06 基建①:flow-evidence 的下沉——
            // 直连 CLI 失败原本只有错误信封,诊断材料全靠人手工补采):
            // INTERNAL 级失败自动落一份窗口清单+失败上下文快照,evidence 附
            // 路径。采集自身失败静默跳过(诊断旁路不得影响错误语义)。
            if err.code == actl_core::ErrorCode::Internal
                && let Some(path) = failure_capture(command_name)
            {
                let evidence = err.evidence.get_or_insert_with(|| json!({}));
                evidence["failure_capture"] = json!(path);
            }
            let env = ErrorEnvelope::new(command_name, &err, duration_ms);
            println!(
                "{}",
                serde_json::to_string(&env).expect("envelope serializes")
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            // 退出码契约(06 §4):PROTOCOL 不可恢复 → 2;其余业务失败 → 1。
            // 同成功路径:flush 后 TerminateProcess,不进 CRT 清理(挂起面)。
            let code = if err.code == actl_core::ErrorCode::Protocol {
                2
            } else {
                1
            };
            actl_uia::watchdog::hard_exit_after_envelope(code);
        }
    }
}

/// INTERNAL 失败的轻量采集包:原生窗口清单 + 失败上下文(前台/目标窗/标题
/// 分类)。纯本地采集(观察失败时 UIA 通道不可信——原生 Win32 走 actl-uia
/// 的 native_windows);写 `LOCALAPPDATA/actl/failures/<时间戳>.json`。
fn failure_capture(command_name: &str) -> Option<String> {
    let dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .map(|base| base.join("actl").join("failures"))?;
    std::fs::create_dir_all(&dir).ok()?;
    let stamp = actl_core::state::unix_ms();
    let path = dir.join(format!("{stamp}-{command_name}.json"));
    let windows = actl_uia::native_windows()
        .ok()
        .and_then(|rows| {
            serde_json::to_value(
                rows.iter()
                    .map(|w| {
                        json!({"title":w.title,"class":w.class,"pid":w.pid,
                               "hwnd":w.hwnd,"visible":w.visible,"foreground":w.foreground})
                    })
                    .collect::<Vec<_>>(),
            )
            .ok()
        })
        .unwrap_or_else(|| json!("unavailable"));
    let bundle = json!({
        "command": command_name,
        "captured_ms": stamp,
        "windows": windows,
        "context": actl_uia::diagnostic::failure_context(),
    });
    serde_json::to_vec(&bundle).ok().and_then(|bytes| {
        std::fs::write(&path, bytes).ok()?;
        Some(path.to_string_lossy().into_owned())
    })
}
