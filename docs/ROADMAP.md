# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 里程碑

- **当前：M3 = v0.3.0（协议稳定+按需扩展）** · 终点：1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

- [ ] **探索模式（用户标记关键）**：S1–S3、L2 原语与先验飞轮已全部真机通过（[77](77-exploration-s2-acceptance.md)）。待办：①~~观察锚点随弹窗转移~~（2026-10-05 已落地并真机验收,[77 锚点转移增补](77-exploration-s2-acceptance.md);剩余 RuntimeBroker pid 放宽待复现）；②type 的 `--paste` 通道选项（中文 IME）；③路线积累（§7 目标 5 条已 2 条）；④L0 命中重放 skill 层流程化、UCB 统计剪枝
- [ ] release-review-draft 写段联测（待用户授权，[43](43-release-review-draft.md)）

### 流程与集成

- [ ] 流程扩展（余项）：循环、嵌套分支
- [ ] 浏览器通道余量：flow 级 uncertain 持久化、pipe 传输 spike（`--remote-debugging-pipe`）、`--user-agent` 覆盖（headless UA 壁垒站点）、选择器索引消歧、batch 组合验收
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] 中央任务提示专项验收：真实失败/停止/接管暂停、显示端重启后旧流程恢复、跨屏 DPI 与减少动态设置（[75](75-action-start-notification.md)）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] MCP 远程访问适配（轮 1-2 已实现并活体验证：[82](82-mcp-adapter-research.md) / [06 §7.8](06-design-blueprint.md)；**剩余：跨设备真实客户端接入验收 + 模式 B（SSH/stdio）与 tasks 扩展（轮 3 可选）**）
- [ ] 本地 serve 与增量观察
- [ ] 游戏场景（Minecraft 命令通道）

### 应用深化

- [ ] VS Code 关闭重开完整链（当前：命令面板可定位、type 受护栏保护）
- [ ] Excel 写入闭环（当前：UIA 树空属 Office 慢启动，需 MSAA 路线或等待）
- [ ] Zotero 导入导出
- [ ] 腾讯会议 draft-date 键盘路线（ctrl+a+paste+tab，QDateEdit set-value 无效）
- [ ] 应用 pack 长尾维护

### 遗留验收

- [ ] 非开发机 US3 复现（用户裁定 2026-10-02 搁置——作为已知限制已记录于 0.2.0 CHANGELOG；机器到位后补验）
- [ ] Word 流程 evidence 二进制补跑 note-typo-010（009 真机验收已收口，[81 §9](81-word-style-flow.md)；此为指纹级补强，桌面空闲时执行）

<!-- 2026-10-05 收口移出：录制值读回 #4 双轨收口（rec-ma-12 证据二进制轮，[80](80-record-acceptance.md)）；健壮性基建 009 轮 10 步 155s 全链通过（[81 §9](81-word-style-flow.md)）；网易云收起方向识图证实（pack 同步，Table 树滞留非未收起）——见 CHANGELOG Unreleased。 -->

## 长期观察项

- [ ] UIA 观察长阻塞的**根因定位**（防护已落地：进程级看门狗 + UIA 区段看门狗双保险；根因——60s NOT_FOUND 未复现、故障提供程序未确认，见 [41](41-logging-metadata.md)、[65](65-observation-blocking-diagnosis.md)）
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）

## 维护

- [ ] 显示端按钮触发的暂停/继续 request_id 核对
- [ ] 非 UIA 观察阶段的耗时诊断表达
- [ ] 存储目录不可写/磁盘耗尽/进程中断验收

---

1. 完成的实现从本文件移出，写入 CHANGELOG
2. 新增范围先核对 PRD 非目标
3. 优先修可靠性与补验收，不以铺命令数量代替业务结果
