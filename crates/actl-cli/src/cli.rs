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

/// snapshot 视图参数(06 §3.2 L2 投影;值域扩展属 spec 提交)。
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewMode {
    /// Keep only Document content subtrees (web/document body); drop app chrome
    Content,
}

#[derive(Subcommand)]
pub(crate) enum DesktopCommand {
    /// Inspect execution cancellation state without granting permission
    ExecutionStatus,
    /// Explicitly repair execution state, archive old records and revoke old calls; never resume
    ExecutionRecover,
    /// Read recent local call events, newest calls first
    History {
        /// Filter by the caller-provided ACTL_TASK_ID
        #[arg(long)]
        task: Option<String>,
        /// Filter by workflow step ID (requires --task)
        #[arg(long, requires = "task")]
        step: Option<String>,
        /// Include archived reports and current local evidence availability
        #[arg(long, requires = "task")]
        reports: bool,
        /// Maximum calls (1-100)
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Archive a caller-authored task report; does not verify task completion
    TaskArchive {
        /// UTF-8 JSON report, up to 64 KiB; see docs/06 for schema
        #[arg(long)]
        file: String,
    },
    /// Inspect the companion lease without starting or changing it
    DisplayStatus,
    /// Start the matching companion if missing and wait for its visible rendered frame
    DisplayStart,
    /// Minimize one ordinary application window
    MinimizeWindow {
        #[arg(conflicts_with_all = ["pid", "hwnd"])]
        pattern: Option<String>,
        #[arg(long, conflicts_with_all = ["pattern", "hwnd"])]
        pid: Option<u32>,
        #[arg(long, conflicts_with_all = ["pattern", "pid"])]
        hwnd: Option<String>,
    },
    /// Restore one window from minimized state
    RestoreWindow {
        #[arg(conflicts_with_all = ["pid", "hwnd"])]
        pattern: Option<String>,
        #[arg(long, conflicts_with_all = ["pattern", "hwnd"])]
        pid: Option<u32>,
        #[arg(long, conflicts_with_all = ["pattern", "pid"])]
        hwnd: Option<String>,
    },
    /// Minimize ordinary application windows, recording a restorable desktop session
    DesktopPrepare {
        /// Explicitly request desktop-wide preparation; excludes tool/topmost/owned windows
        #[arg(long, conflicts_with = "hwnd")]
        all: bool,
        /// Prepare only these native handles (repeat --hwnd); mutually exclusive with --all
        #[arg(long)]
        hwnd: Vec<String>,
    },
    /// Restore unchanged windows from a desktop preparation session
    DesktopRestore {
        /// Session id returned by desktop-prepare, including partial-failure evidence
        #[arg(long)]
        session: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum FlowCommand {
    /// Validate YAML and every actl invocation without executing any step
    #[command(name = "flow-validate")]
    Validate {
        #[arg(long)]
        file: String,
    },
    /// Run a validated workflow and save resumable state under a unique ID
    #[command(name = "flow-run")]
    Run {
        #[arg(long)]
        file: String,
        #[arg(long)]
        id: String,
        /// Save the plan and start paused for inspection
        #[arg(long)]
        paused: bool,
    },
    /// Inspect durable workflow state and whether its executor is alive
    #[command(name = "flow-status")]
    Status {
        #[arg(long)]
        id: String,
    },
    /// Request a pause at the next command boundary or workflow wait poll
    #[command(name = "flow-pause")]
    Pause {
        #[arg(long)]
        id: String,
    },
    /// Resume the saved plan, checking uncertain results before replay
    #[command(name = "flow-resume")]
    Resume {
        #[arg(long)]
        id: String,
        /// Explicitly authorize repeating this uncertain step if verification fails
        #[arg(long)]
        retry_step: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// List every command with its argument schema (machine-readable self-description)
    Commands,
    /// Report version, protocol version and platform info
    Status,
    #[command(flatten)]
    Desktop(DesktopCommand),
    #[command(flatten)]
    Flow(FlowCommand),
    /// Capture the interactive element tree of a window as refs (@e1..)
    Snapshot {
        /// Target window by title substring or hwnd:<n> (@eN refs require it; otherwise focused window)
        #[arg(long, conflicts_with = "snap_hwnd")]
        app: Option<String>,
        /// Target window by native handle (decimal or 0x-hex; untitled special windows
        /// like the Win11 taskbar XAML island are only reachable this way)
        #[arg(long, id = "snap_hwnd", value_name = "HWND")]
        hwnd: Option<String>,
        /// Keep only interactive elements and their ancestor chain (token saver)
        #[arg(long)]
        skeleton: bool,
        /// Content-scoped view: keep only Document (web/document body) subtrees,
        /// drop app chrome entirely; combines with --skeleton (folding applies
        /// inside the document only) — the token saver for browsers/editors
        #[arg(long, value_enum)]
        view: Option<ViewMode>,
    },
    /// Preview one element without clicking or typing; close the preview or let it expire
    Preview {
        /// UIA element selector or snapshot-bound @eN (coordinates are not supported)
        target: String,
        /// Target window title substring or hwnd:<n>
        #[arg(long)]
        app: String,
        /// Snapshot identity required for @eN
        #[arg(long)]
        snapshot: Option<String>,
        /// Additional snapshot-bound @eN candidates; repeat to enable explicit selection (2..20 total)
        #[arg(long)]
        candidate: Vec<String>,
        /// Intended action shown to the user (maximum 160 characters; no control characters)
        #[arg(long, default_value = "Preview only; no action will be performed")]
        message: String,
        /// Display duration in milliseconds, 500..30000
        #[arg(long, default_value_t = 3000)]
        duration: u64,
    },
    /// Click an element (UIA Invoke by default; --physical for a real mouse click)
    Click {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | xy:<x>,<y> (physical-only; needs --app)
        target: String,
        /// Target window by title substring or hwnd:<n> (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
        /// Physically move the mouse and click at the UIA clickable point
        /// (uses the real pointer + input lock; UIA Invoke is the default)
        #[arg(long)]
        physical: bool,
        /// Hold these modifier(s) atomically during a --physical click (ctrl = multi-select, shift = range)
        #[arg(long, requires = "physical", value_delimiter = ',')]
        with: Vec<String>,
        /// Suppress activation: temporarily set WS_EX_NOACTIVATE on the target
        /// window root — the click lands by coordinates but does not bring the
        /// window to the foreground or steal focus (pointer-only; keyboard
        /// commands refuse it fail-closed); cursor is returned afterwards
        #[arg(long, requires = "physical")]
        no_activate: bool,
        /// Post-condition: window:<substr> must NEWLY appear after the click
        #[arg(long)]
        expect: Option<String>,
        /// Timeout for --expect in milliseconds (default 2000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Read properties of an element
    Get {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | xy:<x>,<y> (physical-only; needs --app)
        target: String,
        /// Property: name | role | automation_id | offscreen | value | text (default: all)
        #[arg(long)]
        property: Option<String>,
        /// Target window by title substring or hwnd:<n> (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
    },
    /// Launch a program with optional arguments
    Launch {
        /// Wait for a unique native window title match, restore and verify foreground
        #[arg(long)]
        wait_window: Option<String>,
        /// Window readiness timeout in milliseconds (requires --wait-window)
        #[arg(long, requires = "wait_window", value_parser = clap::value_parser!(u64).range(1..=60000))]
        timeout: Option<u64>,
        /// Program and arguments (everything after `launch`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Type text into an element via keyboard (focuses the element first)
    Type {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Text to type (literal)
        text: String,
        /// Target window by title substring (required: keyboard injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
        /// Deliver via clipboard + ctrl+v (the only reliable text channel
        /// under a Chinese-mode IME; uses the shared clipboard, reported in output)
        #[arg(long)]
        paste: bool,
        /// Read back the complete value on the same element within 2s; never repeats input
        #[arg(long)]
        verify_value: Option<String>,
    },
    /// Capture pixels as a PNG file (explicit fallback when UIA is blind)
    Screenshot {
        /// Capture this window (title substring, or --hwnd; PrintWindow-based)
        #[arg(long, conflicts_with_all = ["shwnd", "srect", "selement"])]
        app: Option<String>,
        /// Capture the window at this native handle (decimal or 0x-hex)
        #[arg(long, id = "shwnd", value_name = "HWND", conflicts_with_all = ["app", "srect", "selement"])]
        hwnd: Option<String>,
        /// Capture a screen region: <x>,<y>,<w>,<h> physical pixels
        #[arg(long, id = "srect", value_name = "X,Y,W,H", conflicts_with_all = ["app", "shwnd", "selement"])]
        rect: Option<String>,
        /// Capture an element's bounding rectangle (goes through the locator chain)
        #[arg(long, id = "selement", conflicts_with_all = ["app", "shwnd", "srect"])]
        element: Option<String>,
        /// Window title substring for --element scoping
        #[arg(long, requires = "selement")]
        sapp: Option<String>,
        /// Anchor disambiguation for --element
        #[arg(long, requires = "selement")]
        snear: Option<String>,
        /// Output PNG path (default: %LOCALAPPDATA%\actl\shots, keep-20)
        #[arg(long)]
        out: Option<String>,
    },
    /// Paste (ctrl+v) into the current focus/selection of the target window
    Paste {
        /// Target window by title substring or hwnd:<n> (required: injection is fail-closed)
        #[arg(long)]
        app: String,
    },
    /// Press a key combination acting on the current focus (e.g. ctrl+s)
    Press {
        /// Key expression: ctrl+s, ctrl+shift+t, enter, f5 ...
        keys: String,
        /// Assert this exact element already has focus; do not refocus or guess a field
        #[arg(long)]
        target: Option<String>,
        /// Bind a target ref to its snapshot identity
        #[arg(long, requires = "target")]
        snapshot: Option<String>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | xy:<x>,<y> (physical-only; needs --app)
        target: String,
        /// Value to set
        value: String,
        /// Target window by title substring or hwnd:<n> (@eN refs require it; otherwise focused window)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor element's name
        #[arg(long)]
        near: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
    },
    /// Right-click an element (physical; opens context menus)
    RightClick {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
        /// Suppress activation: click lands but the window does not come to the
        /// foreground (WS_EX_NOACTIVATE guard; cursor returned afterwards)
        #[arg(long)]
        no_activate: bool,
    },
    /// Double-click an element (physical)
    DoubleClick {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
        /// Suppress activation: click lands but the window does not come to the
        /// foreground (WS_EX_NOACTIVATE guard; cursor returned afterwards)
        #[arg(long)]
        no_activate: bool,
    },
    /// Middle-click an element (browser: link in new tab; taskbar: new instance)
    MiddleClick {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
        /// Suppress activation: click lands but the window does not come to the
        /// foreground (WS_EX_NOACTIVATE guard; cursor returned afterwards)
        #[arg(long)]
        no_activate: bool,
    },
    /// Triple-click an element (editors: paragraph-level selection)
    TripleClick {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
        /// Suppress activation: click lands but the window does not come to the
        /// foreground (WS_EX_NOACTIVATE guard; cursor returned afterwards)
        #[arg(long)]
        no_activate: bool,
    },
    /// Move the pointer onto an element (hover; physical)
    Hover {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
        /// Suppress activation: pointer lands but the window does not come to
        /// the foreground (WS_EX_NOACTIVATE guard; cursor returned afterwards)
        #[arg(long)]
        no_activate: bool,
    },
    /// Drag from one element onto another (physical, left button held)
    Drag {
        /// Drag source target
        target: String,
        /// Drag destination target
        to: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
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
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
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
        /// Target window by title substring or hwnd:<n> (required: injection is fail-closed)
        #[arg(long)]
        app: Option<String>,
    },
    /// Release a key held by key-down (must pair)
    KeyUp {
        /// Key expression (same as the paired key-down)
        keys: String,
        /// Target window by title substring or hwnd:<n> (required: injection is fail-closed)
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
        /// Read channel for --value: value (legacy fallback), name, or text
        #[arg(long, requires = "value", value_parser = ["value", "name", "text"])]
        property: Option<String>,
        /// Expected value (exact unless --contains)
        expected: Option<String>,
        /// Substring match instead of exact
        #[arg(long)]
        contains: bool,
        /// Assert a toggle is On
        #[arg(long)]
        checked: Option<String>,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
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
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
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
    ListWindows {
        /// Enumerate Win32 HWNDs directly, including hidden and untitled windows; no UIA dependency
        #[arg(long)]
        native: bool,
        /// Read exactly this HWND (including child windows); requires --native
        #[arg(long, requires = "native")]
        hwnd: Option<String>,
    },
    /// Find elements by condition (token-saving targeted search vs snapshot)
    Find {
        /// Resolve one strict uia:{"match":{"role":"Button","name":"OK"}} query; no transient ref
        #[arg(long, conflicts_with_all = ["name", "role", "id", "limit"])]
        target: Option<String>,
        /// Match element name containing this substring
        #[arg(long)]
        name: Option<String>,
        /// Match control role exactly, case-insensitive (Button, Edit, Document ...)
        #[arg(long)]
        role: Option<String>,
        /// Match AutomationId exactly
        #[arg(long)]
        id: Option<String>,
        /// Target window by title substring or hwnd:<n> (default: focused window)
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
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Disambiguate multi-matches by picking the one nearest to this anchor
        #[arg(long)]
        near: Option<String>,
    },
    /// Toggle an element's state (checkbox/switch; reports the resulting state)
    Toggle {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
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
        /// Wait until the system has been idle (no keyboard/mouse input
        /// anywhere) for this many milliseconds — the "user is away" gate
        /// before long batches; observes physical input only, not UIA injection
        #[arg(
            long,
            value_parser = clap::value_parser!(u64).range(1..=86_400_000),
            conflicts_with_all = ["element", "window", "gone"]
        )]
        idle: Option<u64>,
        /// Wait until --element satisfies a property predicate:
        /// value=<expected> | name=<substr> | text=<expected> | checked=on|off (requires --element)
        #[arg(long, requires = "element")]
        property: Option<String>,
        /// Wait until --element settles: its observed sample (text > value > name
        /// channel) stops changing across consecutive polls (scroll/navigation
        /// load detection; sample interval settle_ms, stable window ~500ms)
        #[arg(long, requires = "element", conflicts_with = "property")]
        stable: bool,
        /// Substring semantics for the value/name predicate (exact by default)
        #[arg(long, requires = "property")]
        contains: bool,
        /// Target window by title substring or hwnd:<n> (for --element)
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
        Command::Preview { app, .. } => Some(app.clone()),
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
        Command::Preview { .. } => "preview",
        Command::Desktop(DesktopCommand::DisplayStatus) => "display-status",
        Command::Desktop(DesktopCommand::ExecutionStatus) => "execution-status",
        Command::Desktop(DesktopCommand::ExecutionRecover) => "execution-recover",
        Command::Desktop(DesktopCommand::TaskArchive { .. }) => "task-archive",
        Command::Flow(FlowCommand::Validate { .. }) => "flow-validate",
        Command::Flow(FlowCommand::Run { .. }) => "flow-run",
        Command::Flow(FlowCommand::Status { .. }) => "flow-status",
        Command::Flow(FlowCommand::Pause { .. }) => "flow-pause",
        Command::Flow(FlowCommand::Resume { .. }) => "flow-resume",
        Command::Desktop(DesktopCommand::History { .. }) => "history",
        Command::Desktop(DesktopCommand::DisplayStart) => "display-start",
        Command::Desktop(DesktopCommand::MinimizeWindow { .. }) => "minimize-window",
        Command::Desktop(DesktopCommand::RestoreWindow { .. }) => "restore-window",
        Command::Desktop(DesktopCommand::DesktopPrepare { .. }) => "desktop-prepare",
        Command::Desktop(DesktopCommand::DesktopRestore { .. }) => "desktop-restore",
        Command::Snapshot { .. } => "snapshot",
        Command::Click { .. } => "click",
        Command::Get { .. } => "get",
        Command::Launch { .. } => "launch",
        Command::Type { .. } => "type",
        Command::Press { .. } => "press",
        Command::Paste { .. } => "paste",
        Command::Screenshot { .. } => "screenshot",
        Command::SetValue { .. } => "set-value",
        Command::RightClick { .. } => "right-click",
        Command::DoubleClick { .. } => "double-click",
        Command::Hover { .. } => "hover",
        Command::MiddleClick { .. } => "middle-click",
        Command::TripleClick { .. } => "triple-click",
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
        Command::ListWindows { .. } => "list-windows",
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
        "flow-validate" => {
            "Example:\n  actl flow-validate --file task.yml\nValidates all steps and output references before any action. outputs select typed read-only data via JSON Pointer; bind replaces a whole args entry by index. Resolved CLI arguments are revalidated before execution.\nErrors: PROTOCOL invalid YAML/command/policy | INTERNAL file error"
        }
        "flow-run" => {
            "Example:\n  actl flow-run --file task.yml --id report-01 --paused\nUnique ID; saves immutable plan and durable state. Physical actions retain explicit authorization.\nErrors: PROTOCOL invalid/reused ID | NOT_ACTIONABLE executor busy | ABORTED stop | INTERNAL storage"
        }
        "flow-status" => {
            "Example:\n  actl flow-status --id report-01\nReports run state, declared saved outputs, pause request and runner_active. Output contents are not redacted by error evidence policy. An inactive running record is interrupted.\nErrors: PROTOCOL invalid ID/state | INTERNAL storage"
        }
        "flow-pause" => {
            "Example:\n  actl flow-pause --id report-01\nRequests a boundary pause; workflow waits also check each poll. Only status=paused confirms it. In-flight COM calls and other CLI calls are not interrupted.\nErrors: PROTOCOL invalid ID/state | INTERNAL storage"
        }
        "flow-resume" => {
            "Example:\n  actl flow-resume --id report-01\nUses the saved plan; never clears emergency stop. Optional resume_check verifies current context before uncertain-result checks. retry=stale_read retries only read-only STALE_REF, at most three calls. --retry-step requires an explicit failed assertion; observation errors never authorize write replay.\nErrors: PROTOCOL invalid state/step | NOT_ACTIONABLE executor busy | ABORTED stop | INTERNAL storage"
        }
        "task-archive" => {
            "Example:\n  actl task-archive --file report.json\nImmutable caller report, retained 30 days; never implies verified task success.\nErrors: PROTOCOL invalid report | INTERNAL file/storage failure"
        }
        "history" => {
            "Example:\n  actl history --task report-01 --step submit --reports --limit 20\nContent-minimal events, newest calls first. integrity validates sequence, identity and boundaries; unfinished may be running or interrupted. logging_health reports recorded journal/state failures, never guarantees complete logging. Control request IDs correlate pause/resume requests without changing desktop feedback.\nErrors: PROTOCOL invalid task/limit | INTERNAL storage failure (health evidence included)"
        }
        "display-status" => {
            "Example:\n  actl display-status\nReports native readiness, reason, lease, popup diagnostics, and sticky stop state; never starts the companion.\nErrors: INTERNAL unavailable state storage"
        }
        "display-start" => {
            "Example:\n  actl display-start\nStarts sibling actl-signal.exe if missing; restores a hidden/offscreen matching display; waits up to 4s for a matching visible frame. Does not clear stop.\nErrors: NOT_ACTIONABLE missing/hidden/incompatible display | INTERNAL launch failure"
        }
        "minimize-window" => {
            "Example:\n  actl minimize-window --hwnd 12345\nExactly one title, --pid or --hwnd. Only ordinary unowned windows.\nErrors: PROTOCOL missing selector | NOT_FOUND | AMBIGUOUS | NOT_ACTIONABLE | ABORTED"
        }
        "restore-window" => {
            "Example:\n  actl restore-window --hwnd 12345\nExactly one title, --pid or --hwnd. Restores the selected window.\nErrors: PROTOCOL missing selector | NOT_FOUND | AMBIGUOUS | NOT_ACTIONABLE | ABORTED"
        }
        "desktop-prepare" => {
            "Example:\n  actl desktop-prepare --all\nExplicit opt-in. Returns session id for restoration. Failures retain partial evidence and session.\nErrors: PROTOCOL missing --all | NOT_ACTIONABLE | ABORTED | INTERNAL journal failure"
        }
        "desktop-restore" => {
            "Example:\n  actl desktop-restore --session <session-id>\nRestores unchanged minimized window instances; reports skips. Never clears stop.\nErrors: PROTOCOL invalid record/id | NOT_FOUND session | NOT_ACTIONABLE | ABORTED"
        }
        "commands" => {
            "Example:\n  actl commands\nOutput: envelope with every command + arg schema (machine self-description)"
        }
        "status" => "Example:\n  actl status\nOutput: version, protocol version, platform",
        "snapshot" => {
            "Example:\n  actl snapshot --app Notepad --skeleton\n  actl snapshot --app Edge --view content --skeleton\n--skeleton keeps interactive elements + collapsed ancestors (token saver).\n--view content keeps only Document (web/document body) subtrees, drops app chrome;\ncombines with --skeleton (folding inside the document) — the browser/editor token saver.\nOutput: truncated + truncation_reasons report actual node/depth cuts.\nErrors: NOT_FOUND no window/Document | INTERNAL/PERM_DENIED/STALE_REF observation failed | NOT_ACTIONABLE incomplete content scope"
        }
        "click" => {
            "Example:\n  actl click @e3 --app Notepad --snapshot s1a2b\n  actl click name:item --app Explorer --physical --with ctrl\n(multi-select)\n  actl click id:session_item_x --app 微信 --physical --no-activate\n  actl click id:session_item_x --app 微信 --expect element:id:chat_message_list\n--expect window:<substr> = a NEW window must appear; element:<target> = postcondition:\nthe element must resolve afterwards (implicit wait, default 2s) — the materialization\ncheck for apps whose UIA actions report success without effect.\n--no-activate (--physical only): the click lands but the window does not come to the\nforeground or steal focus (WS_EX_NOACTIVATE guard; cursor returned afterwards).\nErrors: NOT_FOUND no match | AMBIGUOUS multi-match (candidates returned) | STALE_REF stale @eN (re-snapshot) | NOT_ACTIONABLE no pattern, occluded, or xy: point outside window bounds | PERM_DENIED foreground guard (--physical) or no-activate style rejected (elevated window) | ASSERTION_FAILED --expect unmet | PROTOCOL xy: without --physical/--app, bad --expect, or --no-activate without --physical"
        }
        "execution-status" => {
            "Usage: actl execution-status\nArguments: none\nExample:\n  actl execution-status\nReports ready/stopped/repair_required; never grants permission.\nErrors: PROTOCOL invalid arguments"
        }
        "execution-recover" => {
            "Usage: actl execution-recover\nArguments: none (invocation explicitly requests recovery)\nExample:\n  actl execution-recover\nArchives raw state and invalidates old calls. No task resumes and no permission is granted.\nErrors: NOT_ACTIONABLE incompatible companion or unreadable state; PROTOCOL invalid arguments"
        }
        "preview" => {
            "Example:\n  actl preview id:saveButton --app MyApp --message 'Save this draft' --duration 5000\n  actl preview @e3 --app MyApp --snapshot s123\n  actl preview @e3 --candidate @e7 --app MyApp --snapshot s123 --duration 30000\nPreview only: no click, typing, clipboard change or desktop consent. Repeated --candidate enables explicit selection among 2..20 distinct refs from one snapshot. Select returns a target and identity; revalidate before executing. Close or expiry returns no selection. Target movement or identity changes stop the preview. Not allowed in persisted flows.\nErrors: PROTOCOL invalid duration/message/candidates, coordinate or unbound ref | NOT_FOUND | AMBIGUOUS | STALE_REF target changed | NOT_ACTIONABLE invisible/invalid bounds or unavailable preview | ABORTED stop | INTERNAL observation/render failure"
        }
        "get" => {
            "Example:\n  actl get name:状态 --app 设置 --property value\nProperties: name role automation_id offscreen value text (text = TextPattern body; Chromium Document value is the URL).\nEmpty strings are values. Value falls back to Text only when ValuePattern is unsupported. Default reads list unsupported channels; explicit unsupported reads fail.\nErrors: PROTOCOL unknown property | NOT_FOUND no match | AMBIGUOUS multi-match | NOT_ACTIONABLE unsupported channel | STALE_REF stale provider | PERM_DENIED access denied | INTERNAL observation failed"
        }
        "launch" => {
            "Example:\n  actl launch notepad\nOptional: actl launch --wait-window Notepad --timeout 5000 notepad\nOptions precede the program. Default returns window_ready:false (process started only). --wait-window matches an explicit title, reports matched PID/HWND, and verifies foreground; no automatic relaunch. For an existing instance use focus-window.\nErrors: NOT_FOUND program | TIMEOUT no window | AMBIGUOUS multiple windows | NOT_ACTIONABLE handoff/activation | ABORTED stop"
        }
        "type" => {
            "Example:\n  actl type id:Edit 'hello world' --app Notepad --paste\n--verify-value <expected>: poll the same element for an exact complete value for up to 2s after input; no write retries.\n--paste: clipboard+ctrl+v, the only reliable channel under a Chinese-mode IME (uses the shared clipboard, reported in output).\nErrors: PROTOCOL --app required (injection is fail-closed) | NOT_ACTIONABLE foreground guard aborted | NOT_FOUND / AMBIGUOUS target | NOT_ACTIONABLE cannot take or verify exact focus | STALE_REF identity changed | ASSERTION_FAILED --verify-value readback failed (input may already be delivered)"
        }
        "screenshot" => {
            "Example:
  actl screenshot --app Notepad
  actl screenshot --element id:CalculatorResults --app 计算器
  actl screenshot --rect 100,100,400,300
Explicit pixel fallback for UIA-blind content (canvases, self-drawn menus, image
messages) and visual verification. Pixels go to a PNG file; the envelope carries
only path+metadata (vision consumption is the caller's decision). Region mode
captures what is ON SCREEN (occluders included); window mode renders the window
itself even when occluded.
Errors: PROTOCOL bad args | NOT_FOUND no window | NOT_ACTIONABLE minimized/oversize/empty bounds"
        }
        "paste" => {
            "Example:
  actl paste --app Excel
Ctrl+v into the CURRENT selection (no element needed): the Excel cell route,
file-manager rename, selected items. Deliberately does NOT focus an element
(focusing would move the selection); window foreground + guard + lock only.
Errors: PROTOCOL --app required | PERM_DENIED foreground guard | NOT_FOUND window"
        }
        "press" => {
            "Example:\n  actl press ctrl+s --app Notepad --expect window:另存为\n--target <selector>: assert this element already has keyboard focus, without refocusing. --snapshot binds a target ref to its captured identity.\n--expect window:<substr> = a NEW window must appear (pre-existing same-title does not count);\nelement:<target> = postcondition: the element must resolve afterwards (implicit wait, default 2s).\nErrors: PROTOCOL --app required or bad key expr/--expect | NOT_ACTIONABLE foreground guard aborted | ASSERTION_FAILED --expect never met | NOT_ACTIONABLE target focus mismatch | STALE_REF snapshot identity changed"
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
            "Example:\n  actl right-click name:file.txt --app 项目\nPhysical action (naming = authorization): opens context menus.\n--no-activate: the click lands but the window does not come to the foreground\n(WS_EX_NOACTIVATE guard on the window root; cursor returned afterwards).\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded | PERM_DENIED cannot set the no-activate style (elevated window)"
        }
        "double-click" => {
            "Example:\n  actl double-click name:file.txt --app 项目\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
        }
        "middle-click" => {
            "Example:
  actl middle-click name:link --app Browser
Browser: open link in new tab; taskbar: new app instance.
Errors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
        }
        "triple-click" => {
            "Example:
  actl triple-click role:Document --app Notepad
Editors: paragraph-level selection (double=word, triple=paragraph).
Errors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded"
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
            "Example:\n  actl verify --value id:CalculatorResults '56,877' --property name --contains --app 计算器\nImmediate assertion (vs wait): fails now, not later; failure carries expected/actual/target/channel. --property name|value|text selects an explicit channel; omitted uses Value then Text fallback.\nErrors: ASSERTION_FAILED | NOT_FOUND / AMBIGUOUS | PROTOCOL bad flags"
        }
        "extract" => {
            "Example:\n  actl extract role:List --app 系统信息\nGrid rows first; tree fallback only when GridPattern is unsupported (fallback_reason: grid_pattern_unsupported). Empty cells and empty grids are preserved.\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no rows, budget exceeded or table changed | INTERNAL/PERM_DENIED/STALE_REF observation failed"
        }
        "focus-window" => {
            "Example:\n  actl focus-window 记事本\nKeyboard commands act on global focus — focus the window first in multi-window scenes.\nErrors: NOT_FOUND / AMBIGUOUS window"
        }
        "close-window" => {
            "Example:\n  actl close-window 记事本\nGraceful WM_CLOSE (unsaved work may prompt).\nErrors: NOT_FOUND / AMBIGUOUS window | NOT_ACTIONABLE no native handle"
        }
        "list-windows" => {
            "Example:\n  actl list-windows\n  actl list-windows --native\n  actl list-windows --native --hwnd 123\nEach entry: title/class/pid + stacking facts (foreground, topmost, minimized, visible, z_order 0=top, rect, owner_title).\n--native reads Win32 top-level HWNDs without UIA, including hidden/untitled windows; not a list of task-owned resources. --hwnd requires --native and directly queries one handle, including child windows.\nErrors: INTERNAL native observation failed; never substituted with an empty list | PROTOCOL invalid HWND"
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
            "Example:\n  actl wait --element name:完成 --property value=OK --timeout 8000 --app 应用\n  actl wait --element role:Document --stable --app Edge\n  actl wait --idle 8000 --timeout 120000\nPredicates: value=<expected> | name=<substr> | text=<expected> | checked=on|off; --contains for substring; existence implied.\n--stable waits until the element's observed sample (text > value > name) stops changing\nacross consecutive polls — settle detection after scroll/navigation.\n--idle waits until no keyboard/mouse input system-wide for N ms — the \"console is free\"\ngate before long batches; observes physical input only (UIA-injected input stays invisible).\nErrors: TIMEOUT predicate unmet, element absent, content still changing, or user still active | PROTOCOL bad predicate/flags | ASSERTION_FAILED (--window)"
        }
        "batch" => {
            "Example:\n  actl batch --json '[\"focus-window Notepad\",\"press ctrl+s --app Notepad\"]'\nStops on first error; steps[] carries per-step ok/duration. windows_created is the native HWND difference, not proof of task ownership. A final inventory failure preserves steps and reports window_observation_error instead of windows_created. steps may carry blocked_by for an unexpected foreground window.\nErrors: PROTOCOL bad JSON/nesting | INTERNAL initial native enumeration failed (no steps run) | the first failing step's code is propagated"
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
            let selector_help = if matches!(
                sub.get_name(),
                "find"
                    | "click"
                    | "get"
                    | "type"
                    | "set-value"
                    | "select"
                    | "toggle"
                    | "right-click"
                    | "double-click"
                    | "triple-click"
                    | "hover"
                    | "drag"
                    | "scroll"
                    | "extract"
                    | "press"
                    | "screenshot"
                    | "verify"
                    | "wait"
            ) {
                "\nStrict UIA target: uia:{\"match\":{\"role\":\"Button\",\"name\":\"OK\"},\"within\":{\"id\":\"panel\"},\"scope\":\"descendants\"}\nConditions: role (case-insensitive exact), id/name (exact), name_contains (case-sensitive substring).\nScope: children or descendants (default). Container must be unique. No fuzzy/ordinal/--near fallback.\nExample (PowerShell 7.3+): actl find --target 'uia:{\"match\":{\"role\":\"Button\",\"name\":\"OK\"}}' --app MyApp\nfind --target returns one persistent target, no snapshot ref; conflicts with --name/--role/--id/--limit.\nErrors: PROTOCOL invalid query | NOT_FOUND zero matches | AMBIGUOUS multiple matches | INTERNAL provider read failed | NOT_ACTIONABLE traversal budget exceeded."
            } else {
                ""
            };
            *sub = std::mem::take(sub).after_help(format!("{text}{selector_help}"));
        }
    }
    root
}
