# 76 · GUI 操作录制设计提案：用户手动操作 → 自动生成 YAML 流程

日期：2026-10-03。状态：**设计提案，未实现**。M3 候选，0.2.0 后优先立项（用户标记高价值）。2026-10-03 增补：Quicker 调研结论与 agent 在环体系修正（§11，调研见 [79](79-quicker-recording-research.md)）。
关联：[ROADMAP](ROADMAP.md) · [YAML 流程协议](20-yaml-workflow-protocol.md) · [探索模式](42-exploration-mode.md) · [UIA 观察](37-uia-observation.md) · [输入后端调研](09-input-backends-review.md)

## 1. 动机与定位

当前 YAML 流程的编写成本是人工的——agent 或用户需要逐条翻译"要做什么"为 YAML 步骤。录制功能让用户**直接用鼠标键盘演示一遍任务**，系统自动捕获并转成可复用流程文件，实现"做一遍就能自动化"。

这是 RPA 工具（UiPath、Power Automate Desktop）的核心功能，也是 actl 降低使用门槛的关键——不需要懂 YAML，不需要翻译，直接操作。

## 2. 与探索模式的关系

| | GUI 录制 | 探索模式 |
|---|---|---|
| 驱动者 | **用户手动操作** | agent 自主探索 |
| 输入 | 物理键鼠事件 | UIA 观察 + 算法决策 |
| 输出 | YAML 流程 | YAML 流程 + pack 路线 |
| 语义理解 | 无（人做的每步都有意图） | 需外抛决策卡 |
| 技术 | 键鼠钩子 + UIA 实时反查 | 差分快照 + 前沿搜索 |

两者互补：录制是"人教 agent"，探索是"agent 自学"。生成的 YAML 都走同一套 flow-run/verify/pause 基建。

## 3. 工作原理

```
actl record start
    ↓ 安装低级键鼠钩子 (WH_MOUSE_LL / WH_KEYBOARD_LL)
    ↓ 用户正常操作应用

用户点击鼠标 (x=500, y=300)
    ↓ 钩子捕获坐标 + 时间戳
    ↓ 同步 UIA ElementFromPoint(500,300) 反查该位置控件
    ↓ 发现是 Button name="保存"
    ↓ 记录: click name:"保存" --app "记事本"

用户输入 "hello"
    ↓ 键盘钩子捕获按键序列
    ↓ 同步查当前焦点元素 (GetGUIThreadInfo / UIA focused element)
    ↓ 发现焦点在 Edit name="文件名"
    ↓ 记录: type id:filename "hello" --app "另存为"

用户按 Ctrl+S
    ↓ 键盘钩子捕获组合键
    ↓ 记录: press ctrl+s --app "记事本"

actl record stop --out my-flow.yml
    ↓ 全部动作序列 → 语义分类 → YAML 生成
    ↓ 输出可 flow-validate 的流程文件
```

## 4. 核心技术组件

| 组件 | 职责 | 复用/新建 | 量级 |
|---|---|---|---|
| 低级键鼠钩子 | 捕获物理输入事件 | **新建**（WH_MOUSE_LL / WH_KEYBOARD_LL，现有 actl-physical 只有注入没有捕获） | ~150 行 |
| 点击时 UIA 反查 | 坐标 → 元素身份 | **复用**（actl-uia 已有 ElementFromPoint 能力，pointer_at_physical 的守卫代码同源） | ~100 行 |
| 焦点元素追踪 | 按键时查"输入到哪了" | **复用**（actl-uia 已有 focus 追踪，front-report 的三视图之一） | ~50 行 |
| 语义动作分类器 | 原始事件 → actl 命令 | **新建**（点击→click/双击/右键/拖拽；按键→type/press；窗口切换→focus-window） | ~200 行 |
| 延迟分析 | 识别哪些等待有意义 | **新建**（区分"等应用加载"与"人在犹豫"；规则：>2s 且后续有新窗口/UIA 树变化→ wait 步骤） | ~100 行 |
| 录制会话管理 | start/stop/暂停/标记 | **新建**（信号协议 + 状态文件） | ~100 行 |
| YAML 生成 | 动作序列 → flow.yml | **复用**（流程协议的 schema/builder） | ~200 行 |

总计：~800-1200 行。

## 5. 设计原则

1. **零侵入**：录制时用户正常操作，不弹窗不干扰（状态提示走信号条）。
2. **语义优先**：捕获的是"点了保存按钮"而不是"鼠标在(500,300)左键单击"——反查在事件发生的瞬间执行，不是事后分析。
3. **生成可验证的 YAML**：自动为写操作添加 verify 步骤（根据动作后 UIA 树变化推断）；无法确定的目标标记 TODO 供人工补。
4. **与既有流程协议完全兼容**：生成的 YAML 走 flow-validate/flow-run，不发明新格式。
5. **可暂停**：`actl record pause` / `actl record resume`，暂停期间输入不被录制。
6. **敏感操作标记**：删除/发送/支付类动作（词法闸）在生成的 YAML 中自动标注 `pause_before: true`。

## 6. 目标泛化（关键技术难点）

录制时的目标是"当时的那个元素"（有 automation_id/name/role），但 YAML 需要的是"下次运行时也能找到的目标"。泛化策略：

| 录制时捕获 | YAML 生成 | 说明 |
|---|---|---|
| automation_id="btnSave" | `id:btnSave` | 最优——AutomationId 通常稳定 |
| name="保存", role=Button | `role:Button --name "保存"` | 次优——名称可能因语言/状态变化 |
| name="" + role=MenuItem | `id:menuItemId` 或位置 | 需 UIA 树上下文判断 |
| 坐标点击（UIA 查不到） | `xy:x,y --app X --physical` | 兜底——自绘内容，标记为视觉依赖 |

## 7. 分阶段切片

| 切片 | 内容 | 量级 | 独立收益 |
|---|---|---|---|
| R1 | 键鼠钩子 + 点击 UIA 反查 + 基础 YAML 生成 | ~500 行 | 手动操作→可编辑的 YAML 骨架 |
| R2 | 语义分类 + 焦点追踪 + 延迟分析 | ~300 行 | 生成的 YAML 更接近可运行 |
| R3 | verify 自动生成 + 敏感操作标记 | ~200 行 | 生成的 YAML 可直接 flow-run |
| R4 | 录制 UI（显示条上的录制按钮/状态） | ~200 行 | 非技术用户友好 |

## 8. 验收标准（草案）

- R1：在记事本中录制"打开→输入→保存"三步，生成的 YAML 含 3 个 run 步骤，目标可解析。
- R2：在计算器中录制"按 5 + 3 ="，生成的 YAML 含 click × 4 步骤，延迟分析正确排除犹豫。
- R3：生成的 YAML 通过 flow-validate；敏感操作（如记事本"另存为"覆盖文件）自动标注 pause_before。
- 全程：录制不影响用户正常操作体验（无弹窗、无焦点抢夺）。

## 9. 非目标与边界

- 不做屏幕录像（视频）——只捕获结构化输入事件。
- 不做跨机器回放适配——生成的 YAML 针对本机目标；跨机器需人工调整 app 锚/目标。
- 不做后台录制（用户不知情）——录制必须显式启动/停止，信号条有明确提示。
- 不替代探索模式——录制是"人教"，探索是"agent 学"，两套独立。
- 不自动安装钩子到系统——录制会话期间临时安装，结束即卸载。

## 10. 已有基建的复用清单

| 基建 | 复用点 |
|---|---|
| actl-uia `ElementFromPoint` | 点击时实时反查控件身份 |
| actl-uia focus 追踪（GetGUIThreadInfo） | 按键时确定输入目标 |
| actl-physical SendInput | 反向知识——知道注入怎么工作，就知道捕获要拦截什么 |
| 流程协议 schema/builder | YAML 生成的目标格式 |
| 信号条 + 动作卡 | 录制状态的实时显示 |
| display_gate / stop 生命周期 | 录制暂停/停止的信号协议 |
| 词法闸（单向门） | 敏感操作的识别与标记 |

## 11. 增补：Quicker 调研结论与 agent 在环体系（2026-10-03）

[79](79-quicker-recording-research.md) 完成外部调研（Quicker 一方录制形态、作者立场、社区痛点）后，本提案的核心决策——事件瞬间语义反查、输出可编辑 YAML、flow-validate 兼容——被 Quicker 的失败面反向验证（其 UIA 模块有现成的坐标反查原语却从未接入录制器；官方自己承认坐标重放"只适合特定环境，不要分发"）。体系按下述修正，论据与来源见 79：

- **产物改为双件套**：初版 YAML（机械编译）+ **录制 trace**（一等的本地证据文件，agent 改写轮的原料）；编译器不提前丢信息，§5 原则 3 的"TODO 供人工补"升级为**带 trace 证据引用，供 agent 或人工补**；
- **文本输入录提交后最终文本生成 `type`，仅组合键生成 `press`**——原始按键序列在中文 IME 下重放必错（Quicker 实证）；中文文本依赖 `--paste`/Value 通道（是否随 R1 同步做，spec 裁定）；
- **钩子架构**：WH_*_LL 回调只入队（时间戳+事件），UIA 反查在工作线程立即执行（钩子超时被 Windows 回收是 Quicker 实证；本仓 UIA 反查 P50 200ms+ 不可进回调）；原始坐标留存作降级；钩子互斥与自动重挂进 spec；
- **坐标兜底存窗口相对坐标**并随步记录窗口矩形/DPI/标题/进程元数据（Quicker DPI 四因素困境的显式绕法）；
- **agent 改写轮入 R3**（skill 层，LLM 不进 CLI）：读 trace 做选择器稳健化/参数化/等待验证注入/噪音清除/TODO 修复，**验收判据 = 改写后重放通过率 ≥ 机械编译版**；
- **§7 切片重排**：R1 = 钩子+trace+机械编译；R2 = 编译增强且**输出对齐 explore-writeback 片段格式**（录制与探索在回写层汇合，`{window}` 泛化/证据头/unverified→人工复核迁移全套复用）；R3 = agent 改写轮；R4 = 录制 UI；
- **触发面**（[79 §8](79-quicker-recording-research.md)）：CLI `record-start/stop/pause` 为正典入口；显示端右键菜单加动态项（"开始/停止录制"，套 flow 暂停/继续的动态项+请求模式先例）为人类便利入口；信号条带"录制中"提示满足 §9 反后台录制要求；会话状态文件落盘（菜单发现依赖）；录制进程独立于显示端存活；trace 过滤 actl 自家进程的事件；
- 新增 spec 决策点：trace 隐私脱敏（密码框文本不入 trace）、trace 消费方式（摘要+定点读取）、YAML 步骤证据字段（schema 变更，硬规则 8）。

两点边界再确认（与 §9 一致）：改写智能在 skill 层不进 CLI，是"agent 替代编排层"定位的自然延伸而非例外；语义选择器是录制 YAML 可复用、可分发的前提——Quicker 官方劝用户"不要分发含坐标重放的动作"正是同一判断的反面表述。
