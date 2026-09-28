# YAML 流程使用说明

适用于 actl 0.1.6，包含编写、执行和恢复所需说明。
现有业务流程见 [流程索引](../workflows/README.md)。

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
- 不支持字符串插值、表达式、分支、循环、并行或自动引用 pack。物理操作仍需显式 `--physical`，沿用命令的焦点及桌面交接护栏。

```powershell
actl flow-validate --file task.yml
actl flow-run --file task.yml --id report-01 --paused
actl flow-status --id report-01
actl flow-resume --id report-01
actl flow-pause --id report-01
```

先预检，再执行；预检不证明实际目标存在。run/resume 同步返回，暂停或需核对不是任务完成。
运行状态保存在 `%LOCALAPPDATA%/actl/flows/<id>/`；修改源 YAML 不会改变已保存计划。
新计划用新 ID，并先核对已完成操作，避免重复执行。

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
- resume 不解除紧急停止；不得自动清除急停或代替用户点击桌面授权。
- 已完成步骤跳过；running/verifying/uncertain 先核验结果，核验成功则不重做动作。
- resume_check 用于恢复时检查会话、接收人、当前页面等业务上下文；首次运行不执行它，首次必要条件放 before。
- needs_review 时先观察。只有明确需要重复、后置核验返回 ASSERTION_FAILED，且用户授权后，才使用 `flow-resume --id report-01 --retry-step step-id`。
- 核验本身失败或结果未知（result_observation_failed）时，即使 --retry-step 也不重放。断言失败也不能单独证明原写入未发生。
- status 返回 interrupted=true 时，旧 running 状态不能证明进程仍在执行。恢复以最新状态和执行器锁为准。

## 结果与记录

completed 只表示步骤按所给断言完成；verified 只表示该步断言通过，断言充分性由作者负责。
运行 ID 自动关联任务历史。查询与归档方法见 [任务记录](task-records.md)。
