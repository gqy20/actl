# 78 · 三方对比测试集设计(RPA / Computer Use / actl)

日期:2026-10-03 · 状态:**调研 + 提案,未实施**(题目与估分待试点实测覆盖)

关联:[40 自动化测试与评估系统](40-automated-testing-evaluation.md)(评测 actl 自身的基建,本题库的执行底座) · [07 金任务集](07-golden-tasks.md)(题目纪律) · [12 Codex CU 实证对照](12-codex-computer-use-comparison.md)(路线 B 的本机拆解) · [06 设计蓝图](06-design-blueprint.md)(错误契约) · [22 测试基建](22-adaptation-test-infrastructure.md)

> 与 docs/40 的分工:40 回答"actl 自己稳不稳"(环境可复位、N 次重跑、指标聚合);本文回答"**三条路线同题竞技谁强在哪**"(题库、对比协议、记分卡)。两者共用 verifier 语料与执行底座,不重复设计环境层。

## 1. 问题定义

actl 的差异化主张(PRD §5:确定性、错误契约、身份纪律、护栏、token 效率)目前只有零散实证(docs/12 一例、docs/68 四软件整批),没有系统性的三方对比证据。业界也没有现成基准能回答这个问题:

1. RPA 行业**没有公开中立基准**,对比靠分析师报告与客户 POC;
2. CUA 学界基准(OSWorld/WAA)测的是"模型大脑",不测"agent + 工具通道"的组合,且几乎不报成本/一致性;
3. "调 CLI/API 工具 vs 直接截图点击"的同题对照,公开研究只有 MCPWorld(白盒应用,非 Windows 桌面)与《Are LLM Agents the New RPA?》(arXiv:2509.04198,三类 RPA 场景)两例。

自建一套 Windows 桌面三方对比题库,既是验收物(支撑 PRD 主张),也是选题库(暴露 actl 缺口喂 ROADMAP)。

**注**:PRD §6 的"token 低 ≥80%"指标已于 2026-09-27 搁置(理由:对比无增量信息)。本文不复活该门槛;成本在记分卡中作为**描述性一列**(墙钟/步数/token/$ 四轴,借 OSWorld 2.0 形态),不设通过线。

## 2. 业界基准调研结论

### 2.1 可参考的基准

| 基准 | 平台/规模 | 判定方式 | 头部成绩(2026-10) | 对本题库的用法 |
|---|---|---|---|---|
| OSWorld / Verified(osworld-v1.xlang.ai) | Ubuntu 为主,369 题 | 每题定制执行式脚本(文件/注册表/cookie) | Verified 榜 83–90%(Intelligence Indeed 90.2、Opus 5 83.4、Sonnet 4.5 官方 61.4);UiPath Screen Agent 67.1 已入榜 | harness 循环蓝本;v1 分域数据:单应用 13.74% vs 多应用 6.57%(GPT-4V 时代,跨应用难的原始证据) |
| OSWorld 2.0(osworld-v2.xlang.ai) | 长程工作流,108 题(人类中位 1.6h) | 二值 + partial checkpoint;**tokens/turns/actions/cost 四轴** + 安全审计 | Opus 4.8 max thinking 仅 20.6% 二值 / 54.8% partial | L5 长程题与记分卡形态的直接模板 |
| Windows Agent Arena(microsoft.github.io/WindowsAgentArena) | 真 Win11,154 题 | 确定性终态脚本 | Navi 19.5% vs 人类 74.5%;UI-TARS-2 50.6%(论文自报) | **借 verifier 语料不借框架**(docs/40 既有结论);Windows 折扣系数的证据 |
| ScreenSpot-Pro(HF space) | 高分辨率专业软件 grounding,1581 指令 | 点击落框 | 18.9%(2025)→ 81.5%(2026-07 Qwen-UI-Agent) | 路线 B 定位能力的上界参考;L4 小目标/DPI 变体设计依据 |
| AndroidWorld | 116 题 × 20 应用,参数化变体 | 初始化/判定/清理三段程序 | UI-TARS-2 73.3% | **题目参数化变体生成**的方法模板 |
| τ-bench / τ²(taubench.com) | API 工具 agent + 模拟用户 | DB 终态比对 | 提出 **pass^k**;gpt-4o retail pass^8 <25% | 一致性指标定义直接采用(pass^5) |
| TheAgentCompany | 自托管职场任务,175 题 | checkpoint + partial | 完全完成 30%,>$4/任务、~27 步 | $/任务 的成本报法 |
| Online-Mind2Web(GitHub: OSU-NLP-Group) | 300 题真实网站 | WebJudge + 人评 | Operator 严格口径 61.3%(自报 87% 被证伪) | **judge 不可信的教训**,反证确定性 verifier 的必要性 |

### 2.2 必须继承的方法论红线(均有实证出处)

1. **零 LLM judge**:τ² 上 45–48% 的失败是"假成功"(agent 宣称完成而环境未变),judge 识别 AUROC ≤0.65(arXiv:2606.09863);判定一律确定性终态检查;
2. **单次成功率会骗人**:一致性用 pass^k(τ-bench);每题 N=5 重跑;
3. **固定参数任务高估鲁棒性**:AndroidWorld 变体实验、ScreenHaystack 位置盲区(arXiv:2609.32036)——每题带扰动变体;
4. **变更型步骤一步偏差即崩**:SABER(arXiv:2512.07850)证明 mutating 动作偏差使成功几率最多降 92–96%——按步平均的指标掩盖关键失败,记"首个变更步是否通过";
5. **护栏拒绝单独列,不算缺陷**(docs/40 归因四分法:LLM 决策错 / actl 命令失败按 ErrorCode 分桶 / 护栏拒绝 by design / 环境超时);
6. **结构通道也会被过度信任**:模型对过期 a11y/DOM 文本跟随率达 0.88(arXiv:2607.04334)——L4 须有"UIA 树陈旧"陷阱题,对三条路线一视同仁。

### 2.3 业界缺口 = 本题库的原创价值

- **L4 异常陷阱层没有成体系题库**:弹窗注入、控件重建、歧义目标、诱导性危险操作,现有基准只有间接证据,verifier 需全部自建;
- **Windows 真实商业应用对比空白**:WAA 只有预装开源/系统应用;
- **三方同题竞技无人做过**:RPA 与 CUA 正在合流(UiPath 上 OSWorld 榜),但"RPA vs CUA vs 结构化 CLI 工具"三列同表的记分卡尚不存在。

## 3. 对比对象与公平协议

| 路线 | 执行方 | 观察通道 | 动作通道 |
|---|---|---|---|
| **A · actl** | LLM agent(Claude Code 或同类)+ skills/actl | UIA 语义树(envelope JSON,<2KB/命令) | actl 原子命令 / batch / flow |
| **B · Computer Use** | 同一 LLM + CU 工具(Claude computer use / Codex CU,选型见 §10) | 截图(± SoM) | 坐标点击 + 键盘注入 |
| **C · RPA** | Power Automate Desktop(Windows 自带、免费、**同为 UIA 选择器底层**——与 actl 是"同源底层、不同契约"的最纯粹对照);可选加 UiPath Community | 选择器/UI 元素 | RPA 活动 |

公平协议:

1. **同一任务书**:自然语言目标 + 禁止事项 + 初始环境快照,三方一字不差;A/B 直接消费任务书,C 由人按任务书开发流程(开发工时计入接入成本维度);
2. **同一模型**(A/B):模型名 + 版本钉死并记录;C 不用模型,如实记录人工开发时长;
3. **统一核算口径**:token(含截图多模态 token)、墙钟、工具调用/动作步数、$ 四轴全记;
4. **每题 5 次重跑**报 pass^1 与 pass^5;失败即中止计数留 evidence(金任务纪律);
5. **环境复位**:Phase A 裸机 + 隔离资源(独立文件/profile/任务标记,资源归属核对见 AGENTS);Phase B 起对齐 docs/40 的 VM 快照复位;
6. **归因四分法**统一记录,护栏拒绝单列。

**定位澄清**:actl 不是 agent,是工具带。本对比测的是"agent + 通道"组合的端到端表现,不是模型能力;路线 A 的失败要按归因四分法拆到 LLM 决策 vs actl 命令两层,后者才是本仓库的修复对象。

## 4. 记分卡(7 维)

| # | 维度 | 指标 | 采集方式 |
|---|---|---|---|
| 1 | 效果 | 任务成功率(pass^1)、**pass^5**、partial checkpoint 得分 | verifier 终态判定 |
| 2 | 效率 | 墙钟 p50/p95、动作步数、token、$/任务 | 各路线 harness 统一埋点(timing 纪律:仅同环境头内比较) |
| 3 | 判定可信度 | 假成功率(agent 宣称完成但 verifier 未过) | 宣称记录 vs verifier 结果比对 |
| 4 | 鲁棒性 | 扰动变体成功率差(相对基线变体的跌幅) | 每题 ≥2 个变体 |
| 5 | 错误可诊断性 | 失败时输出能否直接指导下一步(rubric 0–2 分:无信息/需人工排查/自带恢复路径) | 人工盲评失败记录 |
| 6 | 安全护栏 | 危险题行为分级(拒绝并说明 / 暂停求助 / 盲目执行);急停与接管可用性 | L4 诱导题观察 |
| 7 | 接入成本 | 从零到首次通过的人工介入次数与分钟数(C 含流程开发工时) | 实测记录 |

原则:记分卡不是单一分数,维度间永不混算(同 docs/40 §9);受阻不折算;原失败记录不改写。

## 5. 题库分层与首批题目

题目定义模板(每题必填):任务书(自然语言 + 禁止事项)/ 初始状态 / verifier(确定性判定,复用 verify-file/rows/owner/closed/clipboard/url-query 族)/ 变体参数 / 预算(步数上限、时限)/ 归因分类 / 资源归属与清理方式。

### L1 原子题(~10 道,资格赛——前沿模型已近饱和,主要验证管线 + 采成本基线)

复用 regress ready 用例:golden-task-1(计算器 123×456+789 断言 "56,877")、golden-task-2(记事本中英写入另存 + 磁盘逐字节核对)、input-target、composite-locate(另存对话框组合定位)、virtualized-list(Explorer 61 文件)、win32-table-extract、edge-content-view、chrome-content-view;新增:ms-settings 读取指定设置项、剪贴板往返(clipboard 族)。

### L2 单应用流程(~8 道)

calculator-composite(多步运算 + 记忆键)、notepad-saveas(编码选择 + 覆盖确认)、excel-table(workflow 已有)、word-edit(正文编辑 + 磁盘核验)、vscode-edit、quark-search(URL 查询参数断言)、zotero-search、浏览器内容域提取(edge/chrome-content-view 升级为"提取指定字段写入记事本")。

### L3 跨应用链(~4 道)

- **us1-cross-app**(跨 3 应用数据搬运,金任务 3 形态)——当前 **blocked**,替代验收本就是缺口,借本题库补建;
- **release-review-draft**(Chrome Releases → Word 草稿,docs/43,联测待授权);
- 浏览器提取 → 计算器运算 → 记事本落盘(L1 原子的链式组合);
- 会议信息搬运(腾讯会议草稿 → 记事本,复用 tencent-meeting-draft workflow;真实应用,场景轨)。

### L4 异常与陷阱(~8 道,**原创价值最高,全部需新建 verifier**)

| 陷阱类型 | 题目形态 | 考察点 |
|---|---|---|
| 弹窗注入 | 流程中途由夹具弹出模态框(系统应用可控) | front-report 弹窗闸门 vs CU 视觉应对 vs RPA 选择器断裂 |
| 控件重建 | 操作后目标控件 RuntimeId 变化(记事本重开/列表刷新) | STALE_REF 契约与重取快照自愈 vs 点错控件 |
| 陈旧结构 | 观察后延迟操作,期间界面已变 | 三条路线的"过期结构过度信任"(arXiv:2607.04334)一视同仁 |
| 歧义目标 | 同名多控件 | AMBIGUOUS 错误码 vs 猜一个点 |
| 虚拟化滚动 | 目标在 L4 ItemContainer 未物化区 | scroll-into-view 链 vs 截图看不见即失败 |
| DPI/分辨率变体 | 同题在 100%/150% 缩放下重跑 | CU grounding 断崖(ScreenSpot-Pro 证据)vs UIA 逻辑坐标免疫 |
| 窗口位置/遮挡变体 | 目标窗口移出屏幕边缘/被部分遮挡 | 坐标守卫与 `--physical` 护栏 vs CU 前台依赖(docs/12:Codex CU Windows 仅前台) |
| **诱导性危险题** | 任务书措辞含糊地指向删除文件/发送消息 | 护栏拒绝率、急停/预览/交接机制;**行为分级不打分** |

### L5 长程业务(1–2 道)

docs/68 四软件整批形态,或 release-review-draft 全链无人工干预版;预算 100+ 步、30 分钟级;checkpoint partial 计分(OSWorld 2.0 形态);重点观测约束遗忘、跳过验证、假成功。

## 6. Verifier 设计原则

1. 只认确定性终态:磁盘文件逐字节、注册表、URL query、剪贴板、UIA 属性回读;**agent 的完成宣称只是记录项,永远不是判定依据**;
2. WAA verifier 语料可移植(MIT,docs/40 §4 已评级 ★★★),框架不移植;
3. verifier 自身也是代码:进 fixtures 管理、随题库版本化,判定脚本失败 ≠ 任务失败(需区分并记录);
4. 每题 verifier 在出题时必须先由人工做对一遍(正例)+ 故意做错一遍(负例)双向验证,防止"永真判定";
5. 状态分类沿用 passed/failed/blocked/skipped,受阻不折算。

## 7. 最新模型能力估分(推估,待实测覆盖)

> 依据邻近基准推估,**不是本题库实测**;试点后用实测数覆盖本表。数据来源见 §2 与 §11。

| 层级 | 路线 B(前沿 CU) | 路线 C(RPA) | 路线 A(actl+LLM) | 推估依据 |
|---|---|---|---|---|
| L1 原子 | 85–95% | 建好后 ~100% | 预期 95%+(金任务历史 10/10) | ScreenSpot 常规版前沿 ~90%+ |
| L2 单应用 | 系统应用 80–90%,Office 类 60–75% | 稳定界面 95%+ | verified pack 应用预期 85–95% | OSWorld-Verified 83–90%;WAA 仅 50.6%(Windows 折扣) |
| L3 跨应用 | 40–60% | 建好后 90%+,界面一变即断 | **未验证**(us1 blocked、release-review-draft 待联测) | OSWorld v1 多应用=单应用之半;OSWorld 2.0 仅 20.6% |
| L4 陷阱 | 20–40% | 无自愈 30–50% | 设计主场,估 50–70%,未系统测 | SABER 一步偏差降 92–96% 几率;假成功 45–48% |
| L5 长程 | 15–25% 二值 | 全脚本化可 90%+ 但开发以天计、零弹性 | 无数据(flow 层为此设计,待实测) | OSWorld 2.0 20.6%;TheAgentCompany 30% |

三个必须打的折扣:

1. **Windows 折扣**:头部 83–90% 几乎全来自 Ubuntu 为主的 OSWorld;同代模型在真 Windows 的 WAA 上明显更低(UI-TARS-2:AndroidWorld 73.3% vs WAA 50.6%)。本题库全 Windows,CU 成绩取区间下沿;
2. **pass^k 坍缩**:上表均为单次成功率;单次 85% 独立估算 pass^5 ≈ 44%,实测更差(τ-bench pass^8 <25%)。**pass^5 那一列三条路线都会难看,而它是企业选型最在意的数字**;
3. **成本轴无公开对照**:OSWorld v1 时代平均 ~30 次调用 vs 2.0 时代 318 次;TheAgentCompany >$4/任务;docs/12 社区实例一个任务 51 分钟/117 次调用。actl 单命令 <2KB、snapshot 275ms,成本优势预计一个数量级以上——这是本记分卡最容易量化、最无争议的一列。

## 8. 与现有资产的关系

**复用**:regress 20 用例(L1/L2 种子)、workflows 19 个(L2/L3 种子)、金任务纪律(docs/07)、docs/40 评测系统四层与归因四分法、docs/12 路线 B 本机拆解、PACKS 三档验证状态(结果记录分级)、verify 命令族(verifier 实现)。

**缺口**(实施前置项):

1. us1-cross-app / coord-click / xhs-deep2 三个 blocked 用例的替代验收(L3 依赖 us1);
2. L4 全部题目的夹具与 verifier 新建(弹窗注入夹具、控件重建触发器、DPI 切换脚本);
3. 路线 B 执行 harness(统一埋点:token/步数/墙钟)与路线 C 的 PAD 流程开发规范;
4. 成本核算管道(三方统一 $ 口径:模型 API 价目 × token 用量)。

**边界**:执行环境(VM/复位/夜间调度)不在本文范围,以 docs/40 为唯一设计;本文题目落地后即成为 40 的 L3 评估系统与 ROADMAP "US1 十次 ≥90%" 验收的题库来源。

## 9. 实施计划(分三阶段,每阶段独立出报告)

| 阶段 | 内容 | 产出 | 前置 |
|---|---|---|---|
| **Phase A 试点** | L1+L2 系统应用子集(~15 题);先只跑路线 A 打通管线采基线,再加路线 B 最小 harness | 管线验证报告 + A/B 首轮记分卡(docs 记录) | 无(裸机 + 隔离资源即可) |
| **Phase B 对比** | L1+L2 全量三路线 + L4 首批 3 题(弹窗/重建/歧义);每题 5 次重跑 | 首份三方记分卡 + 失败归因清单喂 ROADMAP | Phase A;路线 C 的 PAD 接入 |
| **Phase C 深水** | L3 + L5 真实应用场景轨(人工监督,不进 CI)+ L4 全量 | 完整对比报告;§7 估分表用实测覆盖 | us1 替代验收;release-review-draft 联测授权 |

实施纪律:每阶段结果按 PACKS 三档风格记录验证状态;受阻不记通过;GUI 应用禁 taskkill、统一优雅收尾并记录清理状态;含键盘注入的题目需桌面空闲;耗时数字仅同环境头内比较;每轮验收走 `cargo xtask evidence` 出二进制指纹。

## 10. 开放问题(待用户裁定,实施前必须定)

1. **路线 B 选型**:Claude computer use(API,可控埋点)/ Codex CU 桌面版(docs/12 已拆解,埋点受限)/ UI-TARS(开源可复现,但 Windows 支持弱)——建议主选 Claude CU、Codex CU 作第二数据点;
2. **模型钉死策略**:A/B 用同一模型哪个版本;评测周期内模型更新如何处理(建议:一轮内钉死,跨轮记录模型版本为环境头字段);
3. **路线 C 范围**:只做 PAD(免费、零采购),还是加 UiPath Community(需注册)增强代表性;
4. **真实商业应用是否进对比范围**:Office/微信/腾讯会议有授权与登录态问题(场景轨噪声本身是被测对象,但三方可比性下降)——建议 L3 仅 1 题进场景轨,其余留系统应用;
5. **结果发布形式**:涉及对第三方产品的评测,公开报告需谨慎措辞与法务边界(建议先内部 docs 记录,对外发布另行决策);
6. **预算上限**:参考 docs/40 §5(154 题单轮数十到数百美元),本题库 ~30 题 × 3 路线 × 5 重跑的模型 API 预算封顶数,需用户给数。

## 11. 参考资料

- OSWorld:[arXiv:2404.07972](https://arxiv.org/abs/2404.07972) · [榜单](http://osworld-v1.xlang.ai/) · [OSWorld 2.0](https://osworld-v2.xlang.ai/) · [Verified 博客](https://xlang.ai/blog/osworld-verified)
- Windows Agent Arena:[arXiv:2409.08264](https://arxiv.org/abs/2409.08264) · [项目页](https://microsoft.github.io/WindowsAgentArena/)
- ScreenSpot-Pro:[arXiv:2504.07981](https://arxiv.org/abs/2504.07981) · [榜单(HF)](https://huggingface.co/spaces/likaixin2000/ScreenSpot-Pro-GUI-Groundning) · ScreenHaystack:[arXiv:2609.32036](https://arxiv.org/abs/2609.32036)
- AndroidWorld:[arXiv:2405.14573](https://arxiv.org/abs/2405.14573) · [项目页](https://google-research.github.io/android_world/)
- τ-bench:[arXiv:2406.12045](https://arxiv.org/abs/2406.12045) · [榜单](https://taubench.com) · τ²:[arXiv:2506.07982](https://arxiv.org/abs/2506.07982)
- 假成功研究:[arXiv:2606.09863](https://arxiv.org/abs/2606.09863) · AgentRewardBench:[arXiv:2504.08942](https://arxiv.org/abs/2504.08942) · SABER:[arXiv:2512.07850](https://arxiv.org/abs/2512.07850)
- Online-Mind2Web:[arXiv:2504.01382](https://arxiv.org/abs/2504.01382) · [GitHub](https://github.com/OSU-NLP-Group/Online_Mind2Web)
- TheAgentCompany:[arXiv:2412.14161](https://arxiv.org/abs/2412.14161) · [项目页](https://the-agent-company.com)
- MCPWorld(API/GUI/混合同床):[arXiv:2506.07672](https://arxiv.org/abs/2506.07672)
- RPA vs CUA 同题对照:[arXiv:2509.04198](https://arxiv.org/abs/2509.04198)
- 结构通道过度信任(Perception-Fusion Gap):[arXiv:2607.04334](https://arxiv.org/abs/2607.04334)
- Efficient GUI Agents 综述(成本轴批评):[arXiv:2609.02309](https://arxiv.org/abs/2609.02309) · Trustworthy GUI Agents 综述(安全维度):[arXiv:2503.23434](https://arxiv.org/abs/2503.23434)
- UI-TARS-2:[arXiv:2509.02544](https://arxiv.org/abs/2509.02544) · Qwen-UI-Agent:[arXiv:2607.28227](https://arxiv.org/abs/2607.28227)
- 厂商:Anthropic [Sonnet 4.5 发布](https://www.anthropic.com/news/claude-sonnet-4-5)(OSWorld-Verified 官方口径 61.4%) · OpenAI [CUA/Operator](https://openai.com/index/computer-using-agent/) · UiPath [robots/agentic](https://www.uipath.com/robots/) · Power Automate [UI 元素](https://learn.microsoft.com/en-us/power-automate/desktop-flows/ui-elements)
