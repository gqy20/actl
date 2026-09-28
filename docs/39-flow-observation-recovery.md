# 39 · 观察中断与流程恢复

## 本批实现

1. 只读 STALE_REF 可显式启用有界重新查询；写操作不参与自动重试。
2. 恢复前通过 resume_check 确认当前业务上下文，随后核验 uncertain 步骤的结果。
3. 修复 --retry-step 在核验返回 STALE_REF 时仍可能重放写操作的问题。
   修复前复现日志保存在本地 `target/recovery-before.log`。

每次重新查询仍由 CLI 解析语义选择器，不使用旧 ref、不放宽唯一命中要求。
恢复检查适合核对当前会话、页面、记录或接收人，具体断言由流程作者提供。
它不自动证明 HWND 对应相同进程实例，也不凭“窗口存在”推导业务对象未变化。

## 验收矩阵

| 场景 | 自动化覆盖 | 真机状态 |
|---|---|---|
| 一次 STALE_REF 后重新核验成功 | 验证只重查、写入一次、审计落盘 | 待复测 |
| 持续 STALE_REF | 三次上限，保留未确定状态 | 待可控复现 |
| 权限/歧义/内部错误/目标不存在/断言失败 | 不作为 stale_read 自动重试 | 待分场景补齐 |
| 重查期间暂停或急停 | 不发下一次查询，不标记完成 | 真实按钮路径待验收 |
| 已写入但结果无法观察，显式重试 | 仍 needs_review，禁止重放 | 待隔离写操作验收 |
| 恢复时上下文不匹配 | 先停止，保留 uncertain；匹配后只核验已有结果 | 待真机上下文切换验收 |
| 人工键鼠介入分段输入 | 沿用已有交接与部分送达测试 | 人工接管→继续→业务核验待完成 |
| 输入 hook 被系统静默移除 | 无独立存活探针 | 未解决，不宣称输入检测绝对可靠 |

计算器上一轮的原始失败与 HWND 局部通过记录保留在 [38](38-flow-output-bindings.md)。
用户补充当时操作了其他软件；这是现场信息，不能将该次 STALE_REF 改记为已识别人工接管。

## 边界

已进入 Windows 的输入不能撤销，provider 调用不能保证立即中断。
只读查询不要求用户交接，允许人在其他软件工作；如果窗口树变化导致查询失效，
按声明策略重新查询或暂停，不通过抢焦点恢复。人工接管、业务断言失败、观察失效分别记录。
本批不增加常驻观察服务、自动刷新已完成输出或自动迁移计划。

## 2026-09-28 验证记录

新增 6 项引擎恢复测试与 1 项 CLI 预检测试通过。恢复重查审计有独立 golden；
原有流程、数据与时效测试通过。第一轮 gate 的 clippy 风格问题已修正；随后一轮
桌面契约测试有 3 项失败（2 项 STALE_REF，1 项前台为 actl 流程控制窗口），
日志保留在 `target/recovery-gate-final.log`。串行复查 28 项通过，日志为
`target/recovery-cli-serial.log`，不将该局部结果改写为原轮 gate 通过。
随后完整重跑 `cargo xtask gate` 通过，日志为 `target/recovery-gate-recheck.log`。

真机局部验证使用 `target/debug/actl.exe`，SHA256：
`5EA930790AE5E16B8F317AEE64407C533E1955264443A9949782B7BE3A4A73AC`。
Windows 11 10.0.26200 中文环境，沿用上一轮计算器现场。
证据目录 `target/recovery-live-20260928/` 保存二进制哈希、原生窗口、逐调用参数、
退出码、时间、JSON/stderr、脚本及本地计划。

原生查询确认 HWND 61738386 已最小化；get CalculatorResults 返回 NOT_FOUND。
不恢复窗口，运行带 resume_check 的只读计划：首次 checkpoint 暂停，恢复时
保存 paused/resume_check_failed、NOT_FOUND、pending、attempts=0，runner_active=false。
**上下文缺失时阻止 run 的真机负向路径通过**；未测试恢复匹配后的正向路径，
也未制造 STALE_REF 或人工输入，不宣称人工接管、写入恢复、hook 检测通过。
清理：没有新建或关闭应用窗口，没有写入应用；最小化状态和暂停记录保留。

## 日志可观测性补充

针对后续日志排查新增：call_finished 的流程结果摘要、workflow_invocation 阶段事件、
flow_resume_requested 的来源标记。语义以 [20 §7](20-yaml-workflow-protocol.md) 为准。
修复前“暂停流程缺少结束摘要”的复现保存在 `target/logging-before.log`。
来源只是调用方声明；旧显示进程不会主动获得新标记能力，须运行本次构建的显示端才会标记 signal_control。
本批不替换或重启正在运行的显示端，不将测试设置的来源标记当作真实点击证据。

新增 1 项核心历史契约测试和 2 项 CLI 集成测试通过，覆盖暂停摘要、来源白名单、
恢复检查开始/失败、业务内容不入日志，以及重复继续已完成流程的摘要。
本机使用 `target/debug/actl.exe` 对已完成的 recovery-live-guard-20260928 再次执行 resume：
history 记录 source=cli、workflow_completed=true、workflow_status=completed，
持久化 attempts 仍为 1、updated_ms 不变；没有重做应用查询或写操作。
证据保存于 `target/logging-live/`，被测文件 SHA256 为
`B4457B6A317155AB680DCFE38F7151826F11D89B9A3A6CD4B012913CA034C9BD`。

本批日志修复的 gate（`target/logging-gate.log`）中 fmt、clippy 和测试通过；发布构建
因运行中的 `target/validation/release/actl-signal.exe` 被占用，替换文件返回 os error 5，
因此**整轮 gate 未通过**。没有关闭或重启显示端，不将上一批门禁结果挪作本批通过。

用户随后明确授权将此类显示端关闭/重启作为常规维护，规则已记入 AGENTS.md。
核对 PID 55228 的路径与无活动 actl 命令后，正常关闭显示端；完整重跑 gate 通过，
日志为 `target/logging-gate-after-signal-close.log`。随后通过 display-start 启动新版，
display-status 确认 PID 27748、ready=true、responsive=true、stop_requested=false。
证据为 `target/logging-live/display-restarted.json` 与 `display-verified.json`；没有续跑旧流程。
