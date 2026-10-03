# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 里程碑

- ~~M2 = v0.2.0 已发布（2026-10-03）~~ · M3 = v0.3.0（协议稳定+按需扩展）· 1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

- [ ] **GUI 操作录制 → YAML**（用户标记高价值；用户用鼠标键盘手动操作一遍应用，系统自动转成可复用流程文件——像 RPA recorder；与探索模式互补——录制是"人演示→YAML"，探索是"agent 自主→YAML"，两者在回写层汇合。Quicker 调研已完成并修正体系（[79](79-quicker-recording-research.md)、[76 §11](76-gui-recording.md)）：录制双产物（初版 YAML + trace 证据文件）、agent 改写轮放 skill 层（重放对比验收）、文本录最终值不录按键序列、钩子回调极简+工作线程反查、坐标兜底用窗口相对坐标；中文文本前置依赖 `--paste` 通道（与探索待办 ③ 合并裁定）；切片重排 R1-R4，spec 决策点清单见 [79 §8](79-quicker-recording-research.md)）
- [ ] **探索模式（用户标记关键）**：S1 已落地（0.2.0）；S2 已实现并真机验收通过（[77](77-exploration-s2-acceptance.md)）；S3 已实现并真机闭环验收通过（[77 S3 增补](77-exploration-s2-acceptance.md)）；**L2 原语已实现（2026-10-03，spec 提交：`explore.texts` 输入原语 + 跨窗 goal 探测，见 [docs/06 L2 增补](06-design-blueprint.md)）——texts 真机通过（[77 L2 增补](77-exploration-s2-acceptance.md)）**——待办：①~~观察有界重试~~（已落地 2026-10-03，发现 #3 收口）；②观察锚点随弹窗转移（对话框内继续动作）与 RuntimeBroker 跨进程选择器的 pid 过滤放宽——**含动作改经快照 ref 身份绑定（`click @eN --snapshot`，docs/31 身份纪律，消除选择器重定位与快照两通道的分叉，发现 #6 的结构性收口）**；③type 的 `--paste` 通道选项（中文 IME）；④路线积累（§7 目标 5 条已 2 条）；⑤L0 命中重放 skill 层流程化、UCB 统计剪枝
- [ ] release-review-draft 写段联测（待用户授权，[43](43-release-review-draft.md)）

### 指针与输入扩展

- [x] 三项全落地并真机通过（0.2.0 已发）：框选/长按/轨迹拖拽，[74](74-pointer-extensions.md)
- 触摸手势：PRD 非目标区，除非明确需求不做

### 流程与集成

- [ ] 流程扩展：循环、嵌套分支、计划修订迁移
- [ ] 浏览器通道余量：flow 级 uncertain 持久化、pipe 传输 spike（`--remote-debugging-pipe`）、`--user-agent` 覆盖（headless UA 壁垒站点）、选择器索引消歧、batch 组合验收
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] 中央任务提示专项验收：真实失败/停止/接管暂停、显示端重启后旧流程恢复、跨屏 DPI 与减少动态设置（[75](75-action-start-notification.md)）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] MCP 远程访问适配
- [ ] 本地 serve 与增量观察
- [ ] 游戏场景（Minecraft 命令通道）

### 应用深化

- [ ] VS Code 关闭重开完整链（当前：命令面板可定位、type 受护栏保护）
- [ ] Excel 写入闭环（当前：UIA 树空属 Office 慢启动，需 MSAA 路线或等待）
- [ ] Word 启动稳定与完整写入
- [ ] Zotero 导入导出
- [ ] 腾讯会议 draft-date 键盘路线（ctrl+a+paste+tab，QDateEdit set-value 无效）
- [ ] 应用 pack 长尾维护

### 遗留验收

- [ ] 非开发机 US3 复现（用户裁定 2026-10-02 搁置——作为已知限制已记录于 0.2.0 CHANGELOG；机器到位后补验）

## 长期观察项

- [ ] UIA 观察长阻塞及提供程序超时定位（60s NOT_FOUND 仍待复现；2026-10-02 已捕获桌面 children 枚举 TIMEOUT，故障提供程序与修复仍待确认，见 [41](41-logging-metadata.md)、[65](65-observation-blocking-diagnosis.md)）
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）

## 维护

- [ ] 显示端按钮触发的暂停/继续 request_id 核对
- [ ] 非 UIA 观察阶段的耗时诊断表达（~~构建指纹~~已落地 2026-10-03：`cargo xtask evidence` 产 FNV-128 二进制指纹并落 `target/evidence/binary-log.txt`）
- [ ] 存储目录不可写/磁盘耗尽/进程中断验收

---

1. 完成的实现从本文件移出，写入 CHANGELOG
2. 新增范围先核对 PRD 非目标
3. 优先修可靠性与补验收，不以铺命令数量代替业务结果
