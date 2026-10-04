# actl

[简体中文](README.md) · [English](README.en.md)

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> 让 AI 像人一样操作 Windows 软件——但更快、更准、每一步可验证。

## 为什么是 actl

**actl** 是一个给 AI agent 用的 Windows 命令行工具:agent 在 shell 里敲一条条命令,就能点击按钮、输入文字、读写剪贴板、操作任何桌面应用。它**不看截图、不猜坐标**——直接读取系统的无障碍树(UIA),像 Playwright 读网页 DOM 一样精确找到控件再操作,输出结构化 JSON。

| 常见方案 | 问题 |
|---|---|
| 截图 + 视觉模型(Agent-S、UI-TARS 等) | 慢、贵:每步过一遍 VLM,看图猜坐标,点错无法归因 |
| 让 AI 写 AutoHotkey 脚本 | 能力够,但是"整段盲跑":没有逐步反馈,失败只能整段重来 |
| 仅提供 MCP 接口 | 本机 shell 调用还需额外服务；actl 的 CLI 可独立使用 |

一句话定位:**Playwright 之于浏览器,actl 之于 Windows 桌面**。Codex Computer Use 的官方降级顺序即"有结构化集成就优先于 computer use"——actl 就是 Windows 桌面上的那个结构化集成([对照分析](https://github.com/gqy20/actl/blob/main/docs/12-codex-computer-use-comparison.md))。

平台范围：Windows；Linux/macOS 桌面执行后端暂不考虑实现。计划提供可选 MCP 服务，
在被控 Windows 设备本地启动，供其他设备调用本设备能力；当前尚未实现，见 [路线图](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md)。

## 安装

要求:Windows x64(UIA 为系统自带,无需驱动或后台服务)。

**scoop**(推荐):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

**cargo**:

```bash
cargo install actl-cli
cargo install actl-uia --bin actl-signal
```

crate 名 [actl-cli](https://crates.io/crates/actl-cli),二进制名 `actl`;workspace 另有
[actl-core](https://crates.io/crates/actl-core)(协议层)与
[actl-uia](https://crates.io/crates/actl-uia)(UIA 后端)两个可独立复用的 crate。

<details>
<summary><b>直接下载</b>(免包管理器)</summary>

从 [Releases](https://github.com/gqy20/actl/releases/latest) 下载完整 ZIP，将
`actl.exe` 和同版本 `actl-signal.exe` 保存在同一目录，并把该目录加入 PATH。
只读观察可独立使用主程序；桌面写操作需要配套显示端就绪和人工交接。
完整 ZIP 包含 `actl.exe`、`actl-signal.exe`、`skills/actl/`、README 和 LICENSE。

</details>

## 给 Agent 加载技能

技能入口是 `skills/actl/SKILL.md`，应用知识、错误说明和流程索引在同目录下。
解压 ZIP 不会自动安装技能；按所用 Agent 的技能配置方式，将完整的 `skills/actl`
目录注册或链接到它的技能目录，并确保 Agent 能调用 PATH 中的 `actl`。
流程编写、恢复、任务归档和错误说明随技能保存在 `references/` 中，可单独复制完整
`skills/actl/` 目录；不要只复制 `SKILL.md`。技能内仅使用本地相对链接，无需联网查阅使用说明。

## 快速上手

两个真实办公场景,感受"agent 操作你的软件"是什么样子——**每个都以可核对的效果收尾**,不是"大概点了"。(命令取自真实验证过的路线,完整流程见 [Agent Skill](skills/actl)。)

**场景一:让 agent 读取 Excel 表格里的数据**

```bash
actl launch excel
actl extract role:DataGrid --app Excel          # GridPattern 直读整个表格,拿到结构化数据
```

效果:agent 拿到的是**结构化的行列数据**,不是截图——可以直接回答"B 列合计是多少"、把数据搬进另一个表、或写进报告。表格位置变了重跑一条命令就行,不用重新教它。

**场景二:让 agent 在 Word 文档里写一段话,并自己核对写对了**

```bash
actl set-clipboard '本报告由 agent 自动生成'
actl click role:Document --app Word                              # 定位正文焦点
actl press ctrl+v --target role:Document --app Word              # 粘贴(--target 断言焦点真的在正文,不是搜索框)
actl get role:Document --property text --app Word                # 读回全文
```

效果:最后一条**读回全文**——agent 自己确认文字真的在文档里,不会在"可能没粘上"的情况下宣称完成;`--target` 保证输入落在正文而不是恰好在窗口里就算数。点错目标时错误自带候选列表和恢复建议,agent 自己消歧重试,不需要人工看日志。

两条使用提示:

- **桌面写操作(点击/输入)需要 `actl-signal` 显示端就绪,并在横条上人工放行**——agent 动你的桌面,你全程知情,Ctrl+Alt+F12 随时急停。这是产品性格,不是限制;读操作(场景一)可独立使用。
- 中文输入法环境下粘贴通道(`set-clipboard` + `ctrl+v`)是可靠输入方式;每条命令 `actl <cmd> -h` 均有四要素 help(用途/参数/示例/错误码);完整命令语义与协议契约见[设计蓝图](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md)。

## 核心特性

**它找得到,也点得准。** 直接读系统无障碍树(UIA),像 Playwright 读 DOM 一样精确定位控件——不截图、不猜坐标。虚拟化大列表、老 Win32 报表、Chromium/Electron 正文都实测通达。看截图的 agent 点错只能重来;actl 点错会告诉你点错了,以及有哪几个候选。

**它做完一步,验证一步。** 每次操作后可以立即断言"屏幕上现在写着什么"——不是"应该写上了"。值比对、属性谓词、文件内容、窗口状态全套断言原语;失败的错误自带错误码、恢复建议和结构化证据,agent 拿到就能自己消歧重试,不用人工看日志猜哪里断了。

**它动你电脑,你全程知情。** 屏幕右上角常驻一条迷你状态条(不抢焦点):绿=执行中、琥珀=正在注入键鼠、红=已停止。**Ctrl+Alt+F12** 随时全局急停,在途注入在动作边界被拒。桌面写操作默认要你在状态条上放行一次——agent 用你的键盘鼠标,先取得你的同意。

**它不抢你的手。** 默认全部走 UIA 后台调用,不碰你的鼠标键盘;需要真实移动鼠标的动作(拖拽等)必须显式 `--physical` 声明。输入前核对物理前台窗口,你正在打字时它拒绝注入,绝不踩掉你的按键。

**流程是文件,不是黑盒。** 多步任务写成 YAML 流程(带输入声明、验证步骤、暂停点),`flow-run` 一步执行、断点可恢复。跑偏了不要从头重跑——`flow-revise` 版本化修订原地续跑(旧计划字节归档、谱系可审计);环境准备步用 `ensure:` 幂等声明,重跑前缀自动短路;写步动手前先听前台弹层,未知弹窗停下判读而不是硬闯。跑完的流程沉淀成可复用模板——同一个任务第二次就是一条命令的事。

**你演示一遍,它变成流程。** 信号条右键「开始录制」,人用鼠标键盘真实操作一遍,`record-stop` 出机械可复放的 YAML 草稿(IME 中文提交值、窗口等待、输入参数化全自动编译),agent 改写一轮后单命令重放。人演示与 agent 自主探索两条路,在回写层汇成同一份应用知识。

### 命令一览(76 条,`actl commands` 自发现)

| 类别 | 命令 |
|---|---|
| 感知 | `snapshot` `find` `get` `list-windows` `screenshot` `status` `commands` |
| 执行 | `click` `right-click` `double-click` `middle-click` `triple-click` `hover` `drag` `scroll` `type` `paste` `press` `key-down` `key-up` `set-value` `select` `toggle` |
| 窗口 | `launch` `focus-window` `close-window` `resize-window` `minimize-window` `restore-window` `wait` `batch` |
| 验证 | `verify` `extract` `verify-file` `verify-rows` `verify-owner` `verify-closed` `verify-window` `verify-clipboard` |
| 剪贴板 | `get-clipboard` `set-clipboard` |
| 流程 | `flow-validate` `flow-run` `flow-status` `flow-pause` `flow-resume` `flow-revise` `flow-evidence` `flow-clean` `explore-writeback` |
| 录制 | `record-start` `record-worker` `record-stop` `record-status` |
| 浏览器 | `browser-launch` `browser-navigate` `browser-read` `browser-type` `browser-click` `browser-close` `browser-list` `browser-verify` |
| 应用注册 | `app-register` `app-list` `app-remove` |
| 桌面与诊断 | `desktop-prepare` `desktop-restore` `display-start` `display-status` `execution-status` `execution-recover` `focus-report` `front-report` `history` `task-archive` `preview` |

## 文档

- [设计蓝图](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md)——完整命令语义与协议契约
- [场景实测记录](https://github.com/gqy20/actl/blob/main/docs/14-office-scenarios.md)——Office 三闭环、微信核心通道等真实应用战记
- [浏览器场景战记](https://github.com/gqy20/actl/blob/main/docs/16-browser-scenarios.md)——Edge/Chrome 双验证、xhs/bili/douyin 登录态深度闭环、扩展操作面、视觉兜底 xy: 端到端
- [调研与设计全索引](https://github.com/gqy20/actl/blob/main/docs/README.md)——竞品调研、PRD、技术选型、实测战记
- [Agent Skill](skills/actl)——给 agent 的 actl 使用技能与应用档案
- [业务流程目录](skills/actl/workflows/README.md)——actl 技能内部的业务 YAML 归属、索引与实现边界
- [路线图](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md)——阶段划分与版本口径

## 项目状态

当前 v0.2.1(M3 阶段):76 条命令全链路可用——计算器与记事本端到端任务、跨
3 应用数据搬运、浏览器自有 CDP 通道(登录态会话/内容投影/坐标兜底)、框选/长按/
轨迹拖拽指针扩展、探索模式(有界自主探索+决策卡外抛)、**GUI 录制(人演示→
YAML,右键启停)、焦点元素寻址(`focused:`,树不可见应用的单次注入打字)、
运行时治理(失败采集包/弹层门禁/修订续跑/幂等步骤/进程看门狗)**均实测落地;
应用知识以 pack 形式沉淀,流程以 YAML 治理。场景结论持续沉淀进 docs 与应用档案。
阶段划分与版本口径见 [ROADMAP](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md)。

## 参与开发

Rust workspace,trunk-based(`main` 始终绿),协作规范人与 AI 共用([AGENTS.md](AGENTS.md))。

```bash
make setup    # 克隆后一次:git hooks + 锁定工具链
make dev      # 诊断循环:秒级增量 DEBUG 构建
make gate     # 提交前门禁:fmt + clippy + test + packs
```

开发机要求:Rust(锁定于 [rust-toolchain.toml](rust-toolchain.toml));详情页前端另需 Node 24 与 pnpm(仅开发与预览,发布 EXE 内嵌静态页面不需要)。真机回归与验收入口见 `cargo xtask`(无参列出全部任务)。

## License

[MIT](LICENSE)
