# 01 · 调研全景:AI 操作电脑的开源方案格局

> 历史调研/估算，保留当时口径。当前范围以 [PRD](PRD.md) 和 [ROADMAP](ROADMAP.md) 为准：
> MCP 计划在被控 Windows 设备本地启动供其他设备调用；Linux/macOS 执行后端暂不考虑实现。
> 下文对纯 CLI 的估算不包含未来 MCP 服务。

日期:2026-09-26 · 结论已按 ≥1000 GitHub stars 的可信度门槛筛选

## 1. 需求定义(三轮澄清的最终版)

我们要的东西同时满足以下条件,缺一不可:

| # | 条件 | 反例(被否的方向) |
|---|---|---|
| 1 | **充分操作电脑**:键鼠、窗口、剪贴板、应用、系统级操作全覆盖 | 浏览器域方案(browser-use 等) |
| 2 | **底层驱动**:SendInput / UIA / 窗口消息 / COM 级确定性执行 | 截图→VLM 看图→猜坐标的视觉路线(Agent-S、UI-TARS Desktop 等) |
| 3 | **CLI 是给 AI 用的**:agent 在工具循环里通过 shell 调用子命令 | "终端里塞 AI 聊天"(Open Interpreter、Agent TARS CLI 的交互形态) |
| 4 | **不用 MCP**:纯 CLI 可执行文件 + AI-native 输出 | MCP server 形态的 desktop control 项目 |
| 5 | **输出 AI-native**:结构化 JSON、稳定元素引用、机器可读错误码 | 面向人类的 stdout(AutoHotkey、nircmd 等老牌工具) |
| 6 | **开源 + 社区验证(≥1000★)** | desktop-cli(1★)等仅可作方向佐证 |

一个精确的类比:**浏览器域的 Playwright/agent-browser 之于 agent,我们要做桌面域的同等物**。

## 2. 严格匹配层(全部 ≥1000★,只有两个)

| 项目 | Stars | 平台 | 形态 | 路线 | 一句话评价 |
|---|---|---|---|---|---|
| [lahfir/agent-desktop](https://github.com/lahfir/agent-desktop) | 1.7k | **仅 macOS**(Windows/Linux 为 Planned) | 纯 CLI + FFI,无 MCP | accessibility tree + JSON envelope + 错误码契约 | 该品类的"教科书",设计最正统,平台缺失 |
| [mediar-ai/terminator](https://github.com/mediar-ai/terminator) | 1.6k | **Windows 完整** | Rust 库 + CLI(terminator-cli)+ MCP(可选) | UIA 确定性选择器(`name:"确定" role:Button`),后台执行不抢鼠标 | Windows 落地唯一合格选项,但 CLI 是附属品 |

关键数据(agent-desktop):**Windows crate 只有 76 行(空壳)**,确认没有现成实现可复用;其协议层(Apache-2.0)可作设计蓝本。

次级参照(未过 star 门槛,仅作方向佐证):
- [akiselev/desktop-cli](https://github.com/akiselev/desktop-cli)(1★,GPL-3.0):单人用 15k 行 Rust 做了三平台归一化(UIA/AT-SPI2/AXUIElement)+ application packs 投影,证明下限存在、深度存疑。
- AutoHotkey 生态:AI 生成 .ahk 脚本 → `AutoHotkey.exe` CLI 执行,是当下无专用工具时的现实过渡方案。

## 3. 近似层(不符合形态但提供了局部能力)

| 项目 | Stars | 不符合的原因 | 可取之处 |
|---|---|---|---|
| [AutoHotkey](https://github.com/AutoHotkey/AutoHotkey) | 13.2k | 输出非 AI-native,面向人类 | Windows 底层操作能力覆盖 20 年验证,超 Quicker |
| [microsoft/UFO](https://github.com/microsoft/UFO) | 9.8k | "塞 AI"研究框架形态 | UIA 混合执行(GUI+API)的工程参考 |
| [OpenAdaptAI/OpenAdapt](https://github.com/OpenAdaptAI/OpenAdapt) | — | RPA 录制-编译路线 | "录制→LLM 编译脚本"的实验验证 |
| [openclaw/openclaw](https://github.com/openclaw/openclaw) | ~390k | GUI 控制非核心,是个人助理运行时 | agent 侧的入口/编排层可以选它 |
| 微软 [AutoGenesis/pywinauto-mcp-server](https://github.com/microsoft/AutoGenesis) | 134 | MCP 形态、实验性 | verify_* 工具族(RPA 思路移植)可参考 |

## 4. 对照层(高星但全在别的赛道)

browser-use 108k · Stagehand 23.7k · Skyvern 22.7k · Cua 21k(SDK/沙箱形态)· UI-TARS-desktop 39.1k · Open Interpreter 60k+ · Agent-S 12.4k —— 全部为视觉路线或"塞 AI"形态,不符合条件 2/3/4。

**browser-use 108k vs agent-desktop 1.7k 的悬殊,量化说明了该品类的注意力真空:社区资源被视觉路线吸走,accessibility-first 的 CLI 形态 2025 底才冒头。**

## 5. 品类观察

1. **同类项目全是 Rust**(agent-desktop / terminator / desktop-cli),不是巧合:单二进制分发 + 大 accessibility 树遍历性能 + COM 生命周期安全 + 无运行时依赖,是品类需求交集,恰为 Rust 甜区。
2. **"CLI 优于 MCP"正在形成共识**:Simon Willison([MCP tag](https://simonwillison.net/tags/model-context-protocol))与 [HN 讨论](https://news.ycombinator.com/item?id=47208398)公认 CLI 的优势是 Unix 可组合性、更低上下文开销、模型对经典 CLI 的先验知识;[Arize 实测](https://arize.com/blog/mcp-vs-cli-skills-for-agents-what-our-eval-found-and-which-you-should-use)支持该结论;Anthropic 的 skills 路线本质是"CLI 工具 + 使用说明"。MCP 剩余优势主要是鉴权隔离与工具发现。
3. **浏览器域已验证该哲学**:Playwright MCP 默认 accessibility snapshot 而非截图,是可靠性/成本共识;桌面域尚无等价物。
4. **平台割裂**:macOS 有 agent-desktop(+macos-use),Windows 有 terminator,Linux 无,没有任何项目同时做到"跨平台 + 设计完整 + 社区验证"。

## 6. 参考链接汇总

核心:见上表。其余:agent-desktop 的 [HN 讨论](https://news.ycombinator.com/item?id=47982708) · [terminator-cli crates.io](https://crates.io/crates/terminator-cli/0.23.29) · [macos-use](https://macos-use.dev) · [PyWinAuto MCP 对比](https://skywork.ai)(以上链接来自搜索结果,未逐一直接核验仓库路径的:Cua → github.com/trycua/cua)
