//! xtask —— 仓库任务入口:`cargo xtask setup|gate|fmt`
//!
//! 存在的意义:新 clone 者唯一前置依赖 = git + rustup(全平台一致,无需 make/python)。
//! `gate` 是质量门禁的**唯一实现**(file-lines + fmt-check + clippy + test + lint-packs),
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
        Some("fixture" | "diagnose" | "package") => devtools(),
        Some("lint-packs") => lint_packs(std::env::args().nth(2).as_deref()),
        Some(_) | None => {
            eprintln!(
                "tasks:\n  \
                 cargo xtask setup   install git hooks + activate pinned toolchain (run once after clone)\n  \
                 cargo xtask gate    quality gate: file-lines + fmt --check + clippy -D warnings + tests (= pre-push = CI)\n  \
                 cargo xtask fmt     format all code
                 cargo xtask fixture CASE | diagnose KIND | package [OPTIONS]
                 cargo xtask regress [--list | --suite flow|system|scenario | --case ID] [--unattended] (native Rust)
  \n                 cargo xtask lint-packs [dir] validate app packs against the CLI schema\n                 \
                 (default skills/actl/apps; pass a dir to lint out-of-repo private packs)"
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

/// 质量门禁(唯一定义点):顺序执行,首个失败即停。行数检查在最前(纯本地、最快失败);
/// pack 校验在最后(需先构建 release actl,失败成本最高故放末位)。
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
    if lint_packs(None) != ExitCode::SUCCESS {
        return ExitCode::FAILURE;
    }
    eprintln!("✓ gate passed (file-lines + fmt + clippy + workspace tests + lint-packs)");
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
fn build_pair() -> bool {
    run(&[
        "build",
        "--release",
        "--bin",
        "actl",
        "--bin",
        "actl-signal",
        "--target-dir",
        "target/validation",
    ])
}

fn regress() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(2).collect();
    if !args.iter().any(|arg| arg == "--list") && !build_pair() {
        return ExitCode::FAILURE;
    }
    devtools()
}
fn devtools() -> ExitCode {
    let mut command = Command::new("cargo");
    command.args(["run", "--quiet", "-p", "actl-devtools", "--"]);
    command.args(std::env::args().skip(1));
    match command.status() {
        Ok(status) => ExitCode::from(status.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(error) => {
            eprintln!("devtools: {error}");
            ExitCode::FAILURE
        }
    }
}
/// 路线收束校验:status 必填且取值合法(id 唯一性在收集时已查)。返回问题数。
fn check_route(rel: &str, id: &str, status: Option<&str>) -> usize {
    match status {
        Some("verified" | "manual" | "unverified") => 0,
        Some(other) => {
            eprintln!(
                "✗ {rel}: route {id:?} invalid status {other:?} (verified|manual|unverified)"
            );
            1
        }
        None => {
            eprintln!("✗ {rel}: route {id:?} missing `status:`");
            1
        }
    }
}

/// 应用档案 pack 校验(零依赖,不引 YAML 解析器——xtask 须离线可跑)。
/// 契约的单一事实来源是 skills/actl/PACKS.md(pack_format: 1):
/// ① 结构:必含 name/launch/window/app_version/verified_date/routes,pack_format=1;
/// ② 标识:quirks 与 routes 的 id 共用命名空间,全文件唯一;
/// ③ 信任:每条 route 必带 status ∈ {verified, manual, unverified};
/// ④ 语义:steps/unblock 数组的首 token(变量 {x} 先替换哑值)必须是真实
///    actl 命令——命令全集取自 `actl commands`(单一事实来源,防漂移)。
/// 目录参数指向仓库外时校验私有/企业 pack;默认 skills/actl/apps。
fn lint_packs(dir_arg: Option<&str>) -> ExitCode {
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let packs_dir = match dir_arg {
        Some(d) => PathBuf::from(d),
        None => ws_root.join("skills").join("actl").join("apps"),
    };
    let Ok(entries) = std::fs::read_dir(&packs_dir) else {
        eprintln!("✗ lint-packs: {} not found", packs_dir.display());
        return ExitCode::FAILURE;
    };
    if !build_pair() {
        return ExitCode::FAILURE;
    }
    let actl = ws_root.join("target/validation/release/actl.exe");
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
        // ① 结构:顶层必填键(注释行不算)
        for key in [
            "name:",
            "launch:",
            "window:",
            "app_version:",
            "verified_date:",
            "routes:",
        ] {
            let hit = text
                .lines()
                .any(|l| !l.trim_start().starts_with('#') && l.trim_start().starts_with(key));
            if !hit {
                eprintln!("✗ {rel}: missing `{key}`");
                bad += 1;
            }
        }
        let mut has_format = false;
        for l in text.lines() {
            let t = l.trim_start();
            if t.starts_with('#') {
                continue;
            }
            if let Some(v) = t.strip_prefix("pack_format:") {
                has_format = true;
                if v.trim() != "1" {
                    eprintln!("✗ {rel}: pack_format must be 1, got {:?}", v.trim());
                    bad += 1;
                }
            }
        }
        if !has_format {
            eprintln!("✗ {rel}: missing `pack_format:`");
            bad += 1;
        }
        // ②③ id 唯一性 + 路线 status(顶层 quirks:/routes: 分节的简单状态机)
        let mut ids: Vec<String> = Vec::new();
        let mut in_routes = false;
        let mut route: Option<(String, Option<String>)> = None;
        for l in text.lines() {
            let t = l.trim_start();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if !l.starts_with(' ') {
                // 顶层键:收束上一条 route;只有 routes: 节内的条目要求 status
                if let Some((id, status)) = route.take() {
                    bad += check_route(&rel, &id, status.as_deref());
                }
                in_routes = t.starts_with("routes:");
                continue;
            }
            if let Some(v) = t.strip_prefix("- id:") {
                let id = v.split('#').next().unwrap_or(v).trim().to_string();
                if ids.contains(&id) {
                    eprintln!("✗ {rel}: duplicate id {id:?}");
                    bad += 1;
                }
                ids.push(id.clone());
                if in_routes {
                    if let Some((prev_id, prev_status)) = route.take() {
                        bad += check_route(&rel, &prev_id, prev_status.as_deref());
                    }
                    route = Some((id, None));
                }
            } else if let Some(v) = t.strip_prefix("status:")
                && let Some(r) = route.as_mut()
            {
                r.1 = Some(v.split('#').next().unwrap_or(v).trim().to_string());
            }
        }
        if let Some((id, status)) = route.take() {
            bad += check_route(&rel, &id, status.as_deref());
        }
        // ④ 步骤首 token(取每行中第一个 [" 之后的首个引号串)
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
