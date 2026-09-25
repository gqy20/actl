//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5)。

use std::time::Instant;

use actl_core::{CtlError, Envelope, ErrorEnvelope};
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
}

fn main() {
    let cli = Cli::parse();
    let command_name = command_name(&cli.command);
    let started = Instant::now();

    let result = match cli.command {
        Command::Commands => commands(),
        Command::Status => status(),
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(data) => {
            let env = Envelope::success(command_name, Some(data), duration_ms);
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
