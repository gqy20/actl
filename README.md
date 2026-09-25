# actl

[![CI](https://github.com/gqy20/actl/actions/workflows/ci.yml/badge.svg)](https://github.com/gqy20/actl/actions/workflows/ci.yml)

> 让 AI 像人一样操作 Windows 软件——但更快、更准、每一步可验证。

**actl** 是一个给 AI agent 用的 Windows 命令行工具:agent 在 shell 里敲一条条命令,就能点击按钮、输入文字、读写剪贴板、操作任何桌面应用。它**不看截图、不猜坐标**——直接读取系统的无障碍树(UIA),像 Playwright 读网页 DOM 一样精确找到控件再操作,输出结构化 JSON。

## 为什么需要它

| 常见方案 | 问题 |
|---|---|
| 截图 + 视觉模型(Agent-S、UI-TARS 等) | 慢、贵:每步过一遍 VLM,看图猜坐标,点错无法归因 |
| 让 AI 写 AutoHotkey 脚本 | 能力够,但是"整段盲跑":没有逐步反馈,失败只能整段重来 |
| 挂一个 MCP server | 重、占 agent 上下文;actl 是普通 CLI,任何能跑 shell 的 agent 拿来就用 |

一句话定位:**Playwright 之于浏览器,actl 之于 Windows 桌面。**

## 它长什么样(设计预览,M1 起实现)

agent 的一次典型工作循环:

```bash
# 1. 看一眼:当前窗口有什么可操作的元素,拿到稳定引用 @eN
$ actl snapshot --app notepad
{"version":"1","ok":true,"command":"snapshot","data":{"elements":[
  {"ref":"@e1","role":"MenuItem","name":"文件(F)"},
  {"ref":"@e5","role":"Edit","name":"文本编辑器"}]},"snapshot_id":"s8f3k2p9"}

# 2. 操作:输入文字
$ actl type @e5 "hello from agent"
{"version":"1","ok":true,"command":"type","data":{"ref":"@e5"},"duration_ms":38}

# 3. 确认:按键保存,并附带对结果的预期(失败立即报错而不是静默)
$ actl press ctrl+s --expect window:"另存为"
{"version":"1","ok":true,"command":"press","data":{...}}
```

三个关键设计:输出全是结构化 JSON;元素用稳定引用(`@e5`)而非坐标定位;失败返回机器可读错误码与下一步建议(如 `STALE_REF` → 重新 snapshot),agent 能自我恢复。完整命令面见[设计蓝图](docs/06-design-blueprint.md)。

## 安装

尚未发布(pre-M1)。发布后:`cargo install actl-cli`(二进制名 `actl`,约 10MB 单文件,零依赖)。

## 项目状态

🚧 规范基建与设计已闭合,进入 M0 spike / M1 激进 MVP。阶段目标见 [ROADMAP](docs/ROADMAP.md)。

## 文档

[调研与设计全索引](docs/README.md)(竞品调研、PRD、技术选型、设计蓝图)· [AGENTS.md](AGENTS.md)(协作规范,人与 AI 共用)

## 开发

```bash
git clone <repo-url> && cd actl
make setup        # 一步配置:安装 git hooks + 激活 pinned 工具链
make gate         # 质量门禁:fmt + clippy + test(与 pre-push、CI 同一实现)
```

> 无 make 的环境(如纯净 Windows)等价直达:`cargo xtask setup` / `cargo xtask gate`——
> Makefile 是惯例入口(纯转发),实现单点在 [crates/xtask](crates/xtask)。

## License

TBD(倾向 Apache-2.0,与协议蓝本 agent-desktop 对齐)
