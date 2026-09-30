//! timing-profile / timing-sweep:时序画像与参数扫描(docs/timing-2026-09 方法论的固化)。
//! 仪器 = envelope 自带的 duration_ms(零额外埋点);扫描通过 `ACTL_*` env 旋钮
//! 免重编译切档(crate actl-core/src/timing.rs 的预设消费者)。
//! 测量纪律:每命令 = 独立普通 actl 进程(无任务 env,capture::process 强制剥离
//! ACTL_TASK_ID/ACTL_TASK_CONFIG),与 2026-09-26 基线口径一致;绝对值仅同环境头内可比。
//! 清理纪律:一律优雅关闭(close-window --hwnd + verify-closed),禁 taskkill;
//! 记事本用标记文件名隔离,预检发现既有记事本窗口即阻塞让行(保留用户现场)。
//! 标题类目标(计算器/另存为/不保存)依赖 zh-CN 本地化——非中文系统如实报 err/未采集。

use crate::{Args, Result, capture, io, require};
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const KNOB_ENV: [&str; 14] = [
    "ACTL_POLL_MS",
    "ACTL_WAKE_BOUND_MS",
    "ACTL_PROBE_MS",
    "ACTL_FOCUS_RETRIES",
    "ACTL_PRESS_FOCUS_MS",
    "ACTL_TYPE_FOCUS_MS",
    "ACTL_SEGMENT_CHARS",
    "ACTL_MODIFIER_WAIT_MS",
    "ACTL_LOCK_WAIT_MS",
    "ACTL_SETTLE_MS",
    "ACTL_FAST_POLL_MS",
    "ACTL_READY_POLL_MS",
    "ACTL_VIS_PROBE_MS",
    "ACTL_HANDOFF_POLL_MS",
];

pub fn run(kind: &str, args: &Args) -> Result<Value> {
    let result = match kind {
        "profile" => profile(args),
        "sweep" => sweep(args),
        _ => return Err(CtlError::protocol("expected timing profile or sweep")),
    };
    // 预检让行(急停/显示端未就绪/既有记事本/锁屏)按 blocked 退出(77),与 regress 对齐:
    // 这是环境前置不满足,不是测量失败。
    match result {
        Ok(report) => Ok(report),
        Err(error) => {
            if error
                .evidence
                .as_ref()
                .is_some_and(|e| e["stage"] == "fixture_preflight")
            {
                Ok(json!({
                    "passed": false,
                    "status": "blocked",
                    "error": {"code": error.code, "message": error.message, "evidence": error.evidence}
                }))
            } else {
                Err(error)
            }
        }
    }
}

/// 普通调用台:capture::process 保证剥离任务 env(单命令口径),env 逐调用覆盖(扫描用)。
struct Bench {
    binary: PathBuf,
    dir: PathBuf,
    calls: usize,
    timeout: Duration,
}

impl Bench {
    fn new(args: &Args, label: &str) -> Result<Self> {
        let binary = PathBuf::from(
            args.value(
                "actl",
                crate::root()
                    .join("target/validation/release/actl.exe")
                    .to_string_lossy()
                    .as_ref(),
            ),
        );
        let dir = PathBuf::from(
            args.value(
                "output",
                crate::root()
                    .join("target")
                    .join(crate::id(label))
                    .to_string_lossy()
                    .as_ref(),
            ),
        );
        fs::create_dir_all(&dir).map_err(io)?;
        Ok(Self {
            binary: binary.canonicalize().map_err(io)?,
            dir,
            calls: 0,
            timeout: Duration::from_secs(30),
        })
    }
    fn call(&mut self, args: &[&str]) -> Result<Value> {
        self.call_env(args, &[])
    }
    /// 采样调用:失败样本计 ERR 而不是中断整轮(测量语义);关键路径(启动/清理)用 call + ?。
    fn sample(&mut self, args: &[&str]) -> Value {
        self.call(args).unwrap_or(Value::Null)
    }
    fn sample_env(&mut self, args: &[&str], env: &[(&str, &str)]) -> Value {
        self.call_env(args, env).unwrap_or(Value::Null)
    }
    fn call_env(&mut self, args: &[&str], env: &[(&str, &str)]) -> Result<Value> {
        self.calls += 1;
        let prefix = self.dir.join(format!("call-{:04}", self.calls));
        let owned: Vec<_> = args.iter().map(|s| s.to_string()).collect();
        let env: BTreeMap<String, String> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let record =
            capture::process(&self.binary, &owned, &self.dir, &prefix, &env, self.timeout)?;
        let mut v = capture::read_json(&prefix.with_extension("stdout.json"))?;
        if v["duration_ms"].is_null() {
            v["duration_ms"] = record["elapsed_ms"].clone();
        }
        Ok(v)
    }
    fn version(&self) -> String {
        Command::new(&self.binary)
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    }
}

fn preflight(bench: &mut Bench) -> Result<()> {
    let status = bench.call(&["execution-status"])?;
    if status["data"]["state"] == "stopped" {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "desktop stopped or needs repair; retain state",
            json!({"stage":"fixture_preflight"}),
        ));
    }
    // 写入动作全部经显示端门控(display_gate prepare_action):表面被盖/隐藏时
    // 每个 injecting 样本都会被拒,先快速失败并报出就绪原因(2026-09-30 实测
    // covered_or_offscreen 会让整轮样本无意义)。
    let display = bench.call(&["display-status"])?;
    if display["data"]["ready"] != true {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "display companion not ready; timing samples inject desktop input",
            json!({"stage":"fixture_preflight","reason":display["data"]["reason"]}),
        ));
    }
    let windows = bench.call(&["list-windows", "--native"])?;
    if windows["data"]["windows"].as_array().is_some_and(|ws| {
        ws.iter()
            .any(|w| w["class"].as_str().is_some_and(|c| c.contains("Notepad")))
    }) {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "existing Notepad window; measurement would touch a user scene",
            json!({"stage":"fixture_preflight"}),
        ));
    }
    // 注入测量需要桌面安静(Standing 信任下用户输入仍让行):等 ≥1.5s 无键鼠
    // 再开跑(上限 60s),把"卡时机"变成"手离开即自动起跑"。
    let idle = bench.call(&["wait", "--idle", "1500", "--timeout", "60000"])?;
    require(
        idle["ok"] == true,
        "desktop not idle within 60s; hands off keyboard and mouse to measure",
    )?;
    Ok(())
}

/// 注入预检:锁屏或前台被占用时注入护栏会拒绝(2026-09-26 实证锁屏 PERM_DENIED;
/// 用户正在打字同样让前台核验失败——测量语义上本就该让行)。
/// 探测用自有标记窗口的 hwnd 定位(O(1),零桌面枚举);用不存在的目标名探测
/// 会退到桌面级 UIA 枚举,是 docs/65 已知的分钟级慢路径,不可用作探测。
fn probe_injection(bench: &mut Bench, marker: &str) -> Result<()> {
    let (owner, _file) = launch_marker_notepad(bench, marker)?;
    let probe = bench.sample(&["press", "f12", "--app", &format!("hwnd:{}", owner.hwnd)]);
    close_owned_notepad(bench, &owner)?;
    if probe["ok"] != true {
        return Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "keyboard injection refused: desktop locked or foreground busy; samples would be meaningless",
            json!({"stage":"fixture_preflight","probe":{"code":probe["error"]["code"],"message":probe["error"]["message"]}}),
        ));
    }
    Ok(())
}

struct Owner {
    hwnd: String,
    pid: u32,
    start: String,
}

fn find_marker_window(bench: &mut Bench, marker: &str) -> Result<Option<(String, u32)>> {
    let windows = bench.call(&["list-windows", "--native"])?;
    let hits: Vec<_> = windows["data"]["windows"]
        .as_array()
        .ok_or_else(|| io("window list unavailable"))?
        .iter()
        // 只认 Notepad 类窗口:另存为对话框(标题含文件名)会造成第二匹配
        // (2026-09-30 实测 marker not unique 的根因)。
        .filter(|w| {
            w["title"].as_str().is_some_and(|t| t.contains(marker))
                && w["class"].as_str().is_some_and(|c| c == "Notepad")
        })
        .collect();
    require(hits.len() <= 1, "marker window not unique")?;
    Ok(hits.first().map(|w| {
        (
            w["hwnd"].to_string(),
            w["pid"].as_u64().unwrap_or_default() as u32,
        )
    }))
}

fn owner_of(bench: &mut Bench, marker: &str, since_ms: u64) -> Result<Owner> {
    let (hwnd, pid) =
        find_marker_window(bench, marker)?.ok_or_else(|| io("marker window missing"))?;
    let metadata = actl_uia::evidence::process(pid, true)?;
    let start = metadata["start"].as_str().unwrap_or_default().to_string();
    require(
        metadata["created_unix_ms"]
            .as_u64()
            .is_some_and(|t| t >= since_ms),
        "window process predates this launch; not owned",
    )?;
    Ok(Owner { hwnd, pid, start })
}

/// 记事本可执行路径:Store 版机器没有 system32\notepad.exe,执行别名在
/// %LOCALAPPDATA%\Microsoft\WindowsApps;PATH 缺该目录时 `actl launch notepad.exe`
/// 解析失败(2026-09-30 实测)。按别名 → system32 → 裸名顺序解析。
fn notepad_program() -> String {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let alias = PathBuf::from(&local)
            .join("Microsoft")
            .join("WindowsApps")
            .join("notepad.exe");
        if alias.is_file() {
            return alias.to_string_lossy().into_owned();
        }
    }
    let classic = PathBuf::from(r"C:\Windows\system32\notepad.exe");
    if classic.is_file() {
        classic.to_string_lossy().into_owned()
    } else {
        "notepad.exe".into()
    }
}

fn launch_marker_notepad(bench: &mut Bench, marker: &str) -> Result<(Owner, PathBuf)> {
    let file = bench.dir.join(format!("{marker}.txt"));
    fs::write(&file, "").map_err(io)?;
    let since_ms = actl_core::state::unix_ms();
    let program = notepad_program();
    let launched = bench.call(&["launch", &program, &file.to_string_lossy()])?;
    require(
        launched["ok"] == true,
        &format!(
            "notepad launch failed: {}",
            launched["error"]["message"].as_str().unwrap_or("unknown")
        ),
    )?;
    let waited = bench.call(&["wait", "--window", marker, "--timeout", "12000"])?;
    require(waited["ok"] == true, "marker window did not appear")?;
    Ok((owner_of(bench, marker, since_ms)?, file))
}

/// 优雅关闭:close-window → 未保存提示点「不保存」→ verify-closed(进程级证明)。
fn close_owned_notepad(bench: &mut Bench, owner: &Owner) -> Result<&'static str> {
    let app = format!("hwnd:{}", owner.hwnd);
    bench.call(&["close-window", "--hwnd", &owner.hwnd])?;
    let remaining = bench.call(&["list-windows", "--native", "--hwnd", &owner.hwnd])?;
    if remaining["data"]["windows"]
        .as_array()
        .is_some_and(|w| !w.is_empty())
    {
        let snapshot = bench.call(&["snapshot", "--app", &app])?;
        if snapshot["ok"] == true
            && let Some(button) = snapshot["data"]["elements"].as_array().and_then(|es| {
                es.iter().find(|e| {
                    e["role"] == "Button"
                        && e["name"].as_str().is_some_and(|n| n.contains("不保存"))
                })
            })
        {
            let name = button["name"].as_str().unwrap_or_default().to_string();
            bench.call(&["click", &format!("name:{name}"), "--app", &app])?;
        }
    }
    bench.call(&[
        "verify-closed",
        "--hwnd",
        &owner.hwnd,
        "--pid",
        &owner.pid.to_string(),
        "--instance",
        &owner.start,
    ])?;
    Ok("closed_verified")
}

fn ensure_notepad_closed(bench: &mut Bench, marker: &str) -> Result<()> {
    if let Some((hwnd, pid)) = find_marker_window(bench, marker)? {
        let metadata = actl_uia::evidence::process(pid, true)?;
        close_owned_notepad(
            bench,
            &Owner {
                hwnd,
                pid,
                start: metadata["start"].as_str().unwrap_or_default().to_string(),
            },
        )?;
    }
    Ok(())
}

fn shell_start(target: &str) {
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "start", "", target])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let _ = cmd.status();
}

/// 延迟启动线程:构造确定延迟事件(Q2 检出开销 = duration - delay)。
fn delayed_notepad(
    binary: PathBuf,
    program: String,
    file: String,
    delay_ms: u64,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay_ms));
        let mut cmd = Command::new(binary);
        cmd.args(["launch", &program, &file])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let _ = cmd.status();
    })
}

fn duration_ms(envelope: &Value) -> Option<u64> {
    envelope["duration_ms"]
        .as_u64()
        .filter(|_| envelope["ok"] == true)
}

fn delivered_chars(envelope: &Value) -> Option<u64> {
    envelope["data"]["delivered"]
        .as_u64()
        .or_else(|| envelope["delivered"].as_u64())
}

fn median(sorted: &[u64]) -> u64 {
    sorted[sorted.len() / 2]
}

/// 与 2026-09-26 报告同口径:err = 失败样本数;P50=(n+1)/2、P95=n*95/100+1(1-based)。
fn stats_line(label: &str, samples: &[Option<u64>]) -> String {
    let errs = samples.iter().filter(|s| s.is_none()).count();
    let mut nums: Vec<u64> = samples.iter().filter_map(|s| *s).collect();
    if nums.is_empty() {
        return format!("{label}: 全部失败({errs})");
    }
    nums.sort_unstable();
    let n = nums.len();
    let i50 = n.div_ceil(2);
    let i95 = (n * 95 / 100 + 1).min(n);
    format!(
        "{label}: n={n} err={errs} P50={}ms P95={}ms max={}ms",
        nums[i50 - 1],
        nums[i95 - 1],
        nums[n - 1]
    )
}

fn samples_block(samples: &[Option<u64>]) -> String {
    samples
        .iter()
        .map(|s| match s {
            Some(v) => v.to_string(),
            None => "ERR".into(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parent_knob_pollution() -> Vec<&'static str> {
    KNOB_ENV
        .iter()
        .filter(|k| std::env::var(k).is_ok())
        .copied()
        .collect()
}

fn env_header(bench: &Bench) -> Vec<String> {
    let size = fs::metadata(&bench.binary).map(|m| m.len()).unwrap_or(0);
    let mut lines = vec![
        format!(
            "- 构建: {};二进制: {}B ({})",
            bench.version(),
            size,
            bench.binary.display()
        ),
        format!(
            "- 机器: {} / {} 逻辑处理器;UTC 时间戳",
            std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".into()),
            std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_else(|_| "?".into())
        ),
    ];
    let pollution = parent_knob_pollution();
    if !pollution.is_empty() {
        lines.push(format!(
            "- ⚠ 父环境残留时序旋钮,样本被污染: {}",
            pollution.join(", ")
        ));
    }
    lines
}

fn utc_stamp(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        y,
        m,
        d,
        secs / 3600 % 24,
        secs / 60 % 60,
        secs % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn write_report(kind: &str, markdown: &str) -> Result<PathBuf> {
    let dir = crate::root().join("docs/timing-data");
    fs::create_dir_all(&dir).map_err(io)?;
    let path = dir.join(format!(
        "{kind}-{}.md",
        utc_stamp(actl_core::state::unix_ms())
    ));
    fs::write(&path, markdown).map_err(io)?;
    Ok(path)
}

/// 标题发现 + hwnd 定向优雅关闭(发现与关闭之间重解析的歧义窗口不误伤)。
fn close_by_title(bench: &mut Bench, title: &str) -> Result<()> {
    if let Some((hwnd, _)) = find_marker_window(bench, title)? {
        bench.call(&["close-window", "--hwnd", &hwnd])?;
        let _ = bench.call(&[
            "verify-window",
            "--app",
            &format!("hwnd:{hwnd}"),
            "--state",
            "absent",
        ]);
    }
    Ok(())
}

pub fn profile(args: &Args) -> Result<Value> {
    args.check(&["actl", "output"], &[])?;
    let mut bench = Bench::new(args, "timing-profile")?;
    preflight(&mut bench)?;
    let mut md = String::new();
    md.push_str("# actl 时序画像(原生 fixture)\n\n");
    for line in env_header(&bench) {
        md.push_str(&line);
        md.push('\n');
    }
    md.push('\n');
    let mut scenarios: Vec<Value> = vec![];

    // 场景 A:记事本读路径(get value;UIA 属性读取基准)
    let marker = crate::id("timing");
    let (owner, _file) = launch_marker_notepad(&mut bench, &marker)?;
    let app = format!("hwnd:{}", owner.hwnd);
    let mut samples = vec![];
    for _ in 0..20 {
        let v = bench.sample(&["get", "role:Document", "--app", &app, "--property", "value"]);
        samples.push(duration_ms(&v));
    }
    md.push_str(&stats_line("notepad-get", &samples));
    md.push_str("\n```\n");
    md.push_str(&samples_block(&samples));
    md.push_str("\n```\n");
    scenarios.push(
        json!({"name":"notepad-get","n":20,"err":samples.iter().filter(|s|s.is_none()).count()}),
    );

    // 场景 B:另存为链(press --expect 全链;ctrl+shift+s 对已有文件也弹窗)
    let _ = bench.sample(&["type", "role:Document", "timing", "--app", &app]);
    let mut dlg = vec![];
    for _ in 0..3 {
        let v = bench.sample(&[
            "press",
            "ctrl+shift+s",
            "--app",
            &app,
            "--expect",
            "window:另存为",
            "--timeout",
            "5000",
        ]);
        dlg.push(duration_ms(&v));
        let _ = bench.sample(&["press", "esc", "--app", "另存为"]);
        thread::sleep(Duration::from_millis(1000));
    }
    md.push_str(&stats_line("notepad-saveas-dlg", &dlg));
    md.push_str("\n```\n");
    md.push_str(&samples_block(&dlg));
    md.push_str("\n```\n");
    scenarios.push(
        json!({"name":"notepad-saveas-dlg","n":3,"err":dlg.iter().filter(|s|s.is_none()).count()}),
    );
    let cleanup_state = close_owned_notepad(&mut bench, &owner)?;

    // 场景 C:计算器热点击(UIA Invoke 基准;内容树未物化则跳过——已知环境坑)
    shell_start("shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App");
    let _ = bench.sample(&["wait", "--window", "计算器", "--timeout", "8000"]);
    thread::sleep(Duration::from_millis(2000));
    let buttons = bench.sample(&[
        "find",
        "--role",
        "Button",
        "--app",
        "计算器",
        "--limit",
        "40",
    ]);
    let healthy = buttons["data"]["total"].as_u64().unwrap_or_default() >= 10;
    if healthy {
        let mut clicks = vec![];
        for _ in 0..30 {
            let v = bench.sample(&["click", "id:clearButton", "--app", "计算器"]);
            clicks.push(duration_ms(&v));
        }
        md.push_str(&stats_line("calc-click", &clicks));
        md.push_str("\n```\n");
        md.push_str(&samples_block(&clicks));
        md.push_str("\n```\n");
        scenarios.push(
            json!({"name":"calc-click","n":30,"err":clicks.iter().filter(|s|s.is_none()).count()}),
        );
    } else {
        md.push_str(&format!(
            "calc-click: 跳过——计算器内容树未物化(buttons={};僵尸框架后实例坏状态)\n",
            buttons["data"]["total"].as_u64().unwrap_or_default()
        ));
        scenarios.push(json!({"name":"calc-click","status":"skipped"}));
    }
    close_by_title(&mut bench, "计算器")?;

    // 场景 D:设置冷快照(一次性;Electron/惰性激活计入)
    shell_start("ms-settings:");
    if bench.sample(&["wait", "--window", "设置", "--timeout", "10000"])["ok"] == true {
        let v = bench.sample(&["snapshot", "--app", "设置", "--skeleton"]);
        md.push_str(&format!(
            "ms-settings-cold-snapshot: {}ms(一次性;a11y 惰性激活计入)\n",
            duration_ms(&v)
                .map(|n| n.to_string())
                .unwrap_or_else(|| "ERR".into())
        ));
        scenarios.push(json!({"name":"ms-settings-cold-snapshot","ok":v["ok"]==true}));
    } else {
        md.push_str("ms-settings-cold-snapshot: 未采集\n");
        scenarios.push(json!({"name":"ms-settings-cold-snapshot","status":"skipped"}));
    }
    close_by_title(&mut bench, "设置")?;

    finish(bench, "profile", md, cleanup_state, scenarios)
}

pub fn sweep(args: &Args) -> Result<Value> {
    args.check(&["actl", "output"], &[])?;
    let mut bench = Bench::new(args, "timing-sweep")?;
    preflight(&mut bench)?;
    let mut md = String::new();
    md.push_str("# actl 时序扫描(原生 fixture)\n\n");
    for line in env_header(&bench) {
        md.push_str(&line);
        md.push('\n');
    }
    md.push('\n');
    let delay_ms = 1500u64;
    let marker = crate::id("timing");
    probe_injection(&mut bench, &marker)?;
    let mut scenarios: Vec<Value> = vec![];

    // Q2:轮询间隔 × wait 检出延迟(构造确定延迟事件;开销 = duration - delay)
    md.push_str("## Q2 轮询间隔 × wait 检出延迟(理论下限 = 间隔/2)\n");
    for poll in [50u64, 100, 250] {
        let mut overhead = vec![];
        for _ in 0..5 {
            ensure_notepad_closed(&mut bench, &marker)?;
            let file = bench.dir.join(format!("{marker}.txt"));
            fs::write(&file, "").map_err(io)?;
            let handle = delayed_notepad(
                bench.binary.clone(),
                notepad_program(),
                file.to_string_lossy().into(),
                delay_ms,
            );
            let v = bench.sample_env(
                &["wait", "--window", &marker, "--timeout", "10000"],
                &[("ACTL_POLL_MS", &poll.to_string())],
            );
            let _ = handle.join();
            if let Some(d) = duration_ms(&v) {
                overhead.push(Some(d.saturating_sub(delay_ms)));
            } else {
                overhead.push(None);
            }
        }
        let done: Vec<u64> = overhead.iter().filter_map(|s| *s).collect();
        if done.is_empty() {
            md.push_str(&format!("poll={poll}ms: 无有效样本\n"));
            scenarios.push(json!({"name":format!("q2-poll-{poll}"),"ok":false}));
        } else {
            md.push_str(&format!(
                "poll={poll}ms: 中位检出开销 {}ms(样本: {})\n",
                median(&sorted(&done)),
                samples_block(&overhead)
            ));
            scenarios.push(json!({"name":format!("q2-poll-{poll}"),"samples":done.len()}));
        }
    }
    ensure_notepad_closed(&mut bench, &marker)?;

    // Q3:分批大小 × type 吞吐(300 字符键盘通道;停止粒度 = 批大小)
    md.push_str("\n## Q3 分批大小 × type 吞吐(300 字符键盘通道;停止粒度=批大小)\n");
    let (owner, _file) = launch_marker_notepad(&mut bench, &marker)?;
    let app = format!("hwnd:{}", owner.hwnd);
    let body = "x".repeat(300);
    for segment in [16u64, 32, 64, 128] {
        let mut times = vec![];
        for _ in 0..3 {
            let _ = bench.sample(&["press", "ctrl+a", "--app", &app]);
            thread::sleep(Duration::from_millis(300));
            let v = bench.sample_env(
                &["type", "role:Document", &body, "--app", &app],
                &[("ACTL_SEGMENT_CHARS", &segment.to_string())],
            );
            if duration_ms(&v).is_some() && delivered_chars(&v) == Some(300) {
                times.push(duration_ms(&v));
            } else {
                times.push(None);
            }
        }
        let done: Vec<u64> = times.iter().filter_map(|s| *s).collect();
        if done.is_empty() {
            md.push_str(&format!("segment={segment}: 无有效样本(污染或注入被拒)\n"));
        } else {
            md.push_str(&format!(
                "segment={segment}: 中位 {}ms / 300 字符(样本: {})\n",
                median(&sorted(&done)),
                samples_block(&times)
            ));
        }
    }

    // Q5:焦点链参数 × press 成功率(每格 10 次)
    md.push_str("\n## Q5 焦点链(press_focus_ms × retries)× 成功率(每格 10 次 press)\n");
    for (focus_ms, retries) in [(60u64, 3u64), (120, 2), (120, 3), (150, 3)] {
        let mut ok = 0u32;
        for _ in 0..10 {
            let v = bench.sample_env(
                &["press", "f12", "--app", &app],
                &[
                    ("ACTL_PRESS_FOCUS_MS", &focus_ms.to_string()),
                    ("ACTL_FOCUS_RETRIES", &retries.to_string()),
                ],
            );
            if v["ok"] == true {
                ok += 1;
            }
            thread::sleep(Duration::from_millis(200));
        }
        md.push_str(&format!(
            "focus={focus_ms}ms retries={retries}: ok={ok}/10\n"
        ));
    }
    let cleanup_state = close_owned_notepad(&mut bench, &owner)?;
    scenarios.push(json!({"name":"q5","ok":true}));

    finish(bench, "sweep", md, cleanup_state, scenarios)
}

/// 汇总收尾:写 markdown 报告 → 机器可读 report.json(含报告路径)。
fn finish(
    bench: Bench,
    kind: &str,
    md: String,
    cleanup: &str,
    scenarios: Vec<Value>,
) -> Result<Value> {
    let report = json!({
        "passed": cleanup == "closed_verified",
        "cleanup": cleanup,
        "scenarios": scenarios,
        "directory": bench.dir.to_string_lossy(),
        "calls": bench.calls,
    });
    let path = write_report(kind, &md)?;
    let mut report = report;
    report["report"] = json!(path.to_string_lossy());
    crate::save(&bench.dir.join("report.json"), &report)?;
    Ok(report)
}

fn sorted(values: &[u64]) -> Vec<u64> {
    let mut v = values.to_vec();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_match_legacy_percentile_convention() {
        let one = stats_line("x", &[Some(100)]);
        assert!(
            one.contains("n=1 err=0 P50=100ms P95=100ms max=100ms"),
            "{one}"
        );
        // n=19(1 处 err):i50=10 → 第 10 小 = 110;i95=19*95/100+1=19 → max
        let mut samples: Vec<Option<u64>> = (1..=20).map(|i| Some(i * 10)).collect();
        samples[0] = None;
        let line = stats_line("y", &samples);
        assert!(line.contains("n=19 err=1"), "{line}");
        assert!(line.contains("P50=110ms"), "{line}");
        assert!(line.contains("P95=200ms"), "{line}");
        assert!(stats_line("z", &[None, None]).contains("全部失败"));
    }

    #[test]
    fn median_takes_middle_of_sorted() {
        assert_eq!(median(&[1, 2, 3]), 2);
        assert_eq!(median(&[1, 2, 3, 4]), 3);
    }

    #[test]
    fn utc_stamp_known_epochs() {
        assert_eq!(utc_stamp(0), "19700101-000000");
        // 2024-01-01 00:00:00 UTC = 1704067200
        assert_eq!(utc_stamp(1_704_067_200_000), "20240101-000000");
        // 2026-09-26 21:18:53 UTC = 1790457533
        assert_eq!(utc_stamp(1_790_457_533_000), "20260926-211853");
    }

    #[test]
    fn samples_block_renders_errs() {
        assert_eq!(samples_block(&[Some(5), None]), "5 ERR");
    }
}
