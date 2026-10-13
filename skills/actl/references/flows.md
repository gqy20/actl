# YAML 流程使用说明

## 无准备脚本的应用流程

应用验证模板现在声明 resources 和输入默认值，可直接 `actl flow-run --file skills/actl/workflows/vscode-edit.yml --id vscode-001`。每次从头运行使用新的 ID；恢复仍用 flow-resume 和旧 ID。

resources.directories 声明相对目录，resources.files 每项为 {path, text} 或 {path, copy}，copy 相对 YAML 目录。资源只写入 actl 状态目录/flow-resources/<ID>，拒绝已有目录及文件覆盖。输入 default 支持 ${run.id}、${run.dir} 与有限环境变量；资源路径和 text 只支持 ${run.id}。外部 inputs 和步骤字符串不做插值。不支持脚本命令。

flow-validate 只读预检，返回使用 validation-preview 的资源清单，不创建文件；emit-plan 不证明资源已准备。flow-run 完整预检通过后准备资源，--paused 也会准备；flow-resume 不重置资源或重读模板。初始化失败保留现场，不自动清理或重试。当前输入覆盖不会改变资源声明，覆盖到外部路径时须自行准备并核对文件归属。

适用于 actl ≥0.1.9，包含编写、执行和恢复所需说明。
现有业务流程见 [流程索引](../workflows/README.md)。

## 浏览器命令在流程中的规则

browser-* 命令可入 YAML 流程，但有专属治理规则：

1. **`browser-launch` 不能带 `outputs`**（生命周期命令，不是只读生产者）——
   会话句柄经 `browser-list` 以观察者身份产出（`pointer: /sessions/0/session_id`），
   后续步骤 `bind` 注入 `--session` 值位。
2. **有副作用的步骤必须带 `verify`**——`browser-verify` 断言族可用：
   launch 后 `--newest-alive`、close 后 `--none`、写入后 `--session X --alive`。
3. **登录暂停点**用 `pause_before: true` + `resume_check: browser-verify --css <登录标记>`
   （用户在可见窗口完成登录后恢复；谓词不满足保持 paused）。
4. **值级断言在写动词上**（`browser-click --expect-text` / `browser-type` 自动读回核验），
   不用通用 `verify` 命令。
5. **持久 profile 必须声明**：`browser-launch --profile <name>` 需在
   `resources.browser_profiles` 声明该名字（未声明即 PROTOCOL 拒绝）。

示例流程：`workflows/browser-login-session.yml`（登录暂停+复用全链）。

## 编写与预检

```yaml
version: 1
name: 核对金额
steps:
  - id: check-amount
    title: 核对金额为 1200
    run:
      command: verify
      args: ["--value", "id:amount", "1200", "--property", "value", "--app", "报表系统"]
```

这是需按实际应用调整的格式示例，不是已验收的业务流程。

- 必填 version=1、name、steps；每步必填 id、title、run，run 包含 command 和 args 字符串数组。
- 计划最多 256 KiB、1–256 步；name/title 最多 256 UTF-8 字节。ID 为 1–80 位 ASCII 字母、数字、`-`、`_`，步骤 ID 不得重复。
- 可选 before、verify、resume_check 只能调用 verify 或 wait。每个断言需明确目标和条件。
- 写操作（包括 launch、截图输出）必须提供 verify；核对业务结果，不能用“窗口存在”替代“提交成功”。
- retry 默认 never；read_only 仅限只读 run。stale_read 只重试阶段中的只读 STALE_REF，最多三次、间隔 200ms，写 run 不因此重放。on_failure 为 pause（默认）或 fail。
- pause_before: true 在首次进入步骤前暂停。evidence 默认为 metadata；full 会保存断言内容。计划参数和显式输出本身仍可能包含敏感信息。
- 用语义选择器，每次重新定位。禁止临时 `@eN`、`--snapshot`、batch、嵌套 flow、管理命令和 key-down/key-up。
- 支持简单 `when`/`unless` 与可选 `otherwise`：条件为即时只读断言，成功为真、ASSERTION_FAILED 为假，其他错误暂停。严格 UIA `verify --exists` 仅在已确认窗口/容器中完整查询零命中才为假。未派发步骤恢复重新判断；不确定步骤保留路线先核验。条件步骤不声明 outputs。不支持字符串插值、通用表达式、嵌套分支、循环、并行或自动引用 pack。物理操作仍需显式 `--physical`，沿用命令的焦点及桌面交接护栏。
- `verify-window --app <唯一标题片段|hwnd:句柄> --state exists|absent|foreground|minimized|visible` 可判断原生窗口状态；visible 不证明无遮挡。窗口缺失时 absent 为真、其他为假；多个匹配报 AMBIGUOUS。模板见 workflows/ensure-app-window.yml、dismiss-optional-control.yml、ensure-element-value.yml。两条路线都预检，公共 before/resume_check/verify 须适用于两条路线。
- 可配置 `recovery: {max_retries: 2, timeout_ms: 3000, interval_ms: 200}`，与 retry 互斥。预算按步共享并持久化，仅额外重查 STALE_REF/TIMEOUT/NOT_FOUND 的只读观察，截断、交接和临时 ref 不重试。继续可重新观察一次，但不补满预算；超时限制后续派发，不能强制中断同步 provider 调用。不会自动缩小查询、选择歧义候选或启动窗口；已经派发的动作先核验。

```powershell
actl flow-validate --file task.yml
actl flow-run --file task.yml --id report-01 --paused
actl flow-status --id report-01
actl flow-resume --id report-01
actl flow-pause --id report-01
```

先预检，再执行；预检不证明实际目标存在。run/resume 同步返回，暂停或需核对不是任务完成。
运行状态保存在 `%LOCALAPPDATA%/actl/flows/<id>/`；修改源 YAML 不会改变已保存计划。
步骤级修复用 `flow-revise --id <run> --file <修后.yml>` 版本化修订原地续跑（完成步规格
未变保留结果；写步变更需 `--accept-rerun` 点名；uncertain 步变更即重试授权；旧计划
字节归档、谱系在 flow-status 的 revisions 可见；`--resume` 链式续跑）。路线级重构或
需要干净对照才开新 ID。失败采集包（截图/窗口清单/UIA/弹层）用 `flow-evidence
--id <run> [--step <sid>]` 读取。

## 输出与绑定

只读 run 可声明 outputs，pointer 相对于成功响应的 data；每项声明 type：
string、number、boolean、array、object 或 null。例如：

```yaml
outputs:
  amount: {pointer: /properties/value, type: string, max_age_ms: 300000}
```

后续调用可写 `bind: {"2": observe.amount}`，将 observe 步骤的 amount 输出替换 args
第三项（零基索引）。该位置仍需提供合法占位参数供静态预检；解析失败不会执行占位值。
只能引用先前已完成步骤的已声明输出，只有 string/number/boolean 能绑定完整参数。
不绑定命令、选项名，不拼接或展开参数；负号开头值、NUL、临时 ref 和超长参数会被拒绝。

每步最多 32 个输出，单值 JSON 最多 16 KiB，全部输出最多 64 KiB。输出会写入状态文件和
flow-status 响应，metadata 错误脱敏不覆盖输出内容。完成步骤默认复用原输出，不自动重新读取。

对易变数据设置正整数 max_age_ms。缺采集时间、时间倒退或数据过期都会拒绝消费，
reason 为 output_freshness_failed。不要修改时间或从头重跑；先核对已有动作，再准备后续计划。
时效检查不替代目标身份及业务状态核验。

## 暂停与恢复

- flow-pause 只保存暂停请求；等 flow-status 显示 paused 才确认暂停，在途调用可能尚未结束。
- resume 不解除紧急停止，不得自动清除急停。用户明确授权代操作时，可用 computer use 核对任务后点击可见的开始/继续；不得伪造授权、抵消用户暂停/停止或盲目重放不确定动作。
- 已完成步骤跳过；running/verifying/uncertain 先核验结果，核验成功则不重做动作。
- resume_check 用于恢复时检查会话、接收人、当前页面等业务上下文；首次运行不执行它，首次必要条件放 before。
- needs_review 时先观察。只有明确需要重复、后置核验返回 ASSERTION_FAILED，且用户授权后，才使用 `flow-resume --id report-01 --retry-step step-id`。
- 探索运行的 `reason=decision_pending`（悬挂决策卡）有三类应答方式，回填通道唯一：
  人/agent 读 `flow-status` 的 `data.decision_card` 自选；经 `actl decide`（`--run <id>` 直读悬挂卡，
  或管道 `actl flow-status --id <run> | actl decide`）问配置的决策端点拿 `{choice, confidence}`——
  危险候选构造前已剥离、低于置信门槛即弃权（弃权=裁决权交回人）；或
  `flow-run --auto-decide`（2026-10-13 授权，探索专属）：run 停卡时 actl 自问自答续跑直至终态，
  任何弃权/停止/暂停/预算耗尽停下交回人。
  手工应答统一经 `flow-resume --id <run> --decision <choice_id>` 回填；decide 只生成应答，不执行、不续跑
  （auto-decide 的续跑=引擎同一校验链逐次回填）。
  配套流程：`workflows/explore-decide-answer.yml`（decide 步+输出保存，回填仍归编排层）。
- 核验本身失败或结果未知（result_observation_failed）时，即使 --retry-step 也不重放。断言失败也不能单独证明原写入未发生。
- status 返回 interrupted=true 时，旧 running 状态不能证明进程仍在执行。恢复以最新状态和执行器锁为准。

## 结果与记录

completed 只表示步骤按所给断言完成；verified 只表示该步断言通过，断言充分性由作者负责。
运行 ID 自动关联任务历史。查询与归档方法见 [任务记录](task-records.md)。

## 可复用流程输入（2026-09-29；--set 2026-10-13）

`flow-validate` / `flow-run` / `flow-revise` 支持 `--inputs values.yml`（UTF-8 YAML 或 JSON 字符串映射）
与可重复 `--set key=value` 内联覆盖（**set 优先于 --inputs 同名键**——换词重跑免值文件；
在第一个 `=` 处切分；`--set` 自身同键给两次、缺等号、空键返回 PROTOCOL）。
流程顶层 `inputs: {app: {}, text: {default: hello}}` 声明必填参数或字符串默认值。
四种调用阶段的 args 可用 `{input: app}` 替换一个完整参数，不做 shell 求值、拼接或环境变量展开。
缺少必填值、未知参数、重复键、非字符串、NUL、超过 16 KiB 或以 `-` 开头的输入返回 PROTOCOL
（--set 的值与键沿用同一裁决）。最多 128 个声明；名称沿用 1–80 位 ASCII ID。
所有解析后的命令仍须通过 CLI 和流程策略预检。
仅参数值可绑定，command、策略、步骤 ID 等不可参数化；命令选项应明确写在流程中。

```powershell
actl flow-validate --file skills/actl/workflows/verify-text.yml --inputs target/readback-inputs.yml
actl flow-run --file skills/actl/workflows/quark-search.yml --id search-01 --set app=夸克网盘 --set query=今日天气
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

文件/行集合/关闭断言对已知不匹配在 5 秒观察窗口内复查，未知错误不转为空或成功。文件内容上限 4 MiB、单个 XML part 上限 4 MiB、expected 参数上限 16 KiB。OOXML 由 zip 解压指定 XML part，再经 Windows.Data.Xml.Dom 只读解析；进程身份通过 Win32 句柄读取，完整命令行通过 WMI 读取。运行时不启动 PowerShell，不使用 Office COM。XML 禁用 DTD/外部实体，最大深度 128；WMI 枚举等待上限 5 秒，无独立可中断的硬超时，断言不保证 5 秒硬实时。依赖登记见 TECH_STACK §3。

### 暂停与 UIA 超时排障（2026-10-02）

先保留失败 envelope 和冻结运行记录，再用 `actl history --task <run_id> --timeline --limit 500`
合并查询阶段与控制请求；不要只看最后一次返回。检查 total/truncated，仍截断可把 limit 增至 2000。
`observation_finished` 的阶段、耗时、native_code 与 observation_span_id 可关联失败步骤；
TIMEOUT 表示明确超时，pause_requested 表示已观察到暂停请求，二者不能互相归因。
前台/输入变化是相关证据，输入监听缺失、过期或显示端重启时保留未知；信号计数不等于键数/点击数。
`source_verified:false` 的控制来源仅供诊断，不作为用户授权。

### 紧邻检查复用（2026-09-29）

恢复执行时，只有仍为 pending 的步骤，且解析后的 resume_check 与 before 完全相同时，成功的恢复检查才直接满足此次前置检查。两者之间仍检查暂停/停止；不跨步骤、跨暂停或跨执行器缓存。uncertain/running/verifying 步骤仍先核验结果，显式重试前重新执行 before。不同参数或不同命令不合并。
