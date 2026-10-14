# actl

[简体中文](README.md) · [English](README.en.md)

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> Desktop hands for AI agents: let AI operate Windows software like a human —
> faster, more precise, and verifiable at every step.

<!-- TODO(asset): assets/hero.gif — 15s hero demo: the agent extracts data from
     Excel, writes it into Word, and reads it back to verify; signal bar visible
     throughout. Recording script: assets/demo-recording.md, task 1. -->

**actl** is a Windows command-line tool built for AI agents: the agent runs one
command at a time in a shell and can click, type, use the clipboard, and drive
any desktop application, with structured JSON output. It **does not guess
click points from screenshots** — it reads the system accessibility tree (UIA)
directly, getting each control's name, role, and structure the way Playwright
reads the web DOM; screenshots serve only as observation evidence and a
last-resort fallback. One-line positioning: **what Playwright is to the
browser, actl is to the Windows desktop.**

## Quick start

Run one command first, then a real task — **each ends with a verifiable
result, not "probably clicked"**. Commands come from field-verified routes;
demo data: [assets/demo-sales.xlsx](assets/demo-sales.xlsx) (7 regions, two
months of revenue, totals and growth that add up).

**① One command reads the whole table** (read-only — works straight after
install):

```bash
actl extract role:DataGrid --app Excel
```

<!-- TODO(asset): assets/extract-output.png — a real excerpt of the JSON this
     command returns. -->

**② A full task: move Excel data into Word and check itself** (writes need a
one-time approval on the signal bar):

```bash
actl extract role:DataGrid --app Excel                              # read: whole table → structured rows
actl set-clipboard 'October revenue: ¥84,000 total, +12% MoM, all seven regions growing.'
actl click role:Document --app Word                                 # locate the document body
actl press ctrl+v --target role:Document --app Word                 # paste; --target asserts focus is on the body, not the search box
actl get role:Document --property text --app Word                   # verify: read the full text back
```

The last command **reads the text back** — the agent confirms the sentence is
really in the document instead of claiming success on "probably pasted"; when
a click misses, the error carries the candidate list and recovery hints, so
the agent disambiguates and retries on its own. Chinese input goes through
the paste channel (`set-clipboard` + `ctrl+v`), the reliable route under an
IME. Turn the task into YAML and the next run is a single `flow-run`; or
right-click the signal bar, demo it yourself once, and let it become a flow.

## What it can do

- **Hits the target, no guessing from pixels.** Locates controls by reading the
  system accessibility tree (UIA) — names, roles, structure. Virtualized large
  lists, legacy Win32 reports, and Chromium/Electron document bodies are all
  verified in practice. Screenshot-based approaches can only retry a miss;
  actl tells you it missed, where, and which candidates exist.
- **Verifies every step.** Value assertions, property predicates, file
  contents, window states — not "it should have worked" but "what the screen
  says now". Failures carry an error code, a recovery hint, and structured
  evidence.
- **When unsure, your model answers first — the call stays with you.** On
  exploration ties or ambiguous selectors, pipe the decision card or the
  AMBIGUOUS error envelope to `decide`: a discriminative model you configure
  (local llama-server/LM Studio; not generative, no LLM is bundled) answers
  with a choice, confidence, and replayable resolved metadata. Dangerous
  candidates are stripped before the request is built, so the model
  physically cannot see them; low confidence abstains and hands the call back
  to you. `flow-run --auto-decide` collapses card→answer→continue into one
  command.
- **Moves your desktop with you watching.** A compact always-on-top signal
  strip (no focus stealing): green=running, amber=injecting input,
  red=stopped. **Ctrl+Alt+F12** is a global emergency stop; in-flight
  injections are rejected at the next action boundary; writes require a
  one-time approval on the strip.
- **Never fights you for the keyboard.** Everything defaults to background
  UIA calls that never touch your mouse or keyboard; actions that must move
  the real mouse require an explicit `--physical`; the physical foreground
  window is checked before injection — while you are typing, it refuses.
- **Flows are files, not black boxes.** Multi-step tasks are YAML (declared
  inputs, verification steps, pause points); `flow-run` executes in one
  command with resumable checkpoints; `flow-revise` amends the plan in place
  and continues — no restart from scratch; repeated preparation steps are
  skipped on rerun. Finished flows become templates: the same task the second
  time is one command.
- **Demo it once, it becomes a flow.** Right-click the signal bar to record;
  operate naturally once, and `record-stop` produces a replayable YAML —
  input-method Chinese, window waits, all the fiddly details handled, and
  what you typed becomes a parameter. Change it and replay with one command.

## Which use fits you?

| You want… | Path |
|---|---|
| An AI agent to drive your software directly | call it from the shell; load the [actl skill](skills/actl) and app profiles for your agent |
| A repeated task collapsed into one command | write a YAML flow, or right-click the signal bar to record → `flow-run` replays it, resumable |
| To reuse the protocol or UIA layer | [actl-core](https://crates.io/crates/actl-core) (protocol) / [actl-uia](https://crates.io/crates/actl-uia) (UIA backend), two independently reusable crates |

## Field-tested in real apps

Verified end-to-end with battle records: the Office suite (Excel extraction,
Word write-and-read-back, [docs/14](https://github.com/gqy20/actl/blob/main/docs/14-office-scenarios.md));
WeChat 4.1 full surface (send/upload/forward/search, app profile `wechat.yml`);
Tencent Meeting ([docs/26](https://github.com/gqy20/actl/blob/main/docs/26-tencent-meeting-interactions.md));
an own CDP browser channel with deep login-state loops on Edge/Chrome
([docs/16](https://github.com/gqy20/actl/blob/main/docs/16-browser-scenarios.md));
Calculator and Notepad end-to-end (fixtures). 16 app profiles live in
[skills/actl/apps](skills/actl/apps) with findings flowing back continuously;
the full battle record index is in [docs](https://github.com/gqy20/actl/blob/main/docs/README.md).

## How it compares

| Common approach | Problem |
|---|---|
| Screenshots + vision models (Agent-S, UI-TARS, …) | Slow and costly: a VLM pass per step, coordinates guessed from images, misses are unattributable |
| Have the AI write AutoHotkey scripts | Capable, but a "blind batch run": no per-step feedback, failures restart the whole script |
| MCP-only interface | Local shell use requires an additional service; actl's CLI works standalone |

Codex Computer Use officially prefers structured integration over computer use
whenever available — actl is that structured integration for the Windows
desktop ([comparison](https://github.com/gqy20/actl/blob/main/docs/12-codex-computer-use-comparison.md)).

## Command surface

91 commands — `actl commands` lists them all; every `actl <cmd> -h` shows
what it does, its arguments, an example, and its error codes.

<details>
<summary><b>Expand all 91</b></summary>

| Category | Commands |
|---|---|
| Sense | `snapshot` `find` `get` `get-selection` `list-windows` `screenshot` `find-image` `collect` `status` `commands` |
| Act | `click` `right-click` `double-click` `middle-click` `triple-click` `hover` `drag` `scroll` `type` `paste` `press` `key-down` `key-up` `set-value` `select` `toggle` `focus` |
| Window | `launch` `focus-window` `close-window` `resize-window` `minimize-window` `maximize-window` `restore-window` `wait` `batch` |
| Verify | `verify` `extract` `verify-file` `verify-rows` `verify-owner` `verify-closed` `verify-window` `verify-clipboard` |
| Clipboard | `get-clipboard` `set-clipboard` `clipboard-save` `clipboard-restore` `clipboard-formats` |
| Decisions | `decide` |
| Flow | `flow-validate` `flow-run` `flow-status` `flow-pause` `flow-resume` `flow-revise` `flow-evidence` `flow-clean` `explore-writeback` |
| Recording | `record-start` `record-worker` `record-stop` `record-status` |
| Browser | `browser-launch` `browser-navigate` `browser-read` `browser-collect` `browser-netlog` `browser-type` `browser-click` `browser-close` `browser-list` `browser-targets` `browser-verify` |
| Apps | `app-register` `app-list` `app-locate` `app-remove` |
| Desktop & diagnostics | `desktop-prepare` `desktop-restore` `display-start` `display-stop` `display-status` `execution-status` `execution-recover` `focus-report` `front-report` `residue-report` `history` `task-archive` `preview` |

</details>

## Install

Requirements: Windows x64 (UIA ships with the OS; no drivers, no services).

**scoop** (recommended):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

<details>
<summary><b>cargo / direct download / loading the agent skill</b></summary>

**cargo**:

```bash
cargo install actl-cli
cargo install actl-uia --bin actl-signal
```

The crate is [actl-cli](https://crates.io/crates/actl-cli) (binary name `actl`);
the workspace also publishes [actl-core](https://crates.io/crates/actl-core)
(protocol layer) and [actl-uia](https://crates.io/crates/actl-uia) (UIA backend)
as independently reusable crates.

**Direct download** (no package manager): grab the full ZIP from
[Releases](https://github.com/gqy20/actl/releases/latest), keep `actl.exe` and
the same-version `actl-signal.exe` in one directory, and put that directory on
PATH. Read-only work needs only `actl.exe`; desktop writes need the signal
bar running and a one-time approval on it. The ZIP contains `actl.exe`,
`actl-signal.exe`,
`skills/actl/`, the README, and the LICENSE.

**Loading the agent skill**: the entry point is `skills/actl/SKILL.md`, with
app knowledge, error reference, and the flow index alongside. Unzipping does
not install the skill; register or link the complete `skills/actl` directory
into your agent's skill configuration (not just `SKILL.md`) and make sure the
agent can invoke `actl` from PATH. Skill docs use local relative links only —
no network needed to use them.

</details>

## FAQ

**Windows only?**
Yes, and it stays that way — precise targeting is built on Windows' own
accessibility layer (UIA). Linux/macOS desktops are not planned for now.

**Remote or MCP?**
Yes. `actl-mcp` runs the local/LAN mode: it starts on the controlled Windows
machine, and other devices connect with token pairing. Commands from a remote
device still execute through actl on that machine, with the signal bar
watching — the remote side cannot get around it. Real cross-device scenarios
are still being verified.

**How mature is it? Can I trust it?**
v0.2.3 (M3 phase): all 91 commands work end-to-end, and the main scenarios
are verified on real machines. What is not verified yet, we say so — every
conclusion in the docs states exactly how far it was tested. See
[project status](#project-status).

**Is Chinese input reliable?**
Yes — use the paste channel (`set-clipboard` + `ctrl+v`, or `type --paste`),
verified character-for-character. Typing Chinese through raw keyboard
injection picks up input-method commit noise, so paste is the recommended
route.

**Where do I find full command details?**
Every `actl <cmd> -h` shows what it does, its arguments, an example, and its
error codes; `actl commands` lists all 91 at once. For the full story, the
[design blueprint](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md)
(Chinese) is the source of truth — the help text is generated from it, so
they never disagree.

**Why don't the badges load?**
While the repository is private, the Actions badge renders only when signed
in; the crates.io and License badges show public data.

## Documentation

Project docs are written in Chinese; translation contributions are welcome.

- [Design blueprint](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md) — full command semantics and protocol contract
- [Field-test records](https://github.com/gqy20/actl/blob/main/docs/14-office-scenarios.md) — real-app war stories: Office loops
- [Browser battle records](https://github.com/gqy20/actl/blob/main/docs/16-browser-scenarios.md) — Edge/Chrome dual verification, login-state deep loops, visual-fallback xy: end to end
- [Research & design index](https://github.com/gqy20/actl/blob/main/docs/README.md) — landscape survey, PRD, tech decisions
- [Agent skill](skills/actl) — the actl usage skill and app profile packs for agents
- [Roadmap](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md) — milestones and versioning

## Project status

Currently v0.2.3 (M3 phase): all 91 commands work end-to-end — Calculator and
Notepad end-to-end tasks, cross-3-app data relays, a self-contained CDP browser
channel (login-state sessions / content projection / coordinate fallback),
bounded autonomous exploration with decision cards plus model-assisted
answering (`decide` with a discriminative provider of your choice, and
`flow-run --auto-decide` where confident answers continue the run and
abstentions hand control back to the human), GUI recording (human demo → YAML),
focus-element addressing (`focused:`), runtime governance (failure captures,
popup gating, revised resume, idempotent steps), and the actl-mcp adapter
service (agents on other devices driving this machine via token pairing under
local signal-bar supervision; cross-device acceptance pending). App knowledge
accumulates as packs; flows are governed as YAML. Field-test findings are
continuously folded back into the docs and app packs. See the
[roadmap](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md) for milestones and versioning.

## Contributing

Rust workspace, trunk-based (`main` always green). After cloning, `make setup`
installs the git hooks and pinned toolchain in one step; run `make gate` before
committing. Collaboration rules (Conventional Commits, testing discipline,
single-source gating) live in [AGENTS.md](AGENTS.md), shared by humans and AI.

## License

[MIT](LICENSE)

> Give the desktop to your agent; keep the decisions for yourself.
