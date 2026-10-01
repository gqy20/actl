# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作、优先级与验收标准。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 范围与里程碑

- [ ] 综合任务编排：按 [流程目录约定](../skills/actl/workflows/README.md) 交付业务 YAML。首条 release-review-draft 写段联测待用户授权（[43](43-release-review-draft.md)）。
- [ ] 浏览器正式 CLI/YAML 接入前补齐人工交接、导航竞态及持久化 uncertain 设计（[34](34-browser-connection-prototype.md)）。
- M2 = v0.2.0（可靠性+验收）；M3 = v0.3.0（协议稳定+按需扩展）。1.0 正式稳定版。

## M2 v0.2.0 出口状态

**已达标的出口标准**：
- ✓ 金任务跨日双百（GT1 10/10 + GT2 10/10，2026-10-01 晚间）
- ✓ NFR 数据已采（冷启动 P50=67ms / snapshot 433ms / 输出体积 <2KB / skeleton 45%）
- ✓ 发布产物核对（gate 全绿 / 5.3MB 零依赖 / 79 契约测试 94 全过 / golden 22 文件）

**唯一未完成项**：非开发机 US3 复现（需用户提供另一台机器）。

## Phase 2 剩余工作

### P0 · 人机协作与恢复验收

- [ ] 定位并限制单次 UIA 观察长阻塞（等复现，不可主动推进；阶段打点已补齐，[65](65-observation-blocking-diagnosis.md)）。
- [x] 状态一致性（2026-10-02：display-status 三态核验 ready/stopped/recover；连续三轮 stop/recover 全通过；伴侣条截图已采但 68px 高度太薄 vision 不可读——以协议级一致性为准）。
- [x] 提示窗场景（2026-10-02：失联恢复已证；隐藏/屏外/多屏 DPI 非阻塞单列）。
- [x] Raw Input（2026-10-02：input_source 活体验证 kind=own_injection；未标记注入可检测；真实硬件接管单列）。
- [x] 显示 v7（2026-10-02：连续三轮 stop/recover 竞态通过 stopped→ready；系统重启/用户关显示端单列）。
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）。

### P0 已完成（2026-10-01/02）

- ✓ UIA 组合定位真机验收（composite-locate 五点全绿）
- ✓ 修复检测/auto_resume 全链真机验收（两判定+预算+急停阻断）
- ✓ once_per_run / 组内暂停 / 任务隔离真机验收（用户协作轮）
- ✓ stale_read/resume_check 真机矩阵（四格收齐）
- ✓ 有界只读恢复六判据
- ✓ 条件模板写入侧三模板（复用/跳过/关闭）
- ✓ agent 代操作通道定案（docs/32）

### P1 · 应用场景与输入可靠性

- [x] 系统输入框边缘（2026-10-02：空值读回 ✓""、清空后空 ✓、截断 false、死窗 NOT_FOUND 如实保留——四格全过）。
- [ ] winapp 双客户端对照（需 Blender/Photoshop/Minecraft 环境——本机未装，需环境）。
- [ ] 深化应用业务闭环（VS Code/Excel/Word 已装可测；Zotero 未装——非 M2 阻塞，按需推进）。
- [x] 键盘 vs 点击比较（2026-10-02 计算器 7×8：点击 5589ms vs 键盘 1519ms——键盘快 3.7 倍，结果均正确；结论：数字输入优先键盘路线，按控件能力选择）。
- [x] 夸克/Zotero 控件重建与读回（2026-10-02：Zotero 快速搜索 set-value+读回 ✓；ctrl+shift+l 侧栏重建 ref 变化后重取快照命中 ✓；UIA 焦点谎报实证 set-value 绕过。quark 已复验通过）。腾讯会议 draft-date 待补。
- [ ] excel paste 中断重放复现（需 Excel 环境构造中断场景——非 M2 阻塞）。

### P1 已完成（2026-10-01/02）

- ✓ 场景轨四场景复验：app-launch-observe / cloudmusic-playback / quark-search / tencent-meeting-draft
- ✓ verify-owner 进程树归属（父链 token 匹配）
- ✓ 选择器纪律盘点（36 处 name_contains：33 低风险 / 3 中风险[zotero 快速搜索] / 0 高风险）
- ✓ 隔离应用实验台 `diagnose app-lab`（诊断动词家族收官）

### P1 补 · 诊断与证据体系

- [ ] verify 族耐心值可调化（spec，按需推进，不主动）。

### P1 补 已完成

- ✓ verify-file IO 失败可区分报错
- ✓ flow-status 陈旧计划提示
- ✓ 交接来源可区分（consent_decision input_source）
- ✓ 续跑 journal 时间线并入
- ✓ 诊断脚本全部沉淀入 devtools（cleanup-tabs / snapshot-diff / focus-report / title-watch / close-gone / app-lab / front-report）
- ✓ front-report 前台弹层报告 + 前台优先纪律
- ✓ 选择器纪律盘点

### P2 · 发布验收

- [x] 金任务跨日双百（GT1+GT2 各 10/10）
- [x] NFR 数据已采（[报告](timing-data/nfr-20261001-evening.md)）
- [x] 发布产物核对
- [ ] 非开发机 US3 复现（需用户提供环境）

### 头部/观察项

- [ ] UWP 挂起恢复（观察项：4 分钟最小化正常；类配对后探针只看宿主窗，原路径疑似绕开）。
- [ ] 夹具简化机会（低优先）。

## Phase 3 · M3 v0.3.0 候选方向

- 流程扩展：循环、嵌套分支、计划修订迁移。
- 应用 pack 长尾维护；list-notifications/surfaces 裁定。
- 本地 serve 与增量观察。
- 录制轨迹、独立环境、复杂面板。
- [ ] WebView2 专项验收（键盘/屏幕阅读器/多屏 DPI/缺失运行时回退）。
- 探索模式（算法主导、决策卡协议）。
- 游戏场景（Minecraft 命令通道）。
- agent 自驱授权白名单通道（[32](32-desktop-handoff.md) 定案：仅真实需求时立项）。
- MCP 远程访问适配。

## 维护规则

- [ ] 显示端按钮触发的暂停/继续 request_id 核对。
- [ ] 构建指纹与逐阶段耗时诊断表达。
- [ ] 存储目录不可写/磁盘耗尽/进程中断验收。

## 维护规则

1. 完成的实现从本文件移出，写入 CHANGELOG。
2. 新增范围先核对 PRD 非目标。
