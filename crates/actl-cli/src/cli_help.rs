//! cli_help —— 帮助注入与命令名映射(从 cli.rs 按 file-lines 治理拆出)。
//! 带注入的 clap 树:-h/--help 渲染 after_help(Example/Errors);与
//! `Cli::parse()` 的差异仅此(参数校验逻辑完全同源)。

use clap::CommandFactory;

use crate::cli::{Cli, Command, DesktopCommand, FlowCommand, RecordCommand};
use crate::help_text::after_help_for;

pub(crate) fn build_cli() -> clap::Command {
    let mut root = Cli::command();
    for sub in root.get_subcommands_mut() {
        if let Some(text) = after_help_for(sub.get_name()) {
            let selector_help = if matches!(
                sub.get_name(),
                "find"
                    | "click"
                    | "get"
                    | "type"
                    | "set-value"
                    | "select"
                    | "toggle"
                    | "right-click"
                    | "double-click"
                    | "triple-click"
                    | "hover"
                    | "drag"
                    | "scroll"
                    | "extract"
                    | "press"
                    | "screenshot"
                    | "verify"
                    | "wait"
            ) {
                "\nStrict UIA target syntax: see `actl find --help`"
            } else {
                ""
            };
            *sub = std::mem::take(sub).after_help(format!("{text}{selector_help}"));
        }
    }
    root
}
/// envelope 的 command 字段(06 §4)。
pub(crate) fn command_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Commands => "commands",
        Command::Status => "status",
        Command::Decide { .. } => "decide",
        Command::Preview { .. } => "preview",
        Command::ImageRegions { .. } => "image-regions",
        Command::VerifyVisual { .. } => "verify-visual",
        Command::PreviewMap { .. } => "preview-map",
        Command::Desktop(DesktopCommand::DisplayStatus) => "display-status",
        Command::Desktop(DesktopCommand::ExecutionStatus) => "execution-status",
        Command::Desktop(DesktopCommand::FocusReport) => "focus-report",
        Command::Desktop(DesktopCommand::GetSelection { .. }) => "get-selection",
        Command::Desktop(DesktopCommand::FrontReport { .. }) => "front-report",
        Command::Desktop(DesktopCommand::ResidueReport) => "residue-report",
        Command::Desktop(DesktopCommand::ExecutionRecover) => "execution-recover",
        Command::Desktop(DesktopCommand::TaskArchive { .. }) => "task-archive",
        Command::Flow(FlowCommand::Validate { .. }) => "flow-validate",
        Command::Flow(FlowCommand::Run { .. }) => "flow-run",
        Command::Flow(FlowCommand::Status { .. }) => "flow-status",
        Command::Flow(FlowCommand::Evidence { .. }) => "flow-evidence",
        Command::Flow(FlowCommand::Pause { .. }) => "flow-pause",
        Command::Flow(FlowCommand::Resume { .. }) => "flow-resume",
        Command::Flow(FlowCommand::Revise { .. }) => "flow-revise",
        Command::Flow(FlowCommand::Clean { .. }) => "flow-clean",
        Command::Flow(FlowCommand::Writeback { .. }) => "explore-writeback",
        Command::Record(RecordCommand::Start { .. }) => "record-start",
        Command::Record(RecordCommand::Worker { .. }) => "record-worker",
        Command::Record(RecordCommand::Stop { .. }) => "record-stop",
        Command::Record(RecordCommand::Status { .. }) => "record-status",
        Command::Desktop(DesktopCommand::History { .. }) => "history",
        Command::Desktop(DesktopCommand::DisplayStart) => "display-start",
        Command::Desktop(DesktopCommand::DisplayStop { .. }) => "display-stop",
        Command::Desktop(DesktopCommand::MinimizeWindow { .. }) => "minimize-window",
        Command::Desktop(DesktopCommand::MaximizeWindow { .. }) => "maximize-window",
        Command::Desktop(DesktopCommand::RestoreWindow { .. }) => "restore-window",
        Command::Desktop(DesktopCommand::DesktopPrepare { .. }) => "desktop-prepare",
        Command::Desktop(DesktopCommand::DesktopRestore { .. }) => "desktop-restore",
        Command::Snapshot { .. } => "snapshot",
        Command::Click { .. } => "click",
        Command::Focus { .. } => "focus",
        Command::Get { .. } => "get",
        Command::Launch { .. } => "launch",
        Command::Type { .. } => "type",
        Command::Press { .. } => "press",
        Command::Paste { .. } => "paste",
        Command::FindImage { .. } => "find-image",
        Command::Screenshot { .. } => "screenshot",
        Command::SetValue { .. } => "set-value",
        Command::RightClick { .. } => "right-click",
        Command::DoubleClick { .. } => "double-click",
        Command::Hover { .. } => "hover",
        Command::MiddleClick { .. } => "middle-click",
        Command::TripleClick { .. } => "triple-click",
        Command::Drag { .. } => "drag",
        Command::Scroll { .. } => "scroll",
        Command::KeyDown { .. } => "key-down",
        Command::KeyUp { .. } => "key-up",
        Command::ResizeWindow { .. } => "resize-window",
        Command::Verify { .. } => "verify",
        Command::VerifyFile { .. } => "verify-file",
        Command::VerifyRows { .. } => "verify-rows",
        Command::VerifyOwner { .. } => "verify-owner",
        Command::VerifyClosed { .. } => "verify-closed",
        Command::VerifyWindow { .. } => "verify-window",
        Command::VerifyClipboard { .. } => "verify-clipboard",
        Command::Extract { .. } => "extract",
        Command::Select { .. } => "select",
        Command::Toggle { .. } => "toggle",
        Command::FocusWindow { .. } => "focus-window",
        Command::CloseWindow { .. } => "close-window",
        Command::ListWindows { .. } => "list-windows",
        Command::Find { .. } => "find",
        Command::Collect { .. } => "collect",
        Command::BrowserLaunch { .. } => "browser-launch",
        Command::BrowserNavigate { .. } => "browser-navigate",
        Command::BrowserRead { .. } => "browser-read",
        Command::BrowserCollect { .. } => "browser-collect",
        Command::BrowserNetlog { .. } => "browser-netlog",
        Command::BrowserType { .. } => "browser-type",
        Command::BrowserClick { .. } => "browser-click",
        Command::BrowserClose { .. } => "browser-close",
        Command::BrowserList => "browser-list",
        Command::BrowserTargets { .. } => "browser-targets",
        Command::BrowserScreenshot { .. } => "browser-screenshot",
        Command::BrowserDrag { .. } => "browser-drag",
        Command::ChallengeSolve { .. } => "challenge-solve",
        Command::AppRegister { .. } => "app-register",
        Command::AppList => "app-list",
        Command::AppLocate { .. } => "app-locate",
        Command::AppRemove { .. } => "app-remove",
        Command::BrowserVerify { .. } => "browser-verify",
        Command::GetClipboard => "get-clipboard",
        Command::SetClipboard { .. } => "set-clipboard",
        Command::ClipboardSave { .. } => "clipboard-save",
        Command::ClipboardRestore { .. } => "clipboard-restore",
        Command::ClipboardFormats => "clipboard-formats",
        Command::Wait { .. } => "wait",
        Command::Batch { .. } => "batch",
    }
}
