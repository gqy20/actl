# actl

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/actl-cli.svg)](https://crates.io/crates/actl-cli)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> 仓库私有期间 Actions 徽章仅登录可见;crates.io/License 徽章为公开数据。

> 让 AI 像人一样操作 Windows 软件——但更快、更准、每一步可验证。

**actl** 是一个给 AI agent 用的 Windows 命令行工具:agent 在 shell 里敲一条条命令,就能点击按钮、输入文字、读写剪贴板、操作任何桌面应用。它**不看截图、不猜坐标**——直接读取系统的无障碍树(UIA),像 Playwright 读网页 DOM 一样精确找到控件再操作,输出结构化 JSON。

| 常见方案 | 问题 |
|---|---|
| 截图 + 视觉模型(Agent-S、UI-TARS 等) | 慢、贵:每步过一遍 VLM,看图猜坐标,点错无法归因 |
| 让 AI 写 AutoHotkey 脚本 | 能力够,但是"整段盲跑":没有逐步反馈,失败只能整段重来 |
| 挂一个 MCP server | 重、占 agent 上下文;actl 是普通 CLI,任何能跑 shell 的 agent 拿来就用 |

一句话定位:**Playwright 之于浏览器,actl 之于 Windows 桌面。**
(Codex Computer Use 的官方降级顺序即"有结构化集成就优先于 computer use"——actl 就是 Windows 桌面上的那个结构化集成,对照分析见 [docs/12](docs/12-codex-computer-use-comparison.md)。)

## 安装

**scoop**(推荐):

```powershell
scoop install https://raw.githubusercontent.com/gqy20/actl/main/scoop/actl.json
```

**直接下载**:从 [Releases](https://github.com/gqy20/actl/releases/latest) 拿
`actl_vX.Y.Z_windows_amd64.exe`(主程序单文件即可用);完整包 zip 额外含
`actl-signal.exe`(桌面信号条,建议同装)。

**cargo**:`cargo install actl-cli`(crate 名 [actl-cli](https://crates.io/crates/actl-cli),二进制名 `actl`;workspace 另有 [actl-core](https://crates.io/crates/actl-core)(协议层)与 [actl-uia](https://crates.io/crates/actl-uia)(UIA 后端)两个可独立复用的 crate)。

**源码**:

```bash
git clone https://github.com/gqy20/actl && cd actl
make setup   # 一步配置:git hooks + pinned 工具链(等价 cargo xtask setup)
cargo build --release
```

## 快速上手

```bash
# 看一眼:目标窗口有哪些可操作元素(拿到稳定引用 @eN)
actl snapshot --app 记事本 --skeleton

# 点击(UIA Invoke,后台执行不抢鼠标)
actl click id:clearButton --app 计算器

# 输入文字(带护栏:焦点被抢自动停在批边界,如实上报已送达量)
actl type role:Document "报告正文..." --app 记事本
actl type role:Document "路径" --app 记事本 --paste   # 中文输入法环境推荐

# 组合键 + 结果断言(新窗口没出现立即报 ASSERTION_FAILED,不静默假成功)
actl press ctrl+s --app 记事本 --expect window:另存为

# 多候选歧义?错误里直接带结构化候选(含可重放 ref),不用重拍快照
actl click role:Button --app 计算器        # → AMBIGUOUS + candidates[]
actl click role:Button#3 --app 计算器      # 序数直选
actl click role:Button --near id:clearButton --app 计算器   # 锚点就近

# 立即断言(verify)与耐心等待(wait)
actl verify --value id:CalculatorResults "56,088" --contains --app 计算器
actl wait --window 另存为 --timeout 5000
```

输出统一为 envelope JSON(`ok`/`command`/`data|error`/`duration_ms`),
错误带机器可读错误码 + 恢复建议 + 结构化 evidence,agent 可自我恢复:

```json
{"version":"1","ok":false,"command":"click","error":{
  "code":"AMBIGUOUS",
  "message":"role:Button matches 34 elements in the window",
  "hint":"tighten the selector or pick by index",
  "evidence":{"candidates":[{"ref":"@e2","role":"Button","name":"最小化 计算器"}]}},
 "duration_ms":1563}
```

## 桌面信号条(actl-signal)

自动化运行时,人的知情权与逃生口:右上角常驻迷你状态条(不抢焦点、DPI 感知、
Source Code Pro),状态灯取市场通行色——**绿=执行中 · 琥珀=正在注入键鼠 ·
红=已停止 · 灰=空闲 · 暗=失联**。

- **Ctrl+Alt+F12**:全局紧急停止,在途注入在动作边界被拒(`ABORTED`);
- **点击状态条**:清除停止标志恢复;
- 状态全部拉取(输入占用锁 + 状态文件),零事件订阅。

## 安全设计

- **verify-then-inject**:任何键盘/指针注入前以 Win32 物理前台为唯一判据核对目标,
  不匹配宁可拒绝不错发;
- **fail-closed**:`press`/`type` 必须显式 `--app`;指针物理动作必须 `--physical`;
  ref 回放可带 `--snapshot <id>` 校验窗口实例(同标题重开 → STALE_REF);
- **输入占用协调**:多 actl 实例互斥;长注入分批复核焦点,被抢即停并上报;
- 修饰键治理:检测用户按住 ctrl/shift 时拒绝注入(防语义劫持),绝不踩掉用户按键。

## 命令面(26,`actl commands` 自发现;`actl <cmd> -h` 四要素:用途/参数/示例/错误码)

感知 `snapshot find get list-windows status commands` ·
执行 `click right-click double-click hover drag scroll type press key-down key-up set-value select toggle` ·
窗口 `launch focus-window close-window resize-window wait batch` ·
验证 `verify extract` · 系统 `get-clipboard set-clipboard`

定位链四级(primary → fuzzy-name → role-ordinal/anchor → **虚拟化容器 item-container**);
`wait` 支持属性谓词(`--property value=OK`/`name=子串`/`checked=on`);虚拟化大列表
(资源管理器条目级)、老 Win32 报表提取(msinfo32)、Chromium/Electron 系正文读取均实测通达。

完整语义与协议契约见[设计蓝图](docs/06-design-blueprint.md)。

## 项目状态

M2(收官批进行中):26 命令全链路,金任务(计算器/记事本保存)双双 10/10,US1 跨 3 应用
数据搬运 16/16;定位链四级含虚拟化容器兜底(Explorer 条目级实测通达);envelope 契约测试;
显示信号层(状态灯波纹动画/实时耗时);batch 步级可观测 + ACTL_TRACE 阶段打点。
阶段与退出标准见 [ROADMAP](docs/ROADMAP.md);版本口径:M2 完成 = 0.2.0。

## 文档

[调研与设计全索引](docs/README.md)(竞品调研、PRD、技术选型、设计蓝图、实测战记)·
[AGENTS.md](AGENTS.md)(协作规范,人与 AI 共用)· [skills/actl](skills/actl)(Agent Skill)

## License

MIT
