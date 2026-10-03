# 79 · Quicker 录制调研与 agent 在环的录制体系

日期：2026-10-03。状态：**调研记录 + 设计输入，未实现**——为 [76](76-gui-recording.md) 的 spec 评审提供外部参照，并修正其体系设计（§6-§7）。
关联：[76 GUI 录制提案](76-gui-recording.md) · [42 探索模式](42-exploration-mode.md) · [77 探索验收](77-exploration-s2-acceptance.md) · [03 RPA 与 Quicker](03-rpa-vs-quicker.md) · [10 Quicker 需求调研](10-quicker-demand-research.md)

## 1. 调研方法与来源

- 对象：Quicker 一方录制功能（官方文档 + 作者 CL 的论坛回复）与社区实际使用（共享动作讨论区）；
- 来源：getquicker.net 官方文档（docs.getquicker.net，部分模块页经代理读取）、Q&A 论坛、共享动作页；关键 URL 随条目内联；
- 既有结论衔接：[10 §4.8](10-quicker-demand-research.md) 已记录第三方共享动作 EVER录精灵的坐标重放约束；本轮补齐**一方功能形态、作者立场与用户痛点**；
- 局限：论坛证据为个案抽样非全量统计；UiPath/PAD 录制器对比是通识未本轮核验（§4 标注）；Quicker 作者未公开解释"为何不做语义录制"与全局键盘钩子的隐私立场，相关处标"推断"。docs.getquicker.net 模块页经代理读取，引文以原文为准。

## 2. Quicker 录制的实际形态（确认事实）

**一方功能存在，但走"原始输入流"路线**：

- 载体：组合动作编辑器的 `sys:record` 模块（文档标注**"本功能为测试功能，可能调整或取消"**）与托盘"键鼠录制工具"，产出同一形态——文本数据块，交 `sys:playRecords` 重放。另有模拟按键参数编辑器里录单个组合键的小按钮。源：[record 模块](https://docs.getquicker.net/v2/xaction/modules/record) · [playrecord 模块](https://docs.getquicker.net/v2/xaction/modules/playrecord) · [托盘](https://getquicker.net/KC/Manual/Doc/tray-icon)。
- **捕获内容：绝对屏幕坐标 + 按键名 + 事件间隔毫秒**。行格式 `delay_ms; TYPE; params;`（MC 点击/KD/KU 按键/MV 移动/MVD 相对位移…），键名用 .NET Keys 枚举；文档原话"坐标是绝对坐标"。**没有任何 (x,y)→控件语义映射**——尽管其 UIA 模块（FlaUI 封装）有现成的 `GetControlInfoByPosition`（坐标反查控件，与 docs/76 计划用的 ElementFromPoint 同源），**录制器从未接入**。源：[uiautomation 模块](https://docs.getquicker.net/v2/xaction/modules/uiautomation)。
- **重放约束（官方文档原话）**：成败取决于窗口位置、屏幕分辨率、输入法是否和录制时一致，"只适合特定环境"，并明确劝告**"含这类步骤的动作不要当通用动作分发"**。重放模块仅 data + speed 两参数：无等待、无重试、无验证，中断只能靠预配置的停止热键。
- **语义机器齐全但全靠手工拼装**：FlaUI XPath（文档自排稳定性：**AutomationId > Name（显示文本，易变）> 位置序号（布局一改就失效）**，支持多行按序回退与 `[@Name='Save' | #3]` 属性|序号回退语法）、等待窗口（结构化终态 matched/timedOut/invalidTarget/targetLost/error）、屏幕找图（只回报位置不点击）。源：[XPath 语法](https://getquicker.net/KC/Help/Doc/flaui-xpath-intro) · [waitwindow](https://docs.getquicker.net/v2/xaction/modules/waitwindow) · [searchbmp](https://docs.getquicker.net/v2/xaction/modules/searchbmp)。
- 第三方录制与一方功能并存：EVER录精灵（2 万+使用、48 个版本）沿用同样的坐标数据格式；社区对新手两条路都推荐。源：[EVER录精灵](https://getquicker.net/Sharedaction?code=e44f7f2d-ba96-47e6-da60-08d7880f80dc) · [论坛 24830](https://getquicker.net/QA/Question/24830)。

## 3. 真实用户痛点（社区证据，按对 actl 的相关度排序）

| 痛点 | 证据 | Quicker 的应对 |
|---|---|---|
| **中文输入按原始按键重放出错** | CL："用模拟键盘打拼音的方式太容易出错了" | 官方绕法：拆段插"发送文本"步骤或用多步骤输入的 `input:`（不受输入法影响）[27897](https://getquicker.net/QA/Question/27897) · [inputscript](https://getquicker.net/KC/Help/Doc/inputscript) |
| 窗口挪动/分辨率变化后坐标漂移 | 用户提议把绝对 MC 转相对位移（用 AI 写转换表达式），EVER录精灵作者回复"动作内做不了，得跟 qk 作者反馈" | 未解 [ViewTopic/36453](https://getquicker.net/Common/Topics/ViewTopic/36453) |
| DPI 缩放破坏录制 | CL 拒绝平台级自动换算：分辨率/DPI/OS 版本/应用自身缩放四因素不可控，"修改了感觉会出现其它意外问题" | 无内置换算 [16282](https://getquicker.net/QA/Question/16282) |
| **钩子被系统回收** | 启动忙时低级钩子超时被 Windows 回收；与第三方手势软件冲突同症 | 提供自动重挂设置 [3537](https://getquicker.net/QA/Question/3537) |
| 录制与其他全局钩子冲突 | 录制期间暂停 Quicker 自身钩子"避免互相影响"（用户 "~" 文字命令触发失效） | 设计使然 [20382](https://getquicker.net/QA/Question/20382) |
| 鼠标移动数据洪水 | "三个点击产生过多数据" | 后补"是否录制详细移动过程"开关 [14634](https://getquicker.net/QA/Question/14634) |
| 数据块不可编辑 | 社区 workaround：文本编辑里复制行伪造循环、拆段拼接其他录制、改数据 | 无解，常态 [EVER录精灵讨论区](https://getquicker.net/Common/Topics/ViewTopic/36453) |
| 部分应用不响应注入输入 | CL："有的软件可能不太响应模拟的鼠标事件" | 建议换应用测试 [32175](https://getquicker.net/QA/Question/32175) |
| UIA 黑盒应用 | 微信 4.0 自绘 DirectUI，UIA/inspect 全不可见 → 只能坐标 + OpenCV 模板匹配 + OCR + 多尺度 + DPI 换算 | 社区知识库方案 [KB 1171](https://getquicker.net/KC/Kb/Article/1171) |

分量最重的社区结论（用户原话，[20382](https://getquicker.net/QA/Question/20382)）："键鼠操作其实都可以用动作写出来的，适应范围还更广"——**社区自己绕开录制、手写语义动作**。

## 4. 对 actl 的直接验证

- docs/76 的三项核心决策——**事件瞬间语义反查、输出可编辑 YAML（而非不透明数据块）、生成可 flow-validate 的流程**——全部被 Quicker 的失败面反向验证；其 UIA 模块里躺着 `GetControlInfoByPosition` 而录制器不接，是"原始流路线"最直观的反例；
- docs/03 的选择器分层回退思想获得 Quicker FlaUI 文档的独立印证（同款稳定性排序与回退语法）；
- 对照（通识，未本轮核验）：UiPath/PAD 录制器输出语义选择器步骤、用户在可视化设计器里改——Quicker 是"录原始流、语义靠手写"的反例极点；
- 录制功能在 Quicker 漂了多年仍标"实验性、可能取消"——**原始输入流路线的上限实证**。

## 5. 六个吸收点（进 [76 §11](76-gui-recording.md)）

1. **文本输入绝不录原始按键**：录到 Edit 焦点记**提交后最终文本**生成 `type`，仅组合键/快捷键生成 `press`；中文 IME 下键序列重放必错（Quicker 实证）。连带：**`--paste` 通道（[ROADMAP](ROADMAP.md) 探索待办 ③）升格为中文文本录制的前置依赖**。
2. **钩子回调极简、反查放工作线程**：UIA 反查 P50 200ms+（本仓 timing-data）绝不能进 WH_*_LL 回调——Windows 会因超时回收钩子（Quicker 实证）。回调只做"时间戳+事件入队"，工作线程立即反查（接受毫秒级竞态）；原始坐标一并留存，作反查失败的降级与事后补查材料。
3. **上下文元数据随步记录**：窗口矩形/显示器/DPI/标题/进程/类名；坐标兜底存**窗口相对坐标**而非全局——绕开 Quicker 拒绝 DPI 自动换算的四因素困境（显式记录+显式换算，不猜应用缩放行为）。
4. **只录语义事件**：点击/按键/值提交/拖拽（→`drag --path`）；不录鼠标移动（Quicker 被迫后补开关的教训直接采纳为默认）。
5. **等待/验证一等公民**：延迟分析→wait/verify 步骤从"加分项"提为"可用性分水岭"（Quicker 用户被迫手工包等待）；waitwindow 的结构化终态枚举值得对齐。
6. **钩子互斥与自动重挂**：录制期间与其他全局钩子工具的互斥表现、钩子回收后的重挂机制，写进 spec。

**明确不学**：坐标数据块形态；以图像匹配作主要定位路线（微信式黑盒属已知边界，截图视觉定位是 PRD 非目标）。

## 6. agent 在环的录制体系（核心修正）

关键认知：**actl 有上层 agent，录制产物是"原料"不是"终产物"**——Quicker 无智能层只能录死；actl 可以录完让 agent 改写，确定性层保下限、智能层提上限。

```
入口A（已落地）：agent 自主探索 ──┐
                                  ├→ 回写层（{window} 泛化、证据头、status 恒 unverified）
入口B（待建）：  人工录制 ────────┘       ↓
                                    agent 改写轮（skill 层，读 trace 证据）
                                          ↓
                                    flow-run 重放对比 → 人工复核 → verified → pack 沉淀
```

右半边**已经存在**（探索模式 S3 落地：`explore-writeback` 格式、重放验证、PACKS 人工复核迁移、先验飞轮，见 [77 S3 增补](77-exploration-s2-acceptance.md)）；录制的编译输出对齐该格式，两个"→YAML 生成器"汇合成**单一回写层**——这是成本最低、结构收益最大的铺垫。

### 6.1 录制产物是双件套，trace 是一等公民

- **初版 YAML**：机械编译，可直接 flow-validate，目标不明带 TODO（不猜测）；
- **录制 trace**（JSON，本地持久化，参照 explore.json 原子写先例）：每事件完整捕获——UIA 反查全身份（automation_id/name/role）、原始坐标、窗口元数据（§5.3）、事件时序、反查失败原因、环境头；
- 纪律：**编译器不提前丢信息，净化决策留给 agent**——人演示常有的"点错又点回来"，YAML 里可能被误合并，trace 完整保留，agent 识别意图步骤、删噪音。

### 6.2 YAML 的可改写性字段（schema 增补，走 spec）

步骤携带：备选选择器链（分层回退全写入，不止 primary）、置信度、指回 trace 的证据引用。[76 §5](76-gui-recording.md) 原则 3 的"TODO 供人工补"升级为"TODO 带结构化证据引用，供 agent 或人工补"。

### 6.3 改写智能放 skill 层——LLM 不进 CLI

架构铁律（PRD 一贯边界）：CLI 保持确定性零 LLM；"智能改写"是 agent 经 actl 工具执行的 **skill 工作流**（`skills/actl/workflows/`）。改写轮动作：

1. 选择器稳健化：从 trace 的多级身份选更稳目标、组回退链；
2. 参数化：录制的字面值（文件名、输入文本）提升为 inputs——复用探索回写把 `--app` 换 `{window}` 的泛化思想；
3. 等待/验证注入：结合 trace 时序与"下一步是否新窗口/UIA 变化"综合判断；
4. 噪音清除与敏感标注（词法闸→pause_before，规则已有）；
5. TODO 修复（§6.4）。

### 6.4 agent 的独有能力：重放到中间步再观察

传统 RPA/Quicker 的改写只有静态信息；actl 的 agent 可**把改写后的 flow 跑到某步暂停（pause 协议已有），snapshot 观察现场，修正选择器，继续**——"确定性重放 + agent 观察修正"的混合循环，是 docs/03"agent 天然可自愈、不把 RPA 的锁死做进协议"哲学在录制场景的延伸。

### 6.5 改写效果的客观判据

改写前后各跑一次 flow-run，比较步骤 verified 率；验收标准：**改写后重放通过率 ≥ 机械编译版**，不凭感觉宣称"更好"。成本纪律：每轮改写真机重放不便宜，按"一轮多假设"一次改写解决多步骤再整轮重放。

## 7. 切片重排（替换 [76 §7](76-gui-recording.md)）

| 切片 | 内容 | 独立收益 |
|---|---|---|
| R1 | 钩子捕获（回调极简 + 工作线程反查）+ **trace 持久化** + 机械编译初版 YAML | Quicker 级下限：录完即可跑 |
| R2 | 编译增强：语义分类/焦点追踪/延迟分析→wait/verify 草案/泛化（{window}+参数化）；**输出对齐 explore-writeback 片段格式** | 与探索回写汇合，单回写层 |
| R3 | **agent 改写轮**：skill 工作流 + 重放对比验收 + 复核迁移（原"verify 自动生成"的规则部分归 R2、agent 部分归 R3） | 可复用健壮的 YAML |
| R4 | 录制 UI（显示条状态） | 非技术用户友好 |

## 8. 触发面与会话生命周期（2026-10-03 代码核实后增补）

显示端右键菜单**已具备两个可直接套用的先例**（`crates/actl-uia/src/bin/signal/shell.rs:601`，信号条右键与托盘右键是同一个菜单）：

1. **预授权开关先例**（菜单第 6 项，固定位置）：本地状态写入 + 弹窗反馈；菜单注释明确分层原则"授予动作只发生在显示端 UI，agent/CLI 无写入通道"；
2. **暂停/继续先例**（动态项）：菜单打开时从磁盘发现 flow 会话（`flow_ui::discover/read`），running/paused 才动态追加；动作走 `flow_action` → 请求文件 + 代次守卫（请求式，非直接操作）。

### 8.1 三个触发入口（同一命令面）

| 入口 | 角色 | 形态 |
|---|---|---|
| CLI `record-start` / `record-stop` / `record-pause` | **正典入口** | agent 与终端用户都走这里——一切能力必须 CLI 可达（CLI 是控制面，显示层是指示/同意面） |
| 信号条/托盘右键菜单 | 人类便利入口 | 套动态项模式：无会话→"开始录制"；有会话→"停止录制/暂停"；显示端 spawn 有界 `record-start` 进程（spawn exe 有先例），停止走请求文件（flow_action 同款） |
| 信号条状态文本 | 反后台录制的合规提示 | docs/76 §9"信号条有明确提示"——条上带"录制中" |

停止热键（演示中一个动作可达）：急停 Ctrl+Alt+F12 已开"会话控制热键"先例——PRD 非目标的"热键"指动作触发面板，会话安全控制不同类；边界在 spec 里写明，v1 可先不做。选右键菜单而非信号条常驻按钮：细条 overlay 挤空间且易误触，右键是显式意图动作、零占位。

### 8.2 代码核实带出的三个设计点

1. **会话状态文件必须落盘**：菜单动态项靠磁盘发现会话（flow discover 先例）——`record-start` 不能只是进程内存态，会话状态文件的位置/格式是 R1 的一部分而非 R4 补充；
2. **录制进程所有权**：右键"开始录制"= 显示端 spawn 的有界 CLI 子进程（start→stop，非常驻，不违反"非常驻 agent"）；录制进程**独立于显示端存活**（钩子挂在自己身上），显示端只做发现与请求——显示端重启不中断录制，spec 定接管语义；
3. **事件过滤**：录制中用户右键信号条/点菜单的输入都过全局钩子——trace 必须过滤 actl 自家进程（actl-signal、录制器本身）的事件，否则 YAML 混入"点录制 UI 自己"（Quicker 录制时暂停自家钩子的同族问题，§5.6 钩子互斥的具体化）。

## 9. spec 决策点清单（立项前必须裁定）

1. **trace 隐私脱敏**：trace 会捕获键盘最终文本，密码框（role/类名可识别）文本必须脱敏存储；
2. **trace 消费方式**：本地文件非 token 载荷，agent 按事件索引摘要+定点读取，不整文件进上下文；
3. **中文文本通道**：依赖 `--paste`/Value 写入通道，是否随 R1 同步做需裁定（否则 R1 验收限 ASCII，同探索 L2 验收先例）；
4. **钩子架构**：回调时限、反查工作线程、互斥表现、自动重挂；
5. **YAML 步骤证据字段**（§6.2）是 schema 变更，按硬规则 8 走 spec 提交（契约 golden + docs/06 + CHANGELOG 同 PR）。
