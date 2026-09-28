# computer_cli 调研与设计文档

> 目标:做一个 **Windows 上的 AI-native 桌面操作 CLI**——CLI 为基础入口、底层驱动(UIA/accessibility 而非截图视觉),输出为 AI 友好的结构化 JSON。定位是"Quicker 的动作引擎"在 AI 时代的原语层,供任意 LLM agent 在 shell 中直接调用。

**状态**：已发布实现与未发布变更见 [CHANGELOG](../CHANGELOG.md)，剩余工作见 [ROADMAP](ROADMAP.md)。
**更新**：2026-09-28。执行端仅面向 Windows，Linux/macOS 暂不考虑实现。
计划在被控设备本地启动可选 MCP 服务，供其他设备调用本设备能力；当前尚未实现。

## TL;DR

1. **需求定义经过三轮澄清收敛**:不是视觉路线的 computer-use agent,不是"终端里塞 AI 聊天",核心通过 shell 独立调用；远程访问另规划 MCP 适配服务。产品基础是**给 AI 用的原子操作 CLI**(类似 ripgrep 之于 Claude Code)。
2. **该品类刚出现且 Windows 是空位**:严格匹配的开源项目只有 agent-desktop(1.7k★,仅 macOS)和 terminator(1.6k★,Windows 但 CLI 非重心);两者设计互不覆盖,高星项目全部集中在视觉/浏览器赛道。
3. **技术选型 Rust**:冷启动 ~10ms、微软官方 UIA binding(windows-rs)、单二进制分发、与品类全部同类项目同语言,可直接对齐 agent-desktop 的 Apache-2.0 协议设计。
4. **代码量实测校准**:认真 MVP 约 15,000–25,000 行 Rust,solo 开发 1.5–3 个月;UIA 后端是行数黑洞且调试时间约为编写时间的 3 倍。
5. **借鉴 RPA 可靠性工程**:选择器回退链、操作-验证闭环、结构化提取、等待谓词、失败留证;但摒弃其"锁死流程"哲学,错误协议要为 agent 自愈设计。

## 文档索引

**执行层(项目启动)**

| 文档 | 内容 |
|---|---|
| [PRD.md](PRD.md) | 产品需求:定位、用户故事、功能/非功能需求、成功指标、风险 |
| [ROADMAP.md](ROADMAP.md) | 路线图：剩余工作、验收标准、MCP 访问方向与平台范围 |
| [TECH_STACK.md](TECH_STACK.md) | 技术选型规格:工具链、workspace 结构、依赖清单、ADR、测试与分发 |
| [07-golden-tasks.md](07-golden-tasks.md) | 金任务集:各 Phase 的验收标的与测试纪律 |
| [spike-findings.md](spike-findings.md) | M0 坑清单:五场景实测结论、性能量级、uiautomation crate 评估 |

**调研层(决策依据)**

| 文档 | 内容 |
|---|---|
| [01-research-landscape.md](01-research-landscape.md) | 需求定义历程、竞品全景(star/链接/结论)、品类观察 |
| [02-gap-analysis.md](02-gap-analysis.md) | 距离"Quicker 级 CLI"的差距矩阵与关键缺口、机会判断 |
| [03-rpa-vs-quicker.md](03-rpa-vs-quicker.md) | RPA 与 Quicker 的本质区别、可借鉴的五条思路、该警惕的教训 |
| [04-tech-decision.md](04-tech-decision.md) | 语言选型论证(Rust 主选/C# 备选)、技术栈清单、架构要点 |
| [05-implementation-estimate.md](05-implementation-estimate.md) | 基于三个同类仓库实测的代码量与工期估算 |
| [06-design-blueprint.md](06-design-blueprint.md) | 产品设计蓝图:命令面、JSON 输出协议、ref 机制、里程碑 |

**后续提案与专题研究（不覆盖现行规范）**

| 文档 | 内容 |
|---|---|
| [08-agent-cli-jev-design-proposal.md](08-agent-cli-jev-design-proposal.md) | 上层 agent 与 Jev 局部执行的职责、任务契约和输入占用边界 |
| [09-input-backends-review.md](09-input-backends-review.md) | 鼠标定位、键盘注入、开源方案、真实输入接管与差距评审 |
| [10-quicker-demand-research.md](10-quicker-demand-research.md) | Quicker 公开动作需求抽样、执行路线和 actl 场景优先级 |
| [11-desktop-interaction-and-display-research.md](11-desktop-interaction-and-display-research.md) | 后期显示层、分屏与隔离、暂停接管、人工介入、状态协议及用户研究方案 |
| [12-codex-computer-use-comparison.md](12-codex-computer-use-comparison.md) | Codex Computer Use(26.924)本机实证、sky window2 API × actl 命令面对照与生态定位 |
| [13-realtime-state-maintenance.md](13-realtime-state-maintenance.md) | 窗口/应用实时状态:三层机制、轮询 vs 事件生态实践、事故清单与 wait/serve 映射 |
| [14-office-scenarios.md](14-office-scenarios.md) | Office 场景实测战记:Excel GridPattern 直读+剪贴板直灌、Word TextPattern 三闭环 |
| [15-system-ui-census.md](15-system-ui-census.md) | 系统 UI 区域普查:快速设置/通知中心/桌面全可达,任务栏退化终审,Win 键勘误 |
| [16-browser-scenarios.md](16-browser-scenarios.md) | 浏览器场景实测战记:内容域投影数字、settle 证据、xhs/bili/douyin 三站深度闭环、扩展操作面、视觉兜底 xy: 端到端 |
| [26-tencent-meeting-interactions.md](26-tencent-meeting-interactions.md) | 腾讯会议：单人开会、预定/取消、云录制播放下载与 UIA/视觉边界 |
| [27-batch1-deep-tests.md](27-batch1-deep-tests.md) | 第一批应用深测：网易云、Zotero、VS Code、腾讯会议与夸克网盘入口 |
| [28-meeting-quark-visual-deep.md](28-meeting-quark-visual-deep.md) | 腾讯会议时间/时长点击、夸克搜索焦点与组件分析、预览和目录导航 |
