# 应用验证脚本迁移到 workflow

> 历史记录：文中的 PS/Bash 入口已退役。当前原生/workflow 入口与未验收事项见 [迁移清单](61-native-tooling-migration.md)；本页原始实测结论保留。

日期：2026-09-29。范围为前轮讨论的 VS Code、Word、Excel、Zotero 四个应用及整批入口。通用性能采样、UIA 对照工具和其他应用场景不在这一批。

## 入口

- `skills/actl/workflows/vscode-edit.yml`：三轮插入/撤销/重做、全文替换、磁盘核验、关闭重开、转到行、查找替换、两个任务标签切换及最终关闭。
- `word-edit.yml`：插入、多段替换、追加、撤销/重做、加粗；逐阶段磁盘正文、UIA 全文与格式核验，关闭重开及收尾。
- `excel-table.yml`：TSV、公式值与表达式、重算、撤销/重做、范围剪贴板、关闭重开及单元格读回。
- `zotero-search.yml`：隔离库 RIS 导入、三种搜索及精确行集合、刊物字段编辑、导出 JSON 内容及关闭。

统一使用 scenarios/prepare-workflow.ps1 -Scenario <名称>，准备唯一标记、文件/profile/数据目录及 inputs.json，预检并以 paused 状态登记任务。直接运行 YAML 可使用 flow-run --file ... --inputs ... --id ... --paused；已有输入文件不需要再跑准备脚本。恢复始终沿用原 flow ID。

`scenarios/verify-applications.ps1` 准备四套隔离资源，调用 flow-validate --emit-plan 获取具体计划，为 step/output 引用加前缀后合并成一个计划，再预检和登记。整批只有一个 flow ID、一次初始交接；检测到人工接管仍暂停，不自动重新授权。

`-PrepareOnly` 只生成资源与预检；不会登记或操作桌面。旧名称转发入口和 VS Code `-Smoke` 已删除，常规小任务使用现有 vscode-resume.ps1。

## 恢复及资源边界

选择/输入/保存组成一个步骤，以完整内容作为后置条件。中途失败后核验整个步骤结果，禁止猜测最后一个键是否送达。部分完成停在待核对，不能自动从组内中间位置继续，也不会重新执行前面的写入。

关闭前核对保存的 HWND、PID、进程创建时间、可执行路径和任务路径；关闭后核对原实例退出。不在异常 finally 中关闭桌面窗口；暂停、未知和失去归属时保留现场。Zotero 主窗口及导出对话框都核对隔离 profile 的进程归属；存在多个匹配窗口则拒绝。

## 验证与限制

- 四份模板与 64 步组合计划已完成无桌面操作的预检。
- Word/Excel 合成文件读取验证通过；不依赖启动 Office。损坏包必须返回未知/不可操作，不能当空正文。
- 新增命令面 golden、文件断言 envelope、组内后续让行不能覆盖已送达写入的测试。
- 按用户要求移除旧逐调用编排、四个同名转发入口、verification-task.yml 和旧 Office 读取/收尾代码。历史真机证据留在 docs 与本地 target，代码可从 Git 历史查阅。新流程仍待真机复验，删除旧实现不代表验收通过。
- 代码/预检通过不代表四个真实软件整轮通过。此次没有替用户点击开始，也没有把尚未运行的新流程登记为真机通过。
- Word/Excel 如果复用的进程命令行不含本任务文件路径，归属核验会拒绝；需实测补齐证据，不能放宽为只看标题。
- UIA 名称、本地化、Office 格式及焦点仍可能受环境影响，失败会保留现场和 flow 状态。OOXML 检查目前限标准正文和 sheet1，明确能力见 docs/06。

## 本轮证据

- `cargo xtask gate`：通过，日志 `target/application-workflow-final-gate.log`。
- 最新模板契约测试：20 项通过，日志 `target/workflow-contract-final.log`。
- 发布二进制准备入口及组合计划预检：通过，64 步，未登记、未操作桌面。
- 二进制：`E:\explore\computer_cli\target\validation\release\actl.exe`；SHA256：`EB3002EDE50CC36C7DD4E8F50489869224936C8741480B6B8A5A3BC629B00F7D`。
- 组合计划与逐应用输入：`target/applications-4b15b83a815c4f6cad9164f29a70f00c/`；汇总 `target/workflow-release-preflight.json`。
- 环境：本机 Windows，PowerShell，工具链遵循 rust-toolchain.toml；隔离资源保留在 target，未启动任务应用，无 GUI 清理动作。

## 旧实现清理后的复核

- 移除旧脚本对应的回归清单项，避免将资源准备或流程登记误记为桌面验收通过。
- Word/Excel 输入焦点约束迁入 workflow 契约测试；浏览器快照绑定检查保留；Office 种子文件由现有 CLI 文件断言测试核验。
- 完整门禁再次通过：target/workflow-cleanup-final-gate.log。
- 清理后的统一准备入口再次预检通过：64 步、未登记、未操作桌面，汇总 target/workflow-cleanup-preflight.json。

首轮真实 workflow 结果及状态条/耗时优化见 [58](58-workflow-live-and-feedback.md)：VS Code 17/22，存在关闭核验和行号输入失败，未记整轮通过。
