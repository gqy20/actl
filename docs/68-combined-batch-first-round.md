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

---

# 第三至五轮追加记录（同夜，修复后重跑）

## 第三轮（apps-verify3-20260929）：共享桌面竞争全灭，作废

四段几乎同时死在第一步（vscode 抢前台 `foreground_not_acquired`、excel 冷启动观察 STALE_REF、word insert 中断、zotero wait INTERNAL），4/68。诊断三段法第一条命中：用户正在活跃使用桌面（前台锁在用户终端）。**非路线缺陷**。现场全部优雅清理。副产品：确认 Zotero `-datadir` 反斜杠修复生效（data2 目录落盘齐全、主窗口出现、按标题优雅关闭全进程树退出）。

## 第四轮（apps-verify4-20260929）：57/68，Word 首次全过

53+4 completed / 2 uncertain / 9 skipped，期间两次显示端触发续跑（signal_control）。

- **VS Code 26/26**；**Word 段 20 步全部通过**（insert 的 UIA 正文核验修复生效，close 亦通过）——Word 路线首次整段验收。
- Excel：close-before-reopen ✓（verify-window absent 修复生效）、reopen ✓、reopened-owner ✓、reopened-content ✓（xlsx 打开期间共享读可用的对照实证）；死于 **reopen-cell-readback**：verify-clipboard NOT_ACTIONABLE——该步仍是旧路线（set-value 名称框直写 + 单元格定向 ctrl+c，即第一轮 copy-range 已证不提交的同族缺陷，当轮修复漏了这步）。已镜像 copy-range 修正（type --verify-value + 窗口级 ctrl+c，commit 51c8841）。
- 中途一次人工让行（excel close 执行中用户输入到达，动作 action_started 后被拒）→ 3 秒后续跑，续跑按设计复活同段 skipped 步骤。
- Zotero：open 首次执行被外来输入打断（见第五轮同因分析），后续跑未再执行该步即终局化——**引擎语义：每次续跑只复活最新 uncertain 步**，旧 uncertain 步保持失败记录。行为符合设计，记录为使用要点。

## 第五轮（apps-verify5-20260929）：zotero 推进至导入前置，随后多波续跑

- 开局又遇前台竞争（vscode-initial-foreground 失败 → 段隔离跳过，本轮注定无法整批全过）。
- 多波续跑后：excel 段在 paste 重放处 ASSERTION_FAILED（xlsx subset 差异——被打断的 paste 重放后的内容状态，**未定案**，需复现诊断）；zotero 段 open/ownership 通过（路线修复完全生效），死于 **zotero-search-import 的 before**：verify-rows 前置断言空库，但全新空库下 `item-tree-main-default` 条目树**不存在**（实测快照：仅 collection-tree/我的文库 等），前置 NOT_ACTIONABLE → precondition_failed 暂停。fail-closed 正确；路线需适配（空库就绪检查应改为 collection-tree 存在性，或去掉对隔离新库冗余的前置）。
- 终态 paused:precondition_failed；现场清理（VS Code 正常关闭，其余进程已退）。

## 让行触发澄清（用户实测反馈：未操作键鼠仍让行）

journal consent_decision 证据：撤销时刻 `hook_foreign_injected_events` 0→2——触发源是**其他程序注入的 2 个键盘事件**（Windows injected 标记、非本工具标记），非用户物理输入。docs/32 设计如此（外来注入也算外部输入，防自动化互搏）。赦免未生效原因：注入为键盘类，属永不赦免类（保守正确）。**改进项已记 ROADMAP**：让行卡/consent 事件应区分「人工物理输入」与「外来程序注入」。

## 遗留与待办

1. 整批四段全过仍未达成（本夜被桌面竞争反复打断）；路线层面四段各自均已单独通过过。
2. cell-readback 修正（51c8841）待真机验证。
3. zotero import 前置适配（空库无 item-tree）待修。
4. excel paste 中断重放后的 xlsx 差异未定案（复现优先）。
5. 显示端续跑执行器的 journal 未完整并入任务时间线（第五轮 525 事件止于首执行器，续跑波次的步骤事件缺失）——诊断工具链缺口，待查。

---

# 单应用全链收官（2026-09-30 凌晨）

## 四软件单链全部首次通过

- **Zotero 9/9**（zotero-verify11）：import 前置改 collection-tree 就绪 + 导入后等行渲染；搜索改 click+ctrl+a+type（实测 type 追加、tab 清空过滤、裸 press 被首启通知条挡）；publication 精确 name；CSL 列表项 role 限定；保存对话框（contentproc 子进程）去 verify-owner；导出路径 `${run.dir_native}`（正斜杠被保存对话框拒收，用户截图实证「X:/.xlsx」/「文件名无效」）。recovery 预算实救 1 次 STALE_REF。
- **Excel 13/13**（excel-verify8）：close 核验根因定案——拆窗实测 3-15 秒（孤立对照 2.8-4.2 秒），旧单发判定必撞假阴性；修为 wait --gone（60s 耐心、观察期错误视为仍在）+ verify-window 15 秒 settle。reopen 冷启动 INTERNAL 纳入观察恢复可重试（真机实救 1 次）。cell-readback 镜像 copy-range 路线通过。
- Word 20/20（第四轮）、VS Code 26/26（第二/四轮）此前已过。

## 引擎侧修复（commit b7dee16）

verify-window settle、wait --gone 观察容错、INTERNAL 纳入 retryable（docs/65 观察瞬态家族），单测覆盖。

## 诊断教训（入档）

- **bash 内联转义事故**：`"C:\...\$T\$T.xlsx"` 在工具层+bash 双引号组合下产出 `...$T$T.xlsx`，Excel 收到残渣 `X:/.xlsx` 弹「无法找到」错误框——closelab 系列实验全部作废后由用户截图定位。含反斜杠/变量的命令一律写脚本文件执行（AGENTS 中文参数纪律的推广）。
- **Excel 僵尸壳堆积**：今晚每轮整链留一壳（现 9 个），拖慢 Excel 全局操作（用户观察到鼠标转圈）；WM_CLOSE 与 COM Application.Quit 均无法退出（加载项钉住）。不 taskkill，留待重启；根因（加载项钉住）与缓解（流程已把壳存在视为正常）记 pack。
- 第六次整批（apps-verify9-20260930）死于**锁屏**（深夜自动锁屏，前景=「Windows 默认锁屏界面」），四段 open 全部被锁屏护栏正确 fail-closed——非路线缺陷。轮询 5 分钟未解锁，按纪律受阻不记通过；**四链已各自通过，整批只差一次解锁状态下的运行**。

## 待办

1. 解锁状态跑一次组合整批（verify-applications.yml 68 步）收尾验收。
2. Excel 壳堆积的根因与优雅退出路径（加载项钉住；重启后复测一轮确认壳数不无限增长）。
3. 早前已记录项见 ROADMAP（交接来源区分、续跑 journal 完整性等）。

## 整批清场运行序列（2026-09-30 定稿）

组合整批启动前清桌面的需求落地为**调用方运行序列**（非流程步骤）：`desktop-prepare --all`
本就是为此设计的命令（ordinary 窗口过滤、逐窗身份+placement 快照、崩溃可恢复会话、
`desktop-restore` 还原），但其会话生命周期与"动作步必须 verify"的协议约束决定它属
调用方。序列与两次授权说明见 workflows/README 整批运行序列节。教训入档：动手前先
搜命令集——本次险些重复实现一套已存在且更完整的 desktop-prepare/restore。

## 第七次整批（combined-batch1）：69/69 完成，含三次救场与一次中途代码修复（2026-09-30）

解锁状态下的组合整批（idle 门 15s → desktop-prepare --all 会话 s18d9f2c45349071c1 → flow-run 69 步）。
最终 69/69 completed，但**不是一次无干预跑通**：全程经历三次人工救场、一次中途定位并修复的代码级
bug、多轮让行/续跑。如实记录如下，供判定验收口径。

### 中途定位并修复：COM 线程模型污染（capture.rs STA 遗留）

现象：zotero-search-import 恢复核验报 `INTERNAL: RPC_E_CHANGED_MODE（无法在设置线程模式后对其
加以更改）`，且 word-edit-append 恢复核验的 `INTERNAL: observed element has no usable runtime
identity` 同族。根因：`uiautomation` crate 的 `UIAutomation::new()` 仅接受 MTA 且不容错；
`screenshot_screen`（失败留证三通道的整屏 PNG）把执行器线程 CoInitializeEx(STA) 成功后**从不
uninitialize**——线程一旦被留证路径走过即永久 STA，同线程后续一切 UIA 调用（恢复核验、快照留证）
全体 RPC_E_CHANGED_MODE 失败。修复：改 COINIT_MULTITHREADED 与全库一致 + 配对释放 +
CHANGED_MODE 容忍（evidence.rs 同款 guard）。修复装上后，import 的恢复核验一次通过。该 bug 同时
解释了此前多轮恢复路径上零星 INTERNAL（观察恢复 retryable 集合里的 INTERNAL 有相当部分来自此处）。

### 三次救场（结果不确定先核验，不盲重放）

1. **word-edit-append**：第一次尝试在动作前即被 user_paused 拒绝（用户 ShareX 截图热键=键盘输入，
   撤销授权）；面板继续重跑时 paste 两次未落地（失败瞬间截图：无对话框遮挡、光标已在文末、Word
   前台——瞬时干扰未定源）。手动以同参数 `type --paste` 复现：**落地成功**，文档恰好补齐断言期望
   文本；随后继续，引擎核验通过，word 段 12 个曾跳过步骤全部补跑。
2. **zotero 模态死锁**：隔离 Zotero（-no-remote -profile，启动本身成功）启动后弹「Zotero Word
   for Windows Integration」对话框（Zotero 检测到开着的 Word 试图集成），**模态禁用主窗**：
   focus-window 报 window_disabled_or_modal，import 的菜单点击全部失效→verify-rows「row parent
   tree not present」（库为空，非渲染慢）。授权卡（允许关闭该对话框）由用户/代点通过后关闭对话框；
   手动执行 import 动作序列（set-clipboard + 两个 UIA click + wait row）一次成功，行数=2，
   引擎随后核验通过。
3. **word-edit-close**：reopen 步 STALE_REF（关闭重开后冻结计划引用旧实例）后该步搁置；轮到它时
   新 Word 实例（37860）仍开、步骤断言瞄着旧身份。手动 close-window --pid 37860（doc 有自动保存，
   无脏弹窗）后 resume，69/69。

### 授权节奏实测

- 每次授权（once_per_run）总长 10 分钟；整批约 30 分钟 + 多轮暂停，实际节奏=「授权→跑一段→
  到期/外部输入让行→面板继续/授权卡→再跑」。用户在场的几次卡由用户点击。
- **代点击的机械边界**：CUA 坐标点击（SendInput）、PostMessage 直投、PowerShell SendInput 三种
  合成输入均无法驱动面板的继续/开始按钮（面板按设计拒绝合成输入；早前几次「点了就恢复」实为
  CLI flow-resume 的效果，与点击无因果）。SetCursorPos 纯移动亦未触发误触赦免自动续跑。
  结论：AGENTS 的代操作授权在当前显示端实现下**机械不可达**，需用户点击或显示端提供 agent 通道
  （已登记 ROADMAP）。
- 桌面清场：desktop-prepare 在用户前台活跃（Wave 全屏）时最小化被拒（handoff）——先过 idle 门
  即通过；desktop-restore 还原 Wave 成功、另一窗因会话中变化如实跳过。

### 收尾状态

Zotero/VS Code/Word 进程全部退出；Excel 又添一隐藏残壳（52824，XLMAIN 隐藏+DDE 服务窗，与已知
僵尸壳同类，未 taskkill）；桌面已 restore。被测二进制：修复后 release 构建（gate 通过 182.8s），
已装 ~/.cargo/bin。证据：flow-resources/combined-batch1/evidence/（append 与 import 各三件套）。
