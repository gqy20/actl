# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 里程碑

- M2 = v0.2.0（可靠性+验收）· M3 = v0.3.0（协议稳定+按需扩展）· 1.0 正式稳定版

## M2 v0.2.0 — 待收口

出口标准已达标（金任务跨日双百 / NFR / 发布产物核对，见 CHANGELOG）。

- [ ] **非开发机 US3 复现**（唯一阻塞项，需用户提供另一台机器）
- [ ] 综合任务编排：首条 release-review-draft 写段联测待用户授权（[43](43-release-review-draft.md)）
- [ ] 浏览器 CDP 通道收尾：v1 命令族与真机验收已落地（[72](72-browser-cdp-integration.md)、CHANGELOG Unreleased）；余量=flow 通道声明、持久 profile 登录验收、pipe spike

## M3 v0.3.0 — 候选方向

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 指针与输入扩展

- [ ] 框选（mouse_down + move + mouse_up 序列，文件管理器批量选择）
- [ ] 长按（现有 key-down 加 duration 参数）
- [ ] 轨迹拖拽（多路径点+插值，画板/签名类）
- 触摸手势：PRD 非目标区，除非明确需求不做

> 已有指针命令覆盖常见场景（单/右/双/三/中键、from→to 拖拽、悬停、滚动、坐标点击、修饰键组合）。以上为 M3 按需扩展，M2 不加——发版优先于功能扩张。

### 流程与集成

- [ ] 流程扩展：循环、嵌套分支、计划修订迁移
- [ ] WebView2 专项验收（键盘跨原生/网页、屏幕阅读器、多屏 DPI、缺失运行时回退）
- [ ] agent 自驱授权白名单通道（仅真实需求时立项，[32](32-desktop-handoff.md)）
- [ ] MCP 远程访问适配
- [ ] 本地 serve 与增量观察
- [ ] 探索模式（算法主导、决策卡协议）
- [ ] 游戏场景（Minecraft 命令通道）

### 应用深化

- [ ] VS Code 关闭重开完整链（当前：命令面板可定位、type 受护栏保护）
- [ ] Excel 写入闭环（当前：UIA 树空属 Office 慢启动，需 MSAA 路线或等待）
- [ ] Word 启动稳定与完整写入
- [ ] Zotero 导入导出
- [ ] 腾讯会议 draft-date 键盘路线（ctrl+a+paste+tab，QDateEdit set-value 无效）
- [ ] 应用 pack 长尾维护

## 长期观察项

- [ ] UIA 观察长阻塞（60s NOT_FOUND，等复现，[65](65-observation-blocking-diagnosis.md)）
- [ ] UWP 挂起恢复（类配对后疑似绕开，更长放置待证）
- [ ] 前台交接 v9 人工接管/嵌套/多窗口真机矩阵（[62](62-foreground-handoff.md)）

## 维护

- [ ] 显示端按钮触发的暂停/继续 request_id 核对
- [ ] 构建指纹与逐阶段耗时诊断表达
- [ ] 存储目录不可写/磁盘耗尽/进程中断验收

---

1. 完成的实现从本文件移出，写入 CHANGELOG
2. 新增范围先核对 PRD 非目标
3. 优先修可靠性与补验收，不以铺命令数量代替业务结果
