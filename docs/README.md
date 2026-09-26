# computer_cli 调研与设计文档

> 目标:做一个 **Windows 上的 AI-native 桌面操作 CLI**——纯命令行、无 MCP、底层驱动(UIA/accessibility 而非截图视觉),输出为 AI 友好的结构化 JSON。定位是"Quicker 的动作引擎"在 AI 时代的原语层,供任意 LLM agent 在 shell 中直接调用。

**状态**:调研完成,设计蓝图已固化,未开始实现。
**日期**:2026-09-26

## TL;DR

1. **需求定义经过三轮澄清收敛**:不是视觉路线的 computer-use agent,不是"终端里塞 AI 聊天",也不是 MCP server——而是**给 AI 用的原子操作 CLI**(类似 ripgrep 之于 Claude Code)。
2. **该品类刚出现且 Windows 是空位**:严格匹配的开源项目只有 agent-desktop(1.7k★,仅 macOS)和 terminator(1.6k★,Windows 但 CLI 非重心);两者设计互不覆盖,高星项目全部集中在视觉/浏览器赛道。
3. **技术选型 Rust**:冷启动 ~10ms、微软官方 UIA binding(windows-rs)、单二进制分发、与品类全部同类项目同语言,可直接对齐 agent-desktop 的 Apache-2.0 协议设计。
4. **代码量实测校准**:认真 MVP 约 15,000–25,000 行 Rust,solo 开发 1.5–3 个月;UIA 后端是行数黑洞且调试时间约为编写时间的 3 倍。
5. **借鉴 RPA 可靠性工程**:选择器回退链、操作-验证闭环、结构化提取、等待谓词、失败留证;但摒弃其"锁死流程"哲学,错误协议要为 agent 自愈设计。

## 文档索引

**执行层(项目启动)**

| 文档 | 内容 |
|---|---|
| [PRD.md](PRD.md) | 产品需求:定位、用户故事、功能/非功能需求、成功指标、风险 |
| [ROADMAP.md](ROADMAP.md) | 路线图:Phase 0–3 的范围、交付物、退出标准、排期原则 |
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
