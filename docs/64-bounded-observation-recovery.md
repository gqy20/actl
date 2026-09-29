# 有界观察恢复

## YAML 契约

步骤可声明 `recovery: {max_retries: 2, timeout_ms: 3000, interval_ms: 200}`。三个参数均必填，分别允许 1..10、1..60000、1..timeout_ms。与旧 retry 策略互斥，省略时保持原行为。

预算是每步所有观察阶段共用的**额外自动重查**预算。适用于 when/unless、before、resume_check、verify 及只读 run；写入 run 不重试。首次失败时开始计时，预算跨暂停、继续及进程重启保存，自动重查在派发前扣除，崩溃不退还。继续可以重新观察一次，但不会补满自动重查预算。已派发而结果不确定的动作保持原路线、先核验，原显式重放要求不变。

仅 STALE_REF、TIMEOUT、NOT_FOUND 可自动重查；带 truncated=true 或 stage=handoff 的失败不重试。断言为假仍是条件结果，不自动等待它变真。歧义、权限拒绝、内部错误和含临时 @ref 的调用不自动重试；重新查询依赖 YAML 中稳定的语义选择器，不自动修改选择器、扩大搜索预算或选择候选。

timeout_ms 限制自动恢复的派发窗口，包括失败后的等待及观察耗时，不能强制中断正在执行的同步 UIA/provider 调用。达到时限后不再派发重查；单次调用仍遵循原命令的超时能力。等待期间至多每 25ms 检查暂停/停止，调用结束后再次检查。跨调用时钟倒退按预算耗尽处理。

## 状态、日志与交互

StepRecord 增加可选 recovery，含 retries、started_ms 和最近观察事件。事件含 phase/outcome/at_ms/retries/elapsed_ms/remaining_ms/error_code；动作结果与 branch 仍用原字段，不混为观察状态。观察开始、结果、预约重查、预算耗尽或需人工核对均写入状态并发布 workflow_recovery 历史事件，关联 flow 和 step。预算与事件结构有 golden 契约。

暂停摘要展示当前业务步骤、错误含义、已安排的自动重查次数及下一步。预算耗尽时说明“处理现场后继续可重新核查一次”，仍提供继续和停止；详细信息保留最近观察阶段、耗时及剩余预算。截断提示缩小到已确认容器；歧义提示核对父容器/名称/ID；这些是修复指引，不表示已自动生成或执行替代选择器。

## 验收与边界

2026-09-29 真机证据：`target/recovery-live-20260929-173716/`。使用 Windows、Chrome 和独立本地页面，不访问外站；被测二进制见 `target/foreground-direct-rounds/binaries.json`（模板最终轮使用同一 debug 构建），首轮旧发布版哈希另存该目录 binaries.json。

- 旧启动计划失败保留：空格分离的 Chrome --user-data-dir 参数被当作网页，等待标题超时；首次修正后窗口已出现但前台资格检查拒绝。原始 run.json/isolated-run.json 均未改写为通过。
- 最终计划把“启动并确认窗口存在”和“确保前台”拆成两步，窗口已创建时不因激活失败而重启。三个模板组合 8 步全部完成：启动、切前台、复用、已前台跳过、关闭可选弹层、弹层已无跳过、写值、值已达标跳过。最终记录 final-run.json。
- 最后一次值查询经历约 60 秒 NOT_FOUND，随后一次自动重查成功并 skip，没有重复写入。此前额外预算尚未启动，因此这次长调用未受 recovery.timeout_ms 限制；这是真实限制，不记作性能达标。定位单次 UIA 调用慢点仍待完成。
- 热启动新窗口与全新隔离目录冷启动各 4 步通过：新建窗口、前台、可执行文件/profile 归属、页面输入框值。热启动 HWND 5571310/PID 35004；冷启动 HWND 1182556/PID 212；证据在 new-windows/。
- 3 个测试窗口均经归属与单个网页标签核对后正常关闭；两个独立进程实例经 verify-closed 确认退出。收尾第一次核验因 PowerShell 自动转换 JSON 时间字符串而拒绝，未关闭窗口；改用 DateKind String 保留原时间后核验通过，相关失败及修正证据均保留。用户原有窗口保留。

新增测试先复现未知 recovery 字段（target/bounded-red.log），再覆盖瞬态恢复、预算持久化、继续不补预算、歧义/截断不重试、停止、时间预算、策略校验、状态 golden 和暂停说明。真实窗口/控件验收证据另行记录；测试通过不替代业务验收。

最终 `cargo xtask gate` 通过，日志为 `target/recovery-final-gate.log`。该门禁生成的 release CLI 与显示端已同步安装，显示端正常替换后 `display-status` 确认 ready/responsive。安装文件 SHA256 记录在上述真机证据目录的 `installed-final-binaries.json`。前述桌面实测使用 debug 构建；发布版此次确认安装和显示端就绪，未重新执行整轮桌面验收。

条件判断与界面操作之间仍可能发生变化。此实现不包含自动收集歧义候选、自动选择父容器、登录/验证码自动处理，也不增加 shell、表达式或补偿逻辑。已有候选预览可供 agent 在确认目标后使用；自动连接到 workflow 的候选纠正仍需独立身份与冻结计划协议。
