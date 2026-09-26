//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5)。

use std::time::Instant;

use actl_core::{CtlError, Envelope, ErrorEnvelope, SnapshotBuilder};
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};

#[derive(Parser)]
#[command(
    name = "actl",
    version,
    about = "actl — desktop hands for AI agents: UIA-driven Windows control via atomic CLI commands"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List every command with its argument schema (machine-readable self-description)
    Commands,
    /// Report version, protocol version and platform info
    Status,
    /// Capture the interactive element tree of a window as refs (@e1..)
    Snapshot {
        /// Target window by title substring (default: focused window)
        #[arg(long)]
        app: Option<String>,
        /// Keep only interactive elements and their ancestor chain (token saver)
        #[arg(long)]
        skeleton: bool,
    },
}

/// 命令执行产物:data 进 envelope,`snapshot_id` 存在时附加到顶层(06 §4)。
struct CommandOutput {
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
    let cli = Cli::parse();
    let command_name = command_name(&cli.command);
    let started = Instant::now();

    let result = match cli.command {
        Command::Commands => commands().map(Into::into),
        Command::Status => status().map(Into::into),
        Command::Snapshot { app, skeleton } => snapshot(app.as_deref(), skeleton),
    };

    let duration_ms = started.elapsed().as_millis() as u64;
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
            std::process::exit(1);
        }
    }
}

fn command_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Commands => "commands",
        Command::Status => "status",
        Command::Snapshot { .. } => "snapshot",
    }
}

/// L2 自发现(06 §3.5):从 clap 命令树同源生成 schema,不手写命令清单。
fn commands() -> Result<Value, CtlError> {
    let root = Cli::command();
    let mut list = Vec::new();
    for sub in root.get_subcommands() {
        if sub.get_name() == "help" {
            continue; // clap 自动生成的 help 不是命令面的一部分
        }
        let args: Vec<Value> = sub
            .get_arguments()
            .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
            .map(|a| {
                json!({
                    "name": a.get_id().as_str(),
                    "help": a.get_help().map(|h| h.to_string()),
                })
            })
            .collect();
        list.push(json!({
            "name": sub.get_name(),
            "about": sub.get_about().map(|h| h.to_string()),
            "args": args,
        }));
    }
    Ok(json!({ "commands": list }))
}

fn status() -> Result<Value, CtlError> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": actl_core::ENVELOPE_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

fn snapshot(app: Option<&str>, skeleton: bool) -> Result<CommandOutput, CtlError> {
    let cap = actl_uia::capture(app)?;
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    let out = builder.finish(skeleton);
    let data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "truncated": cap.truncated || out.truncated,
        "elements": serde_json::to_value(&out.elements).expect("elements serialize"),
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}
