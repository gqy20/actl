# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

1. 完成的实现从本文件移出，写入 CHANGELOG
2. 新增范围先核对 PRD 非目标
3. 优先修可靠性与补验收，不以铺命令数量代替业务结果

本文件只维护剩余工作与优先级。**完成项直接删除整条**（含子项）,不留划线/[x] 收口记录——完成实现的证据归 [CHANGELOG](../CHANGELOG.md),验收细节归 docs/ 与应用 pack,ROADMAP 不承担历史。

## 里程碑

- **当前：M3 = v0.3.0（协议稳定+按需扩展）** · 终点：1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

（当前无——完成项按文件头纪律直接删除,条件触发残留归长期观察项;用户新标记时在此登记）

### 流程与集成

- [ ] 流程扩展（嵌套分支——**缓办**，无真实场景，出现再立项；循环已落地 for_each 形态，2026-10-11）
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] 中央任务提示余项：接管暂停（人工接管→继续→业务核验）、跨屏 DPI 与真实减少动态设置（[75](75-action-start-notification.md)）；急停余量：急停落于 resume 重查窗口内的严格时点、飞行中调用 ABORTED 实测、OS 重启后急停语义保持（[35](35-stop-lifecycle.md)/[39](39-flow-observation-recovery.md)，2026-10-12 真按钮轮已收主链：菜单路径停止记录五字段/暂停期不完成/旧代次 35ms ABORTED/修复解除/失败通知含中文名目视两验）；显示层 UX：详情窗「继续/急停」视觉权重差与修复操作无可见反馈（2026-10-12 观察，两起实证同源于该窗）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] 决策模型任务模式闭环（仅真实需求时立项，[08 §3.2/§13](08-agent-cli-jev-design-proposal.md)）——**应答层全形态已落地**（2026-10-13:`actl decide` 五输入通道 + `flow-run --auto-decide` 探索内环自动应答,后者真机 A/B 双臂验收:confident 自动续跑至 goal_met/门槛拦下即停 decision_pending 交回人,审计 responder=auto_decide;危险候选构造前剥离、弃权/停止/暂停/预算四停机制,证据见 CHANGELOG/06 §7.9/PRD §1.3）。剩余仅任务模式闭环:外置层与 auto-decide 积累的应答数据是立项/否决的依据;弱完成证据不采纳立场不变
- [ ] MCP 远程访问适配——剩余：**跨设备真实客户端接入验收**（等用户场景）+ 模式 B（SSH/stdio）与 tasks 扩展（可选轮 3）;已建成部分见 [82](82-mcp-adapter-research.md) / [06 §7.8](06-design-blueprint.md)
- [ ] 本地 serve 与增量观察
- [ ] 游戏场景（Minecraft 命令通道）——含指针原语缺口（[17 §3](17-game-scenarios.md) M3 候选，「手工搭」路线才需要）：`mouse-move dx,dy` 增量移动（前置 GLFW raw-input spike）、鼠标按键 down/up 拆分带时长（按住挖掘）、节奏敏感动作时窗参数

### 应用深化
- [ ] 浏览器扩展操作面余量（UIA 轨三批已沉淀 [16 §5.5](16-browser-scenarios.md) / browser.yml `extension_surfaces`）：①右键菜单扩展项复验（机制在，两次被用户桌面并发占用护栏正确拒绝，待空闲）；②扩展弹层内写动作成链（现存证据只覆盖「读」）。②原「CDP 自有会话扩展操作实测」已收（2026-10-12 探测:扩展 headless 存活+chrome 层扩展按钮/面板 UIA 全物化可 Invoke,「chrome 层无实测证据」空档闭合,余量并入①③——写动作成链仍待轮）
- [ ] 应用 pack 长尾维护

### 遗留验收

- [ ] 非开发机 US3 复现（用户裁定 2026-10-02 搁置——作为已知限制已记录于 0.2.0 CHANGELOG；机器到位后补验）
- [ ] Phase 2 人工/专用环境验收子集（[44](44-phase2-acceptance.md) 表格聚合）：观察故障注入五类（空值/截断/控件重建/虚拟化/provider 故障）、监听冗余 hook 失效现场验收（[39](39-flow-observation-recovery.md)）、外部对照 Blender/Photoshop/Minecraft 可用环境
- [ ] 浏览器场景实测余项：①持久 profile 真实站点登录验收待用户选站（[72](72-browser-cdp-integration.md)）；②settle 噪声鲁棒（[16](16-browser-scenarios.md)）；③WebView CUA 检查流程重跑（[71](71-signal-webview-details.md)）；④xhs「展开更多评论」低优先附带
- [ ] 浏览器页内弹层声明式处置（pack 级 allow 清单注入,与 popup_ignores 对称——真实站点误报面出现再立项）
- [ ] 浏览器 CDP 工程余量（[72 §13](72-browser-cdp-integration.md)）：UIA 轨路线固化（browser.yml 3 verified/5 manual/2 unverified,随应用轮渐进）；深水层终态吸收已完成（2026-10-13:全动词上骨干,cdp.rs/tungstenite 退役,证据 docs/72 §13）
- [ ] word-note-typography 的 popup_ignores 声明路径重验（随下次 Word 轮顺带,不专跑）
- [ ] preview-choice computer-use 真机鼠标点击验收（[46](46-target-preview.md)：两轮 "window is not a usable app window" 失败如实保留）
- [ ] decide front 判读真实信封低置信形态的使用观察（2026-10-13 双态实测:常驻表面/崩溃框方向全对但真实信封 conf 0.32-0.37<0.5 门槛,日常形态=弃权+top_choice 照报——owned 层只带标题无全文的信号差;若实际使用过保守,候选方案按优先序:①front-report 增强 owned 子窗文本/按钮枚举——**已实现(2026-10-13,EnumChildWindows 直采,A/B 实证同崩溃框 conf 0.32→0.622 跨门槛)**;②判读咨询位局部降门槛;③带 in_tree 全文的调用形态;不动全局阈值;观察残留=Explorer 系新式另存为的子窗是浏览器壳噪声,判读仍走 unknown 属诚实行为）



## 长期观察项

- [ ] **观察步后台慢墙（条件触发项,2026-10-11 登记/release-review-11 根因链副产物）**：wait/verify 等观察命令在后台锚窗的慢 provider 上单次定位 14-22s——前台快拒绝只覆盖输入命令与门禁捕获,读步不激活窗口是设计承诺（docs/06 §7.5.2/§7.7⑦）。触发条件=真机流程反复被观察步慢墙拖垮（超时风暴/分钟级停滞）。候选方案：wait_element 在锚窗非前台时先做原生窗口存在性+轻量探针预筛（不进树）,或 wait 增加锚窗前台前置可选项。不预置。
- [ ] **门禁前台 Electron 慢窗暴露面（条件触发项,2026-10-11 登记）**：release-review-11 首次 runner 死亡走的是前台 ZCode（Electron）树的门禁捕获——前台快拒绝只盖"锚窗在后台"场景,锚窗前台但门禁顺路遍历到**别的前台 Electron 窗**仍可撞 UIA 看门狗。触发条件=第二次同签名死亡。候选方案：门禁捕获限定 app 锚窗口树（不遍历前台窗）或对非锚窗捕获加节点预算。
- [ ] **浏览器窗定位/身份回放慢墙（条件触发项,2026-10-13 登记/Broadcom 门户轮实测）**：Chromium 窗(587 节点)上元素命令步均 ~2s 量级——9 步读 batch 总 12.4s(步均 1.2-2.1s),actl-slow 两次告警 locate.ref-replay 2.0s/4.3s;`@eN`+`--snapshot` 身份回放与 id: 快路径步同量级,热点未分相(疑点:ref 回放的 host 祖先走/verify 回读/定位本体)。触发条件=真实流程被浏览器窗慢墙拖垮(对照:原生应用步均百 ms 级)。处置顺序:先 ACTL_TRACE 分相取证再设计——同进程 batch 内同 snapshot 重复校验短路属树级缓存评估(本节已挂起项)的同族雷区,失效判定弱化违背 fail-closed,不预置方案。
- [ ] **几何事实外露（观察项,2026-10-13 登记/坐标系修复 Fix A/B 的遗留观察面,docs/83 §8）**：坐标单位已全进程统一物理 + `xyw:`/`--wrect` 消灭主链换算,剩余的跨窗/跨屏推理仍要 agent 自己拼装事实。候选:①find-image evidence 附 `scale`(渲染图 vs 物理面比)与客户区原点;③drag 端点的 xyw 形态(现 PROTOCOL 拒,click 族已通)。②已完成(2026-10-14:list-windows 附 monitors[] 拓扑,EnumDisplayMonitors+DPI+主屏,显示配置变更 diff 检测,证据 CHANGELOG)。触发条件=agent 在多屏/缩放场景真实拼装几何事实的第二次现场(Fix B 已消灭单窗场景的拼装需求,跨屏场景密度待观察)。
- [ ] **find --property 逐元素重定位慢墙（条件触发项,2026-10-14 登记/微信搜一搜收割轮实证）**：读批对每个命中元素走完整 locate+read（每条 ~0.5s,Chromium 树 89 元素实测 47s；代码注释自认「每元素一次定位+读」）——而 collect_matches 手里已有元素句柄,属性读被重新定位吃掉。注:此前记录的「0 匹配」是解析侧读错键名（信封键是 elements 非 matches）,语义本身按设计工作（read_error 降级不毁全批）。触发条件=大批量读批成为高频路径。候选方案:capture 阶段持句柄直读属性（观察缓存预取的 role/name 同款快路）,或快路径命中复用 provider 侧属性条件一次往返。

- [ ] Word a11y 缓存回归（疑 Office 更新,2026-10-12 定性）:Malformed Cacheresponse pTreeStructure 跨文档类全量触发——原生 Word 模板/外部最小 OOXML/新空白文档三证(body_read 的 Document text 双通道空+物化拒答→逐节点回落,143 页 walk 6.9s)。时间线:body_read 2026-10-04 verified 于 16.0.20326,现 16.0.20430 失效——Office 更新回归嫌疑,与 Excel 两态探针同款剧本;word.yml body_read 已降 manual+quirk 入册。复验条件=Office 修复后版本复测;候选工程已落地(2026-10-12:snapshot/find 信封 `materialization` 字段=subtree_cache/node_by_node_fallback/node_by_node_legacy,回落态文本通道空自此信封自解,docs/06 §3.7)

- [ ] 探索引擎形态边界与改进候选（[77](77-exploration-s2-acceptance.md)）:④跨进程 picker 形态（记事本另存为+画图打开双证否=同进程 32770,待照片类 Store 应用场景验证 RuntimeBroker 放宽判据）;⑤UCB 真机对照轮（v2 机制已落地冷启动回落,待 quirks 计数积累后对比无统计基线:死候选自动压后/预算下降）。②③①已收（2026-10-12:②texts `focused:` 键 dangerous 永不自动、应答显式裁决;③frontier 同帧重复 id `#N` 序数唯一化;①tried 资格化过滤——dead/no_effect 出局、**reveals 保持可重选**,XAML 按钮序列不再「应答七后前沿空」,MockHost 序列测试 clicks=[b1,b2,b1] 实证）

- [ ] companion_unresponsive 复发观察（机制已实测澄清:显示端消息泵 500ms 往返+lease 心跳,历史事件全落污染期——诊断详情见 git 历史与 [41](41-logging-metadata.md)）。复发条件=干净现场再现;届时怀疑方向=显示端 UI 线程被合法工作阻塞 >1s,500ms 泵窗口为固定常量不为猜测调参

- [ ] UIA 观察长阻塞的**根因定位**（防护已落地：进程级看门狗 + UIA 区段看门狗双保险；根因——60s NOT_FOUND 未复现、故障提供程序未确认，见 [41](41-logging-metadata.md)、[65](65-observation-blocking-diagnosis.md)。**2026-10-13 僵尸 9936 新实证**：另一会话 `get` 挂在对活跃 Chrome 的 UIA RPC 上——诊断 `WaitReason=UserRequest`/ThreadCount=1（看门狗线程已消亡）但主线程 COM 等待未解除、进程存活 25min+，TerminateProcess 未收割、taskkill/Stop-Process 全部无效（用户态无权威可强拆等待中线程）——看门狗已开火但未收尸的形态，根因归此项；对端浏览器常驻不关=无自然解药，镜像 delete-pending 锁波及构建与 lint-packs。**缓解已机制化**：xtask lint-packs 走 CARGO_TARGET_DIR（隔离目录=开发循环标准逃生通道，gate 已实证不受主 target 锁影响））
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）

- [ ] **树级缓存（「第二次观察免费」）——设计再评估后挂起（2026-10-06）**：原设想以窗口 RuntimeId 键控缓存已观察树 + 失效判定。设计推演结论:① 节点级缓存无 COM 代理 → 无法做 locate 缓存式的元素级 RuntimeId 复核,失效判定只剩 WinEvent 钩子(需消息泵线程,工程量大)或有界 TTL(陈旧风险违背 fail-closed);② 主要重复观察方（探索引擎/flow verify）**语义上就需要新鲜帧**（差分/断言）,缓存反而破坏语义;真实重复收益集中在 batch 的连续 find/snapshot——频次低。**结论:价值窄于最初评估,等真实重复消费方出现再立项**;轻量替代（定位缓存/属性预取/provider 快路径）已覆盖主要痛点。
- [ ] **键盘/IME 编码语义余量**（[09 §3.5](09-input-backends-review.md)）：VK/扫描码/扩展键/左右修饰键/目标线程布局未成完整模型；中文输入刚需已由 `--paste` 剪贴板通道覆盖，余量是编码完备性边界（方向键/数字键盘/代理对等）——按真实失败案例收口，不预防性铺
- [ ] **无人值守调度（定位裁定：当前不立项）**：触发层归上层（agent 侧调度），actl 不做 scheduler/常驻（PRD 非目标：触发面板/热键、常驻 agent）；出现真实需求时按 PRD §1.3 另行评审
- [ ] **评测基建提案在案（待裁定，不承诺实施）**：[40](40-automated-testing-evaluation.md) 自动化测试评估系统与 [78](78-comparative-benchmark-suite.md) 三方对比测试集均自述「调研+提案，未实施」（78 含六个待用户裁定开放问题）——与「无人值守调度」同款定位裁定形态，裁定期满未启动再明确关闭
## 维护

- [ ] 窗口锚定参数一致性（2026-10-13 Broadcom 轮烧掉一次 PROTOCOL 往返）：同一窗口三态并存——snapshot 双锚(`--app`+`--hwnd`)、元素命令单锚(`--app`,含 `hwnd:<n>` 形态)、focus-window 位置参数+`--pid/--hwnd`。统一锚面或在各命令 help 交叉提示;命令面变更走 spec。
- [ ] docs/40 §6/§7 虚拟化与云端事实回写（2026-10-13 复核,纯 docs）：Workstation Pro 免费政策已扩大到全用途(2024-11-11 起,非仅个人;版本改日历命名现 26H1u1,vctl 移除/vmrun 全保留);Win11 guest 安装期 NAT 卡"连接网络",官方 workaround=改 Bridged;Enterprise Evaluation 到期=每小时自动关机(非仅通知),rearm 无官方支持,按 90 天生命周期换 ISO 管理;微软预构建开发 VM 下载页已下线;WAA 实际形态=Linux Docker 嵌 QEMU 跑完整 Win11(非 Windows 容器);§7「云端 Windows 沙箱暂不存在」已过时——Daytona(Windows VM+内存快照/fork)、Cua(CUA 专用沙箱)2026 年已出现,复查期提前到期。
- [ ] 非 UIA 观察阶段耗时诊断（2026-10-12 核对轮收敛:trace::scope+actl-slow 慢段机制已在位,覆盖=uia 轨 8 文件;**非 UIA 段（browser CDP 网络/spawn/文件 IO）无 scope**——真实慢诊断需求出现时按同款 scope 补,不预防性铺）
- [ ] 存储验收余项：磁盘耗尽注入验收（不可写注入契约与进程中断真杀专测均已收——2026-10-12:flow_contract `real_kill_midstep` 以 TerminateProcess 中段杀 runner, WAL running 态如实报 interrupted 且 resume 闭合诚实失败态；CLI resume control_log 对称同轮落地，`cli_resume_writes_control_log` 三态断言）
