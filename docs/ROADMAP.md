# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 里程碑

- **当前：M3 = v0.3.0（协议稳定+按需扩展）** · 终点：1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

- [ ] **探索模式（用户标记关键）**：S1–S3、L2 原语与先验飞轮已全部真机通过（[77](77-exploration-s2-acceptance.md)）。待办：①~~观察锚点随弹窗转移~~（2026-10-05 已落地并真机验收,[77 锚点转移增补](77-exploration-s2-acceptance.md);剩余 RuntimeBroker pid 放宽待复现）；②~~type 的 `--paste` 通道选项~~（2026-10-05 落地:texts 原语内容判据自动选通道 + writeback 导出同口径注入,真机中文轮并入路线积累批次）；③路线积累（§7 目标 5 条已 **3** 条——#3 记事本中文输入 2026-10-05 全链+重放 verified 待人工复核;#4/#5 挂起待环境恢复（僵尸堆积,[77 末节](77-exploration-s2-acceptance.md)））；④~~L0 命中重放 skill 层流程化~~（2026-10-05 SKILL.md 双分岔指引落地）;UCB 统计剪枝（待 quirks 样本量）
- [ ] release-review-draft 写段联测（待用户授权，[43](43-release-review-draft.md)）

### 流程与集成

- [ ] 流程扩展（嵌套分支——**缓办**，无真实场景，出现再立项；循环已落地 for_each 形态，2026-10-11）
- [ ] 浏览器通道余量：flow 级 uncertain 持久化、pipe 传输 spike（`--remote-debugging-pipe`）、`--user-agent` 覆盖（headless UA 壁垒站点）、选择器索引消歧、batch 组合验收
- [ ] **browser-read 列表枚举形态（协议面变更，spec 三件套同 PR）**：~~CDP 轨多匹配读动词——现状唯一匹配纪律（0→NOT_FOUND、>1→AMBIGUOUS），枚举靠位置谓词绕行（[73 round7](73-browser-twitter-scenario.md)）；批量通道的并行语义契约已预留（[06 P3 修正](06-design-blueprint.md)——CDP 读是唯一预留并行方向）~~（2026-10-11 `--all [--limit N]` 落地：列表语义+诚实截断+位置谓词绕行废止；真机复验并入浏览器聚合轮）
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] 中央任务提示专项验收：真实失败/停止/接管暂停、显示端重启后旧流程恢复、跨屏 DPI 与减少动态设置（[75](75-action-start-notification.md)）；并轨补入：急停真实按钮 + 跨进程解除完整交互（[35](35-stop-lifecycle.md)/[39](39-flow-observation-recovery.md) 尾自 2026-09-28 悬置——真实按钮路径待验收，与真实停止项同轮收口）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] Jev 任务模式闭环（仅真实需求时立项，[08 §13 增补](08-agent-cli-jev-design-proposal.md)——third-hand 实证对照、弱完成证据不采纳的立场与切入点修正：增量在探索决策步/AMBIGUOUS 消歧/任务闭环，不在 grounding；前置零件 AMBIGUOUS context 与 `find --capable` 已落地，候选工程先于模型）
- [ ] MCP 远程访问适配（轮 1-2 已实现并活体验证：[82](82-mcp-adapter-research.md) / [06 §7.8](06-design-blueprint.md)；**剩余：跨设备真实客户端接入验收 + 模式 B（SSH/stdio）与 tasks 扩展（轮 3 可选）**）
- [ ] 本地 serve 与增量观察
- [ ] 游戏场景（Minecraft 命令通道）——含指针原语缺口（[17 §3](17-game-scenarios.md) M3 候选，「手工搭」路线才需要）：`mouse-move dx,dy` 增量移动（前置 GLFW raw-input spike）、鼠标按键 down/up 拆分带时长（按住挖掘）、节奏敏感动作时窗参数

### 应用深化

- [ ] VS Code 关闭重开完整链（当前：命令面板可定位、type 受护栏保护）
- [ ] Excel 写入闭环（当前：UIA 树空属 Office 慢启动，需 MSAA 路线或等待）
- [ ] Zotero 导入导出
- [ ] 腾讯会议 draft-date 键盘路线（ctrl+a+paste+tab，QDateEdit set-value 无效）——2026-10-11 排期尝试，**本机未安装 wemeet（注册表 App Paths/常见 Program Files/本地 Programs 均无）如实记录不可验收**，待有环境机器轮
- [ ] 资源管理器保存框路径交接余项（2026-10-11 档案已建 [explorer.yml](../skills/actl/apps/explorer.yml)：虚拟化全量收集/按名查找双路线 verified，[10 §4.2](10-quicker-demand-research.md)；**剩 = 地址栏具体 automation_id 现场探测 + 保存框路径交接真机轮**，探测后回填路线）。⚠ explorer 关闭纪律见长期观察项「极端规模重树」条（勿按 pid 关）
- [ ] 浏览器扩展操作面余量（UIA 轨三批已沉淀 [16 §5.5](16-browser-scenarios.md) / browser.yml `extension_surfaces`）：①右键菜单扩展项复验（机制在，两次被用户桌面并发占用护栏正确拒绝，待空闲）；②CDP 自有会话扩展操作实测（persistent profile 保留扩展策略已定 [72](72-browser-cdp-integration.md)，chrome 层归 UIA 的路径无实测证据，chrome 层 Invoke E_FAIL 同款风险预期）；③扩展弹层内写动作成链（现存证据只覆盖「读」）
- [ ] 应用 pack 长尾维护

### 遗留验收

- [ ] 非开发机 US3 复现（用户裁定 2026-10-02 搁置——作为已知限制已记录于 0.2.0 CHANGELOG；机器到位后补验）
- [ ] Phase 2 人工/专用环境验收子集（[44](44-phase2-acceptance.md) 表格聚合）：观察故障注入五类（空值/截断/控件重建/虚拟化/provider 故障——[50](50-deep-container-query.md) 的 provider 重建待验收同族）、监听冗余 hook 失效现场验收（[39](39-flow-observation-recovery.md)）、外部对照 Blender/Photoshop/Minecraft 可用环境
- [ ] 浏览器场景实测余项聚合：**B 站选卡/锚点层已收口（2026-10-11 搜索页 --all 枚举+href navigate 直达，绕开锚漂移；[16 §5.2 增补](16-browser-scenarios.md)）；评论 shadow 阻断已由 --deep 穿透收口（真机命中 bili-comment-renderer，见 CHANGELOG Unreleased）——评论全量滚动收集仍是组合欠账（browser-collect 候选，见下）**；持久 profile 真实站点登录验收待用户选站（[72](72-browser-cdp-integration.md)）；settle 噪声鲁棒（16 登记 M3 候选未动）；epub 临时 profile 删除竞争（[73](73-browser-twitter-scenario.md) v1 缺口五条中唯一未收口——close 后延迟重试）；CUA 检查 CDP 超时孤立核验（[71](71-signal-webview-details.md)）；xhs「展开更多评论」低优先附带
- [ ] **browser-collect 滚动收集原语（2026-10-11 候选登记，实施前 spec）**：UIA 侧 `collect` 已落地（虚拟化列表差分去重收敛），浏览器长列表（评论区/搜索结果流）无对等动词——当前组合 scroll+browser-read --all 逐轮手动判定收敛，无差分去重与空连两轮收敛权威。形态候选：CDP 滚动注入+DOM 快照差分（复用 collect 状态机语义：复位→页滚→沉降→新鲜读→差分→空连两轮收敛），与已落地的 --deep 穿透组合覆盖 B 站评论全量收集场景
- [ ] **浏览器页内弹层引擎门禁（2026-10-11 缺口登记，候选——真实阻塞事故出现再升级）**：原生弹层门禁结构性不覆盖页内 DOM 弹层（门禁只作用带 `--app` 锚写步；front-report 只收 role:Window 表面）。**v1 检测原语已落地**：`browser-verify --no-popups [--popup-selector]`（可见弹层枚举断言，ASSERTION_FAILED→condition false，when/unless 组合 browser-click 处置分支即弹层守卫；headless fixture 六假设实证，见 CHANGELOG）。候选余量=引擎级自动门禁（browser 写步前自动检测+声明式处置，与 popup_handlers 对称）——待第一起「页内弹层阻塞 flow」事故再立项，不为猜测建基建
- [ ] **浏览器 CDP 工程余量（[72 §13](72-browser-cdp-integration.md) 聚合）**：`--remote-debugging-pipe` 实现形态 spike（当前 port+127.0.0.1，pipe 消除端口暴露面）；写入 uncertain 跨进程持久化（对齐 [47](47-execution-recovery.md) 恢复语义）；UIA 轨路线固化（browser.yml 10 条中 3 verified/5 manual/2 unverified——随应用轮渐进，pack 治理日常非协议项）
- [ ] **flow 工具链缺口聚合（2026-10-11 VS Code 轮实证登记，实施前 spec）**：① **flow-revise 不解析 `{"input":…}` 引用**——校验路径直接跑 schema（args 必须 string），喂含输入引用的模板原文件必报 PROTOCOL；当前绕过=flow-run --emit-plan 产具体计划再修订（docs/70 §4 纪律入册）；正解=revise 复用 run 已存解析值做 input 解析，或 help 明说只收具体计划。② **popup_handlers.app 是静态子串**——模板 app 锚=`${run.id}`（每轮不同）时无法静态声明 handler（VS Code notice handler 探针靠 run id 前缀约定绕过）；正解=handler 支持输入引用（spec 候选）。③ **companion lease 窗口 vs 慢日步长失配**——慢 UIA 日（每步 2-5.5s）单次 resume 累计时长超 lease → companion_unresponsive fail-closed 瞬断（一轮 4 次，均自恢复；恢复路径稳定=幂等核验+--retry-step）；lease 窗口自适应或慢日容忍度待专项。
- [ ] **VS Code 关闭重开完整链（2026-10-11 第四轮：22/26 步，尾段被门禁降噪缺口阻断，如实记录）**：undo/redo 舞步、replace、close-before-reopen、reopen 重开前台全链通过；**focus 原语与 notice handler 自动处置已在探针轮（vscode-notice-probe-01）真机验收**（focus：ctrl+f→esc 绕行后 SetFocus 建立+press --target 断言+focused:Edit 三重实证；notice：门禁匹配→自动点 Clear Notification→清层→写步 completed+verified；notice 每次 launch 必现，reopen 第二次实证）。**尾段四步（find-replace/quick-open×2/close）受阻于 Find 面板常驻 UIA 表面**（表面带按钮→front-report 报 unknown actionable→门禁挡一切写步，结构性死锁——与 Word 停靠窗格误判同族，**门禁降噪缺口第二实例**，降噪 spec 落地后重跑；find-replace 的 Replace All+保存效果已手动核验达成，磁盘=期望值）。慢日伴生：companion lease 瞬断 4 次（每次幂等核验后 --retry-step 推进）；flow-revise 不解析 input 引用（emit-plan 具体化绕过，缺口见下）。任务窗口已清理（数据落盘+优雅关闭）。**降噪第二轮落地后收口（2026-10-11 同日）**：popup_ignores 声明认领（v4 修订）后 find-replace/quick-open×2 同现场 A/B 对照全部 completed——常驻面板死锁收口,run 收于 25/26（close 步 bind 陈旧系操作者清窗连锁,手动等价收尾）；词表启发退役+provider offscreen 路线探针否证(折叠面板仍报 on-screen)。本条目待降噪 spec 提交后移出。
- [ ] Zotero 导入导出（2026-10-11 轮让行未跑——用户桌面活跃，GUI 启动让行）
- [ ] preview-choice computer-use 真机鼠标点击验收（[46](46-target-preview.md)：两轮 "window is not a usable app window" 失败如实保留）

<!-- 2026-10-05 收口移出：录制值读回 #4 双轨收口（rec-ma-12 证据二进制轮，[80](80-record-acceptance.md)）；健壮性基建 009 轮 10 步 155s 全链通过（[81 §9](81-word-style-flow.md)）；网易云收起方向识图证实（pack 同步，Table 树滞留非未收起）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-08 收口移出：Word 流程 evidence 二进制补跑（note-typo-012 干净轮 10 步全链 completed，fnv128=19f4b6a8，五层修复合力验证通过，[81 §10](81-word-style-flow.md)）；虚拟化列表全量收集动词（`collect` spec 落地+资源管理器 600 文件 fixture 真机验收通过，[06 §3.4](06-design-blueprint.md)）；flow 歧义候选串联（`--pick` 运行时候选纠正 spec 落地，[06 AMBIGUOUS 目标串联](06-design-blueprint.md)/[63 收口](63-simple-workflow-conditions.md)；自动缩小查询裁定不做入册）；选区读取原语（`get-selection` spec 落地+记事本真机验收通过：Text/Selection 双通道、caret 退化范围诚实 no_selection，[06 系统组](06-design-blueprint.md)）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：执行器错误原文保留（metadata 证据附有界 message，[68 待办](68-combined-batch-first-round.md)勾销）；flow 锚点标题插值（**降格收口**为 verify-owner `hwnd_anchor` 就绪字符串+bind 惯用法——HWND 不随标题变，不做插值 spec，[06 verify-owner](06-design-blueprint.md)；真机部分验收：改名失配后 hwnd 锚仍寻址同窗，完整链待桌面空闲轮）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：流程扩展循环（`for_each` 数据驱动有界迭代 spec 落地，[06 for_each](06-design-blueprint.md)/[63 收口](63-simple-workflow-conditions.md)；通用 while/嵌套分支缓办入册）；真机验收待首个消费场景（机制层验收=纯函数 7 用例+flow-validate 契约）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：popup notice 词表裁定（**最窄形态落地**——popup_handlers `classification:notice`，name_contains 必填按名认领 INFO 级 unknown 表面，未声明的 unknown 仍永远上抛、已分类面板不被吞并；[06 运行时治理③](06-design-blueprint.md)；机制层验收=popup_gate 13 用例，真机验收待 VS Code 轮探测通知文本+按钮 name 回填模板声明）；浏览器页内弹层检测原语（`browser-verify --no-popups` v1 落地，headless fixture 六假设实证；引擎级自动门禁留候选待事故）——见 CHANGELOG Unreleased。 -->


## 长期观察项

- [ ] UIA 观察长阻塞的**根因定位**（防护已落地：进程级看门狗 + UIA 区段看门狗双保险；根因——60s NOT_FOUND 未复现、故障提供程序未确认，见 [41](41-logging-metadata.md)、[65](65-observation-blocking-diagnosis.md)）
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）
- [ ] **尾段挂起族（根因已定位,主修复已落地 2026-10-06）**：根因 = UIElement 的 Drop（跨进程 Release）与读取同族无界,而析构路径无任何看门狗覆盖。修复两轮:① main.rs 成功路径硬退出（与错误路径对称,析构链跳过）;② `into_info`/显式 drop 家族全收编（read/verify/click/set_value/select/toggle/pointer×4/type/press/cache_store——**Located 出域审计完成,残余项清零**）。**内核态残余已定性(2026-10-06 doctor 线程态探查)**:幽灵 = 每进程仅剩 1 线程卡 Wait/UserRequest(用户态同步原语等待,对端已死)——envelope 送达与主体清退均成功,仅进程终结被最后一根线卡住;MiniDump 无法附加(dump 失败本身佐证楔死),精确线程归属待 dump 工具(转储级定位降级为可选——doctor + 自动 ghost 规避 + 重启清退已工程化)。**新签名(2026-10-08,note-typo-012)**:flow-resume 成功路径 envelope 落盘后楔死——成功路径硬退出未覆盖 flow 命令的收尾路径(推测 flow 引擎持有的 Word 元素在 close 步后 Release 对已死 provider),待按类修根因。

- [ ] **树级缓存（「第二次观察免费」）——设计再评估后挂起（2026-10-06）**：原设想以窗口 RuntimeId 键控缓存已观察树 + 失效判定。设计推演结论:① 节点级缓存无 COM 代理 → 无法做 locate 缓存式的元素级 RuntimeId 复核,失效判定只剩 WinEvent 钩子(需消息泵线程,工程量大)或有界 TTL(陈旧风险违背 fail-closed);② 主要重复观察方（探索引擎/flow verify）**语义上就需要新鲜帧**（差分/断言）,缓存反而破坏语义;真实重复收益集中在 batch 的连续 find/snapshot——频次低。**结论:价值窄于最初评估,等真实重复消费方出现再立项**;轻量替代（定位缓存/属性预取/provider 快路径）已覆盖主要痛点。
- [ ] **极端规模重树 A/B（收窄至 Word 级非虚拟化大树）**：provider 支持度与虚拟化交互均已消除（2026-10-07:资源管理器 6866 文件目录实测——列表虚拟化是 provider 侧行为,双模式各见 294 元素/find total=17 一致/输出逐字节一致;虚拟化应用观察树天然保持小,极端规模只能来自 Word 类非虚拟化大树——与 Word 物化验证并轮做）。⚠ 资源管理器关闭纪律:close-window --pid 对 explorer.exe 危险（shell 进程持任务栏/桌面,本轮拒绝属安全巧合）——一律按唯一标题或精确 hwnd 关,勿按 pid。
- [ ] **键盘/IME 编码语义余量**（[09 §3.5](09-input-backends-review.md)）：VK/扫描码/扩展键/左右修饰键/目标线程布局未成完整模型；中文输入刚需已由 `--paste` 剪贴板通道覆盖，余量是编码完备性边界（方向键/数字键盘/代理对等）——按真实失败案例收口，不预防性铺
- [ ] **无人值守调度（定位裁定：当前不立项）**：触发层归上层（agent 侧调度），actl 不做 scheduler/常驻（PRD 非目标：触发面板/热键、常驻 agent）；出现真实需求时按 PRD §1.3 另行评审
- [ ] **评测基建提案在案（待裁定，不承诺实施）**：[40](40-automated-testing-evaluation.md) 自动化测试评估系统与 [78](78-comparative-benchmark-suite.md) 三方对比测试集均自述「调研+提案，未实施」（78 含六个待用户裁定开放问题）——与「无人值守调度」同款定位裁定形态，裁定期满未启动再明确关闭
- [ ] **显示层 spec 小项**：device 字段设备粒度折叠（[11 §15.4](11-desktop-interaction-and-display-research.md)「随下一个 spec 批次」）、演示速度配置（[19](19-history-and-user-experience.md) 未实现）

## 维护

- [ ] 显示端按钮触发的暂停/继续 request_id 核对
- [ ] 非 UIA 观察阶段的耗时诊断表达
- [ ] 存储目录不可写/磁盘耗尽/进程中断验收

---

1. 完成的实现从本文件移出，写入 CHANGELOG
2. 新增范围先核对 PRD 非目标
3. 优先修可靠性与补验收，不以铺命令数量代替业务结果

## 2026-10-06 Word 轮实录（物化第五族验证 + note-typo-010 补跑受阻）
- **物化 Office 族数据点**：空白文档 93 元素双模式输出逐字节一致、交错 A/B 物化 ~1.55x（各族最高）。
- **Word×物化交互问题（待根因，重要）**：① 冷启动 Word 的物化单次调用 >20s 被 UIA 区段杀（放宽 60s 后 open 步 234s 完成——逐节点 rearm 语义在冷启动上更耐受）；② focus 步在物化模式 NOT_ACTIONABLE、legacy 模式即恢复（坐实交互问题）；③ 失败采集快照报「无可用运行时身份」（物化元素的 live RuntimeId 读在 Word 上异常——其余四族快照身份实证正常）。**Word/Office 轮走 ACTL_WALK_LEGACY=1 是当前稳妥形态**；根因（cache-only 元素的 pattern/live 读边界）需专门诊断轮。
- **note-typo-010 补跑（evidence fnv128=2c670b4d）受阻如实保留**：open(234s 冷启动)/ownership/focus 完成（legacy 模式），fill-1 NOT_ACTIONABLE(uncertain) 暂停——Word 实例交互待 flow-evidence 诊断包分析后 flow-resume 续跑；现场（Word 窗口+运行状态）保留。

## 2026-10-06 Word 根因终局（物化回落落地 + 010 阻塞根因闭环）
- **物化拒答自动回落已落地**：Word 的 MSAA 桥在大树/兼容模式文档上报「Malformed Cacheresponse pTreeStructure」——tree_bounded 物化分支捕获该类 provider 错误后自动回落逐节点遍历（stderr 留痕「物化被 provider 拒答,回落逐节点遍历」,真机实证回落生效;legacy 的错误如实上抛不吞）。
- **010 阻塞根因闭环（修正）**：note-typo-010.docx 为**兼容模式文档**——其交互元素 RuntimeId 为空（全新 Word 进程同样报「observed element has no usable runtime identity」,find/snapshot/locate 全被挡,与遍历模式无关）——fill-1 自始被文档类挡住(legacy 同败,一致)。**解锁路径:模板重存为非兼容模式 docx**(下轮:验证模板另存后 010 resume)。
- 空白文档 Word 物化正常(93 元素/逐字节一致/1.55x);actl restore/focus 对暂停运行窗口的拒绝是**守护正确行为**(010 paused 态不乱动,非故障——原生 ShowWindow 可恢复)。

## 2026-10-07 010 续跑实录（null 身份 + P0 看门狗修复后）
- **P0 修复实证**：reset() 恒等式 bug 修复后，fill-2 完成（此前 120s 硬顶误杀）；fill-1/fill-2/open/ownership/focus 全部 completed。
- **null 身份 spec 实证**：Word 打开文档（此前必炸「no usable runtime identity」）205 元素快照成功、12.9s——同现场从不可观察变为可观察。
- **fill-3 新阻塞（如实入档）**：运行时门禁「弹层门禁」把 Word 的**停靠导航窗格**（in-tree 常驻面板，内容为模板标题导航树「笔记 2026-10-04 过程记录 心得」）误判为 unknown 弹层 → popup_pending 拦截 fill-3。**根因：门禁未区分「停靠面板」与「真弹窗」**——需对 in-tree 常驻面板做降噪判定（参照 0.2.1 Win11 空壳降噪先例），或以步骤目标窗格与面板不相交放行。fill-3 本身 ASSARTION_FAILED "normalized text differs"（模板三段填充内容差异——需读文档核对后决定 --retry-step）。**第二实例（2026-10-11 VS Code 轮）**：Find/Replace 面板=常驻 UIA 表面（关闭仅视觉折叠，树面永驻且带 5 按钮），front-report 报 unknown actionable → 门禁挡后续一切写步（结构性死锁；真机否证了 wait --gone 断言路线——已撤销）。两个独立应用同根因 → **降噪 spec 优先级升级**（常驻面板判定是跨应用类问题，不是 Word 特例）。
- Word 侧遗留：此现场导航窗格可通过点击其「关闭」Button 移除（真机关闭后弹保存提示→「不保存」→进程退出——但流程无法依赖此操作（它关掉了文档）。

## 2026-10-07 fill-3 判读轮实录（显示端 blocked 收口）
- **fill-3 挂点初步判读**：trace 轮启动即遇显示端 covered_or_offscreen 拒绝——根因是 **Windows 锁屏界面（LogonUI）覆盖全屏**（WindowFromPoint 任意点命中锁屏窗口）。用户离开桌面后机器自动锁屏——显示端信号条被判定「被盖」，launch/display-start 全部拒发（守护正确行为：锁屏态不注入输入）。
- **010 fill-3 的 type 挂点判读未完成**（需解锁桌面才能复现 trace 轮）；降噪+null 身份+模板 patch 三层修复已在其他轮实证，fill-3 挂点在用户解锁后可一轮 trace 收口。
- **显示端锁屏行为（入档为 quirk 面）**：锁屏后 companion 窗口被盖、lease state=hidden；正确路径是显示端在锁屏期不注入（不自动解锁、不绕过），解锁后自动恢复。

## 2026-10-07 010 攻坚链终局收口
- **五层修复全部落地并独立实证**（note-typo-010 驱动发现的每一层都修了）：① 模板 compatibilityMode 12→15；② null 身份 spec（Word 交互元素无 rid 不炸观察）；③ 门禁停靠面板降噪（导航窗格不误拦）；④ P0 看门狗 reset() 恒等式 bug（>120s 流程不再被误杀）；⑤ 消费分轨+物化回落（Word MSAA 桥缓存损坏自动降级）。
- **010 本身未完成（如实记录）**：fill-1/fill-2 completed 但文档经多轮 Word 重启+手动操作+测试污染后，fill-3 的 verify 断言持续 "normalized text differs"（文档当前内容≠模板预期——现场已被污染，不是代码缺陷）。**正确收口方式：干净起 note-typo-011 全链轮**（等桌面空闲，一次跑完 10 步，验证五层修复的合力）。
- **本轮元收获**：一个受阻的验收轮驱动出五个独立的基础设施修复——这正是「受阻不记通过，如实入档」纪律的价值；每个修复都是真机实证的独立贡献，010 的完成只是它们的合力验证。

## 2026-10-08 012 干净轮收口（010 攻坚链合力验证通过）
- **note-typo-012 全链 completed（证据二进制 fnv128=19f4b6a8）**：环境清零（进程/产物锁/显示端 0.2.1 匹配）后 10 步一次通过——fill-2/fill-3 干净复现坐实 010/011 卡点=环境污染而非代码缺陷；**默认行走模式**（物化观察+live 动作链）focus 步无 NOT_ACTIONABLE——10-06 实录「Word 轮走 ACTL_WALK_LEGACY=1 是当前稳妥形态」更新为：五层修复后默认模式已过全链（legacy 降为逃生舱非必选）；冷启动 UIA 区段杀当日复现一次（exit 125），既有 knob `ACTL_UIA_WATCHDOG_MS=60000` resume 续跑即收。详见 [81 §10](81-word-style-flow.md)。
- **尾段挂起族新暴露点（如实）**：flow-resume 在 envelope 落盘后楔死（CPU 恒零、进程看门狗 120s 后仍存活——推测收尾期对已死 Word provider 的跨进程 Release，与 14944 同族）；已标记终止，残件归 doctor --clean。挂起族条目补此签名。

## 2026-10-08 collect spec 落地与真机验收（虚拟化列表全量收集）
- **`collect` 引擎有界动词落地（spec，[06 §3.4](06-design-blueprint.md)）**：复位到顶 → 每轮 1 LargeIncrement 页滚（语义 ScrollPattern 零注入）→ 沉降 → 新鲜观察 → 差分去重累积 → 空连两轮收敛；items 无 ref 重定向走 find；截断六因如实（max_items/max_rounds/scroll_reset_failed/scroll_pattern_missing/scroll_call_failed/container_rebuilt）。
- **资源管理器 fixture 真机验收通过**：600 文件目录 **600/600 全收**（42 轮空连收敛、底部起步位验证复位到顶）、5 文件短列表 3 轮即 complete（无滚动 no-op 路径健康）、`--max-items 500` 截断如实（34 轮触发）；容器 AMBIGUOUS 双 List 用 `role:List#1` 序数直选。两窗口按精确 hwnd 优雅关闭，fixture 已清。
- **三处真机否证（设计收敛的实证过程，详 CHANGELOG/06）**：① ItemContainer 枚举模式只盖物化窗（17/600 假报耗尽）——不采用；② 起始位置依赖（底部起步误判起始即末尾）——复位消除；③ 滚动位置信号 percent+view≥100 提前一页误报（漏尾 14 条）——完成权威只认空连两轮。**微信 RecyclerListView/网易云 Electron 列表（无 ScrollPattern 或分轨行为不同）的兜底路径真机覆盖待后续应用轮**——本验收靶为注册的资源管理器。
- 顺带实证：scroll UIA 分支方向反转修复（`--down 5` via uia-scroll 物化窗下移 4-5 条）。
