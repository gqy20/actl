# 42 · 探索模式设计提案：算法主导,语义外抛

更新：2026-09-28
状态：**设计提案,未实现,未排期**。M3 候选方向,实施前须核对 [PRD §1.3](PRD.md) 非目标并完成决策卡协议的 spec 评审。
关联：[ROADMAP](ROADMAP.md) Phase 3 · [实时状态研究](13-realtime-state-maintenance.md) · [YAML 流程协议](20-yaml-workflow-protocol.md) · [UIA 观察](37-uia-observation.md) · [流程恢复](39-flow-observation-recovery.md) · [PACKS](../skills/actl/PACKS.md)

## 1. 动机与定位

业务场景目前靠人工编写 scenario 脚本与 pack 路线,是 P1 场景深化的人工成本主体。
探索模式让 actl 在**给定任务目标(可观测后置条件 + 关键词先验)**后自主摸图、动作、
验证效果,并把成功的路径沉淀为 flow YAML 与 pack 路线。它是 ROADMAP 三条既有线索的
汇合,不是新支柱:增量观察(差分)、录制轨迹(示教泛化)、pack 维护(路线回写)。

自动化上限的判定依据:算法能解决状态图上的搜索(节点=可观测 UI 状态,边=动作,
目标=谓词),不能解决语义(哪个控件承载意图、同名歧义消解)。因此纯算法层承担
观察/剪枝/执行/验证/机械重试,语义只在算法僵局处显式外抛。

## 2. 核心原则

1. **actl 不调用 LLM**。理由均出自既有约束:ADR-001 禁 async(HTTP 客户端难做)、
   直接依赖 ≤8 装不下、API 密钥归属应在"大脑"侧而非被控设备、离线 gate/CI 无法
   测试会发网络请求的路径。决策卡协议用脚本化固定应答即可 golden 全链路测试。
2. **决策点外抛,不是外呼**。探索循环撞到语义僵局时:run 置为等待裁决状态 →
   envelope 携带决策卡 → run 持久化(进程可退) → 外部应答者裁决 →
   `resume --decision` 续跑。应答者对 actl 是同一种协议:启发式脚本(纯算法模式)、
   人(复用交接语义)、LLM agent(经 MCP 或 shell)。与现有 NeedsReview/Resume
   机制同构:人工交接与语义僵局是同一状态机的同一状态,仅应答者不同。
3. **探索运行是有界 flow run,不是常驻 agent**(PRD 非目标边界)。有步数/时长
   预算,可暂停、可急停、可恢复,结束即退出。

## 3. 分层架构

| 层 | 职责 | LLM 参与 | 复用的现有原语 |
|---|---|---|---|
| L0 知识命中 | 任务先查 pack verified routes,命中直接重放 | 无 | routes 三态、quirks |
| L1 确定性观察 | 任务投影、差分快照、状态指纹去重、规范压缩 | 无 | Projection(Skeleton)、snapshot_id、truncated 防失控 |
| L2 语义决策点 | 任务编译(自然语言→谓词+词表)、候选平局裁决、evidence 归因 | 仅此层,选择题形态 | evidence 载荷、recovery_hint |
| L3 确定性执行 | 动作+`--expect`、verify/wait 收口、recovery_hint 机械重试 | 无 | 定位链、输入护栏、归属差集、close-window |

Token 成本模型:决策次数 × 决策卡体积 + 步数 × 观察体积。四个因子各自击破——
本地化循环砍步数因子,任务投影+差分砍观察体积(全量树 5-20KB/步 → 差分卡
0.5-2KB/步),选择题应答砍输出,pack 回写砍决策次数。

**收益为推算值,须以探索报告实测替换**:中等任务(深度 6-8 步,复杂应用)对比
朴素截图循环 agent 约 20-50 倍降幅(单任务约 4-8k token);对比精打细算 CLI
agent 约 4-8 倍;pack 命中后趋近零(飞轮:首次探索贵,回写后同类任务仅剩任务
编译一次)。前提三件真做:差分不复发全树、决策卡硬上限(Top-K 截断)、上下文
不复读(每卡自包含)。

## 4. 降"错误测试"机制

1. **probe-before-act**:候选进前沿队列前先过 `find`(只读)解析与属性读,死控件
   不被点击。
2. **affordance 预标注**:动作前后差分把控件分类(开窗/toggle/菜单/无效果),
   决策卡只含带标签候选。
3. **单向门词法闸**:删除/发送/支付/卸载类模式(name+控件类型先验)不进自动
   探索,转 NeedsReview 或隔离世界(隔离 profile/测试账号,fixtures 纪律)。
4. **强制效果核验**:mmui 应用(UIA 假成功)一律 `--expect element`。
5. **统计剪枝**:pack quirks 积累每控件历史效果,前沿排序用 UCB/Thompson,排序
   不调 LLM。
6. **可回滚探索序**:只读交互先行,状态变更靠后,每步自动生成回滚步(Escape/
   close-window --pid,归属差集判定)。

## 5. 决策卡协议(spec 草案方向,未评审)

- run 新增等待裁决状态(或扩展 NeedsReview 语义,二选一评审定夺);
- 决策卡信封:任务一句话 + 当前差分(新增/消失/变化元素,身份按 automation_id/
  runtime_id/name+role 多级回退配对)+ Top-K 带标签候选 + 各自预期效果 + evidence;
- `resume --decision <id>` 携带应答者选择与置信度;
- 协议变更走 spec 提交:契约 golden + docs/06 + CHANGELOG 三同步;
- golden 测试用脚本化固定应答驱动,全链路离线可测。

## 6. 分阶段切片(实施前不排期,仅量级估算)

| 切片 | 内容 | 估算(含测试) | 独立收益 |
|---|---|---|---|
| S1 | 差分 snapshot API + `Projection::Keyword` | 约 800-1100 行 | 回归断言、agent 观察体积降一个量级;与 M3 增量观察同向 |
| S2 | 探索 Host + 决策卡 + 探索报告 | 约 1200-1800 行 | 探索主体;spec 提交 |
| S3 | pack 回写 + 路线三态迁移 | 约 400-600 行 | 飞轮闭合;验收目标:5 条 unverified 经人工复核转 verified |

不计入:MCP 适配器(ROADMAP 单列)、外层 LLM 应答器(actl 边界外,第一版可用
启发式脚本约 100-200 行)。

## 7. 验收标准(草案)

- S1:差分结果与全量比对一致(身份配对零误报/漏报,虚拟化与动态 id 场景单测覆盖)。
- S2:脚本化应答驱动的 golden 全链路;单向门动作零执行;急停/暂停在任意步生效。
- S3:探索产出的路线经人工复核后写入 pack,`unverified → verified` 迁移有逐调用
  证据;失败路线保留受阻记录,不改写既有通过结论。
- 全程:探索报告字段(步数/决策次数/错误率/覆盖率)落盘,token 数字以实测替换推算。

## 8. 非目标与边界

- 不内置 LLM 客户端,不引入网络依赖与密钥管理(§2 原则 1)。
- 不是常驻 agent、不是通用脚本引擎(PRD §1.3 非目标)。
- 不自动解除急停、不自动授予桌面权限;探索中的交接语义与现有规则完全一致。
- 探索产出不替代真机验收:路线转 verified 必须经人工复核,自动化只做证据收集。
- 冷启动(全新应用、无先验)退化为高决策密度模式,成本接近精打细算 CLI agent,
  这是建 pack 的一次性成本,不视为缺陷。
- 开放意图(无后置条件的模糊任务)、验证码/OTP 不在目标内。

## 9. Spec 评审（2026-10-02，实施前评审记录）

用户裁定探索模式优先（2026-10-02）。本节为实施前的正式评审结论；S2 动工前不再重复本节内容，仅增量变更走 spec 提交。

**结论：通过（有条件），S1 可立即动工。**

### 9.1 PRD 非目标逐条核对

| PRD 边界 | 核对结果 |
|---|---|
| 不内置 LLM 客户端 | ✓ 决策卡外抛协议，actl 零网络依赖（§2 原则 1 保持） |
| 非常驻 agent | ✓ 探索运行=有界 flow run（步数/时长预算，可暂停/急停/恢复） |
| 可视化编排/动作市场 | ✓ 不涉及 |
| MCP 不进核心 | ✓ 外部 LLM 应答者经 MCP/shell 属适配层，本设计不含 |
| 探索产出不替代真机验收 | ✓ 路线转 verified 必须人工复核（§8 保持） |

### 9.2 协议 delta 定稿（裁定 §5 的待决项）

1. **等待裁决状态：扩展 NeedsReview 语义，不新增状态顶点**——`run.reason="decision_pending"`，与 docs/54 可恢复语义同构；人工交接与语义僵局是同一状态机的同一状态（应答者不同），状态机不加顶点。
2. **决策卡载体：flow-status 的 data + envelope evidence 通道**；自包含、硬上限 4 KiB/卡、Top-K≤8 候选；卡内不含窗口标题/选择器原文/文本内容之外的多余载荷（动作日志内容省略纪律）。
3. **应答协议：`flow-resume --decision <choice_id>`**；choice 必须属于卡内候选 id 集，否则 PROTOCOL；置信度为可选字段仅入报告。
4. **浏览器轨兼容**：L1 观察面纳入 browser-read/--property（只读）；L3 动作面走 browser-click/type 的守卫语义（2026-10-02 已落地的通道直接可用，无需新原语）。
5. golden 全链路用脚本化固定应答驱动（离线可测）；协议变更走 spec 三同步不变。

### 9.3 实施顺序裁定

- **S1 先行**（差分 snapshot API + Keyword 投影）：独立收益（回归断言+观察体积），与 M3 增量观察同向，零协议面变更，可先行合入；
- **S2**（探索 Host + 决策卡 + 报告）为 spec 提交主体，含 9.2 全部 delta 的 golden；
- **S3**（pack 回写）收尾，验收对齐 §7。

### 9.4 风险登记（动工即跟踪）

- token 收益数字全部为推算，S2 报告以实测替换（§7 已要求）；
- 虚拟化/动态 id 的身份配对是 S1 验收焦点（零误报/漏报硬指标）；
- 冷启动高决策密度为已知成本形态，不作为缺陷修复目标。

## 10. S2 实施记录（2026-10-03）

S2 落地范围：`actl-core/decision.rs`（决策协议）+ `actl-flow/explore.rs`（探索引擎）+ CLI 接线（flow-run/flow-resume/flow-status）。§9.2 五条 delta 全部兑现；与 §5 草案的偏差与补充裁定如下。

### 10.1 落地形态与偏差

1. **探索 spec 是 Plan 的一段，不是独立命令/文件格式**：`Plan.explore`（task/app/keywords/goal/budgets），`steps` 必须为空（可省略键）。`flow-run` 直接执行探索计划，`flow-validate` 校验后报告 explore 概览——零新命令，协议面最小。
2. **决策卡协议按 §9.2.2 兑现**：4 KiB/卡、Top-K≤8 由构建器硬保证（压缩阶梯：丢样本名→截名字→Top-4→Top-2，仍超即 PROTOCOL）；卡经 flow-status `data.decision_card` 暴露，`data.explore` 携带计数与 token 审计。压缩、评分、排序全部确定性（整数分、同分按 selector 字典序）。
3. **应答协议按 §9.2.3 兑现为单选**，排序回退（§3 token 优化的"一次往返摊平多次尝试"）落在**引擎侧**：应答后队列 = 首选 + 卡内其余非危险候选按序，首选无效果自动推进不产生新卡。危险候选永不自动回退；应答者显式选择危险候选即授权执行。
4. **动作原语 v1 只有点击**：候选选择器为 `id:`/`name:`（语义选择器，跨进程可恢复，不做坐标兜底）；输入类动作（type/press）留给后续——词表投影的前沿里 Edit 零先验分，不误导。
5. **效果归因限于同窗差分**：点击后按关键词投影帧差分归因 reveals/no_effect；跨窗效果（点击开新对话框）v1 不检测，属已知限制（见 §10.3）。
6. **状态指纹刻画身份形状**：可识别元素（automation_id 优先、name 回退）+ 角色多重集的 FNV-1a，排序后哈希——虚拟化重排不敏感；同 id 仅标签变化指纹不变（标签变化由差分层上报）。
7. **终态语义**：预算耗尽/无候选/观察失败均为 needs_review（原因串区分），goal 满足 completed；错误码如实传播不吞。探索运行在显示端 needs_review 显示 resume（静态流程 recheck 不变）。

### 10.2 token 优化设计的兑现对照

| §3 设计 | 落地点 |
|---|---|
| 本地化循环（LLM 不逐步参与） | 引擎闭环；外抛只在平局/全危险/悬挂卡 |
| 任务投影+差分 | `--keywords --skeleton` 观察 + S1 差分；零命中放宽一次记 widened |
| 决策卡硬上限 | 4 KiB/Top-K≤8 构建器保证，golden 断言 |
| 上下文不复读 | 卡静态头一句话级；全量证据留 explore.json，卡只带判别字段 |
| 选择题应答 | `--decision cN` 单 id；--confidence 仅入报告 |
| 排序回退 | 引擎侧队列（§10.1.3） |
| probe-before-act | find 只读探测；死候选不消耗动作预算 |
| 本地谓词核验 | goal 断言在引擎内闭合 |
| token 审计 | explore.json/flow-status 携带 observation_bytes/card_bytes/计数 |

§7 要求的"token 数字以实测替换"：真机验收已补录（[77](77-exploration-s2-acceptance.md)：关键词帧 301–1700B/次，决策往返 0–1 次/任务，对比全量快照循环 5–17× 缩减；对照朴素截图循环 agent 的完整对照轮仍为可选后续）。

### 10.3 已知限制（S3/后续）

- 效果归因不做跨窗检测；点击打开新窗口时同窗差分为空可能误标 no_effect（预算与 tried 集仍保证有界与不循环；真机实证 S2/S3c）。
- 探索动作只有 click；无滚动/键盘动作，长列表场景靠词表命中与 skeleton 放宽兜底。
- goal 断言的目标元素在窗口形态变化后可能不可解析（真机实证 2026-10-03：计算器置顶/导航形态下 `CalculatorResults` 消失）→ `goal_check_failed` 失败闭合是规格行为，有界重试/形态感知是 S3 候选（[77](77-exploration-s2-acceptance.md) 发现 1）。
- 卡候选排序无效果风险先验：含窗口重构类候选（置顶/导航/退格）的队列可能先触 `goal_check_failed` 而非预算终态（两者都诚实；先验排序属 S3 quirks 剪枝范畴）。
- pack 回写（L0 命中重放、quirks 统计剪枝）属 S3。

### 10.4 验收状态

- 单测：actl-core 决策协议 9 项；actl-flow 引擎 MockHost 全链路 13 项（自动门完成/平局外抛+应答回退/危险闸双向/预算耗尽/急停暂停/死候选剪枝/观察失败诚实/无候选/应答协议错误/持久化往返）。
- 契约：explore_contract 6 项离线（spec 校验 golden、混用/预算/bind 拒绝、静态流程 --decision 拒绝、悬挂卡三面协议、commands 面）。
- **真机验收通过（2026-10-03，[77](77-exploration-s2-acceptance.md)）**：计算器/记事本 4 场景全过——熵门自动路径零外抛、平局卡+单选应答+排序回退一次往返完成两次尝试、`budget_exhausted:actions` 与 `goal_check_failed` 两类诚实终态实证、卡 556–1508B 远低于 4 KiB 硬上限；token 审计数字见 §10.2 注。

## 11. S3 实施记录（2026-10-03）

S3 落地范围：路线回写（`actl-flow/route.rs`）+ `explore.priors` 效果风险先验 + goal 断言有界重试。与 §6 估算的偏差与裁定：

### 11.1 形态与裁定

1. **回写是"片段生成"，不是"pack 写入"**：`explore-writeback --id <run> [--out <file>]` 从 goal_met 的完成运行抽取 pack 格式路线片段（envelope data + 可选落盘），由调用方合并进 `apps/<slug>.yml`。依据分层铁律（PACKS.md：核心 CLI 永不读 pack）——actl 不感知 pack 文件的存在，知识流经 agent 手。
2. **verified 迁移不是命令**：片段 status 恒 `unverified`；迁移 = pack 文件编辑 + git PR（review 即人工复核,PACKS 红线生效）。自动化只做证据收集（AGENTS「修正与验收分开」）。重放由 agent 执行片段步骤（batch/flow）并留 envelope 证据随 PR 提交。
3. **L0 命中重放留在 skill 层**：与 PACKS「当前引擎不自动展开 pack 路线」一致。飞轮闭环 = 探索 → writeback 片段 → agent 合并 pack → 人工复核转 verified → 下次任务 agent 读 pack 路线直接执行（零探索）。
4. **quirks 与 priors 是数据不是行为**：writeback 输出死候选 quirk 片段与 suggested_priors（dead=-4/no_effect=-2）；`explore.priors` 由调用方注入,引擎评分时叠加（下限 0）。§4.5 统计剪枝的 v1 = 这条显式数据链,UCB/Thompson 等自动统计留待 pack quirks 积累出样本量后评估。
5. **goal 有界重试**：瞬态 NOT_FOUND/TIMEOUT 重试 2 次（300ms 沉降,goal_retries 如实入状态,重试不计 errors）;耗尽或非瞬态维持失败闭合。docs/77 发现 1 的机制化收口。
6. **不完整路线不导出**：action_log 达上限（可能截断）即 PROTOCOL 拒绝;goal 未达成即 PROTOCOL 拒绝（失败现场保留在 run 目录,quirk 提取走人工读 explore.json）。

### 11.2 验收状态

- 单测：actl-core 先验降权 2 项;actl-flow route 抽取/渲染（saphyr 往返保真）5 项、goal 重试双路径与先验改道 3 项;契约：回写 golden（片段+YAML 全量比对）、未完成拒绝、priors 越界拒绝、commands 面（含 writeback）。
- 真机验收：见 [77](77-exploration-s2-acceptance.md) S3 增补（回写 S1/S2 运行 → 合并 calculator.yml → batch 重放 → 人工复核迁移 verified → priors 飞轮实证）。
