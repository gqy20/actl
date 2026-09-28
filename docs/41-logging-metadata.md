# 41 · 日志健康与控制元数据

更新：2026-09-28。现行字段见 [设计蓝图](06-design-blueprint.md)，流程语义见 [20](20-yaml-workflow-protocol.md)。

## 实现范围

- history 检查保留事件的版本、调用/任务身份、连续 seq 与调用首尾。步骤筛选不会掩盖原始调用的损坏。
- log-health 独立保存 journal_open、journal_write、state_write 失败。日志追加失败后停止追加并累计后续丢弃事件；保留原始 I/O 错误类别与原生码。
- 暂停请求和显示端控制派发使用独立日志，不改正在执行的桌面反馈。request_id 连接父子进程和持久化暂停边界。
- metadata 错误证据仅保留白名单与指定类型，加入送达数量、动作是否开始和原生错误码；普通事件仍不保存业务内容。

## 读取与解释

使用 `actl history --task <run_id>` 查询控制请求与执行事件。按步骤查询仍可核对调用边界；控制请求本身属于任务级事件。

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
真实按钮链路、磁盘耗尽、构建指纹、逐阶段耗时和损坏反馈快照的完整诊断仍见 ROADMAP。

### 本轮验证记录

- 首次 gate 因 history 的旧 golden 未包含 logging_health 失败，日志保留于 `target/metadata-gate.log`。
- 同步协议 golden 后，`cargo xtask gate` 全部通过，见 `target/metadata-gate-recheck.log`；包括格式、clippy、workspace 测试、pack 检查和回归脚本测试。
- 被测发布程序为 `target/validation/release/actl.exe` 和同目录 `actl-signal.exe`，SHA-256 留存于 `target/metadata-live/binaries.json`，环境头见同目录 `environment.json`。
- 构建前核对旧显示端 PID 27748、路径及无 actl 执行进程，正常关闭；构建后 display-start 启动 PID 18688，display-status 返回 ready=true、responsive=true、stop_requested=false。
- 发布版 history 只读查询返回新健康字段，status=no_recorded_failure、complete_logging_guaranteed=false。逐调用输出位于 `target/metadata-live/`。
- 未创建业务应用窗口，显示端保持运行；未清除停止、暂停或流程记录，未续跑旧任务。以上仅为部署及只读诊断核验，真实按钮链路与存储故障仍未完成真机验收。
