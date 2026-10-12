# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。**完成项直接删除整条**（含子项）,不留划线/[x] 收口记录——完成实现的证据归 [CHANGELOG](../CHANGELOG.md),验收细节归 docs/ 与应用 pack,ROADMAP 不承担历史。

## 里程碑

- **当前：M3 = v0.3.0（协议稳定+按需扩展）** · 终点：1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

- [ ] **探索模式（用户标记关键）**：S1–S3/L2/先验飞轮全链真机通过（[77](77-exploration-s2-acceptance.md),已收口部分见 CHANGELOG）。§7 路线积累 5 条目标已达成（S1/S2+#3 记事本+#4 资源管理器 verified;#5 计算器以形态边界实证计入贡献,两改进候选在案）。剩余两项均为**条件触发**:①**UCB 统计剪枝**——等 pack quirks 积累出样本量后评估（v1 显式数据链已落地:quirk 片段+suggested_priors 注入,docs/42 §11.4）;②**跨进程 picker 形态观察**——Store 记事本另存为已证否（同进程 32770,轮 C 实证）,待其他 Store 应用出现 FileOpenPicker 类真跨进程形态时验证 RuntimeBroker 放宽判据

### 流程与集成

- [ ] 流程扩展（嵌套分支——**缓办**，无真实场景，出现再立项；循环已落地 for_each 形态，2026-10-11）
- [ ] 浏览器通道余量：flow 级 uncertain 持久化、pipe 传输 spike（`--remote-debugging-pipe`）、batch 组合验收
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] 中央任务提示专项验收：真实失败/停止/接管暂停、显示端重启后旧流程恢复、跨屏 DPI 与减少动态设置（[75](75-action-start-notification.md)）；并轨补入：急停真实按钮 + 跨进程解除完整交互（[35](35-stop-lifecycle.md)/[39](39-flow-observation-recovery.md) 尾自 2026-09-28 悬置——真实按钮路径待验收，与真实停止项同轮收口）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] Jev 任务模式闭环（仅真实需求时立项，[08 §13 增补](08-agent-cli-jev-design-proposal.md)——third-hand 实证对照、弱完成证据不采纳的立场与切入点修正：增量在探索决策步/AMBIGUOUS 消歧/任务闭环，不在 grounding；前置零件 AMBIGUOUS context 与 `find --capable` 已落地，候选工程先于模型）
- [ ] MCP 远程访问适配——剩余：**跨设备真实客户端接入验收**（等用户场景）+ 模式 B（SSH/stdio）与 tasks 扩展（可选轮 3）;已建成部分见 [82](82-mcp-adapter-research.md) / [06 §7.8](06-design-blueprint.md)
- [ ] 本地 serve 与增量观察
- [ ] 游戏场景（Minecraft 命令通道）——含指针原语缺口（[17 §3](17-game-scenarios.md) M3 候选，「手工搭」路线才需要）：`mouse-move dx,dy` 增量移动（前置 GLFW raw-input spike）、鼠标按键 down/up 拆分带时长（按住挖掘）、节奏敏感动作时窗参数

### 应用深化

- [ ] Excel 写入闭环（当前：UIA 树空属 Office 慢启动，需 MSAA 路线或等待）
- [ ] Zotero 导入导出
- [ ] 腾讯会议 draft-date 键盘路线（ctrl+a+paste+tab，QDateEdit set-value 无效）——2026-10-11 排期尝试，**本机未安装 wemeet（注册表 App Paths/常见 Program Files/本地 Programs 均无）如实记录不可验收**，待有环境机器轮
- [ ] 浏览器扩展操作面余量（UIA 轨三批已沉淀 [16 §5.5](16-browser-scenarios.md) / browser.yml `extension_surfaces`）：①右键菜单扩展项复验（机制在，两次被用户桌面并发占用护栏正确拒绝，待空闲）；②CDP 自有会话扩展操作实测（persistent profile 保留扩展策略已定 [72](72-browser-cdp-integration.md)，chrome 层归 UIA 的路径无实测证据，chrome 层 Invoke E_FAIL 同款风险预期）；③扩展弹层内写动作成链（现存证据只覆盖「读」）
- [ ] 应用 pack 长尾维护

### 遗留验收

- [ ] 非开发机 US3 复现（用户裁定 2026-10-02 搁置——作为已知限制已记录于 0.2.0 CHANGELOG；机器到位后补验）
- [ ] Phase 2 人工/专用环境验收子集（[44](44-phase2-acceptance.md) 表格聚合）：观察故障注入五类（空值/截断/控件重建/虚拟化/provider 故障——[50](50-deep-container-query.md) 的 provider 重建待验收同族）、监听冗余 hook 失效现场验收（[39](39-flow-observation-recovery.md)）、外部对照 Blender/Photoshop/Minecraft 可用环境。注：provider 故障路径已有物化回落真机实证（Word MSAA 拒答自动降级,CHANGELOG）,正式表格对照仍待
- [ ] 浏览器场景实测余项聚合：持久 profile 真实站点登录验收待用户选站（[72](72-browser-cdp-integration.md)）；settle 噪声鲁棒（[16](16-browser-scenarios.md) 登记 M3 候选未动）；WebView CUA 检查流程重跑（[71](71-signal-webview-details.md)——那次 CDP 连接超时的**孤立性已由后续多轮 headless/fixture CDP 全链成功间接核验**,剩检查流程本身）；xhs「展开更多评论」低优先附带
- [ ] 浏览器页内弹层声明式处置（pack 级 browser 弹层 allow 清单注入,与 popup_ignores 对称——引擎级自动门禁已落地,真实站点误报面出现再立项,不为猜测建基建）
- [ ] **浏览器 CDP 工程余量（[72 §13](72-browser-cdp-integration.md) 聚合）**：`--remote-debugging-pipe` 实现形态 spike（当前 port+127.0.0.1，pipe 消除端口暴露面）；写入 uncertain 跨进程持久化（对齐 [47](47-execution-recovery.md) 恢复语义）；UIA 轨路线固化（browser.yml 10 条中 3 verified/5 manual/2 unverified——随应用轮渐进，pack 治理日常非协议项）
- [ ] **flow 工具链缺口聚合（①revise 模板形态/②handlers 锚引用已收口,见 CHANGELOG）——剩余③④**：③ **companion_unresponsive 复发观察（暂缓;机制已实测澄清,2026-10-12）**——真实机制是**显示端消息泵 500ms SendMessageTimeout 往返**（CHECK 类+1 重试,历史事件均先记 handoff_query_retry 再失败）+ lease 心跳 1500ms 新鲜度,**不存在**"resume 累计时长超 lease 窗口"（原始登记与首次重写均误述,数据修正）。历史 20+ 事件全落在污染期（09-29→10-11 上午）;根杀后唯一一次（ts 1791713306）已归因=display-stop 轮对显示端的 close-window 被正确 fail-closed,非复发。归因维持:污染期系统饱和饿死显示端 UI 线程。**复发条件**：干净现场再现 companion_unresponsive——届时怀疑方向=显示端 UI 线程被合法工作（绘制/消息处理）阻塞>1s,非污染;500ms 泵窗口为固定常量,不为猜测调参。④ word-note-typography 的 popup_ignores（导航）声明路径重验随下次 Word 轮顺带（不专跑）
- [ ] Zotero 导入导出（2026-10-11 轮让行未跑——用户桌面活跃，GUI 启动让行）
- [ ] preview-choice computer-use 真机鼠标点击验收（[46](46-target-preview.md)：两轮 "window is not a usable app window" 失败如实保留）

<!-- 2026-10-05 收口移出：录制值读回 #4 双轨收口（rec-ma-12 证据二进制轮，[80](80-record-acceptance.md)）；健壮性基建 009 轮 10 步 155s 全链通过（[81 §9](81-word-style-flow.md)）；网易云收起方向识图证实（pack 同步，Table 树滞留非未收起）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-08 收口移出：Word 流程 evidence 二进制补跑（note-typo-012 干净轮 10 步全链 completed，fnv128=19f4b6a8，五层修复合力验证通过，[81 §10](81-word-style-flow.md)）；虚拟化列表全量收集动词（`collect` spec 落地+资源管理器 600 文件 fixture 真机验收通过，[06 §3.4](06-design-blueprint.md)）；flow 歧义候选串联（`--pick` 运行时候选纠正 spec 落地，[06 AMBIGUOUS 目标串联](06-design-blueprint.md)/[63 收口](63-simple-workflow-conditions.md)；自动缩小查询裁定不做入册）；选区读取原语（`get-selection` spec 落地+记事本真机验收通过：Text/Selection 双通道、caret 退化范围诚实 no_selection，[06 系统组](06-design-blueprint.md)）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：执行器错误原文保留（metadata 证据附有界 message，[68 待办](68-combined-batch-first-round.md)勾销）；flow 锚点标题插值（**降格收口**为 verify-owner `hwnd_anchor` 就绪字符串+bind 惯用法——HWND 不随标题变，不做插值 spec，[06 verify-owner](06-design-blueprint.md)；真机部分验收：改名失配后 hwnd 锚仍寻址同窗，完整链待桌面空闲轮）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：流程扩展循环（`for_each` 数据驱动有界迭代 spec 落地，[06 for_each](06-design-blueprint.md)/[63 收口](63-simple-workflow-conditions.md)；通用 while/嵌套分支缓办入册）；真机验收待首个消费场景（机制层验收=纯函数 7 用例+flow-validate 契约）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：browser-read --all 列表枚举形态（spec 落地+本地 fixture 真机复验+B 站首个真实场景消费，原待办行删除线清理）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：popup notice 词表裁定（**最窄形态落地**——popup_handlers `classification:notice`，name_contains 必填按名认领 INFO 级 unknown 表面，未声明的 unknown 仍永远上抛、已分类面板不被吞并；[06 运行时治理③](06-design-blueprint.md)；机制层验收=popup_gate 13 用例，真机验收待 VS Code 轮探测通知文本+按钮 name 回填模板声明）；浏览器页内弹层检测原语（`browser-verify --no-popups` v1 落地，headless fixture 六假设实证；引擎级自动门禁留候选待事故）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：VS Code 关闭重开完整链（第四轮 22/26→降噪第二轮 v4 认领后尾段 A/B 收口,run 25/26——close 步 bind 陈旧系操作者清窗连锁非机制缺陷；focus 原语与 notice handler 自动处置同轮探针真机验收；Find 面板常驻表面死锁=门禁降噪第二实例,直接驱动 popup_ignores spec 同日落地；应用深化区旧版重复条目一并删除）；尾段挂起族（06 定位→06 主修复(Drop 家族收编)→08 新签名(flow-resume 成功路径楔死)→11 根杀收口——envelope flush 后 TerminateProcess 直调绕过一切析构阻塞面+signal worker detach,真机实证 403 窗僵尸清零;长期观察项整条移出）——见 CHANGELOG Unreleased。 -->
<!-- 2026-10-11 收口移出：探索模式高优先条目（**docs/42 §7 验收目标 5 条全部达成**——S1/S2+#3 记事本中文+#4 资源管理器后退导航均经人工复核转 verified;#5 计算器形态边界实证计入贡献;轮 C 判据落地+复现轮否定性数据;三条引擎改进候选+UCB+RuntimeBroker 观察转长期观察项——[77](77-exploration-s2-acceptance.md)）——见 CHANGELOG Unreleased。 -->


## 长期观察项

- [ ] **观察步后台慢墙（条件触发项,2026-10-11 登记/release-review-11 根因链副产物）**：wait/verify 等观察命令在后台锚窗的慢 provider 上单次定位 14-22s——前台快拒绝只覆盖输入命令与门禁捕获,读步不激活窗口是设计承诺（docs/06 §7.5.2/§7.7⑦）。触发条件=真机流程反复被观察步慢墙拖垮（超时风暴/分钟级停滞）。候选方案：wait_element 在锚窗非前台时先做原生窗口存在性+轻量探针预筛（不进树）,或 wait 增加锚窗前台前置可选项。不预置。
- [ ] **门禁前台 Electron 慢窗暴露面（条件触发项,2026-10-11 登记）**：release-review-11 首次 runner 死亡走的是前台 ZCode（Electron）树的门禁捕获——前台快拒绝只盖"锚窗在后台"场景,锚窗前台但门禁顺路遍历到**别的前台 Electron 窗**仍可撞 UIA 看门狗。触发条件=第二次同签名死亡。候选方案：门禁捕获限定 app 锚窗口树（不遍历前台窗）或对非锚窗捕获加节点预算。

- [ ] Word 新文档物化拒答边界（2026-10-11 release-review-08 实证）：五层修复治的是兼容模式文档（012 场景），**全新空白文档**上 MSAA 桥仍报 Malformed Cacheresponse→自动回落逐节点（不挂但慢，14.8s 慢扫）——回落兜底是设计的，根因（cache-only 元素边界）诊断轮待正常速度日

- [ ] 探索引擎形态边界与改进候选（2026-10-11 轮 C/#4/#5 实证,[77](77-exploration-s2-acceptance.md)）:①差分前沿语义×无增量应用（XAML 计算器按钮序列点击无 added 元素——前沿语义扩展候选）;②texts 原语无全局键盘形态（focused: 不收,而 CLI type focused: 已验证——texts focused 键支持候选）;③automation_id 歧义场景（对话框文件名框与地址栏共用 id:1001/TextBox——引擎候选歧义收紧策略）;④RuntimeBroker 跨进程 picker 形态待其他 Store 应用场景（另存为=同 pid #32770 已证否）;⑤UCB 统计剪枝（待 quirks 样本量,沿 42 §7 挂起）

- [ ] UIA 观察长阻塞的**根因定位**（防护已落地：进程级看门狗 + UIA 区段看门狗双保险；根因——60s NOT_FOUND 未复现、故障提供程序未确认，见 [41](41-logging-metadata.md)、[65](65-observation-blocking-diagnosis.md)）
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）

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
<!-- 2026-10-06~08 攻坚实录段整体移出（本文件只维护剩余工作——七段 Word 物化/010-012 攻坚链/fill-3 判读/collect 落地的过程实录删除；最终态见 CHANGELOG 对应条目与 [81](81-word-style-flow.md)；Word 物化交互根因诊断（cache-only 元素边界）降为可选——消费分轨+物化回落已工程化、默认模式 012 全链实证；微信/网易云列表兜底待后续应用轮见 CHANGELOG collect 条目）。 -->
