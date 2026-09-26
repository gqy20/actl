//! cli —— clap 参数面:命令/参数定义。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5);
//! `commands` 自发现命令读的也是这棵树(schema 与 help 单一事实来源)。

use clap::{Parser, Subcommand};

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
        /// Wait until this element target NO LONGER resolves
        #[arg(long)]
        gone: Option<String>,
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
