//! cli —— clap 参数面:命令/参数定义。
//! help 文本英文、三处输出(L0/L1/L2)从本文件的 doc comment 同源生成(06 §3.5);
//! `commands` 自发现命令读的也是这棵树(schema 与 help 单一事实来源)。

use clap::{CommandFactory, Parser, Subcommand};

use crate::help_text::after_help_for;

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
    /// Read the current user selection (text ranges or selected items) of the focused/target element; read-only, honest unavailable
    GetSelection {
        /// Target element holding the selection (default: focused: — the user's keyboard-focus control)
        target: Option<String>,
        /// Target window by title substring or hwnd:<n> (recommended with focused:)
        #[arg(long)]
        app: Option<String>,
        /// Max characters per text range (default 8000)
        #[arg(long)]
        max_chars: Option<usize>,
    },
    /// Report the app's front guidance layer (owned popups + in-tree dialog surfaces); read-only
    FrontReport {
        /// Target window by title substring or hwnd:<n> (--app 形态与 docs/06
        /// 契约一致;位置参数保留兼容既有调用)
        #[arg(long = "app")]
        app_flag: Option<String>,
        /// Target window (positional compat)
        app: Option<String>,
    },
    /// Report crash-dialog residue across the desktop (WER family, app-agnostic); read-only
    ResidueReport,
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
    /// Gracefully close the companion (lifecycle verb; identity-verified, stop state untouched)
    DisplayStop {
        /// Why the display is being stopped (recorded in the envelope, <=200 chars)
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// Minimize one ordinary application window
    MinimizeWindow {
        #[arg(conflicts_with_all = ["pid", "hwnd"])]
        pattern: Option<String>,
        #[arg(long, conflicts_with_all = ["pattern", "hwnd"])]
        pid: Option<u32>,
        #[arg(long, conflicts_with_all = ["pattern", "pid"])]
        hwnd: Option<String>,
    },
    /// Maximize one ordinary application window (window-manager maximize; app-internal fullscreen like F11 is app state, out of scope)
    MaximizeWindow {
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
        /// Inline input override `key=value` (repeatable; wins over --inputs)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
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
        /// Inline input override `key=value` (repeatable; wins over --inputs)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        #[arg(long)]
        id: String,
        /// Save the plan and start paused for inspection
        #[arg(long)]
        paused: bool,
        /// Exploration runs: answer decision cards via the configured decision
        /// provider until completed or an abstain hands back to a human
        #[arg(long)]
        auto_decide: bool,
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
        /// Pick one candidate of an ambiguous_target pause (@eN from the step's candidates; static flows)
        #[arg(long, requires = "id", conflicts_with_all = ["decision", "retry_step"])]
        pick: Option<String>,
    },
    /// Revise the frozen plan of a paused/needs-review run; old versions archived
    #[command(name = "flow-revise")]
    Revise {
        #[arg(long)]
        id: String,
        /// New plan YAML (validated like flow-run; {"input":...} references
        /// resolve via declared defaults, or the --inputs values file)
        #[arg(long)]
        file: String,
        /// Resolve {"input":...} references in the revision file (defaults apply without it)
        #[arg(long)]
        inputs: Option<String>,
        /// Inline input override `key=value` (repeatable; wins over --inputs)
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// Why this revision (recorded in the version lineage, ≤200 chars)
        #[arg(long, default_value = "")]
        message: String,
        /// Authorize re-running this completed write step whose spec changed (repeatable)
        #[arg(long = "accept-rerun")]
        accept_rerun: Vec<String>,
        /// Continue execution immediately after the revision lands
        #[arg(long)]
        resume: bool,
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
    /// Answer a pending decision card by asking a configured decision model (read-only, no desktop action)
    Decide {
        /// Decision card JSON file (bare card or flow-status envelope); omit to read stdin
        #[arg(long)]
        card: Option<String>,
        /// Read the pending card from a flow run's state (exploration runs; conflicts with --card)
        #[arg(long, conflicts_with_all = ["card", "task"])]
        run: Option<String>,
        /// Task intent for AMBIGUOUS error-envelope disambiguation (required for that form)
        #[arg(long)]
        task: Option<String>,
        /// Config file override (default: %LOCALAPPDATA%/actl/decision.json)
        #[arg(long)]
        config: Option<String>,
        /// Decision endpoint URL override for this call (System One /v1/systemone form)
        #[arg(long)]
        endpoint: Option<String>,
        /// Minimum confidence to auto-answer (below -> abstain; default from config, else 0.5)
        #[arg(long, value_name = "F64")]
        min_confidence: Option<f64>,
        /// HTTP timeout in milliseconds (default from config, else 10000)
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// Print the request that would be sent, without calling the provider
        #[arg(long)]
        dry_run: bool,
    },
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
    /// Locate a template image inside a window by deterministic pixel matching (RPA-style, no AI vision)
    FindImage {
        /// Template PNG path (crop via screenshot --element/--rect on this machine)
        #[arg(long)]
        template: String,
        /// Window anchor: title substring or hwnd:<n> (default: foreground window)
        #[arg(long)]
        app: Option<String>,
        /// ZNCC match threshold in (0,1] (default 0.85)
        #[arg(long)]
        threshold: Option<f64>,
        /// Enumerate all matches (default: unique — multiple matches report AMBIGUOUS)
        #[arg(long)]
        all: bool,
        /// Max matches returned with --all (default 20, max 50)
        #[arg(long, requires = "all")]
        limit: Option<usize>,
        /// Multi-scale search (0.5x-2x, six steps) for DPI/zoom mismatch; ~6x slower
        #[arg(long)]
        scales: bool,
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
        /// Capture a window-relative region (origin = target window top-left, same anchor as xyw:): <x>,<y>,<w>,<h> — requires --app or --hwnd (Fix B)
        #[arg(long, id = "swrect", value_name = "X,Y,W,H", conflicts_with_all = ["srect", "selement"])]
        wrect: Option<String>,
        /// Capture an element's bounding rectangle (goes through the locator chain)
        #[arg(long, id = "selement", conflicts_with_all = ["app", "shwnd", "srect", "swrect"])]
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed) | xy:<x>,<y> (physical-only; needs --app)
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
        /// Read back the complete value on the same element within 2s; never repeats the write (a blank readback fails honestly — some provider-backed dialogs cannot read back; use `type --verify-value` there)
        #[arg(long)]
        verify_value: Option<String>,
    },
    /// Focus an element for subsequent keyboard input (UIA SetFocus + readback verification; no keys injected)
    Focus {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> (needs --app) | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>]
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
    },
    /// Right-click an element (physical; opens context menus)
    RightClick {
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
        target: String,
        /// Target window by title substring, or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
        /// Validate window and ref element identities against this snapshot id
        #[arg(long)]
        snapshot: Option<String>,
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
        #[arg(conflicts_with = "hwnd")]
        pattern: Option<String>,
        /// Window by native handle (decimal or 0x-hex; survives title changes)
        #[arg(long, conflicts_with = "pattern")]
        hwnd: Option<String>,
        /// New width in physical pixels
        #[arg(long)]
        width: i32,
        /// New height in physical pixels
        #[arg(long)]
        height: i32,
        /// Optional new top-left X (physical, negative = left monitor); omit to keep position
        #[arg(long)]
        x: Option<i32>,
        /// Optional new top-left Y; omit to keep position
        #[arg(long)]
        y: Option<i32>,
    },
    /// Immediate assertion (unlike wait): exists / value / checked
    VerifyFile {
        #[arg(long)]
        path: String,
        #[arg(long, value_parser = ["text", "json", "docx", "docx-styles", "meta", "xlsx"])]
        kind: String,
        /// Expected JSON value; text files use a JSON string
        #[arg(long)]
        expected: Option<String>,
        /// Read the expectation from a UTF-8 file (mutually exclusive with --expected;
        /// long/Chinese JSON via shells mangles quotes/encoding — docs/81 §8)
        #[arg(long)]
        expected_file: Option<String>,
        /// Compare only declared object keys (array lengths remain exact)
        #[arg(long)]
        subset: bool,
        /// With --path as a DIRECTORY: verify the newest file matching this wildcard
        /// pattern (only `*`, case-insensitive) instead — save dialogs append the
        /// type-filter extension (typed .csl.json lands as .csl.json.bib), so assert
        /// by discovery; envelope reports the discovered path under "discovered"
        #[arg(long)]
        discover: Option<String>,
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
        /// Process creation time (verify-owner/close-window envelope). Optional:
        /// pid absent → definitive pass; window present → definitive fail; pid
        /// alive but window absent → NOT_ACTIONABLE asking for --instance
        /// (pid reuse vs. windowless original cannot be distinguished)
        #[arg(long)]
        instance: Option<String>,
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
        /// Filter by action-compatibility (advisory, role-derived): click | type
        #[arg(long, conflicts_with = "target")]
        capable: Option<String>,
        /// Read this property on each matched element and attach it to the entry (key = property name) — batch field harvest in one round trip (property whitelist matches get; a per-element read failure degrades to read_error on that entry, never fails the whole find)
        #[arg(long, value_parser = ["name", "role", "automation_id", "offscreen", "enabled", "selected", "value", "text", "rect"], conflicts_with = "target")]
        property: Option<String>,
        /// Target window by title substring or hwnd:<n> (default: focused window)
        #[arg(long)]
        app: Option<String>,
        /// Max matches to report (default 20)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Collect all items from a (virtualized) list container: bounded, zero input injection
    Collect {
        /// Target: list container (standard target grammar: name:/id:/role:/@eN/uia:)
        target: String,
        /// Filter items by role substring (case-insensitive), e.g. ListItem
        #[arg(long)]
        role: Option<String>,
        /// Observation-round budget for the scroll-converge fallback (default 20, incl. first frame)
        #[arg(long)]
        rounds: Option<usize>,
        /// Max items before honest truncation (default 500, max 5000)
        #[arg(long)]
        max_items: Option<usize>,
        /// Target window by title substring or hwnd:<n> (0x ok; survives SPA title changes)
        #[arg(long)]
        app: Option<String>,
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
        /// Override the browser user-agent (printable ASCII, 1-512 bytes) — the compliant
        /// pass for headless-UA walls; overrides the UA string only, not fingerprint surfaces
        #[arg(long)]
        user_agent: Option<String>,
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
        /// Enumerate ALL matches (list semantics: 0 matches = empty set, not NOT_FOUND)
        #[arg(long)]
        all: bool,
        /// Pierce OPEN shadow roots recursively (Web Components, e.g. <bili-comments> comments);
        /// closed shadow roots stay invisible to page JS — honest boundary, reported as such
        #[arg(long)]
        deep: bool,
        /// Max elements returned with --all (default 50, max 500; total stays honest)
        #[arg(long, requires = "all")]
        limit: Option<usize>,
        /// Settle predicate: poll until at least N matches before returning (async-rendered
        /// pages settle on their own; timeout carries the observed count in evidence)
        #[arg(long)]
        wait_for: Option<u64>,
        /// CDP target override (id from browser-targets): default is the bound tab;
        /// extension popups are separate targets while open (docs/72)
        #[arg(long)]
        target: Option<String>,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Scroll-collect ALL items of a lazy-loading list into one set (empty-streak convergence)
    BrowserCollect {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// CSS selector (mutually exclusive with --xpath)
        #[arg(long, conflicts_with = "c_x")]
        css: Option<String>,
        /// XPath expression (mutually exclusive with --css)
        #[arg(long, id = "c_x", conflicts_with = "css")]
        xpath: Option<String>,
        /// Whitelisted property per item instead of text (value | href | ...)
        #[arg(long)]
        property: Option<String>,
        /// Pierce OPEN shadow roots on every read round (Web Components lists)
        #[arg(long)]
        deep: bool,
        /// Settle interval after each page scroll (100..=5000 ms, default 500)
        #[arg(long)]
        interval_ms: Option<u64>,
        /// Round budget (1..=200, default 100); hitting it reports truncation
        #[arg(long)]
        max_rounds: Option<u64>,
        /// Item budget (1..=5000, default 2000); hitting it reports truncation
        #[arg(long)]
        max_items: Option<u64>,
        /// Timeout per operation in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Snapshot the session tab's network requests over a bounded window (deep lane, read-only)
    BrowserNetlog {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
        /// Only keep requests whose URL contains this substring (1-2048 bytes)
        #[arg(long)]
        filter: Option<String>,
        /// Capture window in milliseconds (100..=60000, default 5000)
        #[arg(long)]
        window: Option<u64>,
        /// Max entries returned (1..=2000, default 500; hitting it reports truncated)
        #[arg(long)]
        max_entries: Option<u64>,
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
        /// CDP target override (id from browser-targets): default is the bound tab;
        /// extension popups are separate targets while open (docs/72)
        #[arg(long)]
        target: Option<String>,
        /// Skip the pre-action in-page popup gate (only after judging popup samples)
        #[arg(long)]
        allow_popups: bool,
        /// Timeout in milliseconds (default 10000)
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// Click a unique element — guarded write: stop check, target re-verify, popup gate, unique match, dispatch, read-back
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
        /// CDP target override (id from browser-targets): default is the bound tab;
        /// extension popups are separate targets while open (docs/72)
        #[arg(long)]
        target: Option<String>,
        /// Skip the pre-action in-page popup gate (only after judging popup samples)
        #[arg(long)]
        allow_popups: bool,
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
    /// List the session browser's CDP targets (read-only; discovery channel for --target addressing)
    BrowserTargets {
        /// Session id from browser-launch
        #[arg(long)]
        session: String,
    },
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
    /// Locate an application executable across channels (read-only probe; Start Menu .lnk covers custom install dirs)
    AppLocate {
        /// Application name to search (ASCII alias or Chinese display name, 1-96 bytes)
        name: String,
        /// Register the first found candidate as an app-registry alias (write action)
        #[arg(long)]
        register: bool,
    },
    /// Remove a registered application by name
    AppRemove {
        /// The registered name to remove
        name: String,
    },
    /// Read-only browser assertions for flow verify steps (exactly one mode)
    BrowserVerify {
        /// Session id for --alive/--gone/--css/--xpath/--no-popups assertions
        #[arg(long)]
        session: Option<String>,
        /// With --session: the bound tab target answers (implies browser + endpoint alive)
        #[arg(long, requires = "session", conflicts_with_all = ["bv_gone", "bv_css", "bv_xpath", "bv_popups"])]
        alive: bool,
        /// With --session: the session record no longer exists
        #[arg(long, requires = "session", id = "bv_gone", conflicts_with_all = ["alive", "bv_css", "bv_xpath", "bv_popups"])]
        gone: bool,
        /// Login-state predicate: with --session, assert this CSS selector resolves uniquely
        #[arg(long, requires = "session", id = "bv_css", conflicts_with_all = ["alive", "bv_gone", "bv_xpath", "bv_popups"])]
        css: Option<String>,
        /// Login-state predicate: with --session, assert this XPath resolves uniquely
        #[arg(
            long,
            requires = "session",
            id = "bv_xpath",
            conflicts_with_all = ["alive", "bv_gone", "bv_css", "bv_popups"]
        )]
        xpath: Option<String>,
        /// With --css/--xpath: also assert the resolved element text contains this substring
        #[arg(long, requires = "session")]
        contains: Option<String>,
        /// Popup-surface predicate: with --session, assert ZERO visible in-page popup surfaces
        #[arg(long, requires = "session", id = "bv_popups", conflicts_with_all = ["alive", "bv_gone", "bv_css", "bv_xpath"])]
        no_popups: bool,
        /// With --no-popups: replace the default popup selector (dialog[open], role=dialog/alertdialog, aria-modal)
        #[arg(long, requires = "bv_popups")]
        popup_selector: Option<String>,
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
    /// Save ALL clipboard formats to a named backup (RPA-classic: paste channels clobber the user's clipboard — UiPath "Restore clipboard" equivalent)
    ClipboardSave {
        /// Backup id: 1-64 chars of [A-Za-z0-9-]
        id: String,
    },
    /// Restore all formats from a clipboard-save backup (explicit orchestration decision, never automatic after paste)
    ClipboardRestore {
        /// Backup id from clipboard-save
        id: String,
    },
    /// List the format types currently on the clipboard (diagnostic; reads no data)
    ClipboardFormats,
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
        /// Target: uia:<JSON> (strict match/within/scope) | @eN or e<N> | name:<substr> | id:<AutomationId> | role:<Type> | role:<Type>#N | focused:[<Type>] (OS keyboard focus element; role-checked, fail-closed)
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
        | Command::Focus { app, .. }
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
        | Command::Find { app, .. }
        | Command::Collect { app, .. } => app.clone(),
        _ => None,
    }
}

/// envelope 的 command 字段(06 §4)。
pub(crate) fn command_name(cmd: &Command) -> &'static str {
    match cmd {
        Command::Commands => "commands",
        Command::Status => "status",
        Command::Decide { .. } => "decide",
        Command::Preview { .. } => "preview",
        Command::Desktop(DesktopCommand::DisplayStatus) => "display-status",
        Command::Desktop(DesktopCommand::ExecutionStatus) => "execution-status",
        Command::Desktop(DesktopCommand::FocusReport) => "focus-report",
        Command::Desktop(DesktopCommand::GetSelection { .. }) => "get-selection",
        Command::Desktop(DesktopCommand::FrontReport { .. }) => "front-report",
        Command::Desktop(DesktopCommand::ResidueReport) => "residue-report",
        Command::Desktop(DesktopCommand::ExecutionRecover) => "execution-recover",
        Command::Desktop(DesktopCommand::TaskArchive { .. }) => "task-archive",
        Command::Flow(FlowCommand::Validate { .. }) => "flow-validate",
        Command::Flow(FlowCommand::Run { .. }) => "flow-run",
        Command::Flow(FlowCommand::Status { .. }) => "flow-status",
        Command::Flow(FlowCommand::Evidence { .. }) => "flow-evidence",
        Command::Flow(FlowCommand::Pause { .. }) => "flow-pause",
        Command::Flow(FlowCommand::Resume { .. }) => "flow-resume",
        Command::Flow(FlowCommand::Revise { .. }) => "flow-revise",
        Command::Flow(FlowCommand::Clean { .. }) => "flow-clean",
        Command::Flow(FlowCommand::Writeback { .. }) => "explore-writeback",
        Command::Record(RecordCommand::Start { .. }) => "record-start",
        Command::Record(RecordCommand::Worker { .. }) => "record-worker",
        Command::Record(RecordCommand::Stop { .. }) => "record-stop",
        Command::Record(RecordCommand::Status { .. }) => "record-status",
        Command::Desktop(DesktopCommand::History { .. }) => "history",
        Command::Desktop(DesktopCommand::DisplayStart) => "display-start",
        Command::Desktop(DesktopCommand::DisplayStop { .. }) => "display-stop",
        Command::Desktop(DesktopCommand::MinimizeWindow { .. }) => "minimize-window",
        Command::Desktop(DesktopCommand::MaximizeWindow { .. }) => "maximize-window",
        Command::Desktop(DesktopCommand::RestoreWindow { .. }) => "restore-window",
        Command::Desktop(DesktopCommand::DesktopPrepare { .. }) => "desktop-prepare",
        Command::Desktop(DesktopCommand::DesktopRestore { .. }) => "desktop-restore",
        Command::Snapshot { .. } => "snapshot",
        Command::Click { .. } => "click",
        Command::Focus { .. } => "focus",
        Command::Get { .. } => "get",
        Command::Launch { .. } => "launch",
        Command::Type { .. } => "type",
        Command::Press { .. } => "press",
        Command::Paste { .. } => "paste",
        Command::FindImage { .. } => "find-image",
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
        Command::Collect { .. } => "collect",
        Command::BrowserLaunch { .. } => "browser-launch",
        Command::BrowserNavigate { .. } => "browser-navigate",
        Command::BrowserRead { .. } => "browser-read",
        Command::BrowserCollect { .. } => "browser-collect",
        Command::BrowserNetlog { .. } => "browser-netlog",
        Command::BrowserType { .. } => "browser-type",
        Command::BrowserClick { .. } => "browser-click",
        Command::BrowserClose { .. } => "browser-close",
        Command::BrowserList => "browser-list",
        Command::BrowserTargets { .. } => "browser-targets",
        Command::AppRegister { .. } => "app-register",
        Command::AppList => "app-list",
        Command::AppLocate { .. } => "app-locate",
        Command::AppRemove { .. } => "app-remove",
        Command::BrowserVerify { .. } => "browser-verify",
        Command::GetClipboard => "get-clipboard",
        Command::SetClipboard { .. } => "set-clipboard",
        Command::ClipboardSave { .. } => "clipboard-save",
        Command::ClipboardRestore { .. } => "clipboard-restore",
        Command::ClipboardFormats => "clipboard-formats",
        Command::Wait { .. } => "wait",
        Command::Batch { .. } => "batch",
    }
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
