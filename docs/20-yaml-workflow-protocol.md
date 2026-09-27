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
name/title 最多 256 字节；1–256 步；id 使用历史协议的 1–80 位 ASCII ID 规则且不得重复。
输入文件和规范化计划各最多 256 KiB；每命令最多 128 参数，单参数最多 16 KiB，不含 NUL。
可选 before/verify 只能调用 verify 或 wait；before 每次实际重放前都重新执行。
预检同时检查 wait/verify 恰好一个断言模式、断言目标语法及 wait 属性谓词，缺失或无效时在任何步骤前拒绝。

retry 默认为 never；read_only 仅适用于观察命令，最多尝试 3 次，间隔 200ms。
on_failure 默认 pause，可设 fail。pause_before 默认 false，首次到达该步骤时暂停，继续后不重复暂停。
写操作（包括 launch、截图输出）必须有 verify 且 retry=never。verify 的业务充分性由流程作者负责，
例如“窗口存在”不能作为“提交成功”的有效证据；程序只判断给定断言是否成立。

禁止 batch、嵌套 flow、管理命令、key-down/key-up；流程内不允许临时 `@eN` 或 `--snapshot`。
使用语义选择器，每次动作重新解析目标。`xy:` 与物理操作仍遵循原 CLI 的显式授权和输入护栏。
不提供 shell 字符串执行、变量替换、数据绑定、分支、循环、并行或源文件热更新。
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
步骤状态：pending/running/verifying/completed/uncertain/failed，附 attempts、checkpoint_passed、
verified、error_code。verified 只表示该步骤的给定后置断言通过。

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

run/resume 的历史 task_id 固定为运行 ID，workflow 事件仅存 run_id/status，动作日志沿用原协议。
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

第一版不支持将旧运行进度自动迁移到修改后的 YAML，也不支持输出绑定与动态分支。
后续需要稳定步骤 ID、计划修订关联、已完成步骤不变性检查，以及单独的人工核对协议。
跨应用成果判断、智能体修改计划、完整无障碍与多屏验证继续沿 docs/19 推进。

自动测试覆盖严格解析、状态 golden、边界暂停、检查点、未确认写入不重放、显式重试、
有限只读重试、停止粘性、锁释放、持久化失败阻止动作及 CLI 跨进程控制。
真实业务应用的 selector 与断言需要另外实测；`fixtures/flows/inspect.yml` 是无写入的入门流程。
