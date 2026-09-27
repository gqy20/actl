# actl

[简体中文](README.md) · [English](README.en.md)

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> 仓库私有期间 Actions 徽章仅登录可见;crates.io/License 徽章为公开数据。

> 让 AI 像人一样操作 Windows 软件——但更快、更准、每一步可验证。

## 为什么是 actl

**actl** 是一个给 AI agent 用的 Windows 命令行工具:agent 在 shell 里敲一条条命令,就能点击按钮、输入文字、读写剪贴板、操作任何桌面应用。它**不看截图、不猜坐标**——直接读取系统的无障碍树(UIA),像 Playwright 读网页 DOM 一样精确找到控件再操作,输出结构化 JSON。

| 常见方案 | 问题 |
|---|---|
| 截图 + 视觉模型(Agent-S、UI-TARS 等) | 慢、贵:每步过一遍 VLM,看图猜坐标,点错无法归因 |
| 让 AI 写 AutoHotkey 脚本 | 能力够,但是"整段盲跑":没有逐步反馈,失败只能整段重来 |
| 挂一个 MCP server | 重、占 agent 上下文;actl 是普通 CLI,任何能跑 shell 的 agent 拿来就用 |

一句话定位:**Playwright 之于浏览器,actl 之于 Windows 桌面**。Codex Computer Use 的官方降级顺序即"有结构化集成就优先于 computer use"——actl 就是 Windows 桌面上的那个结构化集成([对照分析](docs/12-codex-computer-use-comparison.md))。

## 安装

要求:Windows x64(UIA 为系统自带,无需驱动或后台服务)。

**scoop**(推荐):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

<details>
<summary><b>直接下载</b>(免包管理器)</summary>

从 [Releases](https://github.com/gqy20/actl/releases/latest) 拿
`actl_vX.Y.Z_windows_amd64.exe`,主程序单文件即可用;完整包 zip 额外含
`actl-signal.exe`(桌面信号条,建议同装)。

</details>

<details>
<summary><b>cargo</b></summary>

```bash
cargo install actl-cli
```

crate 名 [actl-cli](https://crates.io/crates/actl-cli),二进制名 `actl`;workspace 另有
[actl-core](https://crates.io/crates/actl-core)(协议层)与
[actl-uia](https://crates.io/crates/actl-uia)(UIA 后端)两个可独立复用的 crate。

</details>

## 快速上手

三条命令走一遍"感知 → 执行 → 断言":

```bash
# 1. 感知:目标窗口有哪些可操作元素(拿到稳定引用 @eN)
actl snapshot --app 记事本 --skeleton

# 2. 执行:点击(UIA Invoke,后台执行不抢鼠标)
actl click id:clearButton --app 计算器

# 3. 断言:组合键后新窗口必须出现,否则立即报 ASSERTION_FAILED,不静默假成功
actl press ctrl+s --app 记事本 --expect window:另存为
```

中文输入法环境下,`actl type ... --paste` 走剪贴板通道输入文字,更稳。

输出统一为 envelope JSON(`ok`/`command`/`data|error`/`duration_ms`),错误带机器可读
错误码 + 恢复建议 + 结构化证据,agent 可自我恢复:

```json
{"version":"1","ok":false,"command":"click","error":{
  "code":"AMBIGUOUS",
  "message":"role:Button matches 34 elements in the window",
  "hint":"tighten the selector or pick by index",
  "evidence":{"candidates":[{"ref":"@e2","role":"Button","name":"最小化 计算器"}]}},
 "duration_ms":1563}
```

定位歧义时,错误里直接带结构化候选(含可重放 ref),不用重拍快照:

```bash
actl click role:Button --app 计算器                          # → AMBIGUOUS + candidates[]
actl click role:Button#3 --app 计算器                        # 序数直选
actl click role:Button --near id:clearButton --app 计算器    # 锚点就近
```

每条命令 `actl <cmd> -h` 均有四要素 help(用途/参数/示例/错误码);完整命令语义与
协议契约见[设计蓝图](docs/06-design-blueprint.md)。

## 核心特性

- **精准定位,不猜坐标**:直接读系统 UIA 无障碍树,支持 `@eN` 引用、`name:/id:/role:`
  选择器、序数与锚点就近消歧;虚拟化大列表(资源管理器条目级)、老 Win32 报表
  (msinfo32)、Chromium/Electron 系正文读取均实测通达。
- **逐步可验证**:`verify`/`wait` 支持值断言与属性谓词等待(`--property value=OK`/
  `name=子串`/`checked=on`);`batch` 步级可观测;每个错误都带错误码 + hint +
  evidence,agent 拿到就能自我恢复。
- **安全护栏,宁可拒绝不错发**:
  - 键鼠注入前以物理前台窗口为唯一判据核对目标,不匹配即拒绝;长注入分批复核焦点,
    被抢自动停在批边界并如实上报;
  - `type`/`press` 必须显式 `--app`,指针物理动作必须显式 `--physical`——默认走 UIA
    后台调用,不抢用户的鼠标键盘;
  - 检测到用户按住 Ctrl/Shift 时拒绝注入(防语义劫持),绝不踩掉用户按键;多 actl
    实例输入互斥。
- **桌面信号条 `actl-signal`**(建议同装):自动化运行时的知情权与逃生口——右上角
  常驻迷你状态条(不抢焦点、DPI 感知),绿=执行中 · 琥珀=正在注入键鼠 · 红=已停止 ·
  灰=空闲 · 暗=失联;**Ctrl+Alt+F12** 全局紧急停止,在途注入在动作边界被拒
  (`ABORTED`),点击状态条清除停止标志恢复。

### 命令一览(30 条,`actl commands` 自发现)

| 类别 | 命令 |
|---|---|
| 感知 | `snapshot` `find` `get` `list-windows` `status` `commands` |
| 执行 | `click` `right-click` `double-click` `hover` `drag` `scroll` `type` `paste` `press` `key-down` `key-up` `set-value` `select` `toggle` |
| 窗口 | `launch` `focus-window` `close-window` `resize-window` `wait` `batch` |
| 验证 | `verify` `extract` |
| 系统 | `get-clipboard` `set-clipboard` |

## 文档

- [设计蓝图](docs/06-design-blueprint.md)——完整命令语义与协议契约
- [场景实测记录](docs/14-office-scenarios.md)——Office 三闭环、微信核心通道等真实应用战记
- [调研与设计全索引](docs/README.md)——竞品调研、PRD、技术选型、实测战记
- [Agent Skill](skills/actl)——给 agent 的 actl 使用技能与应用档案
- [路线图](docs/ROADMAP.md)——阶段划分与版本口径

## 项目状态

当前 v0.1.x(M2 阶段):30 条命令全链路可用;计算器与记事本保存的端到端任务、跨
3 应用数据搬运均实测全绿;Office(Excel/Word)与微信核心通道已实测通达,场景结论
持续沉淀进 docs 与应用档案。阶段划分与版本口径见 [ROADMAP](docs/ROADMAP.md)。

## 参与开发

Rust workspace,trunk-based(`main` 始终绿)。克隆后 `make setup` 一步配好 git hooks
与锁定工具链,提交前 `make gate` 过门禁;协作规范(Conventional Commits、测试纪律、
门禁单点等)见 [AGENTS.md](AGENTS.md),人与 AI 共用。

## License

[MIT](LICENSE)
