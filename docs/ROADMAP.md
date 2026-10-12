# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。**完成项直接删除整条**（含子项）,不留划线/[x] 收口记录——完成实现的证据归 [CHANGELOG](../CHANGELOG.md),验收细节归 docs/ 与应用 pack,ROADMAP 不承担历史。

## 里程碑

- **当前：M3 = v0.3.0（协议稳定+按需扩展）** · 终点：1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

（当前无——完成项按文件头纪律直接删除,条件触发残留归长期观察项;用户新标记时在此登记）

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
- [ ] Phase 2 人工/专用环境验收子集（[44](44-phase2-acceptance.md) 表格聚合）：观察故障注入五类（空值/截断/控件重建/虚拟化/provider 故障）、监听冗余 hook 失效现场验收（[39](39-flow-observation-recovery.md)）、外部对照 Blender/Photoshop/Minecraft 可用环境
- [ ] 浏览器场景实测余项：①持久 profile 真实站点登录验收待用户选站（[72](72-browser-cdp-integration.md)）；②settle 噪声鲁棒（[16](16-browser-scenarios.md)）；③WebView CUA 检查流程重跑（[71](71-signal-webview-details.md)）；④xhs「展开更多评论」低优先附带
- [ ] 浏览器页内弹层声明式处置（pack 级 allow 清单注入,与 popup_ignores 对称——真实站点误报面出现再立项）
- [ ] 浏览器 CDP 工程余量（[72 §13](72-browser-cdp-integration.md)）：①`--remote-debugging-pipe` spike（消除端口暴露面）；②写入 uncertain 跨进程持久化（对齐 [47](47-execution-recovery.md)）；③UIA 轨路线固化（browser.yml 3 verified/5 manual/2 unverified,随应用轮渐进）
- [ ] word-note-typography 的 popup_ignores 声明路径重验（随下次 Word 轮顺带,不专跑）
- [ ] preview-choice computer-use 真机鼠标点击验收（[46](46-target-preview.md)：两轮 "window is not a usable app window" 失败如实保留）



## 长期观察项

- [ ] **观察步后台慢墙（条件触发项,2026-10-11 登记/release-review-11 根因链副产物）**：wait/verify 等观察命令在后台锚窗的慢 provider 上单次定位 14-22s——前台快拒绝只覆盖输入命令与门禁捕获,读步不激活窗口是设计承诺（docs/06 §7.5.2/§7.7⑦）。触发条件=真机流程反复被观察步慢墙拖垮（超时风暴/分钟级停滞）。候选方案：wait_element 在锚窗非前台时先做原生窗口存在性+轻量探针预筛（不进树）,或 wait 增加锚窗前台前置可选项。不预置。
- [ ] **门禁前台 Electron 慢窗暴露面（条件触发项,2026-10-11 登记）**：release-review-11 首次 runner 死亡走的是前台 ZCode（Electron）树的门禁捕获——前台快拒绝只盖"锚窗在后台"场景,锚窗前台但门禁顺路遍历到**别的前台 Electron 窗**仍可撞 UIA 看门狗。触发条件=第二次同签名死亡。候选方案：门禁捕获限定 app 锚窗口树（不遍历前台窗）或对非锚窗捕获加节点预算。

- [ ] Word 新文档物化拒答边界（2026-10-11 release-review-08 实证）：五层修复治的是兼容模式文档（012 场景），**全新空白文档**上 MSAA 桥仍报 Malformed Cacheresponse→自动回落逐节点（不挂但慢，14.8s 慢扫）——回落兜底是设计的，根因（cache-only 元素边界）诊断轮待正常速度日

- [ ] 探索引擎形态边界与改进候选（[77](77-exploration-s2-acceptance.md)）:①差分前沿语义×无增量应用（XAML 计算器按钮序列点击无 added 元素——前沿语义扩展候选）;②texts 原语无全局键盘形态（focused: 不收,而 CLI type focused: 已验证——texts focused 键支持候选）;③automation_id 歧义场景（对话框控件 id 随宿主变异——记事本 1001 双命中/画图 1148,稳健定位 role+name;引擎歧义收紧策略候选）;④跨进程 picker 形态（记事本另存为+画图打开双证否=同进程 32770,待照片类 Store 应用场景验证 RuntimeBroker 放宽判据）;⑤UCB 真机对照轮（v2 机制已落地冷启动回落,待 quirks 计数积累后对比无统计基线:死候选自动压后/预算下降）

- [ ] companion_unresponsive 复发观察（机制已实测澄清:显示端消息泵 500ms 往返+lease 心跳,历史事件全落污染期——诊断详情见 git 历史与 [41](41-logging-metadata.md)）。复发条件=干净现场再现;届时怀疑方向=显示端 UI 线程被合法工作阻塞 >1s,500ms 泵窗口为固定常量不为猜测调参

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
