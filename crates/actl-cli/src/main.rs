//! actl-cli —— 命令入口与分发。
//!
//! stdout 契约:数据命令输出 envelope JSON(ADR-002);`--help/--version` 为元命令例外(06 §3.5)。
//! 退出码:0 成功 · 1 业务失败(ErrorEnvelope)· 2 参数/协议错误(clap 默认,对齐 06 §4)。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5)。

use std::time::Instant;

use actl_core::{CtlError, Envelope, ErrorCode, ErrorEnvelope, SnapshotBuilder};
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
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Keep only interactive elements and their ancestor chain (token saver)
        #[arg(long)]
        skeleton: bool,
    },
    /// Click an element (UIA Invoke by default; --physical for a real mouse click)
    Click {
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type>
        target: String,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Physically move the mouse and click at the UIA clickable point
        /// (uses the real pointer + input lock; UIA Invoke is the default)
        #[arg(long)]
        physical: bool,
    },
    /// Read properties of an element
    Get {
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type>
        target: String,
        /// Property: name | role | automation_id | offscreen | value (default: all)
        #[arg(long)]
        property: Option<String>,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
    },
    /// Launch a program with optional arguments
    Launch {
        /// Program and arguments (everything after `launch`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Type text into an element via keyboard (focuses the element first)
    Type {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type>
        target: String,
        /// Text to type (literal)
        text: String,
        /// Target window by title substring (required: keyboard injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
        /// Deliver via clipboard + ctrl+v (the only reliable text channel
        /// under a Chinese-mode IME; uses the shared clipboard, reported in output)
        #[arg(long)]
        paste: bool,
    },
    /// Press a key combination acting on the current focus (e.g. ctrl+s)
    Press {
        /// Key expression: ctrl+s, ctrl+shift+t, enter, f5 ...
        keys: String,
        /// Target window by title substring (required: injection is global and
        /// unguarded without a pinned target — fail-closed)
        #[arg(long)]
        app: Option<String>,
        /// Post-condition: window:<substr> must NEWLY appear (pre-existing
        /// same-title windows do not satisfy it; polls up to 2s by default)
        #[arg(long)]
        expect: Option<String>,
        /// Timeout for --expect in milliseconds (default 2000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Set an element's value directly via UIA Value pattern (no keyboard)
    SetValue {
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type>
        target: String,
        /// Value to set
        value: String,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
    },
    /// Bring a window to the foreground (keyboard commands act on global focus)
    FocusWindow {
        /// Window title substring
        pattern: String,
    },
    /// Close a window gracefully (WM_CLOSE; unsaved work may prompt)
    CloseWindow {
        /// Window title substring (must match exactly one window)
        pattern: String,
    },
    /// List top-level windows (title, class, pid)
    ListWindows,
    /// Find elements by condition (token-saving targeted search vs snapshot)
    Find {
        /// Match element name containing this substring
        #[arg(long)]
        name: Option<String>,
        /// Match control role exactly, case-insensitive (Button, Edit, Document ...)
        #[arg(long)]
        role: Option<String>,
        /// Match AutomationId exactly
        #[arg(long)]
        id: Option<String>,
        /// Target window by title substring (default: focused window)
        #[arg(long)]
        app: Option<String>,
        /// Max matches to report (default 20)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Read the clipboard as text (null if it holds no text)
    GetClipboard,
    /// Write text to the clipboard (replaces current content)
    SetClipboard {
        /// Text to place on the clipboard
        text: String,
    },
    /// Wait for an element or window to appear (patience, not assertion)
    Wait {
        /// Wait until this element target (@eN | name: | id: | role:) resolves
        #[arg(long, conflicts_with = "window")]
        element: Option<String>,
        /// Wait until a window whose title contains this substring exists
        #[arg(long)]
        window: Option<String>,
        /// Target window by title substring (for --element)
        #[arg(long)]
        app: Option<String>,
        /// Timeout in milliseconds (default 5000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Run multiple commands in one process (cold-start amortization;
    /// stops on first error, prior results are lost from output)
    Batch {
        /// JSON array of command strings, e.g. '["focus-window Notepad","press enter --app Notepad"]'
        #[arg(long)]
        json: String,
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

    let result = dispatch(&cli.command);

    let duration_ms = started.elapsed().as_millis() as u64;
    finish(command_name, result, duration_ms);
}

/// 单命令执行体;batch 子命令复用同一入口(06 §3.7 唯一复合逃生舱)。
fn dispatch(command: &Command) -> Result<CommandOutput, CtlError> {
    match command {
        Command::Commands => commands().map(Into::into),
        Command::Status => status().map(Into::into),
        Command::Snapshot { app, skeleton } => snapshot(app.as_deref(), *skeleton),
        Command::Click {
            target,
            app,
            physical,
        } => click(target, app.as_deref(), *physical),
        Command::Get {
            target,
            property,
            app,
        } => get(target, property.as_deref(), app.as_deref()),
        Command::Launch { args } => launch(args),
        Command::Type {
            target,
            text,
            app,
            paste,
        } => type_cmd(target, text, app.as_deref(), *paste),
        Command::Press {
            keys,
            app,
            expect,
            timeout,
        } => press(keys, app.as_deref(), expect.as_deref(), *timeout),
        Command::SetValue { target, value, app } => set_value_cmd(target, value, app.as_deref()),
        Command::FocusWindow { pattern } => focus_window_cmd(pattern),
        Command::CloseWindow { pattern } => close_window_cmd(pattern),
        Command::ListWindows => list_windows_cmd(),
        Command::Find {
            name,
            role,
            id,
            app,
            limit,
        } => find_cmd(
            name.as_deref(),
            role.as_deref(),
            id.as_deref(),
            app.as_deref(),
            *limit,
        ),
        Command::GetClipboard => get_clipboard_cmd(),
        Command::SetClipboard { text } => set_clipboard_cmd(text),
        Command::Wait {
            element,
            window,
            app,
            timeout,
        } => wait_cmd(
            element.as_deref(),
            window.as_deref(),
            app.as_deref(),
            *timeout,
        ),
        Command::Batch { json } => batch_cmd(json),
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
        }
        Err(err) => {
            let env = ErrorEnvelope::new(command_name, &err, duration_ms);
            println!(
                "{}",
                serde_json::to_string(&env).expect("envelope serializes")
            );
            // 退出码契约(06 §4):PROTOCOL 不可恢复 → 2;其余业务失败 → 1
            let code = if err.code == actl_core::ErrorCode::Protocol {
                2
            } else {
                1
            };
            std::process::exit(code);
        }
    }
}

fn command_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Commands => "commands",
        Command::Status => "status",
        Command::Snapshot { .. } => "snapshot",
        Command::Click { .. } => "click",
        Command::Get { .. } => "get",
        Command::Launch { .. } => "launch",
        Command::Type { .. } => "type",
        Command::Press { .. } => "press",
        Command::SetValue { .. } => "set-value",
        Command::FocusWindow { .. } => "focus-window",
        Command::CloseWindow { .. } => "close-window",
        Command::ListWindows => "list-windows",
        Command::Find { .. } => "find",
        Command::GetClipboard => "get-clipboard",
        Command::SetClipboard { .. } => "set-clipboard",
        Command::Wait { .. } => "wait",
        Command::Batch { .. } => "batch",
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
        // 激活感知:目标曾挂起并被唤醒时如实上报等待量(0/未挂起时省略)
        "wake_ms": cap.wake_ms,
        "elements": serde_json::to_value(&out.elements).expect("elements serialize"),
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

fn click(target_raw: &str, app: Option<&str>, physical: bool) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    if physical {
        let (loc, (px, py)) = actl_uia::click_physical(app, &target)?;
        return Ok(json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": "primary",
            "pattern": "physical-left",
            "point": [px, py],
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "move_and_click" },
            "resolved": { "role": loc.role, "name": loc.name, "automation_id": loc.automation_id },
            "window": loc.window_title,
        })
        .into());
    }
    let (loc, pattern) = actl_uia::click(app, &target)?;
    Ok(json!({
        "action": "click",
        "target": target.describe(),
        "resolved_by": "primary",
        "pattern": pattern,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
        "resolved": {
            "role": loc.role,
            "name": loc.name,
            "automation_id": loc.automation_id,
        },
        "window": loc.window_title,
    })
    .into())
}

fn get(
    target_raw: &str,
    property: Option<&str>,
    app: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let info = actl_uia::read_property(app, &target, property)?;
    let mut props = serde_json::Map::new();
    if let Some(v) = &info.name {
        props.insert("name".into(), json!(v));
    }
    if !info.role.is_empty() {
        props.insert("role".into(), json!(info.role));
    }
    if let Some(v) = &info.automation_id {
        props.insert("automation_id".into(), json!(v));
    }
    if let Some(v) = info.offscreen {
        props.insert("offscreen".into(), json!(v));
    }
    if let Some(v) = &info.value {
        props.insert("value".into(), json!(v));
    }
    Ok(json!({ "target": target.describe(), "properties": props }).into())
}

fn launch(args: &[String]) -> Result<CommandOutput, CtlError> {
    let Some(program) = args.first() else {
        return Err(CtlError::protocol("launch requires a program name"));
    };
    let child = std::process::Command::new(program)
        .args(&args[1..])
        .spawn()
        .map_err(|e| {
            CtlError::new(
                ErrorCode::NotFound,
                format!("cannot launch {program:?}: {e}"),
            )
        })?;
    Ok(json!({ "pid": child.id(), "program": program }).into())
}

fn type_cmd(
    target_raw: &str,
    text: &str,
    app: Option<&str>,
    paste: bool,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "type")?;
    let target = actl_core::parse_target(target_raw)?;
    let res = actl_uia::type_text(Some(app), &target, text, paste)?;
    let keyboard = if res.stopped_early {
        "partial"
    } else {
        "injected"
    };
    Ok(json!({
        "action": "type",
        "target": target.describe(),
        "chars": res.total,
        "delivered": res.delivered,
        "stopped_early": res.stopped_early,
        "via": if paste { "clipboard-paste" } else { "keyboard" },
        "input_effects": {
            "keyboard": keyboard,
            "clipboard": if paste { "used" } else { "untouched" },
            "pointer": "none",
        },
        "resolved": { "role": res.located.role, "name": res.located.name },
    })
    .into())
}

fn press(
    keys: &str,
    app: Option<&str>,
    expect: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "press")?;
    let spec = actl_core::parse_key_expr(keys)?;
    // --expect = "本动作导致新窗口":注入前先取窗口基线,断言时排除既有同名窗口
    // (存在性检查会被预先打开的同名窗口恒真欺骗,doc 09 §2)
    let expect_substr = match expect {
        Some(raw) => Some(parse_expect(raw)?),
        None => None,
    };
    let baseline = if expect_substr.is_some() {
        actl_uia::window_identities()?
    } else {
        Vec::new()
    };
    // 键盘作用于全局焦点:裸 SendInput 会打进当下前台(实测坑)。
    // press_keys 内部在 --app 时先置前台并稳定窗(--app 语义,focus+delay 在 uia 层)。
    let focused = actl_uia::press_keys(&spec, Some(app))?;
    let mut expect_window = None;
    if let Some(substr) = expect_substr {
        // 06 §4:--expect = 执行后隐式 wait(默认 2s),失败 ASSERTION_FAILED
        actl_uia::wait_new_window(&substr, timeout.unwrap_or(2000), &baseline)?;
        expect_window = Some(substr);
    }
    Ok(json!({
        "action": "press",
        "keys": keys,
        "focused": focused,
        "expect": expect_window,
        "input_effects": {
            "keyboard": "injected",
            "clipboard": "untouched",
            "pointer": "none",
        },
    })
    .into())
}

/// 键盘注入命令的 fail-closed 门:必须显式 --app 绑定目标窗口。
/// 裸注入打进"当下恰好前台"的窗口是事故 #1 的路径之一;护栏随 --app 生效。
fn require_app<'a>(app: Option<&'a str>, cmd: &str) -> Result<&'a str, CtlError> {
    app.filter(|a| !a.trim().is_empty()).ok_or_else(|| {
        CtlError::protocol(format!(
            "{cmd} requires --app <title-substr>: keyboard injection is global and \
                 unguarded without a pinned target window (fail-closed)"
        ))
    })
}

fn set_value_cmd(
    target_raw: &str,
    value: &str,
    app: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let res = actl_uia::set_value(app, &target, value)?;
    Ok(json!({
        "action": "set-value",
        "target": target.describe(),
        "resolved": { "role": res.role, "name": res.name },
    })
    .into())
}

/// --expect 谓词 v1:仅 `window:<substr>`。
fn parse_expect(raw: &str) -> Result<String, CtlError> {
    raw.strip_prefix("window:")
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| {
            CtlError::protocol(format!(
                "invalid --expect {raw:?}: only window:<substr> is supported"
            ))
        })
}

fn focus_window_cmd(pattern: &str) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::focus_window(pattern)?;
    Ok(json!({ "action": "focus-window", "window": title }).into())
}

fn close_window_cmd(pattern: &str) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::close_window(pattern)?;
    Ok(json!({ "action": "close-window", "window": title }).into())
}

/// find:条件查元素,复用 snapshot 的 ref 编号(同一 DFS,同一编号空间,
/// find 的 @eN 与 snapshot 的 @eN 可互换)。至少一个过滤条件,反对全树 dump。
fn find_cmd(
    name: Option<&str>,
    role: Option<&str>,
    id: Option<&str>,
    app: Option<&str>,
    limit: Option<usize>,
) -> Result<CommandOutput, CtlError> {
    if name.is_none() && role.is_none() && id.is_none() {
        return Err(CtlError::protocol(
            "find needs at least one of --name/--role/--id (full-tree dump is `snapshot`)",
        ));
    }
    let cap = actl_uia::capture(app)?;
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    let out = builder.finish(false);
    let role_want = role.map(str::to_lowercase);
    let matched: Vec<&actl_core::snapshot::ElementOut> = out
        .elements
        .iter()
        .filter(|e| {
            name.map(|n| e.name.as_deref().is_some_and(|en| en.contains(n)))
                .unwrap_or(true)
        })
        .filter(|e| {
            role_want
                .as_ref()
                .map(|r| e.role.to_lowercase().contains(r.as_str()))
                .unwrap_or(true)
        })
        .filter(|e| {
            id.map(|i| e.automation_id.as_deref() == Some(i))
                .unwrap_or(true)
        })
        .collect();
    let total = matched.len();
    let limit = limit.unwrap_or(20);
    let elements: Vec<_> = matched
        .iter()
        .take(limit)
        .map(|e| serde_json::to_value(e).expect("element serializes"))
        .collect();
    let data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "total": total,
        "returned": elements.len(),
        "wake_ms": cap.wake_ms,
        "elements": elements,
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

fn get_clipboard_cmd() -> Result<CommandOutput, CtlError> {
    let text = actl_uia::get_clipboard_text()?;
    Ok(json!({ "text": text }).into())
}

fn set_clipboard_cmd(text: &str) -> Result<CommandOutput, CtlError> {
    actl_uia::set_clipboard_text(text)?;
    Ok(json!({ "chars": text.chars().count() }).into())
}

fn wait_cmd(
    element: Option<&str>,
    window: Option<&str>,
    app: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let timeout = timeout.unwrap_or(5000);
    let started = Instant::now();
    let what = match (element, window) {
        (Some(t), _) => {
            let target = actl_core::parse_target(t)?;
            actl_uia::wait_element(app, &target, timeout)?;
            json!({ "element": target.describe() })
        }
        (None, Some(w)) => {
            let title = actl_uia::wait_window(w, timeout)?;
            json!({ "window": title })
        }
        (None, None) => {
            return Err(CtlError::protocol(
                "wait needs --element <target> or --window <substr>",
            ));
        }
    };
    Ok(json!({
        "waited": what,
        "waited_ms": started.elapsed().as_millis() as u64,
    })
    .into())
}

/// batch:单进程顺序执行(--json '["cmd args", ...]'),首错即停。
/// 退出码沿用失败子命令的错误契约;M1 不带出部分结果(M2 加 partial)。
fn batch_cmd(json_str: &str) -> Result<CommandOutput, CtlError> {
    let cmds: Vec<String> = serde_json::from_str(json_str).map_err(|e| {
        CtlError::protocol(format!(
            "--json must be a JSON array of command strings: {e}"
        ))
    })?;
    if cmds.is_empty() {
        return Err(CtlError::protocol("batch needs at least one command"));
    }
    let mut executed = 0usize;
    for (i, line) in cmds.iter().enumerate() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(CtlError::protocol(format!("batch[{i}] is empty")));
        }
        let sub = Cli::try_parse_from(std::iter::once("actl").chain(tokens.iter().copied()))
            .map_err(|e| CtlError::protocol(format!("batch[{i}] {line:?}: {e}")))?;
        if let Command::Batch { .. } = sub.command {
            return Err(CtlError::protocol("batch cannot nest batch"));
        }
        if let Err(err) = dispatch(&sub.command) {
            return Err(CtlError::new(
                err.code,
                format!(
                    "batch[{i}] {:?} failed after {executed} ok: {}",
                    line, err.message
                ),
            ));
        }
        executed += 1;
    }
    Ok(json!({ "executed": executed }).into())
}

fn list_windows_cmd() -> Result<CommandOutput, CtlError> {
    let wins = actl_uia::list_windows()?;
    let list: Vec<Value> = wins
        .into_iter()
        .map(|(title, class, pid)| json!({ "title": title, "class": class, "pid": pid }))
        .collect();
    Ok(json!({ "windows": list, "count": list.len() }).into())
}
