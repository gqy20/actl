//! xtask —— 仓库任务入口:`cargo xtask setup|gate|fmt`
//!
//! Rust 门禁单点；详情页检查另需 Node + pnpm，无需 make/python。
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
        Some("dev") => dev(),
        Some("evidence") => evidence(),
        Some("doctor") => doctor(std::env::args().nth(2).as_deref()),
        Some("fmt") => code(run(&["fmt", "--all"])),
        Some("regress") => regress(),
        Some("timing") => timing(),
        Some("preview-tests") => preview_tests(),
        Some("fixture" | "diagnose" | "package" | "install") => devtools(),
        Some("lint-packs") => lint_packs(std::env::args().nth(2).as_deref()),
        Some(_) | None => {
            eprintln!(
                "tasks:\n  \
                 cargo xtask setup   install git hooks + activate pinned toolchain (run once after clone)\n  \
                 cargo xtask dev     diagnosis loop: incremental DEBUG build of actl (seconds; never --release here)\n  \
                 cargo xtask evidence  evidence round: RELEASE build + binary fingerprint logged to target/evidence/\n  \
                 cargo xtask doctor [--clean]  one-shot diagnosis: process census (live/ghost), build-artifact locks, ghost images, display status; --clean also deletes ghost leftover images\n  \
                 cargo xtask gate    quality gate: file-lines + pnpm UI verify + fmt --check + clippy -D warnings + tests (= pre-push = CI)\n  \
                 cargo xtask fmt     format all code
                 cargo xtask fixture CASE | diagnose KIND | package [OPTIONS] | install --label ID (--zip PATH | --release)
                 cargo xtask regress [--list | --suite flow|system|scenario | --case ID] [--unattended] (native Rust)
  \
                 cargo xtask preview-tests  serially run ignored preview_window live tests (interactive desktop)
  \
                 cargo xtask timing profile|sweep  latency picture / ACTL_* knob sweep (interactive desktop)
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

/// 诊断循环入口(AGENTS 验收纪律:双循环二进制纪律)。
/// 增量 DEBUG 构建 actl(实测 ~4s vs release 全量 ~150s,40× 差距)——
/// 诊断/试错一律用 `target/debug/actl.exe`;**禁止在诊断循环里 --release**:
/// 证据二进制由 `cargo xtask evidence` 单独产出(显式声明证据轮)。
fn dev() -> ExitCode {
    let started = std::time::Instant::now();
    ghost_locked_exes(&["debug"]);
    let ok = run(&["build", "-p", "actl-cli"]);
    eprintln!("· dev build took {}ms", started.elapsed().as_millis());
    if ok {
        eprintln!(
            "✓ 诊断二进制: target\\debug\\actl.exe (诊断循环专用;最终验收/记录哈希走 `cargo xtask evidence`)"
        );
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 证据轮入口:RELEASE 构建 + 二进制指纹(FNV-1a 双通道 128 位,与
/// handoff::plan_fingerprint/flow 源模板指纹同算法族——仓库指纹惯例)追加
/// 落盘 `target/evidence/binary-log.txt`(时间戳+指纹+大小),stdout 侧不打
/// 噪音。每轮验收只调用一次;调用本身即"进入证据轮"的显式声明。
fn evidence() -> ExitCode {
    let started = std::time::Instant::now();
    ghost_locked_exes(&["release"]);
    let ok = run(&["build", "--release", "-p", "actl-cli"]);
    eprintln!("· release build took {}ms", started.elapsed().as_millis());
    if !ok {
        return ExitCode::FAILURE;
    }
    let binary = PathBuf::from("target").join("release").join("actl.exe");
    let Some((fingerprint, size)) = binary_fingerprint(&binary) else {
        eprintln!("✗ 证据二进制不可读: {}", binary.display());
        return ExitCode::FAILURE;
    };
    let record = format!(
        "{} fnv128={} size={}\n",
        chrono_like_now(),
        fingerprint,
        size
    );
    let log = PathBuf::from("target")
        .join("evidence")
        .join("binary-log.txt");
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .and_then(|mut file| {
            use std::io::Write;
            file.write_all(record.as_bytes())
        }) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("✗ 证据日志写入失败: {error}");
            return ExitCode::FAILURE;
        }
    }
    eprintln!(
        "✓ 证据二进制: {} (fnv128={} size={} 已记录 {})",
        binary.display(),
        fingerprint,
        size,
        log.display()
    );
    ExitCode::SUCCESS
}

/// FNV-1a 双通道 128 位指纹 + 文件大小(与 flow.rs bytes_fingerprint 同族)。
fn binary_fingerprint(path: &Path) -> Option<(String, u64)> {
    let bytes = std::fs::read(path).ok()?;
    let (mut h1, mut h2) = (0xcbf2_9ce4_8422_2325u64, 0x9e37_79b9_7f4a_7c15u64);
    for &byte in &bytes {
        h1 = (h1 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        h2 = (h2 ^ u64::from(byte).rotate_left(7)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some((format!("{h1:016x}{h2:016x}"), bytes.len() as u64))
}

/// 本地时间戳(无外部依赖:从系统时间换算 UTC 即可——证据日志只需可排序)。
fn chrono_like_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // UTC 日期换算(civil-from-days 算法,无时区依赖;证据排序够用)
    let days = secs / 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    let rem = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - 365 * yoe - yoe / 4 - yoe / 100;
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
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
/// 各步耗时随 `· <step> Nms` 打进 stderr(观测先行:慢环节按 AGENTS 性能纪律头内比较)。
fn gate() -> ExitCode {
    let gate_started = std::time::Instant::now();
    // 测试步骤全量构建(含 debug 双二进制)——先规避锁,避免"拒绝访问"中断整轮门禁。
    ghost_locked_exes(&["debug"]);
    if !timed("file-lines", file_lines_gate) {
        return ExitCode::FAILURE;
    }
    if !timed("signal-ui", frontend_gate) {
        return ExitCode::FAILURE;
    }
    for (label, args) in [
        ("fmt", &["fmt", "--all", "--", "--check"][..]),
        (
            "clippy",
            &["clippy", "--all-targets", "--", "-D", "warnings"][..],
        ),
        ("test", &["test", "--workspace"][..]),
    ] {
        if !timed(label, || run(args)) {
            return ExitCode::FAILURE;
        }
    }
    if !timed("lint-packs", || lint_packs(None) == ExitCode::SUCCESS) {
        return ExitCode::FAILURE;
    }
    eprintln!(
        "✓ gate passed (file-lines + signal-ui + fmt + clippy + workspace tests + lint-packs) in {:.1}s",
        gate_started.elapsed().as_secs_f32()
    );
    ExitCode::SUCCESS
}

fn frontend_gate() -> bool {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/signal");
    let pnpm = if cfg!(windows) { "pnpm.cmd" } else { "pnpm" };
    for args in [&["install", "--frozen-lockfile"][..], &["verify"][..]] {
        match Command::new(pnpm).args(args).current_dir(&path).status() {
            Ok(status) if status.success() => {}
            Ok(_) => return false,
            Err(error) => {
                eprintln!("signal-ui: pnpm unavailable or failed: {error}");
                return false;
            }
        }
    }
    true
}

/// 步级计时:执行门禁单步并输出耗时(仅诊断,不影响结果)。
fn timed(label: &str, step: impl FnOnce() -> bool) -> bool {
    let started = std::time::Instant::now();
    let ok = step();
    eprintln!("· {label} took {} ms", started.elapsed().as_millis());
    ok
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

/// 时序画像/参数扫描(docs/timing-2026-09 方法论的原生固化):release 构建 + 转发
/// actl-devtools。需交互桌面且显示端就绪(写入动作经 display gate 预告,表面被盖
/// 即阻塞);记事本用标记文件隔离,发现既有记事本窗口/急停/锁屏/前台被占时
/// 整轮让行报阻塞(77)。
fn timing() -> ExitCode {
    if !build_pair() {
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

/// 目标预览真机回归(docs/46):preview_window 子树的交互用例默认 #[ignore],
/// 不进 gate/CI,由本命令显式串行运行(需交互桌面)。用例只创建/关闭自有
/// 预览窗口、不注入输入,但会读取真实停止状态:存在活动急停时先让行并按
/// 阻塞(77)退出。锁屏/遮挡不做预检——用例自身的中心点护栏会正确拒绝
/// (NOT_ACTIONABLE),verify-window 的可见窗口过滤看不到锁屏,标题启发式
/// 又跨 locale 脆弱,不在 xtask 重复造探测。mouse_selection_acceptance
/// 需要人工/computer-use 在 30s 内真实点击,默认跳过;单独验证时手动
/// `cargo test -p actl-uia --lib mouse_selection -- --ignored --test-threads=1`。
fn preview_tests() -> ExitCode {
    let ws_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if !run(&["build", "-p", "actl-cli", "--bin", "actl"])
        || !run(&["build", "-p", "actl-devtools"])
    {
        return ExitCode::FAILURE;
    }
    let actl = ws_root.join("target/debug/actl.exe");
    let devtools = ws_root.join("target/debug/actl-devtools.exe");
    // 让行预检:活动急停时不动桌面,整轮报阻塞而非逐用例失败
    if let Ok(output) = std::process::Command::new(&actl)
        .arg("execution-status")
        .env_remove("ACTL_TASK_ID")
        .output()
        && String::from_utf8_lossy(&output.stdout).contains("\"state\":\"stopped\"")
    {
        eprintln!("✗ preview-tests blocked: active emergency stop on this machine.");
        eprintln!("  Honor it; resolve via execution-recover, then rerun (AGENTS 暂停与停止分离).");
        return ExitCode::from(77);
    }
    eprintln!(
        "▶ preview_window live tests (serial, owned windows only; docs/46)\n  \
         ACTL_PREVIEW_CLI={}\n  \
         ACTL_DEVTOOLS={}",
        actl.display(),
        devtools.display()
    );
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let ok = timed("preview-window tests", || {
        Command::new(&cargo)
            .args([
                "test",
                "-p",
                "actl-uia",
                "--lib",
                "preview_window::tests",
                "--",
                "--ignored",
                "--test-threads=1",
                "--skip",
                "mouse_selection",
            ])
            .env("ACTL_PREVIEW_CLI", &actl)
            .env("ACTL_DEVTOOLS", &devtools)
            .env_remove("ACTL_TASK_ID")
            .status()
            .map(|s| s.success())
            .unwrap_or_else(|error| {
                eprintln!("preview-tests: {error}");
                false
            })
    });
    if !ok {
        eprintln!(
            "✗ preview-tests failed — 先查环境再怀疑代码(诊断三段法):锁屏/遮挡/受保护视图\n  \
             会让预览护栏正确拒绝(NOT_ACTIONABLE center covered);逐用例证据在\n  \
             target/preview-lifecycle、target/preview-choice。"
        );
    }
    code(ok)
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
    // commands 清单要的是**当前源码**的命令面:增量 debug 构建(无变更时
    // ~0.3s no-op;gate 的测试步已把产物编好)。不碰 target/validation 的
    // release 对——那是 regress/evidence 的稳定二进制专用,lint-packs 借用
    // 它曾是每轮 gate ~3-5 分钟 release 重建的根源(AGENTS 承诺的「秒级」
    // 被它吃掉,2026-10-12 三轮 gate 实测 lint-packs 占 58-74%)。
    if !run(&["build", "-p", "actl-cli"]) {
        return ExitCode::FAILURE;
    }
    // 走 CARGO_TARGET_DIR(2026-10-13 僵尸实证修复):主 target 的 actl.exe
    // 被挂死进程锁住镜像时(delete-pending 挂起,cargo 换不了产物),隔离
    // target 目录是开发循环的标准逃生通道——lint-packs 硬编码主路径会
    // 让整轮 gate 在此形态下结构性失败。子 cargo build 同环境变量,同目录。
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| ws_root.join("target"));
    let actl = target_dir.join("debug/actl.exe");
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
        // ⑤ 选择器形态建议(知识层,advisory 不计失败):name: 走全树扫描,
        // id: 走 provider 单调用快路径(2026-10-06 性能轮)——元素有稳定
        // AutomationId 时优先 id:;name: 子串语义保留给无 id 的元素。
        // ⑥ popup_ignores 块形态(降噪第二轮 pack 来源,2026-10-11):每条
        // 必须带非空 app+name_contains(与 plan 级最窄形态同规——空声明
        // =「忽略该 app 全部表面」,会把门禁掏空)。行级状态机与 CLI 注入
        // 同族解析。
        {
            let mut pending_app = false;
            let mut in_block = false;
            for line in text.lines() {
                if !in_block {
                    if line == "popup_ignores:" {
                        in_block = true;
                    }
                    continue;
                }
                if !line.starts_with(' ') {
                    break;
                }
                let value = |l: &str| -> String {
                    l.split_once(':')
                        .map(|(_, v)| v)
                        .unwrap_or("")
                        .trim()
                        .trim_matches('"')
                        .to_string()
                };
                if let Some(app) = line.strip_prefix("  - app:") {
                    if app.trim().trim_matches('"').is_empty() {
                        eprintln!("✗ {rel}: popup_ignores entry with empty app");
                        bad += 1;
                    }
                    pending_app = true;
                } else if line.trim_start().starts_with("name_contains:") && pending_app {
                    if value(line).is_empty() {
                        eprintln!("✗ {rel}: popup_ignores entry with empty name_contains");
                        bad += 1;
                    }
                    pending_app = false;
                }
            }
        }
        let mut name_selectors = 0usize;
        for line in text.lines() {
            let Some(start) = line.find("[\"") else {
                continue;
            };
            let rest = &line[start + 2..];
            let Some(cmd_end) = rest.find('"') else {
                continue;
            };
            let rest = rest[cmd_end + 1..].trim_start();
            let rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
            if let Some(sel) = rest.strip_prefix('"')
                && let Some(sel_end) = sel.find('"')
                && sel[..sel_end].starts_with("name:")
            {
                name_selectors += 1;
            }
        }
        if name_selectors > 0 {
            eprintln!(
                "· 提示 {rel}: {name_selectors} 处 name: 选择器——元素若有稳定 AutomationId 优先 id:(provider 快路径,单次跨进程调用;name: 子串语义保留给无 id 元素)"
            );
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

// ── 构建锁规避与一键诊断(2026-10-06;docs/81 §8 僵尸事故的日常摩擦面) ──
// 锁探针语义实证:运行中进程的映像,write-open 被拒(DELETE 共享拒绝是
// cargo "failed to remove file 拒绝访问"的根因);改名(ghost-rename)
// 始终可用——楔死进程攥住的是旧映像名,构建产物换新名即可重建。

/// 目标二进制:构建会被僵尸/显示端锁住的产物面。
fn builtin_exes(profiles: &[&str]) -> Vec<std::path::PathBuf> {
    profiles
        .iter()
        .flat_map(|p| {
            [
                std::path::PathBuf::from("target").join(p).join("actl.exe"),
                std::path::PathBuf::from("target")
                    .join(p)
                    .join("actl-signal.exe"),
            ]
        })
        .collect()
}

/// 锁探针:write-open 失败 = 有进程映射该映像(运行中或内核楔死滞留)。
fn image_locked(path: &std::path::Path) -> bool {
    if !path.exists() {
        return false;
    }
    std::fs::OpenOptions::new().write(true).open(path).is_err()
}

/// 构建前置:被锁产物自动 ghost-rename(改名不影响持锁进程——它攥旧名,
/// cargo 写新文件)。惯例沿用手工 ghost/ghost2/ghostN 先例;提示走 stderr。
fn ghost_locked_exes(profiles: &[&str]) {
    for exe in builtin_exes(profiles) {
        if !image_locked(&exe) {
            continue;
        }
        // with_extension 换掉最后一个扩展:actl.exe -> actl.exe.ghostN
        let mut n = 0;
        let target = loop {
            let candidate = exe.with_extension(format!(
                "{}.ghost{}",
                exe.extension().and_then(|e| e.to_str()).unwrap_or(""),
                n
            ));
            if !candidate.exists() {
                break candidate;
            }
            n += 1;
        };
        match std::fs::rename(&exe, &target) {
            Ok(()) => {
                eprintln!(
                    "· 构建锁规避: {} 被运行中/楔死进程持有,已改名 {} (旧映像随进程退出回收)",
                    exe.display(),
                    target.display()
                );
            }
            Err(error) => {
                eprintln!(
                    "! 锁规避失败 {} -> {}: {error}(手工改名后重试)",
                    exe.display(),
                    target.display()
                );
            }
        }
    }
}

/// ghost 残件映像清单:目标目录里被改名让路的旧产物——自动 ghost-rename
/// (.ghostN)与手工改名(live-/locked- 先例)同族。判定:actl 家族且名字含
/// `.exe.`(正规产物名不带二级后缀;actl-mcp.exe 等无关产物不误伤)。
fn ghost_images() -> Vec<PathBuf> {
    let mut ghosts = Vec::new();
    for dir in ["target/debug", "target/release", "target2/debug"] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("actl") && name.contains(".exe.") {
                    ghosts.push(entry.path());
                }
            }
        }
    }
    ghosts
}

/// 一键诊断:进程普查(活/幽灵分组)+ 四产物锁探针 + ghost 残留盘点 +
/// 显示端状态;`--clean` 顺带删除残件(仍被进程持有的如实保留,不硬删)。
/// 人类可读输出(诊断工具,不进 stdout 契约面)。
fn doctor(flag: Option<&str>) -> ExitCode {
    let clean = match flag {
        None => false,
        Some("--clean") => true,
        Some(other) => {
            eprintln!("未知参数 {other:?}:用法 cargo xtask doctor [--clean]");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("== actl 进程普查(活 = 未退出;幽灵 = HasExited 仍枚举,内核对象滞留) ==");
    let census = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Process actl, actl-signal -ErrorAction SilentlyContinue | ForEach-Object { \
             '{0,-12} pid={1,-7} exited={2,-5} start={3} path={4}' -f \
             $_.ProcessName, $_.Id, $_.HasExited, \
             $_.StartTime.ToString('MM-dd HH:mm:ss'), $_.Path }",
        ])
        .output();
    match census {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            // Get-Process 无匹配时 exit=1(非错误)——空输出即"无进程"。
            if text.trim().is_empty() {
                eprintln!("  (无 actl/actl-signal 进程)");
            } else {
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    eprintln!("  {line}");
                }
            }
        }
        Err(error) => eprintln!("  普查失败: {error}"),
    }
    eprintln!("== 构建产物锁探针(write-open 被拒 = 有进程映射) ==");
    for exe in builtin_exes(&["debug", "release"]) {
        let state = if !exe.exists() {
            "absent"
        } else if image_locked(&exe) {
            "LOCKED"
        } else {
            "free"
        };
        eprintln!("  {state:6} {}", exe.display());
    }
    eprintln!("== ghost 残留映像(被改名让路的旧产物,--clean 可清) ==");
    let ghosts = ghost_images();
    for ghost in &ghosts {
        eprintln!("  {}", ghost.display());
    }
    if ghosts.is_empty() {
        eprintln!("  (无)");
    }
    if clean {
        eprintln!("== ghost 残件清理(--clean) ==");
        for ghost in &ghosts {
            match std::fs::remove_file(ghost) {
                Ok(()) => eprintln!("  已删除 {}", ghost.display()),
                Err(error) => eprintln!(
                    "  仍被持有,留待进程退出/重启后清: {} ({error})",
                    ghost.display()
                ),
            }
        }
    }
    eprintln!("== 显示端状态 ==");
    let actl = std::path::Path::new("target/debug/actl.exe");
    if actl.exists() {
        match std::process::Command::new(actl)
            .arg("display-status")
            .output()
        {
            Ok(out) => {
                let line = String::from_utf8_lossy(&out.stdout);
                eprintln!("  {}", line.trim());
            }
            Err(error) => eprintln!("  display-status 失败: {error}"),
        }
    } else {
        eprintln!("  (target/debug/actl.exe 不存在;先 cargo xtask dev)");
    }
    eprintln!(
        "提示: 构建前自动规避锁 = cargo xtask dev/evidence/gate;残件清理 = cargo xtask doctor --clean(仍被持有的如实保留)。"
    );
    ExitCode::SUCCESS
}
