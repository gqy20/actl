//! xtask —— 仓库任务入口:`cargo xtask setup|gate|fmt`
//!
//! 存在的意义:新 clone 者唯一前置依赖 = git + rustup(全平台一致,无需 make/python)。
//! `gate` 是质量门禁的**唯一实现**(fmt-check + clippy + test),pre-push hook 与 CI 都调用它,
//! 门禁定义从此单点维护(AGENTS.md 工作流)。

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("setup") => setup(),
        Some("gate") => gate(),
        Some("fmt") => code(run(&["fmt", "--all"])),
        Some(_) | None => {
            eprintln!(
                "tasks:\n  \
                 cargo xtask setup   install git hooks + activate pinned toolchain (run once after clone)\n  \
                 cargo xtask gate    quality gate: fmt --check + clippy -D warnings + tests (= pre-push = CI)\n  \
                 cargo xtask fmt     format all code"
            );
            ExitCode::FAILURE
        }
    }
}

fn code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 一步配置:启用本仓库 git hooks + 触发 rustup 按 rust-toolchain.toml 安装工具链。
fn setup() -> ExitCode {
    let ok = run(&["fmt", "--version"])
        && step("git", &["config", "core.hooksPath", "hooks"])
        && step("rustup", &["show", "active-toolchain"]);
    if ok {
        eprintln!("✓ setup done — run 'cargo xtask gate' to verify the toolchain.");
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 质量门禁(唯一定义点):顺序执行,首个失败即停。
fn gate() -> ExitCode {
    for args in [
        &["fmt", "--all", "--", "--check"][..],
        &["clippy", "--all-targets", "--", "-D", "warnings"][..],
        &["test", "--workspace"][..],
    ] {
        if !run(args) {
            return ExitCode::FAILURE;
        }
    }
    eprintln!("✓ gate passed (fmt + clippy + test)");
    ExitCode::SUCCESS
}

/// 以 $CARGO(或 PATH 中的 cargo)执行子命令,继承 stdio;返回是否成功。
fn run(args: &[&str]) -> bool {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    step(&cargo, args)
}

fn step(program: &str, args: &[&str]) -> bool {
    eprintln!("▶ {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {program}: {e}"));
    status.success()
}
