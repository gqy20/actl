# 简单 workflow 条件

## 使用

步骤可声明 `when` 或 `unless`，二者互斥。`when` 成立执行 `run`；`unless` 成立跳过 `run`。相反结果执行 `otherwise`，未声明时跳过该步骤。两条路线均预检，任一路线无效都拒绝开始。

```yaml
version: 1
name: Ensure application
handoff: {approval: once_per_run}
steps:
  - id: ensure-window
    title: Reuse or launch
    when: {command: verify-window, args: [--app, Notepad, --state, exists]}
    run: {command: focus-window, args: [Notepad]}
    otherwise: {command: launch, args: [--wait-window, Notepad, --timeout, '10000', notepad.exe]}
    verify: {command: verify-window, args: [--app, Notepad, --state, foreground]}
```

示例标题须按系统语言调整；多个匹配窗口会拒绝。此流程仅负责确保窗口，不自动证明窗口归属或负责关闭。

可复用模板见 `skills/actl/workflows/`：

- `ensure-app-window.yml`：输入 app/program/ready_target，独立确认窗口存在、前台状态，再等待业务目标出现且稳定。app 使用唯一标题片段；启动分支不使用旧 HWND。ready_target 应指向页面内的业务控件，不能用窗口根代替；稳定采样不证明可写或焦点正确，业务流程仍须核验内容与动作条件。
- `dismiss-optional-control.yml`：输入 app/dialog/dismiss；dialog 是严格 `uia:` 选择器。在已确认的应用窗口内，有控件才点击 dismiss，再等待该控件消失。窗口本身必须保留；此模板不用于关闭整个窗口。
- `ensure-element-value.yml`：输入 app/target/value；已是目标值则跳过，否则 set-value 后核验。目标必须支持 ValuePattern。

调用方式：`actl flow-validate --file <workflow.yml> --inputs <inputs.yml>`，通过后 `actl flow-run --file <workflow.yml> --inputs <inputs.yml>`。输入文件为名称到字符串的映射。

## 判定与错误

条件只能使用即时只读断言：verify、verify-window、verify-file、verify-rows、verify-owner、verify-closed、verify-clipboard。不能使用 wait、动作命令、任意表达式或脚本。

成功为真，`ASSERTION_FAILED` 为假。仅 `verify --exists` 的严格 UIA 选择器且显式 `--app`，在已定位窗口/容器、完整查询零命中时，带 `phase=match/count=0/truncated=false` 的 `NOT_FOUND` 才视为假。窗口或容器丢失、匹配不唯一、查询截断、超时和 provider 错误均保留为未知；流程以 `condition_observation_failed` 暂停或按 on_failure 失败，不选择备用路线。

`verify-window --app <标题片段|hwnd:十进制或十六进制> --state <exists|absent|foreground|minimized|visible>` 使用原生窗口查询。无匹配时只有 absent 成立；多匹配为 `AMBIGUOUS`。命令成功返回 `{action,state,matched,window}`，window 无匹配时为 null；不成立返回 `ASSERTION_FAILED`，evidence 保留同结构。窗口消失不等于进程退出或资源已清理，收尾仍用归属核验及 verify-closed。foreground 表示系统前台窗口；visible 表示原生可见标志，不证明无遮挡，也不表示位于第一台显示器。

## 暂停与恢复

公共 before 和 pause_before 先执行，然后观察条件；同一步的 verify/resume_check 应适用于两条路线。选择结果在写入前保存为 branch=run/otherwise/skip。跳过记 completed、verified=false，不派发动作，也不执行该步 verify。条件步骤不声明 outputs，需要输出时增加后续只读步骤。

尚未派发的步骤恢复时重新判断；已经派发而结果不确定时保持原路线，先执行 resume_check 和 verify，不重判条件、不重放动作。条件观察期间暂停或停止也不会继续派发。run/otherwise 支持已有的至多 16 条 sequence，整个组仍按既有不确定结果核验规则恢复。

本次范围不含嵌套分支、循环、表达式和任意跳转。判断与动作间界面仍可能变化，因此动作自身护栏及执行后核验继续生效。

## 基础后处理：分阶段接通

基础后处理纳入 Phase 2 可靠性工作，不等待复杂流程。当前已实现条件分派、安全停止、未派发重判、不确定结果核验及 [有界只读恢复、暂停提示和日志](64-bounded-observation-recovery.md)。**歧义候选串联已落地（2026-10-08 spec，[06 AMBIGUOUS 目标串联](06-design-blueprint.md)：run 相多命中 → 引擎锚定骨架快照+候选全量保留 → `flow-resume --pick @eN` 应答，身份核验走既有 ReplayGuard）**；**自动缩小查询裁定不做**——引擎侧静默改写选择器违背冻结计划的显式变更纪律，缩小路径 = 调用方 `--pick` 显式选择或 `flow-revise` 显式修订。**循环已落地 for_each 形态（2026-10-11 spec，[06 for_each 数据驱动迭代](06-design-blueprint.md)）**：数据驱动的有界迭代（列表=前置步骤 array 输出、`@each` 占位注入、max 兜底不静默截断），通用 while/计数循环不做；**嵌套分支裁定缓办**——无真实场景，出现再立项。

| 现场情况 | 处理原则 |
|---|---|
| 原生窗口查询明确不存在 | 按显式 YAML 条件启动或跳过；不能把控件查询的窗口定位失败自动改为启动 |
| 已知窗口正在启动或重建 | 在次数和总时限内重新观察；启动结果不确定先核验，不能重复启动 |
| 查询被截断 | 使用已确认容器和更精确选择器缩小查询；不能只因扩大预算后仍无结果就宣称不存在 |
| 匹配不唯一 | 按已知父容器、ID、名称等条件消歧；仍不唯一时展示候选供一次选择，执行前重新核验身份 |
| 引用过期 | 重新观察、绑定新快照和目标；已派发动作先核验结果，不盲目重放 |
| 登录、验证码、权限提示 | 清楚说明用户要完成的操作，保留继续入口；完成后重新观察，不自动绕过权限 |

实施顺序：先交付有界只读恢复、预算耗尽后的暂停说明及完整日志，再接通歧义候选交互。恢复不得改写用户停止或接管意图，不反复抢焦点，不把暂停变成每步确认。

YAML 通过 recovery 配置受限次数、时限及间隔，引擎负责计时、持久化、取消与恢复，应用档案维护定位知识。引擎仍不自动引用应用档案；若需要此能力，先补协议、golden 和文档，避免各流程复制恢复脚本。

暂停界面应显示目标、失败原因、已尝试措施和下一步操作；用户可继续或停止。日志应关联 flow/步骤/路线，保留原错误、每次观察及恢复措施、次数、已耗时/剩余预算、退出原因和核验结果。预算不得因重启或点击继续被静默重置；显式开启新一轮须保留上一轮证据。

验收至少覆盖：短暂失败恢复成功；持续失败按次数或总时限退出；截断与歧义不进入否定分支；观察期间暂停/停止无后续写入；重启恢复不无限重试；不确定启动或写入不重复派发；候选选择后控件重建可被拒绝。系统 fixture 与真实应用各留独立证据，静态门禁不代表业务验收。

复杂流程阶段再考虑嵌套业务分支、替代业务路线、补偿和回滚；上述基础能力不依赖它们。

## 验证

新增引擎分支恢复测试、CLI 命令/envelope golden 和实际 CLI 只读条件测试。首轮复现保留在 `target/branches-red.log`；结果见 `target/branches-flow-tests.log`、`target/branches-cli-tests.log` 和最终门禁日志 `target/branches-gate.log`。三个模板的参数预检不代表真实软件业务验收；点击关闭弹层、ValuePattern 写入及启动后前置的组合仍须逐应用验证。

2026-09-29 Windows 当前交互桌面，以现有 ZCode 窗口的现场 HWND 完成只读 flow：已存在选择 run、明确不存在选择 otherwise、unless 已存在选择 skip，三步均 completed，attempts 分别为 1/1/0。未启动、关闭或修改应用，无新增 GUI 资源需要清理。具体计划、逐次 envelope、状态及 debug 二进制 SHA256 保存在 `target/branches-live/`；这是条件分派实测，不替代三个写入模板的业务验收。

最终 `cargo xtask gate` 通过（含 11 个 pack、296 步校验）。发布版已更新至 `C:\Users\gqy17\.cargo\bin\actl.exe`，SHA256 `A3780B93962BB1A4E56583CD07CEC3F1B20B665A1C636250341C534BFA2E65F3`；同一只读计划发布版复验三步通过，证据为 `target/branches-live/release-run.json`、`release-status.json`、`installed-binary.json`。显示端未变更、未重启。

## 写入侧三模板真机验收（2026-10-01，全自驱，记事本确定性构造）

- **窗口复用/启动**：`unless verify-window exists` 门控 launch——首轮启动并
  wait 到窗；次轮 ensure 步 **attempts=0（launch 跳过）**、窗口数保持 1。
- **已有值跳过**：`unless verify --contains` 门控写入——首跑 attempts=1 写入、
  次跑 **attempts=0 跳过**，两轮均 completed。
- **可选控件关闭**：`unless verify-window <对话框> absent` 门控 esc——无框
  轮 attempts=0 跳过；有框轮 attempts=1 关闭且框消失。
- 途经实录：流程内 type 在窗口后台化+对话框滞留时 NOT_ACTIONABLE（前台护栏
  正确拒绝），focus-window 后恢复——写入侧模板依赖前台就绪，与判定树一致。
