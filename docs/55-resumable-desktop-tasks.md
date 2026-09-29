# 可恢复桌面任务：暂停、继续与结果核验

## 执行入口

需要“继续”的任务在第一条桌面动作前保存为 flow，后续只恢复同一 run ID。
普通 PowerShell 进程的调用栈不能在退出后恢复；不为它添加假的继续按钮，也不重跑整段脚本。
`scenarios/vscode-resume.ps1` 准备独立文件/profile，登记暂停的持久化计划，界面显示继续。
用户点击后运行；可选 `-Checkpoint` 在打开文件后设置显式检查点，用于验收恢复，默认不增加检查点。
后续让行同样保存 pending/uncertain 状态，由继续入口恢复。

状态条把实际展示的 flow ID 记录在 visible-flow.json。调用快照被清理、普通诊断调用结束或显示端重启后，仍读取该任务的持久化暂停状态并显示继续；不在多个旧任务间猜选，不给已完成任务提供继续。这个显示绑定不授予桌面权限。

## 核验顺序

1. 重新加载原始计划、状态和执行锁，核对停止代次；不信任过期 ref。
2. `resume_check` 用语义选择器重新查找当前任务的目标；目标不存在、歧义或读取失败都停止。
3. pending 步骤执行 `before`，确认当前输入仍符合计划，然后才执行。
4. uncertain/running/verifying 步骤先执行只读 `verify`。
   - 实际结果与期望完全一致：记录完成，跳过原动作，继续后续步骤。
   - 实际结果明确不符：进入待核对，不代表原动作从未执行，不能自动重放。
   - 读取失败/目标消失/歧义：保留未知结果，不将异常转成 false 或空内容。
5. 重复点击继续只重复观察。重做不确定动作需要显式指定步骤；普通继续按钮不传 `--retry-step`。

## 什么能证明结果

| 动作 | 合适的核验 | 不足的证据 |
|---|---|---|
| 写入字段/编辑器 | 指定目标的完整值/文本与预期完全一致 | 控件存在、按键成功返回 |
| 保存文件 | 独立读取目标文件并核对完整内容 | Ctrl+S 已发送、窗口已关闭 |
| 创建或提交记录 | 按本任务唯一业务标识查询且恰好一条、字段一致 | 成功提示曾出现、按钮消失 |
| 启动应用 | 本任务隔离资源对应的目标就绪；清理另核对进程实例归属 | 仅启动 PID 或窗口差集 |

不存在可靠结果条件的动作，不宣称可自动恢复。部分文本匹配不能证明非幂等写入完整成功。

## 当前验收范围

新 VS Code 场景只核验编辑器内完整文本，结束后保留窗口供检查，不宣称保存到磁盘或关闭通过。
旧的全量 PowerShell 应用场景仍保留历史证据，尚未全部迁移为持久化 flow。
误让行的输入来源问题独立跟踪于 [54](54-repeated-handoff.md)，可恢复暂停不等于误判已解决。

## 2026-09-29 验证记录

- `cargo test -p actl-flow --test resumable_task` 通过，覆盖 pending 的现场重查、不确定写入已完成时跳过、部分完成及观察失败不重放、重复继续不重放。完整门禁：target/resumable-task-gate.log。
- 原任务 `vscode-resume-3149c3596f45499786db522414b9e0c8`：用户点击继续后打开文件并完成核验，持久化停在 checkpoint；再次继续重新观察后，空文本预期不符，保持 paused、input.attempts=0，没有输入。
- 修订任务 `vscode-resume-3149c3596f45499786db522414b9e0c8-verified`：复用原窗口，用户继续后 input.attempts=1，实际文本是 `ACTL resumable input verified.`。用例错误地预期尾部换行，故进入 needs_review/uncertain；未自动重放，原失败记录保留。
- VS Code 空编辑器的 TextPattern 返回占位换行；本机不含换行的文本读回完全一致，而输入一个尾部换行后读回两个。未将这种 provider 表现推广为通用规范。最终用例采用已观察过的不含换行文本，做完整相等核验，不做模糊包含或吞掉空白差异。
- `vscode-resume-1d10cc7a282f475a9667d2789ef71dec` 记录了尾部换行预期错误：input.attempts=1、needs_review，仍未重放。该轮失败保留。
- 上述二进制哈希记录于 target/vscode-resume-3149c3596f45499786db522414b9e0c8/binaries.json，窗口均保留供检查，未做隐式保存或强制关闭。
- 新增 paused_task_survives_call_expiry_and_other_unfinished_tasks：先复现 60 秒调用快照清理后按钮变成 Details（target/flow-display-before.log），修复后 Continue 仍绑定原任务，任务完成后不再显示（target/flow-display-after.log）。
- 最终真实任务 `vscode-resume-4351ab6ecca445dc901c2a58272fe88d`：completed，open/input/readback 各 attempts=1，输入后完整文本核验与独立读回均通过。结果、原计划及交接记录在同名 target 目录；此次不记磁盘保存或关闭通过。该轮使用 CLI `AB5DC73FB344B5324E4AA9F620EB79EA5585779A945F3D5D35B7B5F6D6B5A421`、显示端 `79084D3BA222B6FFC78263C3F724F89385C9D9569EA191FB8C3B783598F984BC`。
- 持久显示修复最终门禁通过：target/durable-continue-gate.log。安装显示端 SHA256 `E7D0549CF7C68AFB0155D69EE06568A215B6F2D98E66F557D779FE782B263246`。
- 新显示端只读任务 `vscode-retention-4351ab6ecca445dc901c2a58272fe88d`：暂停 83767ms 后，临时调用快照已超过 60 秒保留时间，原生状态条仍显示“已暂停 · 可操作”，visible-flow.json 保持同一任务。随后仅执行只读 flow-resume，完整文本断言成功、任务 completed；没有授予桌面权限或重放输入。证据：上述目录 retention-window.json、retention-start.json、retention-completed.json。
