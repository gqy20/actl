//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! 参数面在 `cli`(clap 树,help 文本与其 doc comment 同源);命令执行体在
//! `interact`(操作类)与 `query`(观察/编排类);本文件只做入口、分发与 envelope 落盘。

mod browser;
mod cli;
mod cli_help;
mod condition;
mod decide;
mod file_check;
mod file_discover;
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

use crate::cli::{Cli, Command, DesktopCommand, app_of};
use crate::cli_help::command_name;
use crate::{interact::*, query::*};

/// panic 兜底(2026-10-14 日志盘点轮第二层):运行路径的 panic 兜底信封
/// ——硬规则 3「失败必须有错误码」的最后防线(此前 panic=裸 stderr+无信封,
/// 调用方 stdout 解析直接失败)。两阶段注册:main 第一行装 hook(越早越稳),
/// 此时命令未知 → "unknown";clap 解析出命令后 `PANIC_COMMAND.set` 精化。
/// hook 内不 unwind——析构链上有无界阻塞面(COM 代理跨进程 Release),
/// 直接 flush stderr 现场 + stdout INTERNAL 信封 + TerminateProcess
/// (与 finish 同纪律)。退出码 1(INTERNAL 族):三值契约(0/1/2)不变,
/// panic 判别靠 evidence.panic_location 与 stderr [actl-panic] 行。
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let command = actl_core::PANIC_COMMAND.get().copied().unwrap_or("unknown");
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        // panic 现场保 stderr(与信封同内容,人类第一现场);信封走 stdout。
        eprintln!("[actl-panic] {command} {location} {payload}");
        let err = CtlError::with_evidence(
            actl_core::ErrorCode::Internal,
            format!("panic: {payload}"),
            json!({"panic_location": location, "hint": "internal bug — never a caller error; report with the [actl-panic] stderr line"}),
        );
        let elapsed = actl_core::BOOT
            .get()
            .map(|b| b.elapsed().as_millis() as u64)
            .unwrap_or(0);
        let env = ErrorEnvelope::new(command, &err, elapsed);
        if let Ok(text) = serde_json::to_string(&env) {
            println!("{text}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        actl_uia::watchdog::hard_exit_after_envelope(1);
    }));
}

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
    // OnceLock 语义:首 set 生效即返回 Ok;重入(测试多进程不存在)是 Err,
    // 静默忽略——main 单进程一次,不存在竞争路径
    let _ = actl_core::BOOT.set(Instant::now());
    install_panic_hook();
    // 看门狗硬退出注入(剩余逃逸路径收口):击杀路径与 finish 同款
    // TerminateProcess——std::process::exit 的 CRT 清理在主线程持锁
    // 卡死时同样会卡,防挂死机制不得继承挂死模式(13112 签名实证)。
    actl_core::watchdog::install_hard_exit(actl_uia::watchdog::hard_exit_after_envelope);
    // 坐标系不变量先行(docs/83 补遗,2026-10-13):整进程 PMv2 物理像素。
    // 必须在 clap 解析与任何 UIA/DC 调用之前(设置过窗口后被拒)。
    actl_uia::ensure_process_pmv2();
    // clap 树注入 after_help 后再 parse(Example/Errors 进 -h;06 §3.5)。
    // 解析失败信封化(2026-10-14 逃逸路径③):--help/--version 是元命令例外
    // (exit 0 人类文本),其余解析错误转 PROTOCOL 信封 + exit 2——与运行路径
    // 校验层(image-regions 坏 rect 等)行为一致,stdout 不变量全路径成立
    // (此前 clap 错误=裸 stderr+stdout 空,消费者解析落空)。try_get_matches
    // 不内嵌 exit,错误回到本层转信封;错误渲染进 message(stderr 由 clap
    // 不再自动打印——信封 message 已携带,双流仍完整)。
    let matches = match crate::cli_help::build_cli().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            use clap::error::ErrorKind;
            match error.kind() {
                ErrorKind::DisplayHelp
                | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                | ErrorKind::DisplayVersion => error.exit(),
                _ => {
                    let mut tree = crate::cli_help::build_cli();
                    let rendered = error.format(&mut tree).to_string();
                    let err = CtlError::protocol(rendered);
                    finish("unknown", Err(err), 0);
                    unreachable!("finish hard-exits")
                }
            }
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let command_name = command_name(&cli.command);
    // panic 兜底第二阶段:hook 已在第一行,此处才知命令名(解析前 unknown)
    let _ = actl_core::PANIC_COMMAND.set(command_name);
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
    // 测试缝:制造确定性 panic,供兜底信封的契约测试使用(ACTL_TEST_PANIC
    // 存在即触发;非测试环境不设此变量)。
    if std::env::var_os("ACTL_TEST_PANIC").is_some() {
        panic!("test panic seam: ACTL_TEST_PANIC is set");
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
            | Command::PreviewMap { .. }
            | Command::ImageRegions { .. }
            | Command::Decide { .. }
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
        Command::PreviewMap { app, duration } => actl_uia::map::map(app, *duration).map(Into::into),
        Command::ImageRegions {
            app,
            rect,
            threshold,
            min_cells,
            density,
            limit,
            motion,
        } => actl_uia::image_regions::image_regions(actl_uia::image_regions::ImageRegionsSpec {
            app: app.as_deref(),
            rect: rect.as_deref(),
            threshold: *threshold,
            min_cells: *min_cells,
            density: *density,
            limit: *limit,
            motion: *motion,
        })
        .map(Into::into),
        Command::Flow(command) => flow::dispatch(command).map(Into::into),
        Command::Record(command) => record::dispatch(command).map(Into::into),
        Command::VerifyWindow { app, state } => {
            actl_uia::window_check::verify_window(app, state).map(Into::into)
        }
        Command::VerifyClosed {
            hwnd,
            pid,
            instance,
        } => file_check::closed(hwnd, *pid, instance.as_deref()).map(Into::into),
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
            discover,
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
            file_check::verify(path, kind, &expectation, *subset, discover.as_deref())
                .map(Into::into)
        }
        Command::Commands => commands().map(Into::into),
        Command::Status => status().map(Into::into),
        Command::Decide {
            card,
            run,
            task,
            config,
            endpoint,
            min_confidence,
            timeout_ms,
            dry_run,
        } => decide::run(decide::DecideArgs {
            card: card.clone(),
            run: run.clone(),
            task: task.clone(),
            config: config.clone(),
            endpoint: endpoint.clone(),
            min_confidence: *min_confidence,
            timeout_ms: *timeout_ms,
            dry_run: *dry_run,
        })
        .map(Into::into),
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
        Command::Desktop(DesktopCommand::FrontReport { app, app_flag }) => {
            // docs/06 契约是 --app 形态;位置参数为既有调用兼容。
            let anchor = app_flag.as_deref().or(app.as_deref()).ok_or_else(|| {
                actl_core::CtlError::protocol(
                    "front-report needs a window anchor (--app <窗口> or positional)",
                )
            })?;
            actl_uia::front_report(anchor).map(Into::into)
        }
        Command::Desktop(DesktopCommand::ResidueReport) => {
            actl_uia::residue_report().map(Into::into)
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
        Command::Desktop(DesktopCommand::MaximizeWindow { pattern, pid, hwnd }) => {
            actl_uia::desktop::set_maximized(window_ref_of(
                pattern.as_deref(),
                *pid,
                hwnd.as_deref(),
            )?)
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
            wrect,
            element,
            sapp,
            snear,
            out,
        } => screenshot_cmd(
            app.as_deref(),
            hwnd.as_deref(),
            crate::interact::ShotDomain {
                rect: rect.as_deref(),
                wrect: wrect.as_deref(),
                element: element.as_deref(),
            },
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
            verify_value,
        } => set_value_cmd(
            target,
            value,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            verify_value.as_deref(),
        ),
        Command::RightClick {
            target,
            app,
            near,
            snapshot,
            no_activate,
        } => pointer_cmd(
            "right-click",
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *no_activate,
            PointerKind::Right,
        ),
        Command::DoubleClick {
            target,
            app,
            near,
            snapshot,
            no_activate,
        } => pointer_cmd(
            "double-click",
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *no_activate,
            PointerKind::Double,
        ),
        Command::MiddleClick {
            target,
            app,
            near,
            snapshot,
            no_activate,
        } => pointer_cmd(
            "middle-click",
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *no_activate,
            PointerKind::Middle,
        ),
        Command::TripleClick {
            target,
            app,
            near,
            snapshot,
            no_activate,
        } => pointer_cmd(
            "triple-click",
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
            *no_activate,
            PointerKind::Triple,
        ),
        Command::Hover {
            target,
            app,
            near,
            snapshot,
            no_activate,
        } => pointer_cmd(
            "hover",
            target,
            app.as_deref(),
            near.as_deref(),
            snapshot.as_deref(),
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
            human,
            seed,
        } => drag_cmd(
            target,
            to,
            path,
            app.as_deref(),
            near.as_deref(),
            to_near.as_deref(),
            *human,
            *seed,
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
            hwnd,
            width,
            height,
            x,
            y,
        } => resize_window_cmd(pattern.as_deref(), hwnd.as_deref(), *width, *height, *x, *y),
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
            property,
            app,
            limit,
        } => find_cmd(
            target.as_deref(),
            name.as_deref(),
            role.as_deref(),
            id.as_deref(),
            capable.as_deref(),
            property.as_deref(),
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
            target,
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
            target.as_deref(),
            *timeout,
        ),
        Command::BrowserType {
            session,
            css,
            xpath,
            text,
            iframe,
            target,
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
            target.as_deref(),
        ),
        Command::BrowserClick {
            session,
            css,
            xpath,
            iframe,
            expect_text,
            target,
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
            target.as_deref(),
        ),
        Command::BrowserClose { session, timeout } => browser::close_cmd(session, *timeout),
        Command::BrowserList => browser::list_cmd(),
        Command::BrowserTargets { session } => browser::targets_cmd(session),
        Command::BrowserScreenshot {
            session,
            css,
            xpath,
            iframe,
            target,
            out,
            timeout,
        } => browser::screenshot_cmd(browser::ScreenshotArgs {
            session,
            css: css.as_deref(),
            xpath: xpath.as_deref(),
            iframe: *iframe,
            target: target.as_deref(),
            out: out.as_deref(),
            timeout: *timeout,
        }),
        Command::BrowserDrag {
            session,
            from,
            to,
            human,
            seed,
            target,
            timeout,
        } => browser::drag_cmd(browser::DragArgs {
            session,
            from,
            to,
            human: *human,
            seed: *seed,
            target: target.as_deref(),
            timeout: *timeout,
        }),
        Command::ChallengeSolve {
            session,
            track_css,
            piece_css,
            handle_css,
            success_css,
            template,
            threshold,
            attempts,
            no_human,
            seed,
            iframe,
            target,
            timeout,
        } => browser::solve_cmd(browser::SolveArgs {
            session,
            track_css,
            piece_css,
            handle_css,
            success_css,
            template: template.as_deref(),
            threshold: *threshold,
            attempts: *attempts,
            human: !*no_human,
            seed: *seed,
            iframe: *iframe,
            target: target.as_deref(),
            timeout: *timeout,
        }),
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
        Command::AppLocate { name, register } => {
            let mut result = actl_uia::app_locate::locate(name)?;
            if *register {
                // manual 保护:用户显式指定过的条目,自动发现不覆盖——
                // 别名仍解析(指向用户路径),envelope 如实注记 kept_manual
                if let Ok(Some(existing)) = actl_uia::app_registry::manual_entry(name) {
                    result
                        .as_object_mut()
                        .expect("locate result is an object")
                        .insert("kept_manual".into(), serde_json::json!(existing));
                } else {
                    let first = result
                        .get("first")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            CtlError::internal("locate returned no candidate to register")
                        })?
                        .to_string();
                    actl_uia::app_registry::register(name, &first, false)?;
                    result
                        .as_object_mut()
                        .expect("locate result is an object")
                        .insert("registered".into(), serde_json::json!(name));
                }
            }
            Ok(result.into())
        }
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
            challenge,
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
            challenge: *challenge,
            none: *none,
            newest_alive: *newest_alive,
            timeout: *timeout,
        }),
        Command::GetClipboard => get_clipboard_cmd(),
        Command::SetClipboard { text } => set_clipboard_cmd(text),
        Command::ClipboardSave { id } => clipboard_save_cmd(id),
        Command::ClipboardRestore { id } => clipboard_restore_cmd(id),
        Command::ClipboardFormats => clipboard_formats_cmd(),
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
        Command::BrowserNetlog {
            session,
            filter,
            window,
            max_entries,
        } => browser::netlog_cmd(session, filter.as_deref(), *window, *max_entries),
    }
}

fn finish(command_name: &'static str, result: Result<CommandOutput, CtlError>, duration_ms: u64) {
    // 资源快照(诊断旁路,信封前一行):CPU-bound(扫描)与纯等待(睡眠)
    // 的定性数据;WorkingSet 峰值给像素命令族的内存剖面归因。trace 无
    // 消费者时跳过采样(两次系统调用省掉)。
    if actl_core::trace::routed()
        && let Ok(snap) = actl_uia::resource::self_snapshot()
    {
        actl_core::trace::resource(&snap);
    }
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
