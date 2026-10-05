# 77 · 探索模式 S2 真机验收

日期：2026-10-03。状态：**通过**（4/4 场景达到规格预期终态）。协议与实现：[42 §9/§10](42-exploration-mode.md) · [06 探索节](06-design-blueprint.md)。

## 环境

- 被测二进制：`target/release/actl.exe`（6,950,912B，sha256 `f9fb33cc88277b588bf14ff43c7d8bcf635e95b2c26a80c2b5730782b252f607`）；显示端 `actl-signal.exe`（sha256 `8db7d77e…f8c8f34c2`，协议 11，0.2.0 成对）
- OS：Windows 11（10.0.26200）；显示端 `display-status` ready/responsive，停止标志干净
- 授权通道：用户托盘「预授权写入」（验收后用户裁定暂不关闭，已提示风险）
- 应用：UWP 计算器（`--app 计算器`，GT1 同款 ApplicationFrameWindow 配对）+ Win32 记事本（`无标题 - Notepad`）；启动归属按 12 窗口基线差集核验
- 逐调用证据 JSON 留存本地 `target/acceptance-s2/`（01–10 编号），关键数字下文内联

## 场景与结果

| 场景 | spec（fixtures/flows/） | 结果 | 关键断言 |
|---|---|---|---|
| S0 零写入冒烟 | explore-notepad-smoke | ✅ completed | actions=0/decisions=0，goal_met=true，观察 1 次 301B（词表命中，未放宽） |
| S1 熵门自动路径 | explore-calc-auto | ✅ completed | actions=1（唯一点击 `id:num7Button`）/decisions=0——头部领先 3 分自动执行，零外抛 |
| S2 平局外抛+应答+排序回退 | explore-calc-tie | ✅ completed | 平局卡（556B≤4KiB，c1=七/c2=八 同分 8）→ 应答 c2 → 先点八后自动回退点七（显示 87 含 7）→ **一次往返完成两次尝试**；decisions=1、actions=2 |
| S3 有界诚实失败 | explore-calc-budget(-digits) | ✅ needs_review | 两类诚实终态均实证：`goal_check_failed`（S3/S3b，失败闭合）与 `budget_exhausted:actions`（S3c：actions=4/decisions=1/errors=0/goal_met=false，队列余项如实持久化） |

复现命令（S2 为例）：

```
actl flow-run   --file fixtures/flows/calculator-reset.yml --id calc-reset-2
actl flow-run   --file fixtures/flows/explore-calc-tie.yml  --id explore-s2-calc-tie   # → needs_review/decision_pending + decision_card
actl flow-resume --id explore-s2-calc-tie --decision c2 --confidence 0.9              # → completed
```

S2 外部核验：应答后快照显示区 `CalculatorResults` name=`显示为 87`（八→七的回退序列独立确认）。

## token 实测（docs/42 §9.4：推算以实测替换）

同机同日口径（timing 纪律，不跨环境比较）；对照基线 = 同窗口全量快照（7047B/次，`03-calc-full.json`）：

| 场景 | 观察次数 | 观察字节 | 卡字节 | 决策往返 | 动作 | 朴素全量对照 | 缩减 |
|---|---|---|---|---|---|---|---|
| S0 记事本 | 1 | 301 | 0 | 0 | 0 | 2599B（骨架基线） | ~8.6× |
| S1 计算 | 2 | 828 | 0 | **0** | 1 | ~14.1KB | ~17× |
| S2 计算 | 4 | 2060 | 556 | **1** | 2 | ~28.2KB | ~10.8× |
| S3c 计算 | 5 | 5445 | 1342 | 1 | 4 | ~35.2KB | ~5.2× |

- 关键词帧实测 301–1700B/次（S3c 5445/5≈1089B/次），与 42 §3「差分卡 0.5–2KB/步」推算量级吻合；
- **决策往返 0–1 次/任务**（对比朴素 agent 每步一问）；S2 单次往返摊平两次动作尝试（排序回退）；
- 时长：S0 1.25s · S1 2.1s · S2 0.5s+3.5s · S3c 12.5s（含多轮观察/点击，预授权下无人工介入）；
- 与「截图循环 agent 20–50×」的完整对照需同类任务对照轮（可选后续），本轮以全量快照循环为诚实基线。

## 真机发现（新增实证，非缺陷）

1. **窗口形态变化使显示元素不可解析**：队列点到「始终置顶」（S3/S3b）后，`CalculatorResults` 从全量树中消失（`08-s3b-display-check.json` 独立确认），goal 断言 NOT_FOUND → 引擎按规格 `goal_check_failed` 失败闭合，错误码原样传播（退出码 1）、状态如实持久化。后续方向：goal_check 有界重试/形态感知属 S3 候选，已记入 42 §10.3。
2. **probe 判死的真实来源**：`ClearMemoryButton`（记忆面板）在窗口瞬态下 find 失败被标 dead、不消耗动作预算（S3/S3b 各 1 次 errors 如实计数）——probe-before-act 的有界行为符合设计。
3. **同窗关键词帧归因局限实证**：数字键点击只改显示区（不在关键词帧内）→ 帧不变 → `no_effect` 标注（S2 的八、S3c 的 num0–2）。与 42 §10.3 预记录一致；效果标注对回退推进无影响（队列按位置推进）。
4. **队列候选的效果风险先验缺失**：卡内含窗口重构类候选（置顶/导航/退格）时，`goal_check_failed` 会先于 `budget_exhausted` 到达——两者都是诚实终态，但 S3c 用纯数字前沿证明预算终态本身可达；候选排序引入效果风险先验（S3 quirks 统计剪枝）是自然改进方向。

## 清理状态

- 计算器（hwnd 6753906）与记事本（hwnd 6294620）均 `close-window` 优雅关闭（ok:true，无 taskkill）；关窗后窗口清单计算器/Notepad 计数归零，与基线差集闭合；记事本空文档无保存弹窗。
- 流程证据保留：`flows/` 下 6 个 explore 运行（S0–S3c）+ 5 个 reset 运行；失败/受阻运行（s3/s3b/s3c needs_review）按纪律保留原状态不改写。
- 预授权开关：用户裁定验收后暂不关闭（已两次提示全局风险）。

## 结论与遗留

S2 真机验收通过：熵门自动路径、平局决策卡协议（4 KiB/Top-K 硬上限）、单选应答、引擎侧排序回退、本地谓词核验、预算与失败闭合的诚实终态全部在真实 UIA 上符合规格。遗留：S3 pack 回写（路线沉淀）、quirks 统计剪枝、goal_check 有界重试（[42 §10.3](42-exploration-mode.md)、ROADMAP）。

## S3 增补:回写→复核→重放→先验飞轮闭环（2026-10-03）

被测二进制更新:`target/release/actl.exe`(sha256 `a5e03d8db114f57af1f27419fb1d82323239dcd8bac026cb32bc1c73e6f2fbef`);显示端 0.2.0 就绪、预授权开(用户授权通道)、停止标志干净。证据文件 `target/acceptance-s2/12-16`。

### 闭环四步(全部真机)

1. **回写**:`explore-writeback` 抽取 S2 验收的两个完成运行——S1(按七,1 动作 0 决策)与 S2(八→七,2 动作 1 决策)。片段含 `{window}` 替换、证据头(token 审计数字)、S2 的 suggested_priors(`id:num8Button: -2`);status 恒 unverified。失败运行(S3/S3b)被 PROTOCOL 拒绝(goal 未达成),现场保留。
2. **重放**(agent 侧,L0 语义):重放流程沉淀为 `fixtures/flows/replay-explore-s1/s2.yml`(路线步骤+agent 侧中间核对),`flow-run` 真机执行——`replay-s1-01`(复位→点七→显示 7)与 `replay-s2-01`(复位→点八显示 8→点七显示 87 含 7)全部步骤 verified。
3. **人工复核迁移**:**用户确认**两条 unverified → verified(PACKS 纪律:复核=人的裁定);pack 更新带重放证据注记与 verified_date=2026-10-03,`cargo xtask lint-packs` 通过(11 packs/306 steps)。
4. **先验飞轮**(发现 1/2 的收口):从 S3/S3b 失败运行的 tried 映射(经 flow-status 读出)导出先验——`explore-calc-priors.yml` 声明 `{NormalAlwaysOnTopButton:-4, TogglePaneButton:-4, ClearMemoryButton:-4, backSpaceButton:-2}`。同一不可达 goal 对照:无先验(S3/S3b)队列走进「始终置顶」触发窗口重构→`goal_check_failed`;有先验(S3d)危险候选跌出 Top-8,队列走显示持存候选(clear/clearEntry/decimal,全 "acted/reveals"),终态 **`budget_exhausted:actions`,actions=4、errors=0、goal_retries=0**——先验把"撞墙失败"变成"诚实预算终止"。priors 在 flow-status explore 块可见。

### 结论与遗留

S3 真机闭环验收通过:回写(证据收集)、人工复核迁移(用户裁定)、重放(L0 skill 层)、先验飞轮(效果风险闭环)全部实证。至此探索模式 **S1–S3 全链落地**。

诚实记账:docs/42 §7 的 S3 验收目标「5 条 unverified 经人工复核转 verified」本轮完成 **2 条**(现有完成运行的全部——S1/S2;S3 系列是失败运行,产出先验数据而非路线)。余 3 条需在其他应用/任务上继续探索积累(每条同样走 回写→重放→复核 三步);不虚记。遗留:UCB/自动统计剪枝(待 quirks 样本量)、L0 命中重放的 skill 层流程化(SKILL.md 消费 pack 路线的指引)——**后者已收口(2026-10-05:SKILL.md「应用路线」节落地双分岔指引,verified 命中直接重放/未命中走 explore → writeback 闭环)**;UCB 维持挂起。

清理:计算器优雅关闭(窗口清单归零);预授权开关仍开(用户裁定,已再次提示);全部流程运行留 flows/ 作证据。

## L2 增补:输入原语与跨窗探测真机验收（2026-10-03）

被测二进制:`target/release/actl.exe`(sha256 `9254167a5847b1c8a99dff5dff769fbaa90d135c1d19db3bd9a8e768593f200e`)。证据文件 `target/acceptance-s2/17-20`。

### 通过项

- **P1 goal 折叠(隐式随全程生效)**:本轮所有运行的 goal 核验走帧内折叠——输入场景 3 次观察仅 1583B,值断言零额外 verify 采集(观察即核验)。
- **texts 输入原语(真机通过,explore-l2-input)**:spec `texts: {'name:文本编辑器': 'actl-explore-accept'}` + value 通道 goal——引擎把 texts 键值并入关键词激活 Document 评分,与菜单栏「编辑」(ContentButton/MenuItem)形成**真实三方平局卡(同分 8)**,应答 c3 后 type 动作单步执行、ValuePattern 读回核验通过(completed,1 动作 1 决策,`tried: name:文本编辑器=acted`)。这同时是真机上第一个"语义歧义由决策卡裁决"的非构造案例(点编辑菜单 vs 输入目标)。
- **菜单 flyout 在锚窗快照内**(发现):XAML 菜单 flyout 出现在主窗 UIA 树(第一段 saveas 运行点击「文件」后,goal 折叠直接看到菜单项「保存」并达成)——探索可以观察并点击菜单项,不必跨窗。

### 受阻项(如实记录,机制本身离线全绿)

- **跨窗 goal 探测(链路被真机发现 #3 阻断)**:链路 文件→另存为→对话框 内,引擎点击「文件」后**紧随的观察 INTERNAL 失败**——菜单 flyout 开合过渡期 UIA 提供程序短暂锁住(与计算器激活风暴同族,docs/65)。手工诊断确认:菜单打开状态下隔 ~1s 的快照正常、关闭后正常;失败窗口是点击后的瞬时。引擎按规格失败闭合(observation_failed),两次 resume 均同因。探针机制本身有 MockHost 全链路单测(命中/预算封顶),未在真机触发的原因是**观察瞬态先于探测发生**。收口方向:观察的有界重试(与 goal 重试同构)——已记 ROADMAP。
- 由此确认 v1 边界的另一个真实面:即便探测触发,对话框内继续动作仍需观察锚点转移(Store 记事本的文件选择器还可能由 RuntimeBroker 跨进程承载,同 pid 过滤会如实错失)——两者都是 L2 后续项。

### 附带实证

- find 歧义链路:probe `id:ContentButton` 因同名多菜单按钮 find AMBIGUOUS 判死,队列自动回退 `id:File` 执行——死候选剪枝与回退在真机工作正常。
- 窗口标题随输入变化(`*actl-explore-accept - Notepad`),验证了 app 锚用稳定后缀(Notepad)的设计。
- 中文文本输入:引擎 v1 的 type 走键盘通道,中文 IME 下不可靠(已知,docs/09);本轮用 ASCII 文本,中文输入留待 --paste 通道选项(spec 决策)。**已收口(2026-10-05)**:texts 原语按内容自动选通道(非 ASCII → `--paste`,与录制编译器同口径),writeback 导出同步注入——引擎执行侧与导出路线重放均不再走中文键盘退路,详见 [06 §L2](06-design-blueprint.md);真机中文 texts 探索轮并入后续路线积累批次验证。

### 清理

记事本优雅关闭(close-window ok,无保存弹窗——Win11 会话自动保存,下次打开会还原未保存标签 `actl-explore-accept`,已在案);窗口清单 Notepad 计数归零。预授权开关仍开(用户先前裁定)。

## P3 增补:批量并行探测真机验证（2026-10-03）

被测二进制:`target/release/actl.exe`(sha256 `94024f539ff636177c4c36420b00ff68ed08ed81fbd99457cee99ec25c39ae92`)。证据 `target/acceptance-s2/21-*`。

- **卡面预检(并行 8 探测,零错误零误杀)**:explore-p3-card 外抛卡前对 Top-8 批量探测——全部判活(errors=0),卡含全部候选(正确行为)。此前的 ClearMemoryButton 死候选是**间歇性虚拟化**(S3/S3b 判死、本轮可解析)——死排除语义由存活配额单测覆盖(`answer_queue_prefilters_dead_members`),真机未复现死样本不算通过也不算失败:机制在、样本间歇。
- **应答队列预检走通**:应答 c1 后队列批量探测 → 点击 ClearMemory → 运行继续;随后 `goal_check_failed`(点击后窗口重构使 CalculatorResults 缺席,P1 重试 2 次后失败闭合)——S3b 同族形态,非 P3 回归。
- **计时(单样本,诚实)**:3113ms(含 8 并行探测)vs 前轮无批量探测 2046ms——UWP 计算器上并行未显示净收益,疑似提供程序侧串行化(分析预警的形态)。设计下限即退化为串行且无害;速度收益依提供程序而定,**正确性收益(卡卫生/队列预滤)与提供程序无关**。
- 清理:计算器优雅关闭(窗口清单归零)。

## P3 修正:并行读证伪（2026-10-03）

契约测试 pending_card_decision_protocol 的非对称队列结果触发深挖:手工复现(隔离 LOCALAPPDATA + 逐结果打点)确认——**同进程内两个并行 find 必有一个返回 `E_FAIL(0x80004005) 未指定的错误`**,顺序不定(桌面根/UIA 提供程序的进程内并发上限)。叠加 P3 增补节的计时实证(UWP 计算器并行无净收益),结论:并行 UIA 读在本栈上既有害(并发即抖动)又无益(提供程序本就串行)——**CLI 侧回退串行**,`execute_probes` 批量通道与引擎侧卡/队列预检语义保留(正确性收益与提供程序无关;通道为浏览器轨 CDP 读这类天然可并行的将来形态预留)。发现 #4 入档:同进程并发 UIA 读产生 E_FAIL,任何未来并行化需先过这条实证。

## 跨窗链攻坚：三个发现与三次语义修正（2026-10-03）

被测二进制演进：`9254167a`(P3)→`688eff59`(观察重试)→`5801c33f`(goal 故障语义)→`e117f5d9`(帧背书)。证据 `target/acceptance-s2/21-24`。链路目标：notepad 文件→另存为对话框（goal=对话框内文件名编辑框）。

### 发现 #4：同进程并行 UIA 读产生 E_FAIL

契约测试的非对称队列触发深挖：两个并行 find 必有一个返回 `E_FAIL(0x80004005)`（顺序不定）——桌面根/提供程序的进程内并发上限。**并行读被证据否决**（既有害又无计时收益），CLI 回退串行；`execute_probes` 批量通道与引擎语义保留（为将来天然可并行的 CDP 读预留）。

### 发现 #5：菜单开启态下 goal 定位持续 RPC 故障

`verify --exists name:文件名`（item-container 虚拟化查询）在菜单 flyout 开启时**持续**抛 `RPC_E_SERVERFAULT`（手工两次复现，非瞬态）。因 goal 核验先于动作，原语义（INTERNAL→失败闭合）会饿死本该执行的菜单项点击。**修正**：查询类故障计 errors 如实记录、按未满足继续探索（预算有界兜底）；INTERNAL 与 NotFound 同样先经同 pid 跨窗探测。

### 发现 #6：定位通道对 flyout 项间歇性失明（帧背书原则）

同现场 `find --name 另存为` 交替返回 0 结果与 2 命中，而快照树稳定可见——**快照与定位两条通道可靠性倒挂**。原 probe-before-act 让更易抖动的通道否决更可靠的通道，把刚观察到的候选永久判死。**修正（帧背书）**：本轮帧内候选跳过动作前 COM 探测（帧即新鲜度证据）、动作失败不判死（计动作预算有界重试）；非帧内候选（应答队列）保持探测判死；卡构建的批量预检移除（卡与观察同轮，预检会错杀卡面），死候选过滤收敛到应答时队列预检与点击权威。

### 链路推进轨迹与剩余阻塞

每修正一次，链路前进一环：观察重试→点击「文件」后观察不再死；goal 故障语义→菜单开启态不再失败闭合；帧背书→「另存为」不再被 probe 永久判死。最终形态（explore-xwin-v2）：文件点击 ✓（菜单入帧，visited_states=2）→ 平局卡 d2（文件头 6 vs 另存为 5,分差 1<裕度 2 正确外抛）→ 应答 c2 → **「另存为」点击 3 次全部被定位通道拦截**（actions=4 预算耗尽,errors=7 全程如实记录,对话框未开）。剩余阻塞即发现 #6 的通道分叉本身：**点击的 item-container 定位在引擎的连续节拍下持续失明**——结构性收口已登记 ROADMAP:动作改经快照 ref 身份绑定（`click @eN --snapshot`,docs/31 身份纪律,走捕获身份而非模式查询重定位,顺带消除两通道语义分叉）。这是下一轮的第一件事。

### 附带

- goal 折叠/词表投影在整条链上持续工作（菜单项靠关键词 [另存] 进入帧与前沿）。
- errors=7 的构成:goal 查询故障×N(菜单开启态)+点击失败×3——全部如实入账,终态 budget_exhausted:actions 诚实。
- 清理:notepad 未保存文本仍在会话中（对话框未开,无需处理弹窗）；窗口仍在,后续轮继续用。

## 跨窗链路完成:ref 绑定 + 编号口径 + 存在型歧义三连落地（2026-10-03）

被测二进制演进至 `263699fb`(ref 绑定+编号修正+歧义语义)。证据 `target/acceptance-s2/25-27`。**链路全通:explore-xwin-done 状态 completed**——文件点击(reveals,菜单开)→ 另存为点击(acted,对话框开)→ goal(文件名编辑框)存在型歧义判定达成。观察 5 次、动作 4 次、决策 2 次、错误 2(goal 查询故障如实入账),全程零人工干预(预授权通道)。对话框(#32770,owner=记事本,**pid 同进程 49464**)由引擎自主打开;跨窗 goal 探测的同 pid 过滤条件在本形态成立(本次链路中对话框元素经锚窗 UIA 树可见,goal 直接命中,探测机制作为后备已由 MockHost 全链路覆盖)。

### 三块拼图

1. **ref 绑定动作(b1cbe9f)**:Host 新增 `execute_snapshot` 快照通道(返回 snapshot_id+data);引擎把快照 id 与帧同轮记录;act() 对帧内携带 ref 的元素构造 `click/type @eN --snapshot <id> --app`——身份来自看见它的那条通道。
2. **ref 编号口径修正(56d01fa,既有 CLI 缺陷修复)**:@eN 编号改为**全树 DFS 交互序数**(投影只过滤可见性,编号有洞)——修复前投影快照按投影后顺序从 @e1 重编,与 `click @eN --snapshot` 回放的全树计数错位,**skeleton/keyword/content 投影的 ref 从未正确回放过**(攻坚中实证:keyword 快照的 @e1 回放解析到全树第 1 个交互元素,NOT_ACTIONABLE)。修复后输出编号=持久化身份=回放计数三口径对齐,跨投影编号稳定(与 find 同纪律);全量快照不变。
3. **存在型歧义=达成(收官)**:对话框打开后 `name:文件名` 命中 3 元素,verify 报 AMBIGUOUS——CLI 的唯一性纪律服务动作定位,对存在性过严;引擎裁定存在型歧义为达成,值断言歧义维持失败闭合。

### 清理

对话框经 HWND 优雅取消(close-window ok,非 taskkill)、记事本优雅关闭,窗口清单 Notepad/另存为 计数归零。会话自动保存的未保存标签(`actl-explore-accept`)保留在案(此前已记录)。

## 锚点转移真机验收（2026-10-05 轮 A+B）：对话框内继续动作——通过

设计 docs/42 锚点转移（观察/动作锚从不可变 spec.app 升为状态）。轮 A（提交 96280cc）：`anchor_app`/`anchor_stack` 状态化、三判据转移（同 pid + 非当前锚/栈内窗 + **关键词帧非空自校验**——零命中窗自动出局）、差分基准重置（转移帧全量按 added）、**goal-锚合流**（goal 核验窗口优先序 = goal 窗 > 当前锚 > 声明锚——测试实证双转移浪费后提前落地）、锚窗消失出栈回退、goal 转移与锚转移共用预算。MockHost 4 项新测试 + 既有跨窗 goal 测试更新为合流语义。

**轮 B 真机链（run anchor-b-02，dev 二进制）**：记事本「文件 → 另存为 → 输入文件名 → 保存」全链——

- **动作链**（action_log 逐条）：`id:File`（开菜单，reveals）→ `name:另存为`（开对话框，reveals）→ **锚点转移到对话框**（anchor_transfers=1，观察前沿与动作派发改锚）→ `id:1001` type `anchor-save.txt`（texts 输入原语在**对话框内**执行，priors 控序平局）→ 决策卡应答 c1 → `id:1` 保存（answer 模式，acted）；
- **栈回退**：对话框关闭后终态 `anchor_app` 归一化 None（出栈回到 `app: Notepad`——标题子串跨改名存活的设计）；
- **goal 合流**：`verify-file --kind meta --size_min` 缺文件=AssertionFailed=NotMet（存在性继续探索）、保存落盘后 Met，**completed**；
- **落盘核验**：`anchor-save.txt` 存在、内容 `anchor round b` 精确；
- 诚实记账：`id:1001` 的效果标 `no_effect`（对话框帧无 UI 变化——归因如实）；决策卡平局外抛一次（对话框内「保存按钮/新建文件夹」平局——设计行为，texts 输入后按钮非独占领先）。

**附带真发现（当轮修复）**：点击「打开菜单」的 pattern 派发区段（get_pattern/invoke）**没有 UIA 看门狗覆盖**——菜单项 invoke 打开 flyout 时跨进程 COM 无界阻塞（真机两度实证，进程级看门狗 300s 才收尸）。修复：click 的 pattern 派发与 set-value 的 Value pattern 调用补 `watchdog::enter()` 区段（20s 击杀留 stderr 标记）。locate/identity/observation 已有覆盖，动作模式调用是最后一个裸区段。

遗留：对话框决策平局需要应答（priors 只救了 type 目标，保存按钮平局是词表设计问题——非缺陷）；轮 C（RuntimeBroker pid 放宽）待复现。

## 探索路线 #3：记事本中文输入（2026-10-05 第二批）——全链通过

**背景**：② --paste 通道选项收口的真机验证轮（引擎内容判据落地后）+ 路线积累 2/5→3/5。证据二进制 fnv128=81ee14e8（含 8e6cec5）。

**探索轮**（run explore-l2-notepad-input-zh，fixture `explore-notepad-input-zh.yml`）：三方平局卡（编辑 Button/编辑 MenuItem/文本编辑器 Document 同分 8——L2 同款非构造歧义）应答 c3 后单步执行；**goal_met:true**（actions=1, decisions=1, errors=0）。**② 三重物理证据**：① goal 断言 verify --value --contains 通过；② 窗口标题变 `*探索验收中文 - Notepad`（中文 5 字符实际送达）；③ 剪贴板残留 `探索验收中文`（paste 通道的物理痕迹——键盘通道不触剪贴板）。**writeback 导出同口径实证**：type 步骤含 `--paste`（route.rs 内容判据真机生效）。

**重放**（run replay-notepad-input-01，fixture `replay-notepad-input.yml`）：全步 completed、`verified:true`（attempts:1，5.5s，state.json）——人工复核迁移 verified 的走通证据（复核待用户）。

**路线 #3 归档**：`apps/notepad.yml` 首建（unverified，四条 quirk）。**夹具会话事故与破环**：新启记事本被 TabState/云同步复活出录制轮脏标签链（marker4→marker5→anchor-save→无标题→marker3→marker，窗口级 close 不清会话）；破环 = 标签级 `ctrl+w` + 「不保存」逐个废弃，末标签后应用自然退出（入 pack quirk）。

**环境退化记录（诚实）**：本批尾段挂起族扩散——flow-run/flow-resume/get value/launch 均出现"工作完成、进程不退出"（envelope 后滞尾或不出），僵尸堆积（exited=True 幽灵记录 20+）。引擎侧工作与状态落盘不受影响（state.json 均如实），但每轮真机调用产 1-2 个滞尾进程。**路线 #4/#5 与轮 C 挂起待环境恢复**（僵尸自退或重启后），不在退化环境上继续堆真机轮。

**新代码发现（登记待办）**：① `get --property value` 读路径无看门狗区段（实测超 300s 进程级看门狗仍未收——与 493d163 修的 click pattern 同族，watchdog 裸区段又一例）；② `launch` 的 `--wait-window` 参数疑似被透传给被启程序（记事本弹「找不到 --wait-window Notepad --timeout 20000.txt」，launch 的 option 双重处理待复现立案）；③ `actl launch notepad.exe` 解析不到 HKCU App Paths 注册（解析面只查 HKLM）。
