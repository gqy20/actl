# actl

[简体中文](README.md) · [English](README.en.md)

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> 让 AI 像人一样操作 Windows 软件——但更快、更准、每一步可验证。

<!-- TODO(素材):assets/hero.gif——15 秒首屏演示,agent 从 Excel 提取数据、写进 Word、读回验证,全程信号条可见;录制脚本见 assets/demo-recording.md 任务一 -->

**actl** 是给 AI agent 的 Windows 命令行工具:agent 在 shell 里一条条调用,就能点击、输入、读写剪贴板、操作任何桌面应用,输出结构化 JSON。它**不用视觉模型看图猜点**——直接读系统无障碍树(UIA),像 Playwright 读网页 DOM 一样拿到控件的名字、角色和结构;截图只作观察证据与树不可见时的兜底。一句话定位:**Playwright 之于浏览器,actl 之于 Windows 桌面。**

## 快速上手

先跑通一条命令,再完成一个真实任务——**每个都以可核对的效果收尾,不是"大概点了"**。命令取自真机验证过的路线,演示数据用 [assets/demo-sales.xlsx](assets/demo-sales.xlsx)(7 个区域、两个月营收,合计与环比可直接对账)。

**① 一条命令读出整张表**(只读,装完就能跑):

```bash
actl extract role:DataGrid --app Excel
```

<!-- TODO(素材):assets/extract-output.png——上面这条命令的真实 JSON 输出节选 -->

**② 完整任务:把 Excel 的数据写进 Word,自己核对**(写操作要在信号条上放行一次):

```bash
actl extract role:DataGrid --app Excel                              # 读:整张表 → 结构化行列
actl set-clipboard '10月合计营收 84,000 元,环比 +12%,七大区域全部增长。'
actl click role:Document --app Word                                 # 定位正文
actl press ctrl+v --target role:Document --app Word                 # 粘贴;--target 断言焦点在正文,不是搜索框
actl get role:Document --property text --app Word                   # 验:读回全文
```

最后一条**读回全文**——agent 自己确认文字真的在文档里,不在"可能没粘上"时宣称完成;点错目标时错误自带候选列表和恢复建议,agent 自己消歧重试,不用人工看日志。中文输入走粘贴通道(`set-clipboard` + `ctrl+v`),是 IME 环境下的可靠输入方式。这个任务固化成 YAML 后,下次一条 `flow-run` 重放;也可以右键信号条录一遍你的演示,让它变成流程。

## 它能做什么

- **点得准,不看图猜点。** 读系统无障碍树(UIA)定位控件,拿到名字、角色、结构;虚拟化大列表、老 Win32 报表、Chromium/Electron 正文都实测通达。看截图的方案点错只能重来;actl 点错会告诉你点错了、错在哪、有哪几个候选。
- **做完一步,验证一步。** 值比对、属性谓词、文件内容、窗口状态全套断言——不是"应该写上了",是"屏幕上现在写着什么"。失败的错误自带错误码、恢复建议和结构化证据。
- **拿不准时,让模型先答——裁决权在你。** 探索平局、选择器歧义时,决策卡/错误信封直接管道给 `decide`,由你配置的判别式模型(本地 llama-server/LM Studio 即可;非生成式,不内置 LLM)给出选项与置信度,还会指明选中的是哪个控件,拿来就能直接重放。危险的候选在发给模型前就被剔掉,模型根本看不见;模型没把握就弃权,把决定交回你。`flow-run --auto-decide` 把「出卡→人答→继续」折叠成一条命令。
- **动你桌面,你全程知情。** 右上角常驻迷你信号条(不抢焦点):绿=执行中、琥珀=正在注入键鼠、红=已停止;**Ctrl+Alt+F12** 全局急停,在途注入在动作边界被拒;写操作默认要你在横条上放行一次。
- **不抢你的手。** 默认全部 UIA 后台调用,不碰你的鼠标键盘;需要真实移动鼠标的动作必须显式 `--physical`;输入前核对物理前台窗口,你正在打字时它拒绝注入。
- **流程是文件,不是黑盒。** 多步任务写成 YAML(输入声明、验证步骤、暂停点),`flow-run` 单命令执行、断点可恢复;跑偏了 `flow-revise` 改好计划原地接着跑,不用从头来;重复的准备步骤重跑时自动跳过。跑完沉淀成模板——同一个任务第二次就是一条命令的事。
- **你演示一遍,它变成流程。** 信号条右键「开始录制」,人真实操作一遍,`record-stop` 出可复放 YAML(输入法中文、窗口等待这些细节都自动处理,你输入的内容变成参数),换个参数单命令重放。

## 你是哪种用法?

| 你想… | 路径 |
|---|---|
| 让 AI agent 直接操作你的软件 | shell 逐条调用;给 agent 加载 [actl 技能](skills/actl)与应用档案 |
| 把重复任务固化成一条命令 | 写 YAML 流程,或信号条右键「开始录制」→ `flow-run` 单命令重放,断点可恢复 |
| 复用协议层或 UIA 通道 | [actl-core](https://crates.io/crates/actl-core)(协议)/ [actl-uia](https://crates.io/crates/actl-uia)(UIA 后端)两个可独立复用的 crate |

## 真实应用实测

整链实测过并留有战记:Office 三件套(Excel 提取、Word 写入回读,[docs/14](https://github.com/gqy20/actl/blob/main/docs/14-office-scenarios.md))、微信 4.1 全功能(发送/上传/转发/搜一搜,应用档案 wechat.yml)、腾讯会议([docs/26](https://github.com/gqy20/actl/blob/main/docs/26-tencent-meeting-interactions.md))、Edge/Chrome 自有 CDP 通道与登录态深度闭环([docs/16](https://github.com/gqy20/actl/blob/main/docs/16-browser-scenarios.md))、记事本/计算器端到端(fixtures)。16 个应用档案放在 [skills/actl/apps](skills/actl/apps):能用的路线、踩过的坑都记着,持续更新;全部战记见 [docs 索引](https://github.com/gqy20/actl/blob/main/docs/README.md)。

## 与常见方案对比

| 常见方案 | 问题 |
|---|---|
| 截图 + 视觉模型(Agent-S、UI-TARS 等) | 慢、贵:每步过一遍 VLM,看图猜坐标,点错无法归因 |
| 让 AI 写 AutoHotkey 脚本 | 能力够,但是"整段盲跑":没有逐步反馈,失败只能整段重来 |
| 仅提供 MCP 接口 | 本机 shell 调用还需额外服务;actl 的 CLI 可独立使用 |

Codex Computer Use 官方就建议:能走结构化集成就别用 computer use——actl 就是 Windows 桌面上的那个结构化集成([对照分析](https://github.com/gqy20/actl/blob/main/docs/12-codex-computer-use-comparison.md))。

## 命令一览

91 条命令,`actl commands` 一条命令全列出来;每条 `actl <cmd> -h` 都有完整帮助——干什么、参数怎么传、示例、出错是什么码。

<details>
<summary><b>展开全部 91 条</b></summary>

| 类别 | 命令 |
|---|---|
| 感知 | `snapshot` `find` `get` `get-selection` `list-windows` `screenshot` `find-image` `collect` `status` `commands` |
| 执行 | `click` `right-click` `double-click` `middle-click` `triple-click` `hover` `drag` `scroll` `type` `paste` `press` `key-down` `key-up` `set-value` `select` `toggle` `focus` |
| 窗口 | `launch` `focus-window` `close-window` `resize-window` `minimize-window` `maximize-window` `restore-window` `wait` `batch` |
| 验证 | `verify` `extract` `verify-file` `verify-rows` `verify-owner` `verify-closed` `verify-window` `verify-clipboard` |
| 剪贴板 | `get-clipboard` `set-clipboard` `clipboard-save` `clipboard-restore` `clipboard-formats` |
| 决策 | `decide` |
| 流程 | `flow-validate` `flow-run` `flow-status` `flow-pause` `flow-resume` `flow-revise` `flow-evidence` `flow-clean` `explore-writeback` |
| 录制 | `record-start` `record-worker` `record-stop` `record-status` |
| 浏览器 | `browser-launch` `browser-navigate` `browser-read` `browser-collect` `browser-netlog` `browser-type` `browser-click` `browser-close` `browser-list` `browser-targets` `browser-verify` |
| 应用注册 | `app-register` `app-list` `app-locate` `app-remove` |
| 桌面与诊断 | `desktop-prepare` `desktop-restore` `display-start` `display-stop` `display-status` `execution-status` `execution-recover` `focus-report` `front-report` `residue-report` `history` `task-archive` `preview` |

</details>

## 安装

要求:Windows x64(UIA 系统自带,无需驱动或后台服务)。

**scoop**(推荐):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

<details>
<summary><b>cargo / 直接下载 / 给 Agent 加载技能</b></summary>

**cargo**:

```bash
cargo install actl-cli
cargo install actl-uia --bin actl-signal
```

crate 名 [actl-cli](https://crates.io/crates/actl-cli),二进制名 `actl`;workspace 另有
[actl-core](https://crates.io/crates/actl-core)(协议层)与
[actl-uia](https://crates.io/crates/actl-uia)(UIA 后端)两个可独立复用的 crate。

**直接下载**(免包管理器):从 [Releases](https://github.com/gqy20/actl/releases/latest) 下载完整 ZIP,把
`actl.exe` 和同版本 `actl-signal.exe` 保存在同一目录,并把该目录加入 PATH。
只读操作装一个 `actl.exe` 就够;桌面写操作需要信号条就绪并在上面放行。
完整 ZIP 包含 `actl.exe`、`actl-signal.exe`、`skills/actl/`、README 和 LICENSE。

**给 Agent 加载技能**:技能入口是 `skills/actl/SKILL.md`,应用知识、错误说明和流程索引在同目录。
解压 ZIP 不会自动安装技能;按所用 Agent 的技能配置方式,将完整的 `skills/actl`
目录注册或链接到它的技能目录,并确保 Agent 能调用 PATH 中的 `actl`。
不要只复制 `SKILL.md`。技能内仅使用本地相对链接,无需联网查阅使用说明。

</details>

## FAQ

**只支持 Windows?**
对,而且只打算做 Windows——"点得准"靠的就是 Windows 自带的无障碍层(UIA)。Linux/macOS 桌面暂时不做。

**能远程调用,或者接 MCP 吗?**
可以。`actl-mcp` 已经跑通本地/局域网模式:在被告控的 Windows 电脑上启动,其他设备拿 token 配对过来调用。远程下发的动作,最后还是在本地这台机器上走 actl,信号条照常盯着——远端绕不过它。跨设备的真实场景还在验收中。

**现在什么成熟度?能放心用吗?**
v0.2.3,91 条命令全部可用,主要场景都在真实应用上验证过。没验证到的部分我们直说——文档里每条结论都写清楚验证到了哪一步,不吹。详见[项目状态](#项目状态)。

**中文输入可靠吗?**
可靠,走粘贴通道(`set-clipboard` + `ctrl+v`,或 `type --paste`),实测逐字命中。直接模拟键盘打中文会碰上输入法的提交噪声,所以推荐粘贴。

**命令的完整细节哪里查?**
每条 `actl <命令> -h` 都有完整帮助:干什么、参数怎么传、示例、出错是什么码;`actl commands` 一条命令列出全部 91 条。要刨根问底,[设计蓝图](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md)里有全部语义——help 和蓝图是同一份生成的,不会各说各话。

## 文档

- [设计蓝图](https://github.com/gqy20/actl/blob/main/docs/06-design-blueprint.md)——完整命令语义与协议契约
- [场景实测记录](https://github.com/gqy20/actl/blob/main/docs/14-office-scenarios.md)——Office 三闭环等真实应用战记
- [浏览器场景战记](https://github.com/gqy20/actl/blob/main/docs/16-browser-scenarios.md)——Edge/Chrome 双验证、登录态深度闭环、扩展操作面、视觉兜底 xy: 端到端
- [调研与设计全索引](https://github.com/gqy20/actl/blob/main/docs/README.md)——竞品调研、PRD、技术选型、实测战记
- [Agent Skill](skills/actl)——给 agent 的 actl 使用技能与应用档案
- [业务流程目录](skills/actl/workflows/README.md)——actl 技能内部的业务 YAML 归属、索引与实现边界
- [路线图](https://github.com/gqy20/actl/blob/main/docs/ROADMAP.md)——阶段划分与版本口径

## 项目状态

当前 v0.2.3(M3 阶段):91 条命令全链路可用——计算器与记事本端到端任务、跨
3 应用数据搬运、浏览器自有 CDP 通道(登录态会话/内容投影/坐标兜底)、框选/长按/
轨迹拖拽指针扩展、探索模式(有界自主探索+决策卡外抛)与**决策模型应答(`decide`
判别式 provider 消歧探索卡/AMBIGUOUS 信封 + `flow-run --auto-decide` 自动应答,
弃权交回人)**、**GUI 录制(人演示→
YAML,右键启停)、焦点元素寻址(`focused:`,树不可见应用的单次注入打字)、
运行时治理(失败采集包/弹层门禁/修订续跑/幂等步骤/进程看门狗)、**MCP 适配
服务 actl-mcp(其他设备的 agent 远程操作本机,token 配对+本地信号条监督**,
跨设备验收待场景)**均实测落地;应用知识以 pack 形式沉淀,流程以 YAML 治理。
场景结论持续沉淀进 docs 与应用档案。
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

> 把桌面交给 agent,把决定权留给自己。
