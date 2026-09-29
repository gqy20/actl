# YAML 流程协议 v1：评估、实现与边界

日期：2026-09-27。用户授权新增可持久化流程编排；原子命令接口继续保留。

## 1. 评估结论

采用独立 `actl-flow` crate，解析与执行状态机不依赖 Windows；CLI adapter 复用现有 clap 参数树
和 dispatch。YAML 负责计划，JSON 负责运行状态，JSONL 历史负责事件记录。不上新的脚本语言、
可视化设计器或常驻代理。第一版优先解决执行前审阅、步骤边界暂停、崩溃后的结果核对。

| 决策 | 理由 |
|---|---|
| 参数使用字符串数组 | 中文、空格、引号无二次 shell 拆分；不继承 batch 的 split_whitespace 限制 |
| 全流程预检 | YAML、重复 ID、命令参数结构、允许的命令种类、重试政策在第一步执行前验证 |
| 写操作必须 verify | 命令送达不足以证明目标达成；中断后需要只读后置断言判断是否已完成 |
| 写操作禁止自动重试 | 提交、发送、写入可能已生效，失败或进程退出后重放会造成重复 |
| 原计划不可覆盖 | 执行中修改源 YAML 不改变已保存计划；新计划用新运行 ID，不偷偷沿用完成标记 |
| 持久化在动作之前 | 先写 running，再执行；存储失败时不启动动作。崩溃后 running/verifying 均需核对 |
| OS 文件锁 | 同一存储目录单执行器；每运行另持活动锁，进程退出自动释放，无需删除过期锁文件 |

YAML 解析使用固定版本 serde-saphyr，关闭 include/robotics，拒绝重复键、合并键、锚点、别名、
未知字段及多文档，限制字节、深度和节点数。未使用已废弃的 serde_yaml。
依据：[解析器配置](https://docs.rs/serde-saphyr/1.3.0/serde_saphyr/struct.Options.html)、
[serde_yaml 状态](https://docs.rs/serde_yaml/latest/serde_yaml/)。

## 2. 文件格式

### 文件归属

可复用业务流程统一放在 `skills/actl/workflows/<slug>.yml`，由 actl 的单一
`SKILL.md` 入口引导 agent 选用；不创建独立业务 skill，不另设根目录 workflows。
一个 YAML 表达一个完整业务目标，可跨应用。目录与登记规范见
[流程索引](../skills/actl/workflows/README.md)。

`skills/actl/apps/` 保存应用知识，`fixtures/flows/` 保存引擎测试，`scenarios/`
保存真实应用验收脚本。引擎通过 `--file` 接收计划路径，不根据所在目录自动加载或执行。
个人任务输入留在本地；运行状态按下文存储协议保存，不回写技能内的源 YAML。
当前 v1 不支持自动引用 pack、参数插值或子流程调用，这项目录约定不改变协议能力。

### v1 计划示例

```yaml
version: 1
name: 填写报表
steps:
  - id: amount
    title: 填写金额
    pause_before: true
    before:
      command: verify
      args: ["--exists", "id:amount", "--app", "报表系统"]
    run:
      command: set-value
      args: ["id:amount", "1200", "--app", "报表系统"]
    verify:
      command: verify
      args: ["--value", "id:amount", "1200", "--app", "报表系统"]
    retry: never
    on_failure: pause
```

必填：version=1、name、steps；每步 id/title/run，run.command 与可选 args。
顶层可选 `handoff`（如 `handoff: {approval: once_per_run}`）及误触自动恢复策略
`handoff.pardon_input`（enabled/quiet_ms/budget/require_foreground/keyboard_safe_only，
逐流程覆盖全局默认，语义与安全边界见 [32](32-desktop-handoff.md)）。
name/title 最多 256 字节；1–256 步；id 使用历史协议的 1–80 位 ASCII ID 规则且不得重复。
输入文件和规范化计划各最多 256 KiB；每命令最多 128 参数，单参数最多 16 KiB，不含 NUL。
可选 before/verify/resume_check 只能调用 verify 或 wait；before 在进入 run 阶段前执行。
预检同时检查 wait/verify 恰好一个断言模式、断言目标语法及 wait 属性谓词，缺失或无效时在任何步骤前拒绝。

retry 默认为 never；read_only 仅适用于观察命令，最多尝试 3 次，间隔 200ms。
stale_read 仅对只读调用的 STALE_REF 做有界重新定位，规则见 §9；不会重试写调用。
on_failure 默认 pause，可设 fail；2026-09-29 起新增 skip_section（要求该步声明 section 字段）：
失败时保留该步 uncertain/failed 与证据，把同 section 内其余 pending 步骤标为 skipped，
继续执行后续段；整轮以 needs_review、reason=`section_failure:<step>` 收束，不记为 completed。
显式 --retry-step 成功后，同段 skipped 步骤还原为 pending 重新执行。
section 为 1–80 位 ASCII ID；跨段 bind 引用被跳过产出时会按绑定校验失败处理，分段模板应自包含。
pause_before 默认 false，首次到达该步骤时暂停，继续后不重复暂停。
步骤失败记录 attempt_errors（最近 8 次错误码）；同一步骤最后 3 次错误码相同时，
暂停/待核对 reason 追加 `;route_defect_suspected`，提示这是路线缺陷而非瞬态，不应盲目重试。
写操作（包括 launch、截图输出）必须有 verify，retry 只能是 never 或 stale_read。verify 的业务充分性由流程作者负责，
例如“窗口存在”不能作为“提交成功”的有效证据；程序只判断给定断言是否成立。

禁止 batch、嵌套 flow、管理命令、key-down/key-up；流程内不允许临时 `@eN` 或 `--snapshot`。
使用语义选择器，每次动作重新解析目标。`xy:` 与物理操作仍遵循原 CLI 的显式授权和输入护栏。
复杂目标可用 `uia:{"match":{"role":"Button","id":"clearButton"},"within":{"role":"Window","name":"计算器"}}`，
在 args 中作为一个字符串（YAML 单引号包裹）。组合条件、唯一容器和直接子级/后代范围见 docs/06 §3.2。
flow-validate 检查 run/before/verify 里的结构化目标；正文内容不会被误解析。只读示例：
`fixtures/flows/calculator-composite.yml`。不把一次成功预检当作实际窗口或元素存在的证明。
支持下文定义的显式输出绑定和完整参数替换，以及 [简单条件 when/unless/otherwise](63-simple-workflow-conditions.md)；不提供 shell 执行、字符串插值、通用表达式、嵌套分支、循环、并行或源文件热更新。
launch 仍具有原命令的程序启动语义，流程不是权限沙箱。

## 3. 命令

```powershell
actl flow-validate --file task.yml
actl flow-run --file task.yml --id report-01 --paused
actl flow-status --id report-01
actl flow-resume --id report-01
actl flow-pause --id report-01
# 已核实某一步需要重复后，才明确授权：
actl flow-resume --id report-01 --retry-step amount
```

run/resume 同步运行，返回时为完成、暂停、需核对、失败或停止；另一个终端或托盘可请求暂停。
预检输出 `{valid:true,version:1,name,steps}`。其余成功输出
`{run,runner_active,interrupted,pause_requested}`。失败/停止通过原错误 envelope 的 evidence 返回同一结构。
暂停/需核对为成功处理控制请求，不代表任务成功。flow-status 不修改执行状态。
语法/政策/重复 ID 为 PROTOCOL；执行器忙为 NOT_ACTIONABLE；停止为 ABORTED；存储失败为 INTERNAL。

## 4. 状态与恢复

运行状态：ready/running/paused/needs_review/stopped/completed/failed。
步骤状态：pending/running/verifying/completed/uncertain/failed/skipped，附 attempts、checkpoint_passed、
verified、error_code、attempt_errors。skipped 仅由段隔离产生，从未执行也不计尝试；
verified 只表示该步骤的给定后置断言通过。

流程暂停流程：

1. 保存 pause-requested；此时仅为请求，正在执行的调用继续。
2. 当前调用返回后，不启动下一动作；验证尚未执行则保留 verifying。
3. 执行器保存 paused 并退出，释放运行锁。只有此时才确认本流程暂停。
4. resume 清除该流程的暂停请求，不清除紧急停止标志。
5. 已完成步骤跳过；running/verifying/uncertain 先执行 verify。
6. 验证成功则不重放动作；写操作验证不成功进入 needs_review。
7. `--retry-step` 是显式重试授权，先核对后置条件，仍未满足才重跑 before/run/verify。

暂停是协作式控制：流程内 wait 的窗口、元素、消失、属性与稳定性等待在每轮轮询前检查暂停和停止。
暂停中断 run 中的只读等待后，步骤恢复 pending，清除本次控制中断的错误码；attempts 仍记录实际尝试。
before/verify 中的等待因暂停被打断时保留原步骤阶段，不把暂停报告为断言失败。
紧急停止沿用原有语义：写入后的验证被停止时保留 uncertain，恢复必须重新核对。
其他调用仍在命令边界暂停；不强杀 COM 或冻结线程，调用阻塞时暂停确认也会等待。
紧急停止继续走既有输入分段检查，不自动解除。暂停仅控制本流程，不宣称其他进程或用户输入均已停止。
调用间选择器重新定位、before 重新检查，不能提供通用事务回滚或恰好一次提交保证。

状态文件仍显示 running 但活动锁未占用时，status 返回 interrupted=true，不能当作仍在运行或成功。
读取状态时与下一次执行之间可能变化；执行和恢复以 OS 锁及最新磁盘状态为准。

## 5. 存储、显示与归档

`%LOCALAPPDATA%/actl/flows/<id>/`：plan.json、state.json、pause-requested、running.lock。
全目录另有 runner.lock。计划含原始操作参数，应作为任务资料管理；不自动复制到普通事件日志。
未完成运行记录不参与历史日志轮转；v1 不自动删除运行目录，完成后可按归档政策人工保留或清理。
本机用户可以编辑文件，因此这些文件不是防篡改审计或安全授权机制。

run/resume 的历史 task_id 固定为运行 ID，workflow 事件存状态元数据，流程内事件附 run_id/step_id 上下文。
信号快照 workflow 在 run_id/title/status 外可带 reason、step_id、error_code、actions；旧快照兼容。
工作流事件同样记录步骤 ID、原因与错误码，不自动记录标题或操作正文。状态条区分检查点、用户暂停、
前置条件失败和操作失败；详情说明下一步。actions 只是建议，不能绕过执行时检查。
状态条显示真实流程阶段和暂停结果；
托盘对当前显示的流程提供暂停/继续。紧急停止未解除时继续会被拒绝。
最终业务报告仍由智能体验证成果后通过 task-archive 提交；流程 completed 只表示定义的步骤全部完成。

托盘“流程控制”或 `actl-signal.exe --controls` 打开标准 Windows 控件窗口，字号 16 DIP，
按钮提供可访问名称、角色、禁用状态，支持 Tab/Shift+Tab 导航及 Esc 关闭。停止期间继续禁用；
“允许后续操作”只解除停止，不自动恢复。该窗口显示当前选中调用关联的流程，不是全任务列表。
自绘悬浮条与菜单尚未补齐完整 UIA provider，控制窗口提供独立可访问入口；不宣称屏幕阅读器验收完成。

## 6. 后续演进

不支持将旧运行进度自动迁移到修改后的 YAML。简单条件选择在动作前保存，未派发步骤恢复时重新观察，结果不确定时保留已选路线先核验；显式输出绑定见下文。
计划修订仍需关联和已完成步骤不变性检查，以及单独的人工核对协议。
跨应用成果判断、智能体修改计划、完整无障碍与多屏验证继续沿 docs/19 推进。

自动测试覆盖严格解析、状态 golden、边界暂停、检查点、未确认写入不重放、显式重试、
有限只读重试、停止粘性、锁释放、持久化失败阻止动作及 CLI 跨进程控制。
真实业务应用的 selector 与断言需要另外实测；`fixtures/flows/inspect.yml` 是无写入的入门流程。

## 6. 编辑器提示与断言证据

Schema 位于 `schemas/flow-v1.schema.json`。支持 YAML Language Server 的编辑器可在文件首行写
`# yaml-language-server: $schema=../../schemas/flow-v1.schema.json`，路径相对于 YAML 文件。
示例见 `fixtures/flows/calculator-verified.yml`。Schema 提供字段、枚举和结构提示；
UTF-8 字节限制、重复步骤 ID、CLI 参数和写操作验证政策仍以 `actl flow-validate` 为准。
结构和语义错误返回 `error.evidence.field`（如 `steps[0].run.args[2]`）和可用的 `step_id`；
YAML 语法错误由解析器给出位置，不保证存在可解析的步骤 ID。

| 断言 | 读取内容 | 用法 |
| --- | --- | --- |
| Name | 控件名称，包括部分应用的结果标签 | `verify --value id:CalculatorResults "显示为 2" --property name` |
| Value | 严格 ValuePattern | `verify --value id:Amount "100" --property value` |
| Text | 严格 TextPattern 正文 | `verify --value role:Document "已保存" --property text --contains` |
| 兼容默认 | ValuePattern，失败后尝试 TextPattern | 省略 `--property` |

以上命令应加目标应用作用域。快照可见、选择器可定位、属性可读取是三个独立条件。
`wait --property text=...` 支持等待正文；旧 `wait value=...` 保留 Value/Text 回退。
verify 返回实际读取的 channel；断言失败带 target/property/channel/expected/actual/comparison，
定位失败时实际值和通道可为 null。wait 超时同时保留 last_actual，channel 表示所用读取策略。
不可读取不是空字符串，也不能自动视为成功。

每步新增 `evidence: metadata | full`，默认 metadata。state.json 中的 error_evidence 默认仅保留
property/channel/comparison 和 redacted=true；full 才保存断言内容，单条上限 8 KiB，超限标记 omitted。
这只控制流程错误证据落盘，CLI 直接返回值和 plan.json 的原始参数仍可能包含内容；不要把秘密写进 YAML。
恢复时再次验证失败会更新错误码和证据，避免显示上一次结果。

写操作的 verify 应核对该操作产生的业务结果。计算器示例先清零再输入 2，分别核对结果名称为 0/2；
不要把“计算器窗口存在”当作输入完成。提交场景应检查回执编号、目标记录状态或保存后的字段值。
断言的业务充分性由作者负责，CLI 不能从窗口存在推导业务成功。

## 7. 操作追溯

`call_finished.phase=done` 表示命令已处理，不等于流程完成。执行型 flow 在结束事件中
额外返回 workflow_status、workflow_completed、workflow_reason、workflow_error_code；
暂停/待核验仍可有调用 error_code=null，同时保留流程错误码。无法读取流程上下文时字段缺省，
不推断成功。对已完成流程再次 resume 也会发布 completed 摘要，不重做步骤。

每次实际阶段调用记录 `workflow_invocation`，data 包含 phase（run/before/verify/resume_check）、
attempt（当前阶段调用内的重查序号，从 1 开始）、outcome（started/succeeded/failed）、error_code。
这些事件与持久化 run attempts 区别使用；没有结束事件时该调用结果未知，不补写成功。
原 step 事件只描述当前步骤，不表示一次命令实际执行。

`flow_resume_requested` 在进入恢复处理时记录 run_id、request_id、retry_step、source 和 source_verified=false。
source 为 signal_control（显示端通过环境标记启动）、cli（无标记）或 unknown（其他标记）。
来源为可伪造的诊断信息，不是授权；无标记也可能是旧显示端或其他脚本，不能从它证明人工点击。
事件不记录参数、检查值、输出业务内容或未知来源标记原文。来源标记不改变交接与急停规则。

暂停控制使用独立 control_request 日志，记录 requested/accepted/failed，不发布桌面反馈状态。
显示端另外记录 requested/dispatching/child_returned/failed，通过 ACTL_FLOW_REQUEST_ID 关联子进程。
child_returned 只表示控制子进程返回；accepted 只表示暂停请求保存成功。
执行器在安全边界保存暂停状态后记录 flow_pause_observed，并关联暂停标记中的 request_id；
旧的空暂停标记没有请求 ID。并发暂停以最后保存的标记为准，独立日志保留每次请求。

metadata 错误证据保留类型正确的 native_code、delivered_before_error、failed_segment_size、
action_started，以及已知 stage/reason；不保留实际值、期望值、目标或未知 reason 原文。
这些事实用于判断送达和失败阶段，不构成自动重放授权。

流程运行 ID 同时是历史 task_id。流程内事件新增顶层 run_id/step_id，call_id 标识一次 CLI 调用，
action 事件的 data.action_id 标识该调用中的动作。因此动作身份是 `(call_id, action_id)`，不是全局数字。
`actl history --task <run_id> --step <step_id> --reports` 可返回该步的事件、调用边界和任务报告。
报告是任务级，--step 不会把报告内容伪装成某一步的事实。历史轮转后不能保证永久追溯，
未完成 flows 恢复记录独立保留。

## 8. 显式输出与完整参数绑定（2026-09-28）

v1 新增可选 `steps[].outputs` 和 invocation 的 `bind`；旧计划不含这些字段时行为不变。

```yaml
version: 1
name: Read and verify
steps:
  - id: observe
    title: Read a value
    run:
      command: get
      args: ["id:amount", "--property", "value", "--app", "MyApp"]
    outputs:
      amount: {pointer: /properties/value, type: string, max_age_ms: 300000}
  - id: confirm
    title: Verify the captured value
    run:
      command: verify
      args: ["--value", "id:amount", "placeholder", "--property", "value", "--app", "MyApp"]
      bind: {"2": observe.amount}
```

- outputs 只允许在只读 run 步骤上声明，不从 before/verify 或错误响应导出。pointer 是相对于成功命令 `data` 的 JSON Pointer；空 pointer 选择整个 data，`~0`/`~1` 转义及数组索引按 JSON Pointer 解释，不支持通配符或表达式。
- 每个输出必须声明 type：string、number、boolean、array、object、null。字段缺失与显式 null 分开；空字符串、false、0、空数组/对象均保留。类型不符返回 PROTOCOL，不做隐式转换。
- 输出名称沿用 1–80 位 ASCII ID 规则；每步最多 32 项，单值 JSON 编码最多 16 KiB，全部运行输出最多 64 KiB。状态文件与计划各最多 256 KiB，超限拒绝写入。
- bind 使用带引号的零基参数索引作为键，例如 `"2"` 表示 args 第三个元素；值为 `先前步骤ID.输出名称`。不支持自身、后续或未声明输出引用；当前只允许 string/number/boolean 绑定命令参数。数组、对象与 null 可保存，但不能隐式拼成参数或展开参数列表。
- args 中绑定位置仍需提供符合命令语法的占位值，以完成静态结构预检。解析引用后替换完整参数；字符串原样传递，数字与布尔值使用 JSON 字面表示，不经 shell。不支持 `${...}` 等字符串插值。未解析成功时绝不使用占位值执行。
- 不允许绑定 command、选项名或新增参数；拒绝实际值以 `-` 开头、包含 NUL、超过 16 KiB 或为临时 `@eN`。这是本版限制，也包括负数字符串。所有实际参数再次按 CLI、目标和动作政策校验；合法类型不等于合法目标。绑定验证失败保存 paused/failed 与 reason=`binding_validation_failed`，本步 attempts 不增加、不执行 before/run/verify。
- 输出在 `steps[].outputs` 持久化，先前步骤只有 completed 才能被引用。恢复时校验已保存类型和预算；完成标记缺少声明输出会拒绝恢复。详见 [数据路径与故障测试](38-flow-output-bindings.md)。
- 已完成读取默认复用原业务数据；不宣称数据仍然新鲜。需要当前状态时使用显式 before/verify，并重新定位控件。写操作已成功且 completed 后不会因后续绑定失败而重放。
- 输出是显式保存的业务内容，会出现在 state.json、flow-status 与运行响应中；`evidence: metadata` 仅脱敏错误证据，不脱敏 outputs。未声明的完整响应不保存，普通 workflow 事件仍只记录状态元数据。

### 输出时效

- 每项输出可选 `max_age_ms`，必须是正整数（u64）；省略表示不限制业务数据年龄。
- 有输出的步骤保存 `outputs_captured_ms`，取本次读取调用开始的 Unix 毫秒时间，包含读取耗时；重试成功后保存本次时间。恢复、后置核验和读取状态不会刷新时间。
- 对当前步骤 run/before/verify 引用的输出，在步骤入口和每次 run 派发前检查：`当前时间 - 采集时间 <= max_age_ms` 时有效。前置条件等待或自动重试消耗时间后会再次检查。
- 超限、采集时间缺失、当前时间早于采集时间均拒绝消费，错误码 PROTOCOL；运行 reason 为 `output_freshness_failed`，按 on_failure 暂停或失败。保留步骤原状态与 attempts，已有 uncertain 写步骤仍不可重放。
- error_evidence 包含 stage=output_freshness、reason（expired/capture_time_missing/clock_moved_backwards）、source_step、output、max_age_ms、captured_ms、checked_ms、age_ms。只含标识与时间，metadata 策略也完整保留，无业务值。
- 检查失败不自动重新读取来源、不修改旧计划；先核对已完成动作，再准备新的后续计划。旧状态没有采集时间且输出声明了有效期时保守拒绝；未声明有效期的旧计划保持原行为。
- 时间使用系统墙钟，不能检测所有时钟调整，也不保证 provider 返回的内容本身是最新数据。这是派发前检查，不会中断已派发命令内部的等待/人工交接；后置核验仍执行。目标身份、接收人和业务状态须由 before/verify 及命令自身护栏另行核对。

## 9. 有界重新观察与恢复检查

`retry: stale_read` 可用于读或写步骤，但只对 run/before/verify/resume_check 中的只读调用生效。
遇到 STALE_REF 最多调用三次，间隔 200ms；每次重新执行语义定位。其他错误不触发该策略。
写 run 始终只调用一次；检查重试不导致 run 重放。原 read_only 策略仅用于只读 run，行为保持不变。
两者是互斥枚举，不叠加重试次数。

每次拟重试先保存 steps[].observation_retries：phase、error_code、retry_number。
记录表示已安排重试，不保证下一次调用已发生；run 的 attempts 才统计已进入调用阶段的次数。
每步累计最多 32 条，跨恢复保留，达到上限不再自动重查。记录不含业务值。
重试前检查暂停、急停和绑定数据时效；中断或保存失败阻止下一次调用。

可选 `resume_check` 与 before 一样是 verify/wait 调用，支持前序输出 bind。
只在运行从非 ready 状态继续时，对本轮到达的每个未完成步骤执行，位置在 uncertain 结果核验之前。
初次运行跳过，已完成步骤跳过；因此首次执行的必要条件仍放 before。
失败保留原步骤阶段与动作次数，按 on_failure 保存 paused/failed、reason=resume_check_failed；
急停和主动暂停优先。核验内容由作者声明，应匹配具体业务上下文，不能只检查软件窗口存在。

恢复写步骤时，verify 成功表示原结果已确认，可直接完成；verify 返回观察错误或无法确定结果时，
保存 needs_review、reason=result_observation_failed，即使 --retry-step 也不重放。
只有 verify 明确返回 ASSERTION_FAILED 且用户指定 --retry-step 才允许进入重新执行路径，
仍须通过 before 和现有写操作交接护栏。断言失败本身并不证明写入从未发生，作者与用户须确认重放适宜。
已有人工接管错误继续按 handoff 处理；STALE_REF 不会推断为人工接管。

## 可复用流程输入（2026-09-29）

`flow-validate` 和 `flow-run` 支持 `--inputs values.yml`（UTF-8 YAML 或 JSON 字符串映射）。
流程顶层 `inputs: {app: {}, text: {default: hello}}` 声明必填参数或字符串默认值。
四种调用阶段的 args 可用 `{input: app}` 替换一个完整参数，不做 shell 求值、拼接或环境变量展开。
缺少必填值、未知参数、重复键、非字符串、NUL、超过 16 KiB 或以 `-` 开头的输入返回 PROTOCOL。
最多 128 个声明；名称沿用 1–80 位 ASCII ID。所有解析后的命令仍须通过 CLI 和流程策略预检。
仅参数值可绑定，command、策略、步骤 ID 等不可参数化；命令选项应明确写在流程中。

```powershell
actl flow-validate --file skills/actl/workflows/verify-text.yml --inputs target/readback-inputs.yml
actl flow-run --file skills/actl/workflows/verify-text.yml --inputs target/readback-inputs.yml --id readback-001 --paused
actl flow-resume --id readback-001
```

输入文件示例：
```yaml
app: 唯一任务窗口标记
target: 'uia:{"match":{"role":"Edit","name":"任务编辑器"}}'
expected: 预期完整文本
```

运行前解析并保存具体 plan，保存内容没有 inputs 或绑定对象；恢复不重读模板/参数文件。
参数仍会进入本地运行计划，不能因错误证据脱敏而认为计划也已脱敏。
独立测试文件、Office 模板及 profile 可通过 resources 声明准备；引擎不执行任意 PS。

## 可恢复操作组与独立结果核验（2026-09-29）

`run: {sequence: [...]}` 将 1–16 个结构化 invocation 作为一个持久化步骤。每个子调用先预检，禁止嵌套、batch、管理命令、按键按住和子调用输出绑定。必须有最终 verify，禁止 read_only 重试及 outputs。组内沿用同一 task/flow ID，在每个子调用前检查停止与暂停；不是事务，没有回滚。

组开始前写入 running；任何已送达写入使整个组成为 uncertain。后续子调用即使在注入前让行，也不能将整个组改回 pending。恢复先执行 resume_check，再核对最终结果；完成则跳过，部分完成/未知保持待核对，不自动重放。仅明确的 --retry-step 可重放整组，界面继续不携带该参数。workflow_sequence 日志记录子调用序号、命令和错误码，不记录输入正文。

`flow-validate --emit-plan` 可返回解析、预检后的 plan，包含完整输入值。资源准备工具可将多份计划前缀化、合并并再次预检；不是运行中的嵌套流程。保存后的 plan 固定，不跟随源模板变化。

新增只读断言（可用于 before/verify/resume_check，以及只读 run）：

- `verify-file --path <path> --kind text|json|docx|xlsx --expected <JSON> [--subset]`：文本按 CRLF→LF 读取；JSON 按类型比较；docx 返回正文 text 与显式加粗 bold；xlsx 返回 sheet1 单元格的 value/formula。subset 仅忽略未声明的对象键，数组长度及顺序仍须一致。流程中的路径必须为绝对路径。docx 的 bold 指每个含文本 run 均显式声明加粗，不推断样式继承。
- `verify-rows --app ... --prefix <parent-id>-row- --expected <JSON string array>`：完整快照中必须有唯一父树，比较 TreeItem 行名集合（排序后保留重复项），截断或父树缺失拒绝。
- `verify-owner --app ... --program <absolute exe> --token <absolute task path> [--instance ... --hwnd ... --pid ...]`：核对唯一可见原生窗口、进程可执行路径、命令行中的完整任务路径参数，并返回 HWND、PID、进程创建时间。可选身份字段必须全部匹配；不把标题匹配单独当归属证据。
- `verify-closed --hwnd ... --pid ... --instance ...`：观察原窗口和原进程实例；窗口消失但进程仍在不算通过，PID 复用也不对新进程执行操作，状态矛盾/查询失败保留未知。
- `verify-clipboard --expected ... [--normalize-lines]`：精确比较剪贴板文本；显式规范化转换 CRLF 并去除尾部 LF。
- `verify --value ... --property text --normalize-lines`：全文比较前转换 CRLF/CR 为 LF、去除尾部 LF，与 contains 互斥；供 Word TextPattern 行结束符差异使用。

文件/行集合/关闭断言对已知不匹配在 5 秒观察窗口内复查，未知错误不转为空或成功。文件内容上限 4 MiB、单个 XML part 上限 4 MiB、expected 参数上限 16 KiB。OOXML 与进程元数据使用编译进二进制的固定 Windows PowerShell/.NET 只读检查器；请求仅通过 stdin JSON 传递，不拼接用户代码、不接受脚本路径、不使用 Office COM。XML 禁用 DTD/外部实体；单次检查器执行 20 秒上限，因此不是整个断言的 5 秒硬实时保证。无新增 Rust 依赖。

> 注（2026-09-29）：本段为当批历史记录。检查器已随全量原生迁移改为 zip + Windows.Data.Xml.Dom / Win32 / WMI 实现，运行时不启动 PowerShell；现行为以 [06](06-design-blueprint.md) 与 TECH_STACK §3 为准，迁移记录见 [61](61-native-tooling-migration.md)。

### 紧邻检查复用（2026-09-29）

资源初始化协议见下节；恢复不再次执行资源准备。

恢复执行时，只有仍为 pending 的步骤，且解析后的 resume_check 与 before 完全相同时，成功的恢复检查才直接满足此次前置检查。两者之间仍检查暂停/停止；不跨步骤、跨暂停或跨执行器缓存。uncertain/running/verifying 步骤仍先核验结果，显式重试前重新执行 before。不同参数或不同命令不合并。

## 声明式资源初始化（2026-09-29）

顶层可选 resources，严格拒绝未知字段：directories 为相对目录字符串数组，files 为 {path, text} 或 {path, copy} 数组，text/copy 必须且只能有一个。copy 相对于模板目录，规范化后仍须在该目录内；仅复制普通文件。目录与文件合计最多 128 项，每文件最多 1 MiB，总内容最多 4 MiB。文本仍受原 YAML 256 KiB 限制。

```yaml
inputs:
  file: {default: '${run.dir}/${run.id}.txt'}
resources:
  directories: [profile]
  files:
    - {path: '${run.id}.txt', text: 'Initial text'}
    - {path: 'document.docx', copy: 'assets/seed.docx'}
```

仅在 resources 存在时，inputs 的字符串 default 支持 ${run.id}、${run.dir} 与 ${env:NAME}。环境白名单为 LOCALAPPDATA、APPDATA、ProgramFiles、ProgramFiles(x86)、USERPROFILE；缺失或未知 token 拒绝。资源目标 path、directories 和 text 只支持 ${run.id}。copy 不展开 token；外部 --inputs 和步骤字符串不插值，不执行 shell 或递归展开变量。

run.dir 固定为 actl 状态目录/flow-resources/<ID>；资源目标路径必须相对、使用斜杠、长度不超过 240 字符，禁止空段、点段、父级穿越、盘符/ADS、Windows 保留名与尾随点/空格。大小写不敏感的重名和文件作为其他目标的父路径都拒绝。

flow-validate 使用 validation-preview ID，只读取复制源，不创建目录或 flow。含资源时 data.resources 返回 root、directories、files（每项 path/bytes）；emit-plan 的路径仅供预检，未初始化。flow-run 使用真实 ID，完整命令预检后独占创建资源目录和文件，再登记冻结计划。复用 ID、已有资源目录拒绝覆盖；初始化中断保留现场，不自动补写。--paused 也会初始化资源，但不执行步骤。flow-resume 不重读资源声明、默认值或复制源。覆盖默认文件路径不会重定向资源写入，外部输入资源由调用方准备。
