# 第二轮应用实现：Excel 与 Word

## 范围和环境

2026-09-28，Windows 10.0.26200.0 中文交互桌面，Excel/Word 文件版本均为
16.0.20326.20158，使用 `target/validation/release/actl.exe`。
延续 [Office 初测](14-office-scenarios.md)，本轮把一次性探索变成可重复场景脚本。
微信仍跳过；腾讯会议、夸克的未验收项仍按 [28](28-meeting-quark-visual-deep.md) 记录。

两份脚本只创建并修改各自 `target/<scenario>-<uuid>/` 内的合成文件。
Office 使用本机原有配置和加载项，未修改安全设置，也不接触用户已有文档。
通过 ZIP/XML 创建只有 `ACTL seed` 的初始文件；实际编辑均经 actl UIA/键盘通道，
没有用 COM 代替被测操作，也没有新增依赖或修改核心命令协议。
本轮没有物理鼠标动作；键盘输入始终绑定当前核验过的 HWND。

运行（PowerShell 7.2+，交互桌面空闲）：

```powershell
pwsh -NoProfile -File scenarios/excel-table.ps1
pwsh -NoProfile -File scenarios/word-edit.ps1
```

可用 `-Actl` 指定待测二进制、`-Program` 指定安装路径；默认复用本机 Office16。
启动前检查真实停止状态；存在停止标记时退出 77，不发任何 CLI 调用、不清除标记。
每次调用保存 JSON envelope、stderr 和墙钟 `elapsed_ms`，最终报告含文件路径、版本、
检查结果、SHA256、退出码及清理状态。耗时仅用于同环境分析，本轮未做鼠标/键盘速度对照。

## Excel：8 项内容检查

`scenarios/excel-table.ps1` 以 `/x` 启动独立实例，核对新 HWND、XLMAIN、进程路径后绑定。

1. 名称框 `id:1001` 跳到 A1，粘贴 4 行 × 4 列 TSV，包含中文、数值、空单元格及公式。
2. 磁盘 XML 同时核对公式表达式和计算结果：`B2*C2`、`B3*C3`、`SUM(D2:D3)`。
3. B2 从 2 改为 4 后，D2 从 25 变为 50，D4 从 46 变为 71。
4. Ctrl+Z 后 B2、D2、D4 恢复为 2、25、46。
5. Ctrl+Y 后再次变为 4、50、71。
6. 复制 A1:D4，通过剪贴板精确比较整块 TSV，包括空单元格。
7. 保存、关闭、重开后，磁盘中的各值保持正确。
8. 重开后用名称框定位 D4，再复制读回 71，确认 UI 中打开的内容。

每次修改后 Ctrl+S，等待文件内容满足断言。保存成功的证据来自 XLSX 内部内容，
不是命令返回 ok、窗口标题或文件时间戳。`office-package.ps1` 支持这份合成工作表所需的
inline/shared strings、数值和普通公式缓存；不将其当作任意 Excel 文件解析器。

最终通过报告：`target/excel-table-93e94d25bbb64f868fdd840c172b0fcf/result.json`，
8 项通过，`passed=true`、`cleanup=closed_verified`。

### 调试中保留的失败

- `target/excel-table-f72f0c6c7a5143cb978a26b08d48cc90/result.json`：首轮脚本使用了
  不支持的 `escape` 键名，改为 `esc`。关闭时出现保存提示，经实时快照定位“保存”按钮，
  保存本任务测试文件后关闭，没有丢弃内容。
- `target/excel-table-57d6ebbcbdc74e3588a0969a17520ed7/result.json`：读回/哈希遇到打开文件的
  共享访问冲突；改为共享读写、允许文件替换的读取方式。
- `target/excel-table-e4631e87bb62479f9780f23fc821c811/result.json`：前两项通过，下一次保存期间
  短暂独占文件。现只对磁盘读取做最多 10 秒等待，不重放保存或输入动作。
- `target/excel-table-7ef95e9a1c734c09a28a0d9a31d92c8f/result.json` 与
  `target/excel-table-ffd0143c1a9d4228adc9bbc1dd7c3b9c/result.json`：8 项内容检查通过，
  但关闭检查超时，整轮仍记为失败；没有覆盖原始报告。

### 关闭状态

本机 Excel 收到关闭后可能先延迟，再留下不可见、标题为 Excel 的空宿主。
实际快照仅剩 Window/TitleBar/系统菜单等，没有工作簿网格；窗口截图为黑色，
截图本身不足以证明文档已关闭。

场景清理仍复用 HWND/PID/进程启动时间/窗口类/文档标记的身份核对，最多观察 45 秒，
不重复发送关闭。若留下空宿主，额外要求窗口不可见、UIA 树完整且仅有框架节点，
并且测试文件可被独占打开，才记为 `document_closed_host_retained`。
这不代表 Excel 进程退出。保存提示、仍有内容的树、文件未释放都不能走这条分支。

## Word：8 项检查通过

`scenarios/word-edit.ps1` 以 `/n` 打开独立合成 DOCX，核对 OpusApp、新 HWND 和进程路径。
该最小 DOCX 在本机显示为兼容性模式；未升级用户模板或修改 Word 全局设置。

1. 在正文插入前缀，确认原始 `ACTL seed` 完整。
2. 正文全选替换为中英文多段文字。
3. Ctrl+End 后追加含中文、Ω 和数字的末段。
4. 撤销追加，全文回到原状态。
5. 重做追加，全文恢复追加后的状态。
6. 正文全选 Ctrl+B 后，内容仍逐字匹配。
7. 保存文件内每个含文字的 `w:r` 都存在有效 `w:b`，证明加粗确实落盘。
8. 关闭、重新打开，再次同时核对 TextPattern 全文和 DOCX XML。

TextPattern 用 CR 分段，文件用 `w:p`；核验统一换行，保持段落边界。
解析器只覆盖此测试的普通段落、拆分文字 run，不覆盖表格、域、批注、修订和嵌入对象。
Ctrl+B 是切换动作，pack 明确要求已知当前样式，不能无条件重复执行。

通过报告：`target/word-edit-a2eb243c022543abb2104690412d6b26/result.json`，
8 项通过，`passed=true`、`cleanup=closed_verified`。测试文件留作本地证据。

## 离线检查与后续边界

新增 `crates/xtask/tests/office-package.Tests.ps1`，覆盖合成文件读取、shared strings、
公式/数值分别提取、空段落、分段 run、打开文件时读取/哈希、延迟关闭及不可见空宿主分类。
真实 Office 测试属于场景轨，未加入 CI；离线检查通过现有 xtask 回归框架进入 gate。
`browser-probe.Tests.ps1` 增加两个脚本的停止状态检查：退出 77、零 CLI 动作、标记保留。

本轮尚未覆盖 Excel 排序/筛选/数据透视表、Word 查找替换/插入表格/PDF 导出。
跨应用的数据搬运可以在这两份已能核对磁盘结果的场景基础上继续扩展。

## 收尾核验

两份成功报告合计 16 项检查。收尾再次枚举窗口，无本轮 `actl-excel-*` 或 `actl-word-*`
测试文档窗口；两个最终文件都可独占打开，确认文件句柄已释放。
窗口枚举证据：`target/office-final-windows.json`。测试文件、失败报告及逐调用日志保留本地，
不把真实桌面窗口清单提交到仓库。

`cargo xtask gate` 最终通过（file-lines、fmt、clippy、test、lint-packs、regression harness），
日志：`target/office-deep-gate.log`。PowerShell AST 和 `git diff --check` 也通过。

首轮门禁失败记录保留在 `target/office-deep-gate-first.log`：
`batch_steps_have_no_blocked_by_when_foreground_is_baseline` 收到 Excel 前台告警。
恢复到原来的 ChatGPT 窗口后，一次只读 batch 仍把原有窗口列为 windows_created，
说明当时的批前窗口枚举存在瞬时遗漏，不能仅归因于 Excel 空宿主。
随后完整门禁通过；本轮没有改动核心逻辑或放宽契约测试。
这一窗口基线稳定性问题仍需单独复现分析，不将重跑通过解释为缺陷已修复。
