# 四软件整批首轮与 Excel 复制路线受阻

## 入口与被测对象

2026-09-29 晚，`actl flow-run --file skills/actl/workflows/verify-applications.yml --id apps-verify-20260929`（68 步：VS Code 26 + Excel 11 + Word + Zotero）。执行文件 `C:\Users\gqy17\.cargo\bin\actl.exe`，SHA256 `42BDE424A591EABA7D75F58FC00DE52C282F72423FF121B903211C98E3CD6CB1`（与 [67](67-vscode-navigation-controls.md) 验收二进制一致）。显示端为运行前由 `actl display-start` 新起（协议 v9，pid 37836）；启动前旧租约属主已退出，`display-status` 曾报 companion_missing，未带病起跑。证据目录 `target/apps-verify-20260929/`。

本轮用户共享桌面（期间有键鼠输入与剪贴板复制），非无人值守；恢复均先核验现场，未盲目重放。

## 结果

- **VS Code 段 26/26 完成**：含三次"动作已生效、核验观察未跟上"的恢复（见下节），最终 goto-line、find-replace、双文件切换、保存与关闭全部通过。此段结论与 [67](67-vscode-navigation-controls.md) 单软件轮一致。
- **Excel 段 6 步完成**：open（ownership hwnd 2101946 / pid 39236）、paste、recalculate、undo、redo 通过，表格内容经步骤断言与截图核对完好。
- **excel-table-copy-range 三次确定性失败**（attempts=3，均 NOT_ACTIONABLE），路线结构性缺陷，见根因节。
- **Word、Zotero 段未执行**（35 步 pending）。流程停在 needs_review / result_observation_failed，冻结计划与失败记录保留，不得改写。

## VS Code 三次核验瞬态（恢复均无重放）

1. `quick-open-second`：切第二文件后等编辑器控件，等待窗内 NOT_FOUND（0 候选）。恢复核验窗口标题已为第二文件，确认动作生效，标记通过继续。
2. `quick-open-file`：切回主文件后读值 INTERNAL（无 phase，瞬态观察错误，与 [66](66-application-readiness.md) 记录的 quick-open-file INTERNAL 同族）。核验标题已回主文件，恢复通过。
3. `excel-table-open`（Excel 冷启动）：children 通道 STALE_REF / observation_failed（native_code -2147220991）。核验 Excel 已带任务文件打开，恢复通过。

三次均为"观察未跟上已生效的动作"，与 [65](65-observation-blocking-diagnosis.md) 的瞬态失败观察项同向；本轮共享桌面可能是诱因之一，不能据此归因单一根因。

## 交接时间线（澄清：剪贴板内容不参与判断）

journal 的 consent_decision 记录了 4 组 `external_input → user_start → reused/allowed`：每次物理键鼠输入（Raw Input 通道，reason=device_input）落在授权期外/撤销后，新执行器的首个桌面动作重新出卡，用户点击后放行，once_per_run 在此后复用。用户复制内容的剪贴板变化本身不是触发条件；触发条件是复制时的物理键盘输入。共享桌面期间该行为符合"人工输入优先让行"的设计，但导致 4 次点击；无人值守轮应保持桌面空闲。

## Excel copy-range 根因

步骤动作链：`set-value id:1001 "A1:D4"`（名称框）→ `press enter --target id:1001` → `press ctrl+c --target uia:{name:"A1"}` → `press esc`，核验 `verify-clipboard --expected <表 TSV>`。

- journal（34492-s18d9cd57775a2c900）显示动作事件 preparing/executing/delivered 后，失败落在 **phase=verify**。
- 事后只读核验：名称框（automation_id 1001，role Edit，唯一）value 为空；截图（excel-paused.png）确认表格完好、名称框显示 A1、无 A1:D4 选区——**名称框 ValuePattern 直写未提交**，选区从未建立，ctrl+c 无内容可复制。
- 剪贴板为空时 `verify-clipboard` 返回 NOT_ACTIONABLE "clipboard text unavailable"（verify-clipboard-repro.json 复现）——核验命令行为正确。
- 结论：**迁移后的名称框直写路线在本 Excel 环境（含 OfficePLUS 等加载项）结构性无效**。该路线自 [61](61-native-tooling-migration.md) 迁移后从未真机执行，旧 M2 验证（docs/14）使用的是旧脚本语义。三次尝试（本人 --retry-step 一次、用户点击继续后两次）均同样失败，非瞬态。

## 收尾

- 测试 Excel 经归属核对后 `close-window --hwnd 2101946` 正常关闭，`verify-closed` 通过（original_instance_gone）。
- 任务开始前已存在的两个 EXCEL.EXE（39992、32072）非本任务所属，未触碰。
- 用户剪贴板内容（重试前存档 user-clipboard-before-retry.json）已原样还原（含中文，读回核对一致）。
- 流程记录保留 needs_review，失败证据不覆盖。

## 待办（不记通过）

1. 修正 excel-table copy-range 路线：名称框直写不可用，需改为可提交的通道（如点击名称框后键盘输入，或改用表格区域选择），`excel-table.yml` 与 `verify-applications.yml` 同步；按纪律先写复现测试再修，gate 后提交。
2. 修正后以**新任务**重跑四软件整批（不改写本任务冻结计划），Word/Zotero 段仍属未验收。
3. 共享桌面导致的核验瞬态与多次交接在无人值守轮复核是否复现。

---

# 第二轮（apps-verify2-20260929，新二进制 + 失败隔离）

时间：2026-09-29 晚；二进制 actl.exe a7ea9759 / actl-signal.exe 215ea91c（target/validation/release 安装）；批前新建隔离资源与任务 ID。结果：**35 completed / 3 uncertain / 30 skipped，needs_review（section_failure:zotero-search-open）**。

- VS Code 段 26/26 全通过；quick-open-second 出现 1 次 NOT_ACTIONABLE → retry_reserved 恢复（observation recovery 预算机制首次真机生效）。
- Excel copy-range **修复生效**（type --verify-value + 窗口级 ctrl+c 路线通过，第一轮根因关闭）。
- 三处 section 失败各自独立、被 skip_section 隔离，后续 section 正常执行——失败隔离机制真机生效。
- 三处 uncertain 的 attempt_errors/synthetic evidence 均按设计落账。

## 三处失败根因（全部定案，均有直接证据）

### 1. word-edit-insert：verify-file 读被锁 docx → NOT_ACTIONABLE

- `before`（保存前、新开状态）verify-file 种子检查**通过**；ctrl+s 保存后同命令失败。失败后数分钟、Word 仍打开期间，PowerShell ZipFile.OpenRead 同样报共享冲突；Word 退出后立即可读。**Word 保存后对 docx 持排他句柄，打开期间任何共享读不可用**（IO 失败统一映射 NOT_ACTIONABLE "native evidence unavailable"）。
- 动作实际全部成功：UIA 正文 `role:Document` 归一化精确匹配 "ACTL prefix ACTL seed"；文件 22:44:23 重写为 12147 字节；Word 退出后直读 document.xml 正文恰为 "ACTL prefix ACTL seed"。失败纯在核验通道。
- 修正：打开期间的正文核验改走 UIA text（--normalize-lines），磁盘核验只在 reopen 后（reopened-content 兼查 bold:true）。`word-edit.yml` 6 处 verify 与 `verify-applications.yml` 镜像同步。

### 2. excel-table-close-before-reopen：verify-closed 断言进程消失 → ASSERTION_FAILED

- close-window --hwnd 关掉工作簿窗口后，**原 HWND 存活为隐藏 XLMAIN 空壳**（标题变 "Excel"；枚举实证 pid 48636 同 hwnd 657574，全部窗口隐藏）。Excel（Office 后台常驻/加载项，含 HardwareMonitorWindow、CFD File Open Message Window）收到 WM_CLOSE 不退出。
- 壳不锁 xlsx（文件可读实证）、不影响 /x 新实例；但 verify-closed 的"进程实例消失"断言对 Excel 结构性不可满足。
- 修正：close 断言改 `verify-window --app {app} --state absent`（工作簿窗口消失；zotero-search-close 原设计即此形态，先例一致）。`excel-table.yml` 两处 + 组合镜像。
- 遗留：真机清理对隐藏壳 close-window --hwnd 可送达 WM_CLOSE（ok:true）但三个壳进程（48636 本轮；39992/32072 为上午本人验证沙箱残留，命令行归属核对后处置）均未退出——记录为已知限制，未 taskkill。

### 3. zotero-search-open：-datadir 拒绝正斜杠路径 → 无主窗口 → verify TIMEOUT

- 用户截图（报错对话框原文）：标题「错误」（Zotero 图标），正文 **"-datadir requires an absolute path or 'profile' ('C:/Users/.../zotero-search/data' given)"**，按钮 检查更新/退出。ShareX 文件名 WINWORD_* 仅因截图时 Word 在前台。
- 引擎 ${run.dir} 统一正斜杠 → Zotero 10 的 -datadir 校验拒绝；启动只弹错误框、无主窗口，verify 20s 超时。佐证：隔离 profile 已半初始化（.startup-incomplete、prefs.js 等）、**data/ 目录完全空**、crashes 存储为空（无崩溃）、22:48 用户关对话框后进程全部退出。
- 修正：新增输入占位符 `${run.dir_native}`（保留反斜杠，resources.rs fail-closed 语义不变），zotero data 默认值改用之；app 输入收紧「我的文库 - Zotero」。
- 修正后路由真机验证：全新隔离 profile2/data2 反斜杠启动，主窗口出现且 data2 落盘齐全（zotero.sqlite/storage/translators）；按标题优雅关闭全进程树退出。
- 附带发现：--app "Zotero" 会歧义命中浏览器内 Zotero 网页签（Chrome「Zotero | Your personal research assistant - Google Chrome」实测），标题收紧后消除。

## 收尾

- Word（pid 42640）：内容核验 + 落盘确认后 close-window 优雅关闭，进程退出。
- Zotero：用户关对话框后自退；本节修正验证用实例（route-test）按标题关闭，进程树清零。
- Excel 三壳：WM_CLOSE 送达但不退，保留并记录（见上）。
- 用户剪贴板：word insert 的 --paste 通道在 22:44 覆写过剪贴板；本轮未留轮前快照，事后核对发现用户已自行复制新内容（当前为其自有路径），无需还原。后续整批运行前应留存轮前快照。

## 待办

1. 三项路线修正（本轮已实现，gate + 提交后）以**新任务**重跑四软件整批验收。
2. verify-file 的 NOT_ACTIONABLE 错误信息在 step 证据中仅留 code（synthetic），未保留 "native evidence unavailable: 共享冲突" 原文——执行器应保留子命令错误消息，改进项。
