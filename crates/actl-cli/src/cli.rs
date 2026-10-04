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
    /// Report the three focus views (UIA focused element, physical focus root, foreground window) for selection diagnostics
    FocusReport,
    /// Report the app's front guidance layer (owned popups + in-tree dialog surfaces); read-only
    FrontReport {
        /// Target window by title substring or hwnd:<n>
        app: String,
    },
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
        /// Merge every event of the task into one time-ordered stream instead of per-call groups
        #[arg(long)]
        timeline: bool,
        /// Maximum calls (1-100); with --timeline the newest events (1-2000)
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
        /// UTF-8 YAML/JSON string map for declared workflow inputs
        #[arg(long)]
        inputs: Option<String>,
        /// Include the validated concrete plan for resource-preparation tools
        #[arg(long)]
        emit_plan: bool,
    },
    /// Run a validated workflow and save resumable state under a unique ID
    #[command(name = "flow-run")]
    Run {
        #[arg(long)]
        file: String,
        /// Resolve inputs once; resume uses the saved concrete plan
        #[arg(long)]
        inputs: Option<String>,
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
    /// List captured failure bundles (screenshot/windows/UIA/front layer) of a run
    #[command(name = "flow-evidence")]
    Evidence {
        #[arg(long)]
        id: String,
        /// Only bundles of this step id (e.g. s6); omit for all steps
        #[arg(long)]
        step: Option<String>,
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
        /// Answer a pending exploration decision (choice id from the card, e.g. c2)
        #[arg(long)]
        decision: Option<String>,
        /// Optional responder confidence note recorded with the answer (≤32 chars)
        #[arg(long, requires = "decision")]
        confidence: Option<String>,
    },
    /// Batch-archive historical non-completed flow states; preserves all evidence
    #[command(name = "flow-clean")]
    Clean {
        /// Only archive flows whose last update is older than this many minutes
        #[arg(long, default_value_t = 60)]
        older_than_minutes: u64,
        /// Archive without renaming (dry run: list what would move)
        #[arg(long)]
        dry_run: bool,
    },
    /// Extract a pack-format route fragment from a completed exploration run
    #[command(name = "explore-writeback")]
    Writeback {
        /// Exploration run ID (must be completed with goal_met=true)
        #[arg(long)]
        id: String,
        /// Write the YAML fragment to this file (default: envelope data only)
        #[arg(long)]
        out: Option<String>,
    },
}

#[derive(Subcommand)]
pub(crate) enum RecordCommand {
    /// Start a GUI recording session (spawns a detached bounded worker)
    #[command(name = "record-start")]
    Start {
        /// Session id (default: rec-<unix ms>); fails if the id was already used
        #[arg(long)]
        id: Option<String>,
        /// Hard duration cap in seconds; the worker finalizes honestly on cap
        #[arg(long, default_value_t = 600)]
        max_seconds: u64,
    },
    /// Internal recording session process (spawned by record-start; same semantics when run directly)
    #[command(name = "record-worker")]
    Worker {
        /// Session id to capture
        #[arg(long)]
        id: String,
        /// Hard duration cap in seconds
        #[arg(long, default_value_t = 600)]
        max_seconds: u64,
    },
    /// Stop the active recording session and return the compiled draft
    #[command(name = "record-stop")]
    Stop {
        /// Session id (default: the single active session; ambiguous requires --id)
        #[arg(long)]
        id: Option<String>,
    },
    /// Report recording session state and trace statistics (read-only)
    #[command(name = "record-status")]
    Status {
        /// Session id (omit to list active sessions)
        #[arg(long)]
        id: Option<String>,
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
    #[command(flatten)]
    Record(RecordCommand),
    /// Capture the interactive element tree of a window as refs (@e1..)
    Snapshot {
        /// Target window by title substring or hwnd:<n> (@eN refs require it; otherwise focused window)
        #[arg(long, conflicts_with = "snap_hwnd")]
        app: Option<String>,
        /// Target window by native handle (decimal or 0x-hex)
        #[arg(long, id = "snap_hwnd", value_name = "HWND")]
        hwnd: Option<String>,
        /// Keep only interactive elements + ancestors (token saver)
        #[arg(long)]
        skeleton: bool,
        /// Keep only Document (web body) subtrees, drop app chrome
        #[arg(long, value_enum)]
        view: Option<ViewMode>,
        /// Task keyword filter: keep hits + ancestors (zero hits = empty, no fallback)
        #[arg(long, value_name = "K1,K2", conflicts_with = "view")]
        keywords: Option<String>,
        /// Observe only this unique container (strict uia: selector)
        #[arg(long)]
        within: Option<String>,
        /// Maximum depth from container, 0..40 (root = 0)
        #[arg(long)]
        depth: Option<u32>,
        /// Maximum observed nodes including the container, 1..5000
        #[arg(long)]
        limit: Option<usize>,
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
        #[arg(
            long,
            default_value = "Preview only; no action. Box = automation operating rect (UIA provider); deviation from the visual border is a physical-click risk signal"
        )]
        message: String,
        /// Display duration in milliseconds, 500..30000
        #[arg(long, default_value_t = 3000)]
        duration: u64,
    },
    /// Click an element (UIA Invoke by default; --physical for a real mouse click)
    Click {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Real mouse click (vs UIA Invoke default)
        #[arg(long)]
        physical: bool,
        /// Hold these modifier(s) atomically during a --physical click (ctrl = multi-select, shift = range)
        #[arg(long, requires = "physical", value_delimiter = ',')]
        with: Vec<String>,
        /// Suppress activation: temporarily set WS_EX_NOACTIVATE on the target
        /// Click without bringing the window to the foreground (cursor returned)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// Drag source target (element, or xy:x,y for coordinate/marquee drags)
        target: String,
        /// Drag destination target (element, or xy:x,y)
        to: String,
        /// Absolute physical waypoints x,y (repeatable; canvas/signature trajectories —
        /// the drag traverses start → waypoints → destination with linear interpolation)
        #[arg(long = "path", value_name = "X,Y")]
        path: Vec<String>,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes);
        /// required for xy:→xy: drags (both endpoints guarded against the window bounds)
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
        /// Hold the key down for this many milliseconds, then release (long press;
        /// 50-60000; stop requests interrupt the hold and release immediately)
        #[arg(long, value_parser = clap::value_parser!(u64).range(50..=60_000))]
        duration: Option<u64>,
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
    VerifyFile {
        #[arg(long)]
        path: String,
        #[arg(long, value_parser = ["text", "json", "docx", "docx-styles", "xlsx"])]
        kind: String,
        /// Expected JSON value; text files use a JSON string
        #[arg(long)]
        expected: String,
        /// Compare only declared object keys (array lengths remain exact)
        #[arg(long)]
        subset: bool,
    },
    /// Assert the exact named rows of a complete UIA snapshot
    VerifyRows {
        #[arg(long)]
        app: String,
        #[arg(long)]
        prefix: String,
        #[arg(long)]
        expected: String,
    },
    /// Verify a unique native window belongs to the expected executable and task path
    VerifyOwner {
        #[arg(long)]
        app: String,
        #[arg(long)]
        program: String,
        #[arg(long)]
        token: String,
        #[arg(long)]
        instance: Option<String>,
        #[arg(long)]
        hwnd: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Assert native window existence or state without changing focus
    VerifyWindow {
        /// Unique window title substring or hwnd:<decimal/hex>
        #[arg(long)]
        app: String,
        /// Native state to assert without activating the window
        #[arg(long, value_parser = ["exists", "absent", "foreground", "minimized", "visible"])]
        state: String,
    },
    /// Verify both the owned window and its original process instance have exited
    VerifyClosed {
        #[arg(long)]
        hwnd: String,
        #[arg(long)]
        pid: u32,
        #[arg(long)]
        instance: String,
    },
    /// Assert complete clipboard text without changing it
    VerifyClipboard {
        #[arg(long)]
        expected: String,
        #[arg(long)]
        normalize_lines: bool,
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
        /// Normalize CRLF/CR to LF and trim trailing LF for exact text assertions
        #[arg(long, requires = "value", conflicts_with = "contains")]
        normalize_lines: bool,
        /// Compare one percent-decoded URL query parameter from the value channel
        #[arg(long, requires = "value", conflicts_with_all = ["contains", "normalize_lines", "property"])]
        url_query: Option<String>,
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
        /// Send the title-bar close command (WM_SYSCOMMAND/SC_CLOSE) instead of a bare
        /// WM_CLOSE — same as clicking the X button; apps that only hide on WM_CLOSE
        /// (Excel after a proper document close) actually exit on this
        #[arg(long)]
        system_close: bool,
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
    /// Launch an isolated Chromium-family browser (Edge/Chrome) and bind one tab as an actl session (CDP channel, docs/72)
    BrowserLaunch {
        /// Executable path or app-registry alias (e.g. edge); see `actl app-list`
        #[arg(long)]
        browser_executable: String,
        /// Initial page URL (default about:blank)
        #[arg(long)]
        url: Option<String>,
        /// Persistent profile name for login reuse (default: throwaway ephemeral profile)
        #[arg(long)]
        profile: Option<String>,
        /// Outbound proxy as HOST:PORT (e.g. 127.0.0.1:7890) — routes the browser via an
        /// explicit local proxy; no scheme/prefix allowed (not an arbitrary-arg escape hatch)
        #[arg(long)]
        proxy: Option<String>,
        /// Run without a window (HeadlessChrome UA — many sites serve 403 to it and the
        /// session is invisible to the user; default is a visible window)
        #[arg(long)]
        headless: bool,
        /// Disable extensions in a persistent profile (ephemeral sessions always run
        /// extension-free; persistent profiles keep extensions by default — they are legitimate
        /// automation assets, e.g. media sniffers)
        #[arg(long)]
        disable_extensions: bool,
        /// Startup timeout in milliseconds (default 15000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Navigate the session's tab to a URL and wait for the main frame load event
    BrowserNavigate {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// Destination URL (http:// https:// file:// about:blank)
        #[arg(long)]
        url: String,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Read the text (or a whitelisted property) of a unique element in the session's tab
    BrowserRead {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// CSS selector (mutually exclusive with --xpath)
        #[arg(long, conflicts_with = "r_x")]
        css: Option<String>,
        /// XPath expression (mutually exclusive with --css)
        #[arg(long, id = "r_x", conflicts_with = "css")]
        xpath: Option<String>,
        /// Whitelisted property instead of text: value | href | name | id | checked | tag
        /// (value covers input fields — textContent is empty for them)
        #[arg(long)]
        property: Option<String>,
        /// Locate inside the (cross-origin) child iframe instead of the main frame
        #[arg(long)]
        iframe: bool,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Type text into a unique input/textarea — guarded write with value read-back
    BrowserType {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// CSS selector (mutually exclusive with --xpath)
        #[arg(long, conflicts_with = "t_x")]
        css: Option<String>,
        /// XPath expression (mutually exclusive with --css)
        #[arg(long, id = "t_x", conflicts_with = "css")]
        xpath: Option<String>,
        /// Text to type (1-65536 chars; not echoed in output or evidence)
        #[arg(long)]
        text: String,
        /// Locate inside the (cross-origin) child iframe instead of the main frame
        #[arg(long)]
        iframe: bool,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Click a unique element — guarded write: stop check, target re-verify, unique match, dispatch, read-back
    BrowserClick {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// CSS selector (mutually exclusive with --xpath)
        #[arg(long, conflicts_with = "c_x")]
        css: Option<String>,
        /// XPath expression (mutually exclusive with --css)
        #[arg(long, id = "c_x", conflicts_with = "css")]
        xpath: Option<String>,
        /// Locate inside the (cross-origin) child iframe instead of the main frame
        #[arg(long)]
        iframe: bool,
        /// Assert the element text contains this substring after the click (post-write verification)
        #[arg(long)]
        expect_text: Option<String>,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Gracefully close the session's browser (CDP Browser.close) and retire the session
    BrowserClose {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// List active browser sessions, newest first (read-only; safe discovery for flows and agents)
    BrowserList,
    /// Register a local application path (stable) or pattern (Store apps with version wildcards)
    AppRegister {
        /// Short name for the app (1-64 [a-zA-Z0-9_-])
        name: String,
        /// Absolute executable path, or a glob pattern with * for Store version directories
        /// (e.g. C:/Program Files/WindowsApps/Microsoft.Paint_*/PaintApp/mspaint.exe)
        path: String,
        /// Path is a glob pattern (Store apps; auto re-resolves on version change)
        #[arg(long)]
        pattern: bool,
    },
    /// List registered local applications and their resolution status
    AppList,
    /// Remove a registered application by name
    AppRemove {
        /// The registered name to remove
        name: String,
    },
    /// Read-only browser assertions for flow verify steps (exactly one mode)
    BrowserVerify {
        /// Session id for --alive/--gone/--css/--xpath assertions
        #[arg(long)]
        session: Option<String>,
        /// With --session: the bound tab target answers (implies browser + endpoint alive)
        #[arg(long, requires = "session", conflicts_with_all = ["bv_gone", "bv_css", "bv_xpath"])]
        alive: bool,
        /// With --session: the session record no longer exists
        #[arg(long, requires = "session", id = "bv_gone", conflicts_with_all = ["alive", "bv_css", "bv_xpath"])]
        gone: bool,
        /// Login-state predicate: with --session, assert this CSS selector resolves uniquely
        #[arg(long, requires = "session", id = "bv_css", conflicts_with_all = ["alive", "bv_gone", "bv_xpath"])]
        css: Option<String>,
        /// Login-state predicate: with --session, assert this XPath resolves uniquely
        #[arg(
            long,
            requires = "session",
            id = "bv_xpath",
            conflicts_with_all = ["alive", "bv_gone", "bv_css"]
        )]
        xpath: Option<String>,
        /// With --css/--xpath: also assert the resolved element text contains this substring
        #[arg(long, requires = "session")]
        contains: Option<String>,
        /// Assert zero active sessions (post-close in a flow-owned lifecycle)
        #[arg(long, conflicts_with_all = ["session"])]
        none: bool,
        /// Assert the newest session's target responds (post-launch, before its id is known)
        #[arg(long, conflicts_with_all = ["session", "none"])]
        newest_alive: bool,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        /// load detection; two samples separated by settle_ms (default 250ms))
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
        Command::Desktop(DesktopCommand::FocusReport) => "focus-report",
        Command::Desktop(DesktopCommand::FrontReport { .. }) => "front-report",
        Command::Desktop(DesktopCommand::ExecutionRecover) => "execution-recover",
        Command::Desktop(DesktopCommand::TaskArchive { .. }) => "task-archive",
        Command::Flow(FlowCommand::Validate { .. }) => "flow-validate",
        Command::Flow(FlowCommand::Run { .. }) => "flow-run",
        Command::Flow(FlowCommand::Status { .. }) => "flow-status",
        Command::Flow(FlowCommand::Evidence { .. }) => "flow-evidence",
        Command::Flow(FlowCommand::Pause { .. }) => "flow-pause",
        Command::Flow(FlowCommand::Resume { .. }) => "flow-resume",
        Command::Flow(FlowCommand::Clean { .. }) => "flow-clean",
        Command::Flow(FlowCommand::Writeback { .. }) => "explore-writeback",
        Command::Record(RecordCommand::Start { .. }) => "record-start",
        Command::Record(RecordCommand::Worker { .. }) => "record-worker",
        Command::Record(RecordCommand::Stop { .. }) => "record-stop",
        Command::Record(RecordCommand::Status { .. }) => "record-status",
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
        Command::VerifyFile { .. } => "verify-file",
        Command::VerifyRows { .. } => "verify-rows",
        Command::VerifyOwner { .. } => "verify-owner",
        Command::VerifyClosed { .. } => "verify-closed",
        Command::VerifyWindow { .. } => "verify-window",
        Command::VerifyClipboard { .. } => "verify-clipboard",
        Command::Extract { .. } => "extract",
        Command::Select { .. } => "select",
        Command::Toggle { .. } => "toggle",
        Command::FocusWindow { .. } => "focus-window",
        Command::CloseWindow { .. } => "close-window",
        Command::ListWindows { .. } => "list-windows",
        Command::Find { .. } => "find",
        Command::BrowserLaunch { .. } => "browser-launch",
        Command::BrowserNavigate { .. } => "browser-navigate",
        Command::BrowserRead { .. } => "browser-read",
        Command::BrowserType { .. } => "browser-type",
        Command::BrowserClick { .. } => "browser-click",
        Command::BrowserClose { .. } => "browser-close",
        Command::BrowserList => "browser-list",
        Command::AppRegister { .. } => "app-register",
        Command::AppList => "app-list",
        Command::AppRemove { .. } => "app-remove",
        Command::BrowserVerify { .. } => "browser-verify",
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
        "browser-launch" => {
            "Example:\n  actl browser-launch --browser-executable \"C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe\" --url about:blank --proxy 127.0.0.1:7890\nLaunches an OWNED isolated browser (never the user's default profile — Chrome/Edge 136+ disables the debug port there; the daily browser's extensions/settings/cookies are untouched, separate data worlds) and binds one tab as the session. Default is a VISIBLE window (normal UA, user-visible automation, manual-login ready); --headless opts into a hidden session for background tasks (HeadlessChrome UA — many sites serve 403 to it, and no UIA fallback surface exists). --profile <name> keeps a persistent profile for one-time manual logins; same-profile relaunch REUSES the live session (reused:true). Extensions: ephemeral sessions always run extension-free; persistent profiles keep extensions (legitimate automation assets, e.g. media sniffers) unless --disable-extensions. --proxy HOST:PORT routes via an explicit local proxy (browser DoH can bypass system TUN fake-ip routing — the fix for silently unreachable sites). Startup succeeds only when the devtools endpoint answers; otherwise the owned process is reclaimed.\nErrors: PROTOCOL bad executable/url/profile/proxy | INTERNAL endpoint never appeared (reclaimed)"
        }
        "browser-navigate" => {
            "Example:\n  actl browser-navigate --session s1a2b3 --url https://example.com\nConnects to the session's browser, re-verifies the bound tab still exists, navigates and waits for the main-frame Page.frameNavigated event. Reports url/title/frame_id. Network-level load failures (chrome-error pages) fail explicitly with the net error in evidence — never silently reported as success.\nErrors: PROTOCOL bad url | NOT_FOUND unknown session / endpoint unreachable / target lost | TIMEOUT navigation event missing | INTERNAL network-level load failure"
        }
        "browser-read" => {
            "Example:\n  actl browser-read --session s1a2b3 --css \"#result span\"\n  actl browser-read --session s1a2b3 --css \"#username\" --property value\n  actl browser-read --session s1a2b3 --xpath \"//button[@id='go']\" --iframe\nReads the text (or a whitelisted property via --property: value/href/name/id/checked/tag) of exactly one element (hard uniqueness; value covers input fields — textContent is empty for them). --iframe addresses the cross-origin child iframe via its execution context. Text is capped at 64 KiB with a truncated flag.\nErrors: PROTOCOL selector 1-4096 chars / unknown --property / neither --css nor --xpath | NOT_FOUND no match / no iframe | AMBIGUOUS multiple matches | INTERNAL CDP layer"
        }
        "browser-type" => {
            "Example:\n  actl browser-type --session s1a2b3 --css \"input[name='text']\" --text \"hello world\"\nGuarded input write targeting a unique input/textarea: stop check, target re-verify, unique match, native-setter assignment + input/change events (React-compatible), then value read-back must equal the request (controlled-input rewrites fail as ASSERTION_FAILED). The typed text is never echoed in output or evidence; verify with browser-read --property value.\nErrors: PROTOCOL selector/text invalid | ABORTED stop active | NOT_FOUND no match/target lost | AMBIGUOUS multiple matches | NOT_ACTIONABLE not an input / dispatch failed | ASSERTION_FAILED value mismatch | INTERNAL CDP layer"
        }
        "browser-click" => {
            "Example:\n  actl browser-click --session s1a2b3 --css \"#submit\" --expect-text \"Saved\"\nGuarded write: stop-generation check, bound-target re-verify, unique-match locate, dispatch, then read-back. --expect-text turns the read-back into an assertion. A failed click is never replayed blindly — re-read to adjudicate first.\nErrors: PROTOCOL selector/expect-text invalid | ABORTED stop active | NOT_FOUND no match/target lost | AMBIGUOUS multiple matches | NOT_ACTIONABLE dispatch failed | ASSERTION_FAILED expectation unmet | INTERNAL CDP layer"
        }
        "browser-close" => {
            "Example:\n  actl browser-close --session s1a2b3\nCloses the OWNED browser via CDP Browser.close (graceful; never taskkill), removes the session file, and deletes ephemeral profiles. Persistent profiles are kept for reuse. If the endpoint is already unreachable the close is idempotent and reported as such.\nErrors: NOT_FOUND unknown session | INTERNAL endpoint still answers after close (session kept)"
        }
        "browser-list" => {
            "Example:\n  actl browser-list\nRead-only enumeration of active sessions (newest first): session_id/target_id/url/pid/port/mode. In YAML flows browser-launch cannot carry outputs (lifecycle command, not a read-only producer) — bind the session handle from this command instead (e.g. pointer /sessions/0/session_id).\nErrors: INTERNAL sessions dir unreadable"
        }
        "app-register" => {
            "Example:\n  actl app-register edge 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'\n  actl app-register paint 'C:/Program Files/WindowsApps/Microsoft.Paint_*/PaintApp/mspaint.exe' --pattern\nRegisters a local application in %LOCALAPPDATA%/actl/apps.json so `launch <name>` and `browser-launch --browser-executable <name>` resolve via alias. Stable paths for regular apps; --pattern for Store apps whose version directory changes on update (glob re-resolves automatically when the cached path goes stale).\nErrors: PROTOCOL invalid name/path/pattern | INTERNAL storage failure"
        }
        "app-list" => {
            "Example:\n  actl app-list\nLists all registered apps with name, path/pattern, resolved cache, and discovery source. Read-only.\nErrors: INTERNAL apps.json unreadable"
        }
        "app-remove" => {
            "Example:\n  actl app-remove my-tool\nRemoves a registration by name. The underlying application is untouched.\nErrors: NOT_FOUND unknown name | INTERNAL storage failure"
        }
        "browser-verify" => {
            "Example:\n  actl browser-verify --newest-alive\n  actl browser-verify --session s1a2b3 --css '[data-testid=\"app-bar-user\"]'\n  actl browser-verify --session s1a2b3 --css '#avatar' --contains work\n  actl browser-verify --none\nRead-only assertion family for flow verify steps: --newest-alive (post-launch), --none (post-close in a flow-owned lifecycle), --session+--alive/--gone (target responds / record deregistered), and the login-state predicate --session+--css/--xpath [--contains] (unique resolution + optional text substring; the resume_check for login pause points). Value assertions on writes live on browser-click --expect-text.\nErrors: PROTOCOL mode/args | ASSERTION_FAILED assertion unmet (incl. non-unique login selector) | NOT_FOUND unknown session"
        }
        "flow-validate" => {
            "Example:\n  actl flow-validate --file task.yml\nOptional --inputs values.yml resolves declared whole-argument inputs before validating every step. Resources are checked without creating files; data.resources uses the validation-preview context. --emit-plan includes the concrete preview plan and input values, not initialized resources. when/unless select run or otherwise using immediate assertions; unknown observations stop. Both routes are prevalidated. run/otherwise.sequence groups up to 16 calls with one result check. outputs select typed read-only data via JSON Pointer; bind replaces a whole args entry by index. Resolved CLI arguments are revalidated before execution.\nErrors: PROTOCOL invalid YAML/command/policy/resource | INTERNAL file error"
        }
        "verify-file" => {
            "Example:\n  actl verify-file --path note.docx --kind docx-styles --expected '{\"paragraphs\":[{\"text\":\"标题\",\"style\":\"1\"}]}' --subset\nReads text/JSON or OOXML via a fixed read-only native inspector (zip + Windows.Data.Xml.Dom; process facts via Win32/WMI). Text normalizes CRLF to LF; docx returns text and bold; docx-styles returns per-paragraph {text,style,bold} where style is the OOXML styleId (empty = no explicit style); xlsx returns cell values/formulas from sheet1. Maximum input/output 4 MiB. No user scripts, no shell-out, no Office automation. Known mismatches are re-read in a 5-second settle window; unknown read errors stop immediately.\nErrors: PROTOCOL invalid expectation | ASSERTION_FAILED content differs | NOT_ACTIONABLE unreadable/unsupported document | INTERNAL inspector failure"
        }
        "verify-rows" => {
            "Example:\n  actl verify-rows --app Task --prefix item-row- --expected '[]'\nCompares sorted names of TreeItem rows with an automation-ID prefix; rejects truncated snapshots and a missing parent tree.\nErrors: PROTOCOL invalid expected string array or prefix | ASSERTION_FAILED rows differ | NOT_ACTIONABLE incomplete observation"
        }
        "verify-owner" => {
            "Example:\n  actl verify-owner --app TaskMarker --program C:/Apps/editor.exe --token C:/Tasks/unique-profile\nRequires a unique visible native window, matching process executable and an absolute task path in its command line. Reports process creation time; optional --instance/--hwnd/--pid must all match. Read-only; does not close windows or prove process exit.\nErrors: PROTOCOL invalid task path | AMBIGUOUS windows | NOT_ACTIONABLE ownership unconfirmed"
        }
        "verify-closed" => {
            "Example:\n  actl verify-closed --hwnd 123 --pid 456 --instance 2026-09-29T00:00:00Z\nReads native windows and process creation time. Window disappearance alone never proves closure. Known running state is re-read for up to a 5-second settle window.\nErrors: PROTOCOL invalid HWND | ASSERTION_FAILED original instance remains | NOT_ACTIONABLE process observation failed"
        }
        "verify-window" => {
            "Example:\n  actl verify-window --app Chrome --state foreground\nReads native window state without activating it. Zero matches is false for exists/foreground/minimized/visible, true for absent. Multiple matches are ambiguous. State mismatches re-read for up to a 15-second settle window (loaded Office teardown trails the close action).\nErrors: PROTOCOL invalid app/state | ASSERTION_FAILED known state mismatch | AMBIGUOUS multiple windows | INTERNAL/NOT_ACTIONABLE observation failed"
        }
        "verify-clipboard" => {
            "Example:\n  actl verify-clipboard --expected 'Exact text' --normalize-lines\nRead-only exact text assertion. Optional normalization converts CRLF to LF and trims trailing LF.\nErrors: ASSERTION_FAILED text differs | NOT_ACTIONABLE unavailable clipboard | INTERNAL read failure"
        }
        "flow-run" => {
            "Example:\n  actl flow-run --file skills/actl/workflows/vscode-edit.yml --id vscode-01\n  actl flow-run --file explore-task.yml --id explore-01\nOptional --inputs values.yml overrides declared defaults; --paused explicitly registers without executing steps. After full preflight, resources create isolated directories/text files or copy bundled assets once. Saves resolved immutable plan and durable state. Resume never rereads inputs or initializes resources. Physical actions retain explicit authorization. Exploration plans (explore: task/app/keywords/goal) run the bounded exploration engine instead of static steps: observe→diff→score→gate→probe→act with local goal assertion; semantic deadlocks surface as decision_pending cards answered via flow-resume --decision.\nErrors: PROTOCOL invalid/reused ID/resource | NOT_ACTIONABLE executor busy | ABORTED stop | INTERNAL storage"
        }
        "flow-status" => {
            "Example:\n  actl flow-status --id report-01\n  actl flow-status --id explore-01\nReports run state, declared saved outputs, pause request and runner_active. template carries the stale-plan hint: when the source template's byte fingerprint differs from the frozen record, changed=true warns that retries still run the old frozen plan (verify new fixes in a new task); null when the run predates source recording or the template is unreadable. retry_hint names the uncertain write step with a copyable flow-resume --retry-step command (plain resume never replays it); popup_hint appears when the front-layer gate blocked a step (popup_pending). Exploration runs add an explore block (action/decision/observation counts, budgets, tried selectors, goal_met, token audit) and, when a run waits on a decision, the self-contained decision card (≤4 KiB, candidates c1..cK) to answer with flow-resume --decision.\nErrors: PROTOCOL invalid ID/state | INTERNAL storage"
        }
        "flow-evidence" => {
            "Example:\n  actl flow-evidence --id report-01\n  actl flow-evidence --id report-01 --step s6\nRead-only index of a run's failure bundles (runtime governance round 1): each uncertain/blocked step-phase lists what was captured at the failure instant — full screenshot (what a human sees), native window inventory (what Windows sees), targeted UIA snapshot and the app's front guidance layer with classification (what automation sees; docs/68 attribution + popup incidents). Artifacts live under flow-resources/<id>/evidence/; a bundle only lists artifacts that actually exist, missing ones are never fabricated. Returns step, phase, capture time and artifact files with sizes; read a file directly for its content.\nErrors: PROTOCOL unknown id | INTERNAL storage read"
        }
        "flow-pause" => {
            "Example:\n  actl flow-pause --id report-01\nRequests a boundary pause; workflow waits also check each poll. Only status=paused confirms it. In-flight COM calls and other CLI calls are not interrupted.\nErrors: PROTOCOL invalid ID/state | INTERNAL storage"
        }
        "flow-resume" => {
            "Example:\n  actl flow-resume --id report-01\n  actl flow-resume --id explore-01 --decision c2 --confidence 0.8\nUses the saved plan; never clears emergency stop. Optional resume_check verifies current context before uncertain-result checks. retry=stale_read retries only read-only STALE_REF, at most three calls. --retry-step requires an explicit failed assertion; observation errors never authorize write replay. Exploration runs answer pending decision cards with --decision <choice_id> (must be a card candidate id); the engine then walks the chosen candidate first and falls back to the card's remaining non-dangerous candidates without a new card; --confidence is recorded in the report only.\nErrors: PROTOCOL invalid state/step/decision or decision on a static flow | NOT_ACTIONABLE executor busy | ABORTED stop | INTERNAL storage"
        }
        "flow-clean" => {
            "Example:\n  actl flow-clean --older-than-minutes 120\n  actl flow-clean --dry-run\nBatch-archives historical non-completed flow states (paused/needs_review/interrupted) whose last update is older than the threshold; default 60 minutes. Archives like task-archive: state moves to archive, all evidence preserved, nothing deleted. Completed flows are untouched; running flows are skipped (executor alive). --dry-run lists candidates without moving.\nErrors: PROTOCOL invalid arguments | INTERNAL storage"
        }
        "explore-writeback" => {
            "Example:\n  actl explore-writeback --id explore-s1-calc-auto\n  actl explore-writeback --id explore-s2-calc-tie --out route-fragment.yml\nRead-only evidence collection (docs/42 S3): extracts a pack-format route fragment (status ALWAYS unverified — verification migration is human review, PACKS.md discipline) from a completed exploration run (goal_met=true). Steps are the acted click sequence plus the goal assertion, with the app anchor emitted as {window}; dead/no-effect selectors become quirk candidates and suggested_priors (dead=-4, no_effect=-2) for the next run's explore.priors. The yaml fragment (envelope data or --out file) is pasted into apps/<slug>.yml by the caller; actl never writes pack files. Runs without goal_met or with a capped action log are rejected — incomplete routes are never exported.\nErrors: PROTOCOL not an exploration run / goal not met / truncated action log | INTERNAL storage or fragment write"
        }
        "record-start" => {
            "Example:\n  actl record-start\n  actl record-start --id demo-save --max-seconds 300\nGUI recording session (docs/06 R1 / docs/76 / docs/79): spawns a detached bounded worker that captures semantic events via low-level hooks + point-in-time UIA reverse lookup. Trace (records/<id>/trace.jsonl) is the evidence file; record-stop compiles the mechanical draft (records/<id>/draft.yml, flow-validate clean, TODO items as comments with trace line evidence). One active session at a time; password-field keys are never stored; injected events are traced but not compiled.\nErrors: PROTOCOL session already active / id already used | INTERNAL worker spawn or startup confirm"
        }
        "record-worker" => {
            "Example:\n  actl record-worker --id rec-1760 --max-seconds 600\nInternal session process spawned by record-start (CREATE_NO_WINDOW, stdio null). Runs the capture loop until stop-requested or the duration cap, then compiles and writes the draft itself; envelope printed at exit (useful for direct invocation/testing). ACTL_RECORD_DISABLE_HOOKS=1 is the offline test seam.\nErrors: PROTOCOL id already used / invalid id | INTERNAL hook install / trace write"
        }
        "record-stop" => {
            "Example:\n  actl record-stop\n  actl record-stop --id demo-save\nWrites the stop marker, waits for the worker to finalize (<=15s), and returns the terminal summary including the full draft yaml. A dead worker (abandoned session) gets data salvage: the existing trace is compiled, the process is never revived. Already-completed sessions return the summary idempotently.\nErrors: PROTOCOL unknown id / no active session / ambiguous active sessions | INTERNAL stop marker write / finalize timeout"
        }
        "record-status" => {
            "Example:\n  actl record-status --id demo-save\n  actl record-status\nRead-only: with --id reports session state (recording/abandoned when the pid is dead/finalizing/completed/failed), draft/trace paths, and trace statistics (clicks/keys/masked/injected); without --id lists active sessions and the total count.\nErrors: PROTOCOL unknown id / invalid id"
        }
        "task-archive" => {
            "Example:\n  actl task-archive --file report.json\nImmutable caller report, retained 30 days; never implies verified task success.\nErrors: PROTOCOL invalid report | INTERNAL file/storage failure"
        }
        "history" => {
            "Example:\n  actl history --task report-01 --step submit --reports --limit 20\n  actl history --task report-01 --timeline --limit 500\nContent-minimal events, newest calls first. integrity validates sequence, identity and boundaries; unfinished may be running or interrupted. logging_health reports recorded journal/state failures, never guarantees complete logging. Control request IDs correlate pause/resume requests without changing desktop feedback. --timeline merges every recorded event of the task into one time-ordered stream (oldest first, newest kept when truncated), for reconstructing pauses, handoffs and step failures without joining journal files by hand.\nErrors: PROTOCOL invalid task/limit | INTERNAL storage failure (health evidence included)"
        }
        "display-status" => {
            "Example:\n  actl display-status\nReports native readiness, reason, lease, popup diagnostics, and sticky stop state; never starts the companion.\nErrors: INTERNAL unavailable state storage"
        }
        "focus-report" => {
            "Example:\n  actl focus-report\nRead-only diagnostic: reports the UIA focused element, the physical focus root window (GetGUIThreadInfo) and the foreground window in one envelope, plus a process-agreement fact signal; use to arbitrate element-selection and focus disputes (UIA focus can misreport on WinUI; the physical layer is authoritative).\nErrors: INTERNAL UIA unavailable"
        }
        "front-report" => {
            "Example:\n  actl front-report --app 记事本\n  actl front-report --app hwnd:12345\nRead-only: reports the app's front guidance layer — owned native popups (title/visible/foreground) and in-tree dialog surfaces with full text and button labels, plus a naive classification hint (error/save/confirm/update/unknown; judgement stays with the caller). Apps guide the user through front popups: read this BEFORE acting; front_clear=true means no guidance layer. Never clicks anything.\nErrors: NOT_FOUND no window | INTERNAL observation failure"
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
            "Example:\n  actl snapshot --app Notepad --skeleton\n  actl snapshot --app Edge --view content --skeleton\n  actl snapshot --app MyApp --keywords 'save,导出' --skeleton\n--skeleton keeps interactive elements + collapsed ancestors (token saver).\n--view content keeps only Document (web/document body) subtrees, drops app chrome;\ncombines with --skeleton (folding inside the document) — the browser/editor token saver.\n--keywords K1,K2 keeps interactive elements whose name/automation_id contains any keyword (case-insensitive) plus ancestors; the task-projection primitive of exploration mode (docs/42). Zero hits return an EMPTY result (no silent fallback). 1-16 keywords, each ≤64 chars.\nExample local scope:\n  actl snapshot --app MyApp --within 'uia:{\"match\":{\"id\":\"panel\"}}' --depth 3 --limit 100\n--within requires --app/--hwnd and one strict UIA container. Root depth is 0.\nLocal refs require the returned snapshot_id on replay; never mix with full-window refs.\nOutput: scope, ref_scope=container, observed_nodes; truncated + truncation_reasons report actual node/depth cuts.\nErrors: PROTOCOL invalid scope/budgets/keywords | AMBIGUOUS container | NOT_FOUND no window/Document | INTERNAL/PERM_DENIED/STALE_REF observation failed | NOT_ACTIONABLE incomplete content scope"
        }
        "click" => {
            "Example:\n  actl click @e3 --app Notepad --snapshot s1a2b\n  actl click name:item --app Explorer --physical --with ctrl\n(multi-select)\n  actl click id:session_item_x --app 微信 --physical --no-activate\n  actl click id:session_item_x --app 微信 --expect element:id:chat_message_list\n--expect window:<substr> = NEW window appears; element:<target> = postcondition resolves (implicit wait, 2s default).\nErrors: NOT_FOUND | AMBIGUOUS (candidates) | STALE_REF (re-snapshot) | NOT_ACTIONABLE occluded/xy: outside | PERM_DENIED guard | ASSERTION_FAILED --expect unmet | PROTOCOL bad args"
        }
        "execution-status" => {
            "Usage: actl execution-status\nArguments: none\nExample:\n  actl execution-status\nReports ready/stopped/repair_required; never grants permission.\nErrors: PROTOCOL invalid arguments"
        }
        "execution-recover" => {
            "Usage: actl execution-recover\nArguments: none (invocation explicitly requests recovery)\nExample:\n  actl execution-recover\nArchives raw state and invalidates old calls. No task resumes and no permission is granted.\nErrors: NOT_ACTIONABLE incompatible companion or unreadable state; PROTOCOL invalid arguments"
        }
        "preview" => {
            "Example:\n  actl preview id:saveButton --app MyApp --message 'Save this draft' --duration 5000\n  actl preview @e3 --app MyApp --snapshot s123\n  actl preview @e3 --candidate @e7 --app MyApp --snapshot s123 --duration 30000\nPreview only: no click, typing, clipboard change or desktop consent. The box is the automation operating rectangle (UIA provider view; --physical clicks land at its center) — deviation from the visual border is a risk signal, not a rendering defect; verify with screenshot --rect (window mode cannot capture the topmost overlay). Repeated --candidate enables explicit selection among 2..20 distinct refs from one snapshot. Select returns a target and identity; revalidate before executing. Close or expiry returns no selection. Target movement or identity changes stop the preview. Not allowed in persisted flows.\nErrors: PROTOCOL invalid duration/message/candidates, coordinate or unbound ref | NOT_FOUND | AMBIGUOUS | STALE_REF target changed | NOT_ACTIONABLE invisible/invalid bounds or unavailable preview | ABORTED stop | INTERNAL observation/render failure"
        }
        "get" => {
            "Example:\n  actl get name:状态 --app 设置 --property value\nProperties: name role automation_id offscreen value text (text = TextPattern body; Chromium Document value is the URL).\nEmpty strings are values. Value falls back to Text only when ValuePattern is unsupported. Default reads list unsupported channels; explicit unsupported reads fail.\nErrors: PROTOCOL unknown property | NOT_FOUND no match | AMBIGUOUS multi-match | NOT_ACTIONABLE unsupported channel | STALE_REF stale provider | PERM_DENIED access denied | INTERNAL observation failed"
        }
        "launch" => {
            "Example:\n  actl launch notepad\nOptional: actl launch --wait-window Notepad --timeout 5000 notepad\nOptions precede the program. Default returns window_ready:false (process started only). --wait-window matches an explicit title, reports matched PID/HWND, and verifies foreground; no automatic relaunch. For an existing instance use focus-window.\nErrors: NOT_FOUND program | TIMEOUT no window | AMBIGUOUS multiple windows | NOT_ACTIONABLE handoff/activation | ABORTED stop"
        }
        "type" => {
            "Example:\n  actl type id:Edit 'hello world' --app Notepad --paste\n--verify-value <expected>: poll the same element for an exact complete value for up to 2s after input; no write retries; line endings fold to \\n before comparison (\\r, \\r\\n, \\n are the same line break across clipboard/UIA providers).\n--paste: clipboard+ctrl+v, the only reliable channel under a Chinese-mode IME (uses the shared clipboard, reported in output).\nErrors: PROTOCOL --app required (injection is fail-closed) | NOT_ACTIONABLE foreground guard aborted | NOT_FOUND / AMBIGUOUS target | NOT_ACTIONABLE cannot take or verify exact focus | STALE_REF identity changed | ASSERTION_FAILED --verify-value readback failed (input may already be delivered)"
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
            "Example:\n  actl right-click name:file.txt --app 项目\nPhysical action (naming = authorization): opens context menus.\n--no-activate: click without foregrounding the window.\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE no clickable point or occluded | PERM_DENIED elevated window"
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
            "Example:\n  actl drag name:slider --to name:max --app 设置\n  actl drag xy:120,340 --to xy:540,360 --app 画图 --path 300,300 --path 420,420\n--path X,Y (repeatable, ≤64, absolute physical px) traverses start → waypoints → destination with 8px linear interpolation (canvas/signature trajectories). xy:→xy: drags are marquee/coordinate drags: BOTH endpoints are guarded against the --app window bounds and occlusion (--app required).\nErrors: PROTOCOL bad waypoint / xy pair without --app | NOT_FOUND / AMBIGUOUS element end | NOT_ACTIONABLE outside bounds / occluded / no clickable point / window moved"
        }
        "scroll" => {
            "Example:\n  actl scroll role:Document --down 3 --app Notepad\nUIA Scroll pattern first, physical wheel fallback (via reported).\nErrors: NOT_FOUND / AMBIGUOUS | NOT_ACTIONABLE neither channel works"
        }
        "key-down" => {
            "Example:\n  actl key-down shift --app Notepad\n  actl key-down space --app MyApp --duration 800\nHOLD semantics: must pair with key-up; abnormal exit leaves the key held. --duration Ms (50-60000) turns it into a long press: down → hold → automatic up; a stop request interrupts the hold and releases immediately (ABORTED, the key IS released).\nErrors: PROTOCOL --app required / bad duration | PERM_DENIED foreground guard | ABORTED interrupted hold (released)"
        }
        "key-up" => {
            "Example:\n  actl key-up shift --app Notepad\nMust be the same expression as the paired key-down.\nErrors: PROTOCOL --app required | PERM_DENIED foreground guard"
        }
        "resize-window" => {
            "Example:\n  actl resize-window 记事本 --width 800 --height 600\nErrors: NOT_FOUND / AMBIGUOUS window | NOT_ACTIONABLE no native handle"
        }
        "verify" => {
            "Example:\n  actl verify --value id:CalculatorResults '56,877' --property name --contains --app 计算器\nImmediate assertion (vs wait): fails now, not later; failure carries expected/actual/target/channel. --normalize-lines compares complete text after CRLF/CR normalization and trimming trailing LF. --url-query KEY compares exactly one percent-decoded query value (literal plus, UTF-8, duplicate keys rejected); conflicts with --property/--contains/--normalize-lines. --property name|value|text selects an explicit channel; omitted uses Value then Text fallback.\nErrors: ASSERTION_FAILED | NOT_FOUND / AMBIGUOUS | PROTOCOL bad flags"
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
                "\nStrict UIA target syntax: see `actl find --help`"
            } else {
                ""
            };
            *sub = std::mem::take(sub).after_help(format!("{text}{selector_help}"));
        }
    }
    root
}
