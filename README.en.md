# actl

[简体中文](README.md) · [English](README.en.md)

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> While the repository is private, the Actions badge is only visible when signed in;
> the crates.io/License badges show public data.

> Desktop hands for AI agents: let AI operate Windows software like a human —
> faster, more precise, and verifiable at every step.

## Why actl

**actl** is a Windows command-line tool built for AI agents: the agent runs one
command at a time in a shell and can click buttons, type text, use the clipboard,
and drive any desktop application. It **does not look at screenshots or guess
coordinates** — it reads the system's accessibility tree (UIA) directly, locating
controls as precisely as Playwright reads the web DOM, and outputs structured
JSON.

| Common approach | Problem |
|---|---|
| Screenshots + vision models (Agent-S, UI-TARS, …) | Slow and costly: a VLM pass per step, coordinates guessed from images, misses are unattributable |
| Have the AI write AutoHotkey scripts | Capable, but a "blind batch run": no per-step feedback, failures restart the whole script |
| Run an MCP server | Heavy, eats agent context; actl is a plain CLI — any agent that can run a shell can use it |

One-line positioning: **what Playwright is to the browser, actl is to the Windows
desktop.** Codex Computer Use officially prefers structured integration over
computer use whenever available — actl is that structured integration for the
Windows desktop ([comparison](docs/12-codex-computer-use-comparison.md)).

## Install

Requirements: Windows x64 (UIA ships with the OS; no drivers or services needed).

**scoop** (recommended):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

<details>
<summary><b>Direct download</b> (no package manager)</summary>

Grab `actl_vX.Y.Z_windows_amd64.exe` from
[Releases](https://github.com/gqy20/actl/releases/latest) — the main binary is a
single self-contained file. The full zip additionally ships `actl-signal.exe`
(the desktop signal bar; installing both is recommended).

</details>

<details>
<summary><b>cargo</b></summary>

```bash
cargo install actl-cli
```

The crate is [actl-cli](https://crates.io/crates/actl-cli) (binary name `actl`);
the workspace also publishes [actl-core](https://crates.io/crates/actl-core)
(protocol layer) and [actl-uia](https://crates.io/crates/actl-uia) (UIA backend)
as independently reusable crates.

</details>

## Quick start

Three commands cover the "sense → act → assert" loop. Examples assume an
English-locale Windows — match your local app titles:

```bash
# 1. Sense: which elements in the target window are interactive (yields stable @eN refs)
actl snapshot --app Notepad --skeleton

# 2. Act: click (UIA Invoke, runs in the background without seizing the mouse)
actl click id:clearButton --app Calculator

# 3. Assert: the new window must appear after the keystroke, or ASSERTION_FAILED
#    fires immediately — no silent false success
actl press ctrl+s --app Notepad --expect window:Save As
```

Under a Chinese-mode IME, `actl type ... --paste` delivers text via the clipboard
channel, which is more reliable.

All output is a uniform JSON envelope (`ok`/`command`/`data|error`/`duration_ms`);
errors carry a machine-readable code, a recovery hint, and structured evidence so
the agent can recover on its own:

```json
{"version":"1","ok":false,"command":"click","error":{
  "code":"AMBIGUOUS",
  "message":"role:Button matches 34 elements in the window",
  "hint":"tighten the selector or pick by index",
  "evidence":{"candidates":[{"ref":"@e2","role":"Button","name":"Minimize Calculator"}]}},
 "duration_ms":1563}
```

On ambiguous matches, the error directly embeds structured candidates (with
replayable refs) — no need to take a new snapshot:

```bash
actl click role:Button --app Calculator                       # → AMBIGUOUS + candidates[]
actl click role:Button#3 --app Calculator                     # pick by ordinal
actl click role:Button --near id:clearButton --app Calculator # nearest to an anchor
```

Every command ships a four-part `actl <cmd> -h` (usage/args/example/error codes);
full command semantics and the protocol contract live in the
[design blueprint](docs/06-design-blueprint.md) (Chinese).

## Key features

- **Precise targeting, no coordinate guessing**: reads the system UIA tree
  directly; supports `@eN` refs, `name:/id:/role:` selectors, ordinal and
  anchor-based disambiguation. Virtualized large lists (Explorer item level),
  legacy Win32 reports (msinfo32), and Chromium/Electron document reads are all
  verified in practice.
- **Verifiable step by step**: `verify`/`wait` support value assertions and
  property-predicate waits (`--property value=OK`/`name=substring`/`checked=on`);
  `batch` reports per-step results; every error carries a code + hint + evidence
  the agent can act on.
- **Safety guardrails — refuse rather than misfire**:
  - before any keyboard/pointer injection, the physical foreground window is the
    sole ground truth for target verification; long injections re-check focus in
    batches and stop at a batch boundary if focus is stolen, reporting exactly
    what was delivered;
  - `type`/`press` require an explicit `--app`; physical pointer actions require
    an explicit `--physical` — the default UIA path runs in the background and
    never seizes the user's mouse or keyboard;
  - injection is refused while the user is holding Ctrl/Shift (no semantic
    hijacking, never stomping user keys); concurrent actl instances coordinate
    input ownership.
- **Desktop signal bar `actl-signal`** (install recommended): the human's
  awareness and escape hatch while automation runs — a compact always-on-top
  status strip (focus-free, DPI-aware): green=running · amber=injecting input ·
  red=stopped · gray=idle · dark=offline. **Ctrl+Alt+F12** is a global emergency
  stop; in-flight injections are rejected at the next action boundary
  (`ABORTED`). Click the strip to clear the stop flag and resume.

### Command surface (30 commands, self-discovered via `actl commands`)

| Category | Commands |
|---|---|
| Sense | `snapshot` `find` `get` `list-windows` `status` `commands` |
| Act | `click` `right-click` `double-click` `hover` `drag` `scroll` `type` `paste` `press` `key-down` `key-up` `set-value` `select` `toggle` |
| Window | `launch` `focus-window` `close-window` `resize-window` `wait` `batch` |
| Verify | `verify` `extract` |
| System | `get-clipboard` `set-clipboard` |

## Documentation

Project docs are currently written in Chinese; contributions translating them
are welcome.

- [Design blueprint](docs/06-design-blueprint.md) — full command semantics and protocol contract
- [Field-test records](docs/14-office-scenarios.md) — real-app war stories: Office loops, WeChat core channels
- [Research & design index](docs/README.md) — landscape survey, PRD, tech decisions
- [Agent skill](skills/actl) — the actl usage skill and app profile packs for agents
- [Roadmap](docs/ROADMAP.md) — milestones and versioning

## Project status

Currently v0.1.x (M2 phase): all 30 commands work end-to-end; end-to-end tasks
(Calculator, Notepad save) and a cross-3-app data relay pass all tests; Office
(Excel/Word) and the WeChat core channels are verified in practice, with
findings continuously folded back into the docs and app packs. See the
[roadmap](docs/ROADMAP.md) for milestones and versioning.

## Contributing

Rust workspace, trunk-based (`main` always green). After cloning, `make setup`
installs the git hooks and pinned toolchain in one step; run `make gate` before
committing. Collaboration rules (Conventional Commits, testing discipline,
single-source gating) live in [AGENTS.md](AGENTS.md), shared by humans and AI.

## License

[MIT](LICENSE)
