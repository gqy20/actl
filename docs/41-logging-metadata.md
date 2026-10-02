# 41 · 日志健康与控制元数据

更新：2026-10-03。现行字段见 [设计蓝图](06-design-blueprint.md)，流程语义见 [20](20-yaml-workflow-protocol.md)。

## 实现范围

- history 检查保留事件的版本、调用/任务身份、连续 seq 与调用首尾。步骤筛选不会掩盖原始调用的损坏。
- log-health 独立保存 journal_open、journal_write、state_write 失败。日志追加失败后停止追加并累计后续丢弃事件；保留原始 I/O 错误类别与原生码。
- 暂停请求和显示端控制派发使用独立日志，不改正在执行的桌面反馈。request_id 连接父子进程和持久化暂停边界。
- metadata 错误证据仅保留白名单与指定类型，加入送达数量、动作是否开始和原生错误码；普通事件仍不保存业务内容。

## 读取与解释

使用 `actl history --task <run_id> --timeline --limit 500` 合并控制请求与执行事件。
普通 history 默认仅最近 20 个调用，timeline 默认也仅最近 20 条事件，排障时应显式增加 limit 并检查 total/truncated。
按步骤查询仍可核对调用边界；控制请求本身属于任务级事件。

健康状态 no_recorded_failure 只表示保留范围内未读到失败标记；degraded 表示有失败，unknown 表示健康记录自身不可读或损坏。
complete_logging_guaranteed 始终为 false。独立记录仍共享文件系统，磁盘或权限故障可能使记录本身失败，此时只有 stderr 诊断。
标记保留 7 天、预算 8 MiB，返回最近 100 条并报告总数与截断；近期文件保护可能阻止继续写入，预算不是丢弃活跃证据的理由。
状态失败的 dropped_events 不衡量缺失快照数；日志打开失败的 0 也不证明没有丢日志。

integrity=complete 只证明读到的事件结构完整，不证明全部调用都被记录，更不表示业务流程成功。
unfinished 无法单独区分仍在运行、崩溃或日志丢失。日志与恢复状态不是原子事务，恢复仍以 flow 存储和结果核验为准。
accepted、child_returned 与 flow_pause_observed 分别表示请求写入、控制进程返回、边界暂停状态保存；不能互相替代。
来源与请求 ID 可被调用环境设置，仅用于排障，不证明用户授权。原有急停、交接和不确定结果护栏保持约束。

## 验证与限制

Phase 2 性能修订：每次 Journal 打开不再整目录扫描历史日志；`history/retention.lock`
保存最近成功清理时间，并通过标准文件锁避免并发清理。至多每分钟扫描一次，失败不更新时间，
损坏记录或时钟回退重新扫描；现有 7 天/64 MiB 和活跃文件保护规则保留，预算仍非硬实时限额。
启动时的 `calls` 快照清理最多每秒一次，显示端读取仍按现有规则清理。两处均不访问急停或 flow 恢复数据。

故障测试覆盖日志目录被文件占用、事件超限后的丢弃计数、反馈目录不可写、健康文件损坏、序列/身份/边界异常与敏感内容排除。
流程集成测试覆盖暂停请求与实际边界 ID 关联、暂停控制不覆盖桌面反馈，以及恢复来源和流程结果。
真实按钮链路、磁盘耗尽、构建指纹和损坏反馈快照的完整诊断仍见 ROADMAP。

## UIA 观察与人工活动关联（2026-10-02）

原演示 `ribbon-demo-20261002-01` 的首次查询在 21:24:18–19 返回 `0x80131505`，
随后 21:26:03 有显示端继续请求，21:26:59–21:27:04 有多次暂停请求，最终以 pause_requested 收尾。
后续暂停有直接记录，但不能反推它导致较早的超时；历史错误证据不改写。

- `UIA_E_TIMEOUT` 明确映射 TIMEOUT；未知 COM 错误仍为 INTERNAL，不用错误文本猜分类。
- 观察开始先写事件，再调用 UIA；结束记录阶段、单调时钟耗时、原生错误码、前后输入 tick 和前台 HWND/PID。
  目标身份来自已有缓存及原生 PID 查询，未知为 null，不额外调用活跃提供程序读取诊断信息。
- 显示端独立后台线程每 200ms 保存一个有界输入摘要；观察端校验 epoch/PID、版本、采样时刻及监听健康。
  样本过期、监听失败、重启和计数回退均保留未知，不把旧样本作为“用户没有操作”的证明。
- sampled_input_delta 给出采样区间内 device/foreign_injected/own_injection 信号增量；
  Hook 和 Raw 可能重复计数，且采样边界不等于 COM 调用边界。不记录键值、坐标、设备句柄或业务文本。
- 前 32 个观察区间保留首尾，之后预留 8 个慢查询（≥250ms）或失败的结束记录；到达预算时记录 observation_budget。
  预算外的结束事件标 started_recorded=false；缺少结束不能直接认定进程崩溃。
- 错误 evidence 的 observation_stage/span_id/duration_ms 可关联具体事件，嵌套调用保持最内层失败。
  metadata 只保留规定阶段与无符号数值，错误正文不进入观察事件。

排障先把失败步骤的 observation 事件与 control_request/flow_pause_observed 对齐，再检查监听健康与活动时间。
TIMEOUT、pause_requested、会话输入变化分别是错误、控制原因和相关活动，不能互相替代。
同步 UIA 提供程序调用仍可能阻塞；本轮增强定位证据，不改变执行取消、授权或重试策略。

### 查询与数组阶段（2026-10-03，spec）

生产 children 路径细分 find_all_build_cache、array_length、array_elements；包含
query_scope=desktop_children/element_children，结束事件记录 result_count、processed_count、failed_index。
只有 Length 成功后总数才已知；转换失败记录零起始索引和已取得数，返回错误，不返回部分树。
0 是已观察空数组，null 是未知或该阶段不适用；负长度为无效提供程序数据，返回 INTERNAL。
不为日志增加提供程序属性读取。children 外层包含嵌套诊断开销，不能与内层耗时相加。
所有层仍共用原 32/8 预算，可能更早耗尽；错误 evidence 保留最内层阶段，flow metadata 同步放行三个新阶段。
无新动作、授权、重试或生产硬超时。默认 ignore 的开发工具捕获用例复用现有只读 CLI
进程留证器，30 秒上限仅用于诊断，截断时保留未完成日志，不伪造结束或重跑。

本轮真机日志目录含约 2.1 万个 journal，初次 timeline 查询耗时约 49 秒。
按任务提前筛选有效首条，并缓存首条任务身份、大小和修改时刻，避免重复打开未变化的非目标日志。
缓存最大 8 MiB，独立于原始 journal；缺失/损坏/文件变化均回查原文件，目标文件一直读取原事件。
指定 task 时 incomplete_files 不再统计已明确归属其他任务的尾部损坏；未知首条仍报告，
无 task 查询仍扫描全部。这里是任务范围查询，不代替全目录完整性检查。

### 2026-10-02 验证记录

- 最终 `cargo xtask gate` 通过，覆盖 file-lines、signal-ui、fmt、clippy、workspace 测试和 lint-packs；日志为 `target/observation-live/observation-gate-verified.log`。此前慢环境触发的测试时钟假设和技能包跨仓库链接失败已修正，各次失败日志保留，不覆盖原证据。
- 最终发布二进制与哈希见 `target/observation-live/final-binaries.json`：`actl.exe` SHA-256 为 `D2BF80BFE99432A1D342B9EA07B13E7CC24F0BF56A24A3A10C260FBAB5C4FA5D`，`actl-signal.exe` 为 `5517C00499F06E5077144D7002D2D6B134AD3D5F01F1DE32EF9257E478F69006`；环境头见同目录 `environment.json`。
- 最终显示端正常启动为 PID 59012；`final-display-status.json` 返回 ready=true、responsive=true、stop_requested=false。保留急停代次与原演示的冻结计划、失败和暂停记录，未授予桌面操作权限。
- 先前发布版在新任务 `observation-inspect-20261002-01` 完成两步只读流程，保留 39 条时间线事件；其二进制哈希与逐调用结果独立保存在 `release-binaries.json`、`inspect-*.json`，不作为最终二进制的完整业务验收。
- 最终二进制使用已有 `fixtures/flows/inspect.yml` 创建新任务 `observation-final-20261002-01`，因二进制变化重新验证，未续跑旧失败任务。system 完成，windows 在首次尝试失败并保留 paused/action_failed；`final-inspect-resume.json` 记录 TIMEOUT、children、808ms、原生码 `-2146233083`，`final-inspect-timeline.json` 保留 33 条事件。
- 该失败的输入样本有效、监听健康，前后输入 tick 和前台 HWND/PID 未变化，采样信号增量为 0，且没有暂停请求；本次证据不支持归因于人工中断。采样仍不能证明所有输入均不存在。桌面根节点批量枚举尚未返回子窗口，故目标 HWND/PID 未知，具体故障提供程序仍待定位；前台进程身份不能替代故障提供程序身份。诊断摘要见 `final-failure-summary.json`。
- 同一日志目录、同一任务的时间线查询由 48,834ms 降为冷缓存 35,174ms、热缓存 5,126ms；39 条事件完全一致，计数和截断状态一致，见 `timeline-performance.json`。这些数字仅在本次环境比较，冷查询与目录扫描仍有优化空间。
- 未创建业务应用窗口，无需关闭业务资源；显示端保持运行，最终失败任务留在暂停现场。本轮日志定位与部署核验完成，UIA 超时根因修复和完整业务验收尚未完成。

### 历史验证记录

- 首次 gate 因 history 的旧 golden 未包含 logging_health 失败，日志保留于 `target/metadata-gate.log`。
- 同步协议 golden 后，`cargo xtask gate` 全部通过，见 `target/metadata-gate-recheck.log`；包括格式、clippy、workspace 测试、pack 检查和回归脚本测试。
- 被测发布程序为 `target/validation/release/actl.exe` 和同目录 `actl-signal.exe`，SHA-256 留存于 `target/metadata-live/binaries.json`，环境头见同目录 `environment.json`。
- 构建前核对旧显示端 PID 27748、路径及无 actl 执行进程，正常关闭；构建后 display-start 启动 PID 18688，display-status 返回 ready=true、responsive=true、stop_requested=false。
- 发布版 history 只读查询返回新健康字段，status=no_recorded_failure、complete_logging_guaranteed=false。逐调用输出位于 `target/metadata-live/`。
- 未创建业务应用窗口，显示端保持运行；未清除停止、暂停或流程记录，未续跑旧任务。以上仅为部署及只读诊断核验，真实按钮链路与存储故障仍未完成真机验收。
