//! xtask —— 仓库任务入口:`cargo xtask setup|gate|fmt`
//!
//! 存在的意义:新 clone 者唯一前置依赖 = git + rustup(全平台一致,无需 make/python)。
//! `gate` 是质量门禁的**唯一实现**(file-lines + fmt-check + clippy + test),
//! pre-push hook 与 CI 都调用它,门禁定义从此单点维护(AGENTS.md 工作流)。

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// 单文件行数上限(整文件计数,含注释与空行):> WARN 警告,> DENY 门禁失败。
/// 依据:社区惯例手写 Rust 非测试代码 ~400–800 行/文件为宜,超过即拆分信号
/// (serde/clap 等一线 crate 中位数 150–220 行,仅保留个别 2–5k 聚合核心)。
/// 扫描范围 = `crates/*/src`(含 src/bin);examples/tests/benches 不扫;
/// 将来若引入生成代码,须在此登记豁免路径,不得静默放宽。
const FILE_LINES_WARN: usize = 800;
const FILE_LINES_DENY: usize = 1500;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("setup") => setup(),
        Some("gate") => gate(),
        Some("fmt") => code(run(&["fmt", "--all"])),
        Some("regress") => regress(),
        Some("lint-packs") => lint_packs(),
        Some(_) | None => {
            eprintln!(
                "tasks:\n  \
                 cargo xtask setup   install git hooks + activate pinned toolchain (run once after clone)\n  \
                 cargo xtask gate    quality gate: file-lines + fmt --check + clippy -D warnings + tests (= pre-push = CI)\n  \
                 cargo xtask fmt     format all code
                 cargo xtask regress run all live fixtures sequentially (needs an interactive desktop)
  \n                 cargo xtask lint-packs validate skills/actl/apps/*.yml steps against the CLI schema"
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

/// 质量门禁(唯一定义点):顺序执行,首个失败即停。行数检查在最前(纯本地、最快失败)。
fn gate() -> ExitCode {
    if !file_lines_gate() {
        return ExitCode::FAILURE;
    }
    for args in [
        &["fmt", "--all", "--", "--check"][..],
        &["clippy", "--all-targets", "--", "-D", "warnings"][..],
        &["test", "--workspace"][..],
    ] {
        if !run(args) {
            return ExitCode::FAILURE;
        }
    }
    eprintln!("✓ gate passed (file-lines + fmt + clippy + test)");
    ExitCode::SUCCESS
}

/// 行数门禁:crates/*/src 下全部 .rs 文件逐个计数,超限按 WARN/DENY 分级上报。
/// 返回 false 当且仅当存在 > DENY 的文件。
fn file_lines_gate() -> bool {
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."); // crates/xtask → 仓库根
    let mut files = Vec::new();
    if let Ok(crates) = std::fs::read_dir(ws_root.join("crates")) {
        for c in crates.flatten() {
            let src = c.path().join("src");
            if src.is_dir() {
                collect_rs(&src, &mut files);
            }
        }
    }
    files.sort();

    let mut over: Vec<(usize, String)> = Vec::new();
    for f in &files {
        let lines = std::fs::read_to_string(f)
            .unwrap_or_default()
            .lines()
            .count();
        if lines > FILE_LINES_WARN {
            let rel = f
                .strip_prefix(&ws_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| f.display().to_string());
            over.push((lines, rel));
        }
    }
    over.sort();
    over.reverse(); // 行数降序,最大者置顶

    eprintln!(
        "▶ file-lines ({} files, warn >{}, deny >{})",
        files.len(),
        FILE_LINES_WARN,
        FILE_LINES_DENY
    );
    for (lines, rel) in over.iter().filter(|(l, _)| *l > FILE_LINES_DENY) {
        eprintln!("✗ {rel}: {lines} lines (>{FILE_LINES_DENY}) — split the module");
    }
    for (lines, rel) in over.iter().filter(|(l, _)| *l <= FILE_LINES_DENY) {
        eprintln!("⚠ {rel}: {lines} lines (>{FILE_LINES_WARN}) — consider splitting");
    }
    over.iter().all(|(l, _)| *l <= FILE_LINES_DENY)
}

/// 递归收集目录下全部 .rs 文件路径。
fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
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

/// 全量回归入口:release 构建 + 顺序跑全部 live fixtures,汇总 PASS/FAIL。
/// 与 gate 的分工:gate = 离线质量门禁(CI 安全);regress = 真机行为回归
/// (需要交互桌面;金任务含键盘注入,桌面需空闲——并发打字会如实报 FAIL,
/// 这是护栏在工作,不是误报)。fixture 各自负责应用的启停与清理。
fn regress() -> ExitCode {
    if !run(&["build", "--release", "-p", "actl-cli", "--bins"]) {
        return ExitCode::FAILURE;
    }
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = [
        "golden-task-1.sh",       // 计算器(3 轮;正式验收 ACTL_RUNS? RUNS=10)
        "golden-task-2.sh",       // 记事本全链(注入,需桌面空闲)
        "us1-cross-app.sh",       // 跨 3 应用数据搬运
        "virtualized-list.sh",    // Explorer 条目级(L4)
        "win32-table-extract.sh", // msinfo32 报表提取
        "edge-deep-tree.sh",      // Chromium 深树 + 正文 + skeleton
    ];
    // Windows 上裸 "bash" 可能解析到 System32 的 WSL bash(fixture 用 git-bash 语义,
    // 脚本会全线失败)——显式探测 Git Bash;ACTL_BASH 可覆盖。
    let bash = std::env::var("ACTL_BASH").unwrap_or_else(|_| {
        for p in [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ] {
            if Path::new(p).exists() {
                return p.to_string();
            }
        }
        "bash".into()
    });
    let mut failed = Vec::new();
    for f in fixtures {
        let path = ws_root.join("fixtures").join(f);
        eprintln!("▶ {f}");
        let ok = step(&bash, &[path.to_string_lossy().as_ref()]);
        if !ok {
            failed.push(f);
        }
    }
    if failed.is_empty() {
        eprintln!("✓ regress: all fixtures green");
        ExitCode::SUCCESS
    } else {
        eprintln!("✗ regress failed: {}", failed.join(", "));
        ExitCode::FAILURE
    }
}

/// 应用档案 pack 校验(零依赖,不引 YAML 解析器——xtask 须离线可跑):
/// ① 结构:每 pack 必含 name/launch/window/routes 四键;
/// ② 语义:steps/unblock 数组的首 token(变量 {x} 先替换哑值)必须是真实
///    actl 命令——命令全集取自 `actl commands`(单一事实来源,防漂移)。
fn lint_packs() -> ExitCode {
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let packs_dir = ws_root.join("skills").join("actl").join("apps");
    let Ok(entries) = std::fs::read_dir(&packs_dir) else {
        eprintln!("✗ lint-packs: {} not found", packs_dir.display());
        return ExitCode::FAILURE;
    };
    if !run(&["build", "--release", "-p", "actl-cli", "--bins"]) {
        return ExitCode::FAILURE;
    }
    let actl = ws_root.join("target/release/actl.exe");
    // 命令全集(单一事实来源)
    let known: Vec<String> = match std::process::Command::new(&actl).arg("commands").output() {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.split("\"name\":\"")
                .skip(1)
                .map(|rest| rest.split('"').next().unwrap_or_default().to_string())
                .collect()
        }
        _ => {
            eprintln!("✗ lint-packs: cannot run `actl commands`");
            return ExitCode::FAILURE;
        }
    };
    let mut files = 0usize;
    let mut bad = 0usize;
    let mut checked = 0usize;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("yml") {
            continue;
        }
        files += 1;
        let rel = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // ① 结构
        for key in ["name:", "launch:", "window:", "routes:"] {
            if !text.contains(key) {
                eprintln!("✗ {rel}: missing `{key}`");
                bad += 1;
            }
        }
        // ② 步骤首 token(取每行中第一个 [" 之后的首个引号串)
        for line in text.lines() {
            let Some(start) = line.find("[\"") else {
                continue;
            };
            let rest = &line[start + 2..];
            let Some(end) = rest.find('"') else { continue };
            let cmd = &rest[..end];
            if cmd.starts_with('{') {
                continue; // 变量开头不是命令
            }
            checked += 1;
            if !known.iter().any(|k| k == cmd) {
                eprintln!("✗ {rel}: unknown command {cmd:?} in step");
                bad += 1;
            }
        }
    }
    if files == 0 {
        eprintln!("✗ lint-packs: no .yml packs found");
        return ExitCode::FAILURE;
    }
    if bad == 0 {
        eprintln!("✓ lint-packs: {files} packs, {checked} steps, all commands known");
        ExitCode::SUCCESS
    } else {
        eprintln!("✗ lint-packs: {bad} problem(s) across {files} packs");
        ExitCode::FAILURE
    }
}
