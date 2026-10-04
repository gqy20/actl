# PRD · Windows AI-native 桌面操作 CLI

版本:v0.1(草案) · 日期:2026-09-26 · 负责人:待定
关联:[调研全景](01-research-landscape.md) · [差距分析](02-gap-analysis.md) · [设计蓝图](06-design-blueprint.md) · [路线图](ROADMAP.md)

---

## 1. 产品概述

### 1.1 一句话定位

**给 AI agent 用的 Windows 桌面原子操作 CLI**:UIA(accessibility)驱动、原子命令可独立调用、输出机器友好——Playwright 之于浏览器,本产品之于 Windows 桌面。

### 1.2 问题陈述

1. agent 要操作 Windows 桌面应用时,主流方案是"截图→VLM 猜坐标"(慢、贵、不可靠)或 MCP server 形态(重、占上下文);
2. Quicker/AutoHotkey 证明了底层确定性操作的可行性,但它们面向人类,无 AI-native 接口;
3. Windows 已有 `winapp ui` 等 UIA CLI；本项目聚焦原子命令契约、目标身份、结果核验和人工交接的可靠性，并通过同类工具对照发现缺口。2026-09-28 的补充调研与本机证据见 [36](36-winapp-uia-diagnostics.md)，早期“工程空位”的判断不再作为产品依据。

### 1.3 目标与非目标

2026-09-29 用户授权简单 workflow 条件：即时只读断言驱动 when/unless/otherwise，覆盖窗口存在、前台状态、可选控件和已达目标值；未知结果停止，恢复保留已派发路线。不扩展循环、嵌套条件或通用表达式，见 [63](63-simple-workflow-conditions.md)。

2026-09-29 用户授权独立目标预览及后续候选纠正：为用户展示目标边界与拟执行动作，支持退出；为 agent 保留结构化定位诊断。候选纠正限定同一快照的明确候选，确认时重新核验身份。预览不触发目标操作、不授予交接权限，不扩大为可视化流程编辑器，见 [46](46-target-preview.md)。

**目标**:让任意 LLM agent(Claude Code、Codex、自研 agent)通过 shell 调用本 CLI,以确定性、可重试、token 高效的方式操作 Windows 桌面应用与系统。

2026-09-27 用户授权扩展：增加独立 YAML 顺序流程层，用于预检、步骤验证、持久化进度及
协作式暂停恢复；协议见 [20](20-yaml-workflow-protocol.md)。原子 CLI 继续可独立使用。
可视化流程编辑器、常驻 agent、通用脚本引擎仍不在范围内。

2026-09-29 用户授权去除应用 workflow 的外置资源准备脚本：增加受限的隔离目录、文本文件与随包模板复制声明，以及运行上下文默认值。由 flow-run 统一预检、初始化和执行；不开放任意脚本、shell、覆盖外部文件或恢复时重置资源。应用知识仍保留在 YAML 中。

2026-09-28 范围调整：计划提供独立 MCP 适配服务，在被控 Windows 设备上显式启动，
供其他设备上的客户端调用本设备能力；复用本地交接、急停和执行护栏。当前尚未实现，
传输、认证与恢复契约待设计，见 [ROADMAP](ROADMAP.md)。Linux/macOS 执行后端不安排实现。

**非目标(明确不做,详见 [06 §9](06-design-blueprint.md))**:可视化编排、触发面板/热键、动作市场、MCP 进核心、截图视觉定位、常驻 agent、Linux/macOS 桌面执行后端（暂不考虑实现）。

2026-09-28 用户授权浏览器接入原型：独立验证目标会话、iframe、一次写后核验与断连，
限定 Windows 隔离本地测试页面。暂不接入正式 CLI/YAML；生产适配层另行设计与验收，见 [34](34-browser-connection-prototype.md)。

2026-10-02 用户授权浏览器 CDP 正式接入设计与最小客户端 spike：A1 自研同步 CDP 客户端
（tungstenite 为唯一拟新增直接依赖）、系统 Edge 二进制 + actl 专用持久 profile（一次性人工
登录复用）、与 UIA 按会话归属双通道分工；cookie 护栏与协议面另行 spec，实现归 M3，
见 [72](72-browser-cdp-integration.md)。

2026-09-28 登记探索模式设计提案：算法主导的目标导向流程探索，语义决策点以决策卡
外抛，由外部应答者（启发式脚本、人工或 LLM agent）裁决；不内置 LLM 客户端，探索
运行为有界可暂停的流程任务，不是常驻 agent。当前仅设计未实现，实施前须完成 PRD
非目标核对与决策卡协议 spec 评审，见 [42](42-exploration-mode.md)。

2026-10-03 用户授权 GUI 操作录制立项（R1 起步）：低级键鼠钩子捕获 + 点击瞬间 UIA
反查 + 录制 trace 证据文件 + 机械编译初版 YAML；agent 改写轮在 skill 层（LLM 不进
CLI）；录制会话有界（显式 start/stop，非常驻），触发面 = CLI 正典 + 显示端右键菜单
动态项；文本录提交后最终值不录按键序列，密码框文本脱敏；设计提案与 Quicker 调研
见 [76](76-gui-recording.md)、[79](79-quicker-recording-research.md)。

2026-10-04 用户授权焦点元素目标语法（`focused:` / `focused:<Type>`，对话批准后
spec 落地）：解析到 OS 键盘焦点元素，服务树对语义选择器不可见但焦点可达的应用
（Electron 懒树）；属主判据用物理焦点根（GUITHREADINFO），角色/属主不符
fail-closed NOT_FOUND；与 type 组合为单次注入的逐字通道（burst 打字）。协议见
[06 §3 命名规则](06-design-blueprint.md)，真机证据 docs/80。

---

## 2. 用户与场景

### 2.1 目标用户

| 画像 | 描述 | 核心诉求 |
|---|---|---|
| **agent 构建者**(主要) | 用 Claude Code/Codex/自研循环做自动化的开发者 | 一套可靠、可组合、token 省的桌面"手脚" |
| 自动化工程师 | RPA 背景,被脆弱选择器折磨 | 可自愈的定位 + 验证闭环 |
| 高级个人用户 | 用 agent 处理个人重复事务 | 让 agent 能替自己点完"最后一步" |

### 2.2 核心用户故事

- US1(数据搬运):"从只有 GUI 的老 Win32 报表工具里把当月表格抠出来,写进 Excel 并保存" —— `snapshot` → `extract` → Excel `set-value` → `press ctrl+s`
- US2(配置巡检):"检查 20 台机器的某系统设置是否为预期值,不是就改" —— `find` + `verify` + `click --expect`
- US3(落地执行):"agent 完成研究后,自动打开企业应用完成录单" —— `launch` → 交互链 → `wait` 谓词
- US4(状态检查):"确认刚才的保存确实成功了" —— `verify --window "已保存"` / `get @e9 --property value`

---

## 3. 功能需求

命令面完整定义见 [06 §3](06-design-blueprint.md),此处为需求级验收口径。

| 组 | 需求 | 验收要点 |
|---|---|---|
| FR1 观察 | snapshot / find / get(--raw) / list-windows / extract | snapshot 只含可交互元素 + `@eN` ref + snapshot_id;extract 输出 JSON 数组 |
| FR2 交互 | click / type / press / set-value / select / toggle / scroll / drag / hover | UIA pattern 优先;物理模拟必须显式 `--physical`;click 支持 `--expect` 谓词 |
| FR3 窗口应用 | launch / focus / resize / close | 支持按句柄/标题/PID 定位 |
| FR4 系统 | get-clipboard / set-clipboard / list-notifications / status | status 报告版本、权限、屏幕信息 |
| FR5 可靠性 | wait(谓词语法) / verify 族 / batch | wait 支持元素/窗口/属性/消失;batch 单进程共享 snapshot |
| FR6 协议 | JSON envelope / 错误码 / 退出码 / ref + fallback 定位链 | 100% 命令遵守 envelope;错误必带 code + hint;ref 失效走 fallback 链后才是 STALE_REF |

## 4. 非功能需求

| 维度 | 指标 |
|---|---|
| 性能 | 冷启动 P95 < 80ms 且 P50 < 50ms(2026-09-26 实测修订:初版 50ms 未计入 Windows 进程 spawn 固定开销 ~30ms,实测 P50 40.2 / P95 53.9 / P99 102.9);常规应用 snapshot < 500ms(实测 275ms);默认单命令输出 < 2KB |
| token 效率 | skeleton 模式较全量树削减:深单链形态(Electron 类,内容 depth≥15)≥ 70%;浏览器/小窗口形态按内容结构天然受限(2026-09-27 实测:Edge 36%——工具栏交互按钮是内容主体;记事本 38%)。单链折叠投影已落地 |
| 可靠性 | 金任务集(§6)在固定环境成功率 ≥ 90%;无"静默失败"(一切失败有码) |
| 安全 | 默认仅 UIA 后台调用(不抢物理键鼠);`--physical` 显式授权;跨完整性级别返回 PERM_DENIED 而非静默;`--dry-run` 全局可用 |
| 兼容 | Windows 10 1809+ / 11,x64;单二进制 < 10MB,零运行时依赖 |
| 可维护 | 核心命令集成测试覆盖;envelope schema 版本化并有契约测试 |

## 5. 差异化(对齐 [01](01-research-landscape.md))

1. 相对视觉 agent(Agent-S/UI-TARS):确定性、快两个数量级、零模型成本;
2. 相对 terminator:CLI 是一等公民而非附属品,协议完整(STALE_REF 重试 + fallback 链);
3. 相对 AutoHotkey:原子零售(每步结构化反馈)而非脚本批发;
4. 相对 RPA:错误协议为 agent 自愈设计("求韧不求稳"),编排让渡给 agent。

## 6. 成功指标

**M2 结束时(对应 [ROADMAP](ROADMAP.md) Phase 2)必须达成:**

- 金任务集全部通过,其中"跨 3 应用数据搬运"(US1)成功率 ≥ 90%(重跑 10 次);
- 冷启动 P95 < 80ms、P50 < 50ms(连续 100 次;实测 P50 40.2 / P95 53.9);
- ~~同任务较"截图视觉方案"token 消耗低 ≥ 80%(抽样对比)~~ **——搁置(2026-09-27,不再作 M2 门槛)**:截图方案的 token 成本是业界公知,对比结论对用户无增量信息;actl 的 token 优势由 skeleton 削减率(§4)与输出体积承载。待真实 agent 端到端场景实测时有需要再随场景顺带量化。

**v0.3 北极星**:外部 agent 用户靠 README 示例在 15 分钟内完成首个真实桌面自动化任务。

## 7. 风险与对策

| 风险 | 等级 | 对策 |
|---|---|---|
| UIA 长尾(老 Win32/Electron/管理员窗口) | 高 | M0 spike 提前踩坑([spike 实测](spike-findings.md));`--physical` 兜底 |
| uiautomation-rs 维护断档 | 中 | 评估 vendor/fork;核心只依赖 windows-rs 官方 binding |
| windows crate 版本冲突 | 中 | ADR-003 全依赖锁同版本([TECH_STACK](TECH_STACK.md)) |
| 被滥用于恶意自动化 | 中 | 物理操作显式化、dry-run、文档明示安全边界;不做隐身设计 |
| 单人开发带宽 | 中 | 范围铁律:非目标清单(§1.3)不膨胀

## 8. 发布标准

与 [ROADMAP](ROADMAP.md) 各 Phase 退出标准一致;版本语义:0.x 破坏性演进、0.3 = 协议冻结(M3;envelope version 字段与 CLI semver 解耦),1.0 保留给协议冻结后的正式稳定版。
