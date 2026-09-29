# 70 · 调试手册（debug playbook）

来源：2026-09-29/30 四软件整批六轮实战与单链攻坚（全过程见 [68](68-combined-batch-first-round.md)）。
本文只沉淀**可复用的方法与家族结论**，应用特定知识在各 pack 的 quirks。

## 1. 调试阶梯（从最小复现到整批）

1. **单步手工复刻**：用 actl 命令直接跑（launch/type/press/verify），现场逐步观察。
   今晚 Zotero 就是靠这个破的：每个疑点步骤（导入、搜索提交、导出对话框）先手工验证通过，再进 workflow。
2. **单应用 flow 全链**（新任务 ID）：模板级验证，覆盖引擎语义（recovery/before/verify）。
3. **组合整批**（新任务 ID）：验收口径，桌面空闲要求最高（见 §5）。

**冻结计划铁律**：flow 的计划在创建时冻结，YAML 修正后旧任务**永远跑旧路线**——
重试前先确认修复是否已在新任务里验证过。今晚 verify4/verify8 的 export 重试
各废一轮才想起这条。`--retry-step` 只复活"最新 uncertain 步"（一次续跑一个），
更早的 uncertain 需各自显式指定。

## 2. 观察瞬态家族与耐心语义

「动作已生效、观察未跟上」是贯穿六轮的失败主族（docs/65 同源）。各核验命令的耐心现状：

| 命令 | 耐心 | 说明 |
|---|---|---|
| verify-file / verify-rows / verify-closed | settle 5s | 家族基准 |
| verify-window | settle 15s | 拆窗实测 3-15s（Office 负载相关，孤立 2.8-4.2s） |
| wait --gone | 60s 预算，观察期错误视为"仍在" | 垂死窗口的 UIA 查询会抖 STALE_REF/INTERNAL |
| wait --element/--stable | 轮询至超时 | 天生耐心 |
| **verify --value（element_text）** | **无（单发）** | 元素重渲染期可能假阴性 |
| **verify-clipboard** | **无（单发）** | ctrl+c 送达与剪贴板可读之间有竞态 |

引擎侧：观察恢复（recovery 预算）的 retryable 集合 = StaleRef/Timeout/NotFound/**Internal**
（Internal 纳入是 2026-09-30 实证：冷启动窗口 UIA 抖动，有界 3 次/15s 不吞真错误）。
**给易瞬态的步骤配 recovery 预算是模板标配**（vscode 7 步、zotero 9 步、excel 13 步已配）。

## 3. Windows 原生表面 vs 引擎正斜杠约定

引擎把 `${run.dir}` 统一正斜杠（记录一致性），但三个原生表面拒收正斜杠，全部实证过：

| 表面 | 症状 | 解法 |
|---|---|---|
| Zotero `-datadir` | 启动只弹错误框，无主窗口 | `${run.dir_native}` |
| Windows 保存对话框文件名框 | 「文件名无效」/文件不落盘 | `${run.dir_native}` |
| Firefox 系命令行解析 | 未知 | 同上预防 |

**通用规则**：传给原生应用的路径参数（launch args、对话框输入）一律
`${run.dir_native}`+反斜杠后缀；仅 Rust 侧 IO（verify-file --path）两种皆可。
新增 workflow 时审查所有出流程的路径参数。

## 4. 转义与编码纪律（bash 工具链）

- **bash 内联命令禁含**：中文（GBK 污染，AGENTS 已有）、**反斜杠路径、`$`变量**——
  后两者是 2026-09-30 新增实证：`"C:\\...\\$T.xlsx"` 经工具层+双引号组合产出
  `...$T.xlsx`，Excel 收到残渣 `X:/.xlsx` 弹「无法找到」。
- **一律写脚本文件**（.cjs/.ps1/.py）执行；脚本内路径用正斜杠或 join。
- 症状速查：launch 返回 ok 但应用弹「无法找到 X:/…」→ 先查参数被转义层吃掉，
  再怀疑应用本身。

## 5. 共享桌面与无人值守

- **跑批前置检查**：`actl display-status`（ready/covered_or_offscreen）+ 前景窗口标题
  （锁屏=「Windows 默认锁屏界面」，所有动作正确 fail-closed）+ `wait --idle`（存在但
  尚无 workflow 使用）。诊断三段法第一条永远是环境（锁屏/遮挡/前台竞争）。
- **前台竞争**（用户活跃）会让开局步骤死于 foreground_not_acquired；被打断后恢复
  链路（暂停→出卡→续跑→复活同段 skipped）已多次实证可用。
- **外来程序注入**（hook_foreign_injected 计数）会触发让行且不赦免（键盘类）——
  用户的另一个 agent 会话/带热键工具是常见源，journal consent_decision 可证。

## 6. 证据采集实践

- **用户截图两次成为决定性证据**（Zotero -datadir 错误框原文、X:/.xlsx）——用户在
  现场时请其截图是最低成本的根因定位手段；失败现场稍纵即逝，事后枚举常已变样。
- journal 三件套：`history --timeline`（时间线）、consent_decision（输入台账含
  hook 计数）、workflow_sequence（序列内逐步 ok/error）。
- 诊断脚本现场清单（值得沉淀为工具，见 ROADMAP）：窗口标题监视器（500ms 轮询
  谁匹配某串）、close→gone 计时器、隔离应用实验台（flow-resources 路径 launch+就绪）。

## 7. 症状 → 根因速查（本轮实证）

| 症状 | 实证根因 |
|---|---|
| verify-window absent 反复失败、事后窗口却没了 | 单发判定撞 3-15s 拆窗（§2） |
| 步骤动作 delivered 但 verify NOT_ACTIONABLE 且盘上无变化 | 动作实际生效、核验通道被挡（docx 锁/对话框未开/文件名被拒） |
| launch ok 但应用弹「无法找到 X:/…」 | bash 转义层吃掉路径（§4） |
| --retry-step 后仍旧行为 | 冻结计划（§1） |
| 全部步骤同时 NOT_ACTIONABLE | 锁屏/显示端被遮挡（§5） |
| Excel 关不干净、壳越积越多、系统鼠标转圈 | 加载项钉住空壳；WM_CLOSE/COM Quit 均无效；重启清 |
| 选择器间歇 AMBIGUOUS | 动态插入的同名元素（通知条/集合树行/弹出的 List 本身），role/automation_id 精确化 |
