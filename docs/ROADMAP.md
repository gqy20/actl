# ROADMAP · 路线图

关联：[PRD](PRD.md) · [设计蓝图](06-design-blueprint.md) · [技术选型](TECH_STACK.md)

本文件只维护剩余工作与优先级。已完成变更统一见 [CHANGELOG](../CHANGELOG.md)。

## 里程碑

- ~~M2 = v0.2.0 已发布（2026-10-03）~~ · M3 = v0.3.0（协议稳定+按需扩展）· 1.0 正式稳定版

## M3 v0.3.0 — 当前阶段

按需推进，不承诺全部交付；实施前明确边界并核对 PRD 非目标。

### 高优先（用户标记）

- [ ] **GUI 操作录制 → YAML**（用户标记高价值；用户用鼠标键盘手动操作一遍应用，系统自动转成可复用流程文件——与探索模式互补：录制是"人演示→YAML"，探索是"agent 自主→YAML"，两者在回写层汇合。体系见 [79](79-quicker-recording-research.md)、[76 §11](76-gui-recording.md)。**R1–R4 已实现且两条真机轨全部通过（[80](80-record-acceptance.md)，2026-10-04）：注入轨四发现（DPI 口径/LLKHF 位/Text 回退/读回竞态）+ 演示轨发现 #5（yaml 裸数字）全部修复；人工演示轮实证 R4 右键链路、43 真人输入全捕获、IME 值读回→`type --paste`、wait 步骤与三窗口泛化；R3 首次真实改写（最小机械修正→flow-validate 通过）。剩余：① ~~发现 #4 复验~~（2026-10-04 收口：真根因是 #11 值读回护栏按标题比对——编辑改脏星号即失配；护栏改 HWND 后 #4 原始形态三轮复验值事件全出，[80 发现 #11](80-record-acceptance.md)；证据二进制正式轮因桌面被用户占用让行，待空闲补记）；② 其他应用路线按需沉淀 pack（utools pack 已落，含 focused 输入通道结论）。~~burst 打字~~已实现（2026-10-04：`focused:` 焦点元素目标语法 + type 单词注入，[06 §3 增补](06-design-blueprint.md)；uTools 实测 916ms/5 字符，011 零前置全链验证，[80 勘误与 v7](80-record-acceptance.md)）**）。改写+重放对比验收已通过（[80 改写重放轮](80-record-acceptance.md)，2026-10-04：v1→v7 七轮重放，发现 #7-#10 全部修复收口，010/011 全链 completed）。
- [ ] **探索模式（用户标记关键）**：S1 已落地（0.2.0）；S2 已实现并真机验收通过（[77](77-exploration-s2-acceptance.md)）；S3 已实现并真机闭环验收通过（[77 S3 增补](77-exploration-s2-acceptance.md)）；**L2 原语已实现（2026-10-03，spec 提交：`explore.texts` 输入原语 + 跨窗 goal 探测，见 [docs/06 L2 增补](06-design-blueprint.md)）——texts 真机通过（[77 L2 增补](77-exploration-s2-acceptance.md)）**——待办：①~~观察有界重试~~（已落地 2026-10-03，发现 #3 收口）；②观察锚点随弹窗转移（对话框内继续动作）与 RuntimeBroker 跨进程选择器的 pid 过滤放宽——**含动作改经快照 ref 身份绑定（`click @eN --snapshot`，docs/31 身份纪律，消除选择器重定位与快照两通道的分叉，发现 #6 的结构性收口）**；③type 的 `--paste` 通道选项（中文 IME）；④路线积累（§7 目标 5 条已 2 条）；⑤L0 命中重放 skill 层流程化、UCB 统计剪枝
- [ ] release-review-draft 写段联测（待用户授权，[43](43-release-review-draft.md)）

### 指针与输入扩展

- [x] 三项全落地并真机通过（0.2.0 已发）：框选/长按/轨迹拖拽，[74](74-pointer-extensions.md)
- 触摸手势：PRD 非目标区，除非明确需求不做

### 流程与集成

- [ ] **运行时治理（2026-10-04 与用户讨论定向：弹层优先机制化 + 突发应对 + 修改后续跑；事故动因见 [80 发现 #11 增补](80-record-acceptance.md)——弹窗滞留两事故与 005-011 六轮全程重跑）**。两轮 spec 弧，依赖有序：
  **第一轮「失败采集与弹层门禁」**：① `failure_bundle`——uncertain 步落定时自动归集：错误 envelope + 失败瞬间弹层快照（front-report）+ 定向元素快照（`app_context` 通道已有，固定进包）+ 环境标记（前台/锁屏/窗口差集）；flow-status 可读，`flow-evidence --id --step` 取包——失败判读从「数条诊断命令」变「读一个包」；② 写步步前弹层门禁——front 层非 clear 且分类 unknown → pause 出 `popup_pending` 偏差卡（复用 needs_review/决策卡机制，不新增状态）；③ 偏差分类 + 默认策略表（flow/pack 可声明 `popup_handlers`：白名单弹窗自处置留痕、unknown 永远上抛；`foreground_lost`=用户让行不记败、瞬态有界自重试、`state_drift`=uncertain+受控调节菜单 retry/skip-step/resume-from/abort）——把 AGENTS 人肉弹层纪律变成引擎语义。
  **第二轮「修订续跑与抗突发写法」**：④ `flow-revise` 计划版本化修订续跑——步骤级 diff（按 id 对齐新增/删除/修改/不变）、修订作为版本事件**追加**（旧计划字节不覆盖，run 历史含版本谱系与指纹链）、已完成步保留条件=规格未变（结果带 `plan_version` 溯源；写步规格变更需 `--accept-rerun` 显式授权）、失败步以修订本身为重试授权、续跑前 freshness/resume_check 重立前置——替代「改模板另开新 id 全程重跑」；⑤ `ensure:` 幂等语义步骤——环境准备先验证满足则跳过、不满足才执行，重跑/续跑前缀成本趋零（v7 的 launch/清空步是手写雏形）。
  设计边界：调节粒度=步边界（不做指令级打断，注入中急停由停止代次兜底）；冻结计划的四不变量（结果溯源/无静默变更/写步不盲重放/重试先重立前置）全部保持——修订续跑是把「冻结」升级为「版本化」，不是开洞。
- [ ] 流程扩展（余项）：循环、嵌套分支（计划修订迁移已并入运行时治理的 `flow-revise`）
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
