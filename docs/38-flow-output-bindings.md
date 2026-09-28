# 38 · 流程输出绑定与恢复

日期：2026-09-28。本批实现现行 v1 的可选字段扩展，不引入分支、循环、子流程或表达式。
协议与限制以 [20](20-yaml-workflow-protocol.md) 为准。

## 数据路径

CLI adapter 将命令的 `data` 返回给流程引擎。只有步骤明确声明的字段才进入
`state.json` 的 `steps[].outputs`；完整原始响应不默认保存。
来源由保存的 plan 中的步骤 ID、命令和 JSON Pointer 确定；不会自动重新采集已完成步骤。

步骤执行前，解析来自先前已完成步骤的引用，并对替换后的 run/before/verify 重新做 CLI 预检。
缺失输出、类型错误或无效参数阻止本步动作。占位参数用于静态 CLI 预检，执行时必被指定绑定替换，
无法解析时不回退执行占位值。

## 持久化与中断

- 无后置条件的只读步骤：选定输出与 completed 标记在同次状态写入中保存。
- 有后置条件的只读步骤：输出先与 verifying 状态保存，验证通过后才允许其他步骤引用。
- 进程在输出保存前退出：磁盘仍是 running，恢复时可重新执行只读命令。
- 恢复时 verify 成功但声明的输出未保存：仍须重新读取，不能仅凭 verify 标记完成。
- 输出保存失败：执行器返回错误，不启动后续步骤；恢复以磁盘状态为准。
- 写步骤不允许 outputs；原有 uncertain 核验和显式重试规则继续生效。

保存的业务值是当时的观察结果，不保证恢复时界面仍相同。控件 ref 不可作为绑定参数，
窗口/控件须重新观察或通过 before/verify 核对，过期业务结果的重新采集使用新的运行计划。

## 数据有效期

需要限制年龄的输出可以声明 `max_age_ms: 300000`（五分钟）。引擎保存
`outputs_captured_ms`，从来源 run 调用开始计时；只要当前步骤引用其中一项，
就在步骤入口与每次 run 派发前检查。前置条件的等待耗时也计入年龄。
失败保留原步骤阶段和已有结果，按 on_failure 暂停或失败；`flow-status` 的
error_evidence 指明来源、输出名称、时间和失败原因，不保存业务内容。

省略有效期时不限制年龄。时效检查不能证明 UIA 内容本身最新，也不能替代目标身份或
接收人核验。已派发命令内部等待不受本检查中断；墙钟调整存在固有限制，详见协议。
下一步的观察契约、目标能力与恢复核验仍需单独实现和验收。

## 验证

`crates/actl-flow/tests/data.rs` 覆盖跨存储恢复、已完成写入不重放、缺失/错类型输出、
空值与 null、非法引用、运行时参数拒绝、输出保存失败，以及 interrupted read 的重新读取。
状态 golden 固定 outputs 的持久化结构；CLI 测试验证静态预检成功后仍会拒绝无效的实际参数，且不启动目标调用。

只读 fixture 为 `fixtures/flows/output-binding.yml`：读取计算器结果名称、检查点暂停、
恢复后核对保存的名称。它是引擎与系统应用测试，不是 `skills/actl/workflows/` 的业务交付。
本批不发送消息，不操作音乐或联系人，不将受控测试记为音乐分享场景通过。

2026-09-28 验证结果：`cargo xtask gate` 通过，日志保存在本地
`target/flow-bind-gate.log`。新增 10 项引擎数据测试与 2 项 CLI 契约测试通过；
CLI 测试包含跨进程恢复后执行只读查询。门禁中的急停用例使用隔离环境，
预期阻止动作，不代表真实应用验收。计算器 fixture 尚未进行真机验收。

时效扩展新增 5 项引擎测试与 1 项 CLI 契约测试，覆盖持久化恢复、精确到期边界、
缺失时间、时钟回拨、前置条件耗时、fail 策略和已完成写入不重放。
`output_expired_v1.json` 固定诊断契约，`data_completed_v1.json` 增加采集时间。
2026-09-28 时效扩展的 `cargo xtask gate` 通过，日志为本地
`target/flow-freshness-gate.log`。上述 6 项新增测试全部通过；真机验收状态不变。

## 2026-09-28 计算器真机验收

环境：Windows 11 中文版 10.0.26200，zh-CN；计算器 11.2607.0.0。
被测文件为仓库内 `target/validation/release/actl.exe`，版本 0.1.5，SHA256：
`F817F24ABE189F5EBA7C20EC184158AD5C0553D59834BA0D65CE6B857C1C69B8`。
源码基于 91623d8 加本次未提交的时效扩展；证据目录为 `target/flow-live-20260928/`。
该目录保存环境、脚本、输入计划、逐调用参数/退出码/时间、stdout/stderr 与历史事件。

启动前无计算器窗口、急停为 false；显示端 companion_missing，经 display-start 变为 ready。
actl launch calc.exe 成功，随后观察到计算器 HWND 61738386、宿主 PID 19092，
CalculatorApp PID 64356，启动时间 18:02:02 +08:00。没有输入数字或改变计算内容。

| 运行 | 结果 | 证据 |
|---|---|---|
| 原始 fixture，按标题定位 | **失败**：来源输出“显示为 0”保存成功；恢复时 confirm 报 STALE_REF，channel=children，运行暂停 | normal-run.json、normal-resume.json |
| 同现场 HWND 限定的本地计划 | **通过**：跨进程恢复完成，两步 attempts 均为 1；再次 resume 无新增执行 | hwnd-normal-run.json、hwnd-normal-resume.json、hwnd-normal-resume-again.json |
| HWND 限定、有效期 10 秒 | **通过**：真实等待 11 秒后恢复，实际 age_ms=12405；PROTOCOL/expired，confirm 仍 pending、attempts=0，来源 attempts=1 | hwnd-expiry-run.json、hwnd-expiry-resume.json、hwnd-expiry-status.json |
| 结束读取 | **通过**：结果仍为“显示为 0”，显示端 ready、急停 false | hwnd-get-after.json、display-after.json |

限定 HWND 的计划只存在本地证据目录，未向公共 fixture 写入固定句柄。
首次错误后原生窗口仍存在、精确 HWND 的 get 成功；后续通过不能证明标题路径根因，
也不能改写首次失败。**结论为限定窗口的输出绑定、恢复和自然过期通过；原始标题 fixture 未通过。**
本轮没有真机写操作，不扩大为“写入恢复不重放”或完整业务场景验收。

清理：保留计算器与显示端；UWP launch PID 15756 与实际窗口宿主/应用 PID 不同，
缺少任务标记，不能仅凭新窗口差集和启动时间认定独占归属，因此未发送 close-window。
保留正常完成、首次失败及预期过期暂停的运行记录，无活动 runner；未清除急停或修改采集时间。

## 2026-09-28 STALE_REF 根因与修复

原始标题 fixture 的失败已复现并归因。**根因**：`find_window`（及 `uia:` 作用域的
标题路径）对桌面根做一次整批 `FindAll(Children)`，这是全有或全无的调用——任何
**无关**顶层窗口处于创建/销毁中途（provider 返回 UIA_E_ELEMENTNOTAVAILABLE，
0x80040201），整批枚举失败，所有应用的标题定位都报 STALE_REF；窗口不存在时还会
把 NOT_FOUND 误报成 STALE_REF。HWND 路径走 `element_from_handle`，不做桌面根
枚举，因此当日限定 HWND 的计划全部通过——同现场后续通过不能改写首次失败，
但两者差异自此得到解释。

复现：扰动脚本以 350ms 周期调用 `Start-Process notepad`；本机 `notepad` 无文件关联，
每次启动退化为"选择应用以打开 notepad"系统对话框——顶层窗口创建/销毁竞态由此
成立（扰动本质是顶层窗口churn，与窗口种类无关；此前写成"记事本开关联"不准确，
已更正）。修复前 9/10 循环在 observe 步骤报 STALE_REF（channel=children，
native_code=0x80040201）。脚本与证据在本地 `target/flow-stale-repro-20260928/`
（churn.ps1、cycle.ps1、probe.ps1、discrim.ps1 及逐调用 JSON）。遗留的 8 个
选应用对话框已逐一 WM_CLOSE 清理，核实 0 残留。

修复：`observation::with_transient_retry` 仅对桌面根枚举做有界瞬态重试
（仅 StaleRef，3 次/200ms，与 flow 层 stale_read 同参数；其他错误立即上抛，
持久失败保留错误并在 evidence 附 attempts）。`find_window` 与
`scoped_locate::window` 标题路径共用 `observation::desktop_children`；后者此前
把枚举失败误映射为 Internal，现统一为 StaleRef。窗口子树枚举不加此重试——
目标应用自身的控件失效是另一故障域，由既有语义处理。

验证：同机同日，修复后净环境 8/8 循环完整通过（run→checkpoint→resume→completed）；
显示端重启与扰动下 36/36 直查成功（一次 13.1s DCOM 拥塞离群，trace 显示耗时
在 locate.window 阶段）；无匹配窗口探针 30/30 返回 NOT_FOUND、0 次 STALE_REF。
新增 3 项单测覆盖瞬态恢复、持久失败保留尝试次数、非 StaleRef 不重试。

**遗留观察项**：显示端 UI 创建与外部窗口扰动同时发生的数秒窗口内，曾出现
4/10 循环的元素级 NOT_FOUND（窗口命中但子树查询空结果，fail-closed 保留）。
属独立的观察层瞬态现象，未随本次修复扩展范围，待按 ROADMAP 局部观察条目
单独核对；不应把它当作目标缺席直接驱动业务动作。
