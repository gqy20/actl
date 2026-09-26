//! cli —— clap 参数面:命令/参数定义。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5);
//! `commands` 自发现命令读的也是这棵树(schema 与 help 单一事实来源)。

use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "actl",
    version,
    about = "actl — desktop hands for AI agents: UIA-driven Windows control via atomic CLI commands"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate ref replay against this snapshot id (window-instance check)
        #[arg(long)]
        snapshot: Option<String>,
        /// Physically move the mouse and click at the UIA clickable point
        /// (uses the real pointer + input lock; UIA Invoke is the default)
        #[arg(long)]
        physical: bool,
        /// Post-condition: window:<substr> must NEWLY appear after the click
        #[arg(long)]
        expect: Option<String>,
        /// Timeout for --expect in milliseconds (default 2000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Read properties of an element
    Get {
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Property: name | role | automation_id | offscreen | value | text (default: all)
        #[arg(long)]
        property: Option<String>,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate ref replay against this snapshot id (window-instance check)
        #[arg(long)]
        snapshot: Option<String>,
    },
    /// Launch a program with optional arguments
    Launch {
        /// Program and arguments (everything after `launch`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Type text into an element via keyboard (focuses the element first)
    Type {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Text to type (literal)
        text: String,
        /// Target window by title substring (required: keyboard injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate ref replay against this snapshot id (window-instance check)
        #[arg(long)]
        snapshot: Option<String>,
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
        /// Target: @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Value to set
        value: String,
        /// Target window by title substring (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate ref replay against this snapshot id (window-instance check)
        #[arg(long)]
        snapshot: Option<String>,
    },
    /// Right-click an element (physical; opens context menus)
    RightClick {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Double-click an element (physical)
    DoubleClick {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Move the pointer onto an element (hover; physical)
    Hover {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Drag from one element onto another (physical, left button held)
    Drag {
        /// Drag source target
        target: String,
        /// Drag destination target
        to: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Anchor disambiguation for the source
        #[arg(long)]
        near: Option<String>,
        /// Anchor disambiguation for the destination
        #[arg(long)]
        to_near: Option<String>,
    },
    /// Scroll an element (UIA Scroll pattern first, physical wheel fallback)
    Scroll {
        /// Target: scrollable container
        target: String,
        /// Scroll down N notches
        #[arg(long, conflicts_with = "up")]
        down: Option<u32>,
        /// Scroll up N notches
        #[arg(long)]
        up: Option<u32>,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Press and HOLD a key (pair with key-up; abnormal exit leaves it held)
    KeyDown {
        /// Key expression (single key or combo, e.g. shift, ctrl+a)
        keys: String,
        /// Target window by title substring (required: injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
    },
    /// Release a key held by key-down (must pair)
    KeyUp {
        /// Key expression (same as the paired key-down)
        keys: String,
        /// Target window by title substring (required: injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
    },
    /// Resize a window (keeps its position)
    ResizeWindow {
        /// Window title substring (must match exactly one window)
        pattern: String,
        /// New width in pixels
        #[arg(long)]
        width: i32,
        /// New height in pixels
        #[arg(long)]
        height: i32,
    },
    /// Immediate assertion (unlike wait): exists / value / checked
    Verify {
        /// Assert the element resolves
        #[arg(long, conflicts_with_all = ["value", "checked"])]
        exists: Option<String>,
        /// Assert the element's value (expected string is positional)
        #[arg(long, requires = "expected")]
        value: Option<String>,
        /// Expected value (exact unless --contains)
        expected: Option<String>,
        /// Substring match instead of exact
        #[arg(long)]
        contains: bool,
        /// Assert a toggle is On
        #[arg(long)]
        checked: Option<String>,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Extract a table from a container element as JSON rows
    Extract {
        /// Container target (role:Table / DataGrid / list that owns rows)
        target: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
    },
    /// Bring a window to the foreground (keyboard commands act on global focus)
    FocusWindow {
        /// Window title substring (exactly one match; or use --pid/--hwnd)
        #[arg(conflicts_with_all = ["fpid", "fhwnd"])]
        pattern: Option<String>,
        /// Focus the window owned by this process id (from list-windows)
        #[arg(long, id = "fpid", conflicts_with_all = ["pattern", "fhwnd"])]
        pid: Option<u32>,
        //
        /// Focus the window at this native handle (decimal or 0x-hex; from list-windows)
        #[arg(long, id = "fhwnd", conflicts_with_all = ["pattern", "fpid"], value_name = "HWND")]
        hwnd: Option<String>,
    },
    /// Close a window gracefully (WM_CLOSE; unsaved work may prompt)
    CloseWindow {
        /// Window title substring (must match exactly one window; or --pid/--hwnd)
        #[arg(conflicts_with_all = ["cpid", "chwnd"])]
        pattern: Option<String>,
        /// Close the window owned by this process id (from list-windows)
        #[arg(long, id = "cpid", conflicts_with_all = ["pattern", "chwnd"])]
        pid: Option<u32>,
        //
        /// Close the window at this native handle (decimal or 0x-hex; from list-windows)
        #[arg(long, id = "chwnd", value_name = "HWND", conflicts_with_all = ["pattern", "cpid"])]
        hwnd: Option<String>,
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
    /// Select an option inside a container (SelectionItem pattern; discrete choices)
    Select {
        /// Container target (list / combo box / tab strip that owns the options)
        target: String,
        /// Option name to select (exact first, substring fallback)
        name: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Toggle an element's state (checkbox/switch; reports the resulting state)
    Toggle {
        /// Target: @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Wait for an element or window to appear (patience, not assertion)
    Wait {
        /// Wait until this element target (@eN | name: | id: | role:) resolves
        #[arg(long, conflicts_with = "window")]
        element: Option<String>,
        /// Wait until a window whose title contains this substring exists
        #[arg(long)]
        window: Option<String>,
        /// Wait until this element target NO LONGER resolves
        #[arg(long)]
        gone: Option<String>,
        /// Wait until --element satisfies a property predicate:
        /// value=<expected> | name=<substr> | checked=on|off (requires --element)
        #[arg(long, requires = "element")]
        property: Option<String>,
        /// Substring semantics for the value/name predicate (exact by default)
        #[arg(long, requires = "property")]
        contains: bool,
        /// Target window by title substring (for --element)
        #[arg(long)]
        app: Option<String>,
        /// Timeout in milliseconds (default 5000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Run multiple commands in one process (cold-start amortization);
    /// stops on first error — per-step results and timings in `steps[]`,
    /// partial steps ride the error evidence on failure
    Batch {
        /// JSON array of command strings, e.g. '["focus-window Notepad","press enter --app Notepad"]'
        #[arg(long)]
        json: String,
    },
}

/// 从命令提取 --app 摘要(状态展示用;batch 递归取首步)。
pub(crate) fn app_of(cmd: &Command) -> Option<String> {
    match cmd {
        Command::Click { app, .. }
        | Command::Get { app, .. }
        | Command::Type { app, .. }
        | Command::Press { app, .. }
        | Command::SetValue { app, .. }
        | Command::RightClick { app, .. }
        | Command::DoubleClick { app, .. }
        | Command::Hover { app, .. }
        | Command::Drag { app, .. }
        | Command::Select { app, .. }
        | Command::Toggle { app, .. }
        | Command::Scroll { app, .. }
        | Command::KeyDown { app, .. }
        | Command::KeyUp { app, .. }
        | Command::Wait { app, .. }
        | Command::Find { app, .. } => app.clone(),
        _ => None,
    }
}

/// envelope 的 command 字段(06 §4)。
pub(crate) fn command_name(cmd: &Command) -> &'static str {
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
        Command::RightClick { .. } => "right-click",
        Command::DoubleClick { .. } => "double-click",
        Command::Hover { .. } => "hover",
        Command::Drag { .. } => "drag",
        Command::Scroll { .. } => "scroll",
        Command::KeyDown { .. } => "key-down",
        Command::KeyUp { .. } => "key-up",
        Command::ResizeWindow { .. } => "resize-window",
        Command::Verify { .. } => "verify",
        Command::Extract { .. } => "extract",
        Command::Select { .. } => "select",
        Command::Toggle { .. } => "toggle",
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

/// 每命令的 after_help(06 §3.5 四要素的 Example/Errors 两要素;clap derive
/// 的 variant 级 after_help 不传播,故表驱动 + builder 注入,见 build_cli)。
/// 与 doc comment 同为 help 单一事实来源的一部分(改这里 = 改 -h)。
fn after_help_for(name: &str) -> Option<&'static str> {
    Some(match name {
        "commands" => {
            "Example:\n  actl commands\nOutput: envelope with every command + arg schema (machine self-description)"
        }
        "status" => "Example:\n  actl status\nOutput: version, protocol version, platform",
        "snapshot" => {
            "Example:\n  actl snapshot --app Notepad --skeleton\n--skeleton keeps interactive elements + collapsed ancestors (token saver).\nErrors: NOT_FOUND no window matches --app"
        }
        "click" => {
            "Example:\n  actl click @e3 --app Notepad --snapshot s1a2b\nErrors: NOT_FOUND no match | AMBIGUOUS multi-match (candidates returned) | STALE_REF stale @eN (re-snapshot) | NOT_ACTIONABLE no pattern | PERM_DENIED foreground guard (--physical)"
        }
        "get" => {
            "Example:\n  actl get name:状态 --app 设置 --property value\nProperties: name role automation_id offscreen value text (text = TextPattern body; Chromium Document value is the URL).\nErrors: PROTOCOL unknown property | NOT_FOUND no match | AMBIGUOUS multi-match"
        }
        "launch" => {
            "Example:\n  actl launch notepad\nNote: no already-running detection; use focus-window for an existing window.\nErrors: NOT_FOUND program not found"
        }
        "type" => {
            "Example:\n  actl type id:Edit 'hello world' --app Notepad --paste\n--paste: clipboard+ctrl+v, the only reliable channel under a Chinese-mode IME (uses the shared clipboard, reported in output).\nErrors: PROTOCOL --app required (injection is fail-closed) | PERM_DENIED foreground guard aborted | NOT_FOUND / AMBIGUOUS target | NOT_ACTIONABLE cannot take focus"
        }
        "press" => {
            "Example:\n  actl press ctrl+s --app Notepad --expect window:另存为\n--expect = a NEW window must appear (pre-existing same-title does not count).\nErrors: PROTOCOL --app required or bad key expr | PERM_DENIED foreground guard aborted | ASSERTION_FAILED --expect window never appeared"
        }
        "set-value" => {
            "Example:\n  actl set-value id:1001 'report.txt' --app 另存为\nUIValue direct write, no keyboard (vs type).\nErrors: NOT_FOUND / AMBIGUOUS target | NOT_ACTIONABLE no Value pattern (try type)"
        }
        "select" => {
            "Example:\n  actl select role:ComboBox 'CSV (逗号分隔)' --app 导出\nSelects a discrete option via SelectionItem (vs set-value for text values).\nErrors: NOT_FOUND option not under container | AMBIGUOUS multiple options match | NOT_ACTIONABLE no SelectionItem"
        }
        "toggle" => {
            "Example:\n  actl toggle name:自动保存 --app 设置\nReturns the resulting state (On/Off). vs click: no click side effects.\nErrors: NOT_FOUND / AMBIGUOUS target | NOT_ACTIONABLE no Toggle pattern"
        }
        "right-click" => {
            "Example:\n  actl right-click name:file.txt --app 项目\nPhysical action (naming = authorization): opens context menus.\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
        }
        "double-click" => {
            "Example:\n  actl double-click name:file.txt --app 项目\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
        }
        "hover" => {
            "Example:\n  actl hover name:菜单 --app 应用\nMoves the real pointer onto the element (tooltips, hover menus).\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
        }
        "drag" => {
            "Example:\n  actl drag name:slider --to name:max --app 设置\nErrors: NOT_FOUND / AMBIGUOUS either end | NOT_ACTIONABLE no clickable point"
        }
        "scroll" => {
            "Example:\n  actl scroll role:Document --down 3 --app Notepad\nUIA Scroll pattern first, physical wheel fallback (via reported).\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE neither channel works"
        }
        "key-down" => {
            "Example:\n  actl key-down shift --app Notepad\nHOLD semantics: must pair with key-up; abnormal exit leaves the key held.\nErrors: PROTOCOL --app required | PERM_DENIED foreground guard"
        }
        "key-up" => {
            "Example:\n  actl key-up shift --app Notepad\nMust be the same expression as the paired key-down.\nErrors: PROTOCOL --app required | PERM_DENIED foreground guard"
        }
        "resize-window" => {
            "Example:\n  actl resize-window 记事本 --width 800 --height 600\nErrors: NOT_FOUND / AMBIGUOUS window | NOT_ACTIONABLE no native handle"
        }
        "verify" => {
            "Example:\n  actl verify --value id:results '56,877' --contains --app 计算器\nImmediate assertion (vs wait): fails now, not later; failure carries evidence.actual.\nErrors: ASSERTION_FAILED | NOT_FOUND / AMBIGUOUS | PROTOCOL bad flags"
        }
        "extract" => {
            "Example:\n  actl extract role:List --app 系统信息\nGrid pattern rows first (source: grid-pattern), tree flattening fallback (source: tree-flatten).\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no rows found"
        }
        "focus-window" => {
            "Example:\n  actl focus-window 记事本\nKeyboard commands act on global focus — focus the window first in multi-window scenes.\nErrors: NOT_FOUND / AMBIGUOUS window"
        }
        "close-window" => {
            "Example:\n  actl close-window 记事本\nGraceful WM_CLOSE (unsaved work may prompt).\nErrors: NOT_FOUND / AMBIGUOUS window | NOT_ACTIONABLE no native handle"
        }
        "list-windows" => {
            "Example:\n  actl list-windows\nEach entry: title/class/pid + stacking facts (foreground, topmost, minimized, visible, z_order 0=top, rect, owner_title)."
        }
        "find" => {
            "Example:\n  actl find --name 保存 --role button --app 记事本\nRefs interchangeable with snapshot @eN. Zero realized hits + exact name/id falls through to virtualized containers (resolved_by: item-container).\nErrors: PROTOCOL needs >=1 filter | NOT_FOUND nothing matches"
        }
        "get-clipboard" => {
            "Example:\n  actl get-clipboard\nErrors: NOT_ACTIONABLE clipboard held by another process"
        }
        "set-clipboard" => {
            "Example:\n  actl set-clipboard 'text to place'\nReplaces current clipboard content (that is the semantics).\nErrors: NOT_ACTIONABLE clipboard held by another process"
        }
        "wait" => {
            "Example:\n  actl wait --element name:完成 --property value=OK --timeout 8000 --app 应用\nPredicates: value=<expected> | name=<substr> | checked=on|off; --contains for substring; existence implied.\nErrors: TIMEOUT predicate unmet or element absent | PROTOCOL bad predicate | ASSERTION_FAILED (--window)"
        }
        "batch" => {
            "Example:\n  actl batch --json '[\"focus-window Notepad\",\"press ctrl+s --app Notepad\"]'\nStops on first error; steps[] carries per-step ok/duration (partial steps ride the error evidence); windows_created reports windows opened during the run (cleanup: close-window --pid/--hwnd).\nErrors: PROTOCOL bad JSON/nesting | the first failing step's code is propagated"
        }
        _ => return None,
    })
}

/// 带注入的 clap 树:-h/--help 渲染 after_help(Example/Errors);与
/// `Cli::parse()` 的差异仅此(参数校验逻辑完全同源)。
pub(crate) fn build_cli() -> clap::Command {
    let mut root = Cli::command();
    for sub in root.get_subcommands_mut() {
        if let Some(text) = after_help_for(sub.get_name()) {
            *sub = std::mem::take(sub).after_help(text);
        }
    }
    root
}
