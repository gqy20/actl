# 重复交接：Raw Input 自身事件误判

## 2026-09-29 复现

用户明确反馈 VS Code 反复弹窗，需要点击开始/继续。使用同一任务
`verification-47e740c2c65b4f69a793f0d181fce674` 和 `scenarios/verification-task.yml`
的 once_per_run 配置复跑。原始结果位于本地
`target/verification-47e740c2c65b4f69a793f0d181fce674/vscode-edit-b4ed0e5db28740f3bcad71bdfac5246b/result.json`。

新增 consent_decision 记录区分 shown / granted / reused，保留 approval、失效原因和两路活动计数。
首次授权计数 hook=762、raw=614；自动输入后 hook 仍为 762，raw=618，弹窗原因 external_input。
之后多个自动按键重复这一规律：hook 不变、raw 增加 4，然后要求再次确认。
因此不是任务配置丢失或过期，而是 Raw Input 无条件计数把注入事件当作外部输入。
原轮次停止于 step-017，保留窗口及失败结果，不能记为通过。

## 修复与测试

- Raw Input 用 GetRawInputData 读取有界、对齐的结构，核验长度和类型。
- 仅排除无设备、携带 actl INPUT_TAG 的键鼠事件；真实设备、其他标记、未知类型、读取失败或截断继续触发让行。低级 hook 的 injected + tag 检查保留。
- 先新增 raw_own_injection_is_not_human_takeover 复现旧行为失败（target/raw-input-before.log），再修复；针对性测试通过（target/raw-input-after.log）。
- 修正客户端每次请求都闪现 handoff_pending：仅收到 PENDING 才显示等待。
- consent_decision 为本地诊断历史，不记录按键值、坐标或输入内容。

Win32 读取结构依据：[Microsoft Raw Input 示例](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input)。

## 让行与继续

当前普通 PowerShell 场景在让行后退出，未保存可恢复执行器状态，因此不能只给状态条增加“继续”按钮就宣称可续跑。
该缺口已列入 ROADMAP：需保存步骤、归属及不确定结果，继续前核验已发生效果；不能重放整段脚本。
持久化 flow 的恢复能力与普通脚本分别验收。

## 真机复验

旧版诊断轮次共 11 次 shown（首次 revoked，10 次 external_input），10 次用户 granted、12 次 reused；历史副本为同任务根目录 consent-before.json。

新版本 cargo xtask gate 通过：target/raw-input-gate.log。安装后 display-status 为 protocol=8、ready=true、stop_requested=false。

- CLI：`target/validation/release/actl.exe`，SHA256 `F1ACFFE62A7B2281CDE4F9B86A523439C6D2845D7C64A93D0751CD2856362A80`。
- 显示端：`C:/Users/gqy17/.cargo/bin/actl-signal.exe`，SHA256 `DEF2C709CABA52FBF392E6AD893D3CCB3663476C3191357E7E0503330F0C0950`。
- 新轮次 `vscode-edit-cbe6789b387d4ccda881ea8da70ddf8d`：首次 launch 等待 25053ms 后返回 handoff_pending、action_started=false。只有首次 shown，未收到 granted；cleanup=no_owned_window。没有循环重试。

修复代码与门禁已完成；修复后的单次确认真机验收未完成。旧轮次保留窗口及失败记录不改写，后续先核验现场再决定清理。

## 用户再次启动后的失败与补修

- `vscode-edit-e9e40be648de418bb45cabbede3143cf`：launch 在执行前返回 input_monitor_unavailable，未启动应用，cleanup=no_owned_window。监听心跳可用性仍需单独核查。
- 用户要求重新启动后，`vscode-edit-52d4ad27a3a6474499c1930ab08473e7` 再次出现 hook 不变、raw 每次增加 4 并要求确认；停止于 step-013，cleanup=retained_uncertain。该真机结果证明前一版修复尚未奏效。审计副本为任务根目录 consent-size-failure.json。
- 进一步发现 external_message 把 GetRawInputData 的复制字节数与整个 RAWINPUT 缓冲区容量强制比较。键盘结构小于联合体容量，合法短读被当作外部输入。新增 keyboard_read_may_be_smaller_than_union_buffer 先复现失败（target/raw-read-before.log），改为校验实际复制长度不超过容量，再校验事件头和有效负载。
- API 依据：[GetRawInputData 返回值](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getrawinputdata)。这项补修不代替真机通过证据。
- 补修后完整门禁通过：target/raw-read-gate.log。已安装显示端 SHA256 `2EFC9027C68D0999D95523599F20995B05F442F7478F6FA1CEA2B4A5A9CEE79F`，protocol=8、ready=true。
- 补修后轮次 `vscode-edit-13f3895024204d09888b158ecfc72726` 首次交接等待 25152ms 后 handoff_pending、action_started=false；1 次 shown、0 次 granted，cleanup=no_owned_window。未取得单次确认真机通过证据，不循环重启测试。

## 诊断补全

输入读取分支区分 read_failed、buffer_exceeded、invalid_length、unknown_kind、device_input、unrecognized_tag 和 own_injection。内存只保留计数及最近一次外部事件的长度/类型/时间元数据，不保存键值、设备句柄或坐标；随 consent_decision 写入有界历史。监听故障区分 watcher_missing、heartbeat_stale、registration_read_failed 和 registration_missing，并记录心跳年龄。

CHECK 拒绝和后台健康撤销会关联原始任务及调用记录 revoked；同调用去重。Consent 保留首次撤销原因，不被后续撤销覆盖（先失败复现 target/first-revocation-before.log）。VS Code 增加 -Smoke 小场景，仅启动、输入、保存读回和正常关闭，再决定是否执行完整编辑场景。

用户确认窗口激活时没有操作键鼠；该轮仍出现未识别标记的键盘事件，因此不能归因为用户接管。新增低级 hook 的设备事件、其他来源注入、自身注入计数用于后续区分。按用户要求优先完成可恢复暂停，实测与限制见 [55](55-resumable-desktop-tasks.md)；不将暂停恢复通过当作监听误判全部解决。

本轮进一步消除窗口根 UIA SetFocus 调用：已在前台不激活，否则单次原生前台请求并核验。先失败后通过的测试及迁移证据见 [56](56-workflow-inputs-migration.md)。这是移除已知可疑路径，未据此认定所有未标记输入事件均已解释。
