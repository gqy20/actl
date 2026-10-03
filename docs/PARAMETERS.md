# 时序与可调参数总览（PARAMETERS）

> 建立于 2026-09-30。全量登记影响执行行为的时序参数（间隔/等待/点击/轮询/重试计数）：
> 默认值、单位、定义处、控制什么、覆盖通道。供调优、统一化与"按应用自适应时序"演进。
> 默认值速查以本文为地图，**权威实现以代码为准**（引用以 crate/文件/函数名定位，行号会漂移）。

## 1. 三层结构与覆盖通道

| 层 | 定义处 | 覆盖通道 | 变更纪律 |
|---|---|---|---|
| **协议默认值** | `actl-core/src/timing.rs` 的 `EXPECT_MS`/`WAIT_MS`/`WAIT_WINDOW_MS`/`DISPLAY_READY_MS`/`DISPLAY_ACK_MS` | **只有命令行 `--timeout`**；刻意不接 env——help 文本是对 agent 的契约，env 静默改默认会让 help 变谎言 | `spec` 提交（硬规则 8：golden + docs/06 + CHANGELOG 同 PR） |
| **内部时序旋钮** | `actl-core/src/timing.rs` 的 `Timing` 结构（14 项） | 环境变量 `ACTL_*`（进程内 OnceLock 读一次；非法值静默回默认——时序旋钮绝不能因配置错误让命令失败） | 自由调优；调参结论须有 sweep/profile 证据 |
| **使用点硬编码** | 各 crate 使用处 | 无（部分待统一，见 §9） | 改值同步本文 |

实测依据与生态对照：[timing-2026-09](timing-2026-09.md)（profile/sweep）、
[13 §4.2](13-realtime-state-maintenance.md)（生态共识：poll ~100ms）。

## 2. 内部时序旋钮（`Timing`，`crates/actl-core/src/timing.rs`）

读取：`Timing::load()`（经 `actl_uia::timing()` 再导出）。

| 旋钮 | 默认 | env 覆盖 | 控制什么 | 主要使用点 |
|---|---|---|---|---|
| `poll_ms` | 100 | `ACTL_POLL_MS` | wait 族轮询间隔（生态共识 pywinauto 90 / terminator 100） | `wait --element/--gone/--property/--window/--idle`、`--expect` 窗口轮询、修饰键空置等待、焦点重试、滚动后重绘、`--verify-value` 读回、挂起唤醒循环 |
| `wake_bound_ms` | 1600 | `ACTL_WAKE_BOUND_MS` | 挂起（UWP）应用恢复泵消息的有界等待上界（实测 stall ~2s，取略低） | `window.rs::ensure_window_responsive` |
| `probe_ms` | 60 | `ACTL_PROBE_MS` | 快照路径 WM_NULL 探针超时（快路径零误伤） | 同上；注意唤醒循环内的再探针是硬编码 60ms（§9） |
| `focus_retries` | 3 | `ACTL_FOCUS_RETRIES` | press 链中元素级 SetFocus 的重试轮数 | `action.rs::ensure_element_focus` |
| `press_focus_ms` | 120 | `ACTL_PRESS_FOCUS_MS` | press/paste 链窗口 SetFocus 后的稳定等待 | `action.rs::prepare_keyboard`、paste 路径 |
| `type_focus_ms` | 150 | `ACTL_TYPE_FOCUS_MS` | type 链元素 SetFocus 后、进前台护栏前的稳定等待 | `action.rs::type_text` |
| `foreground_settle_ms` | 600 | `ACTL_FOREGROUND_SETTLE_MS` | 前台护栏入口的沉降预算：激活过渡期前台可能是空标题过渡窗（ForegroundStaging）或目标标题未就位，轮询至匹配或耗尽；真抢焦（标题稳定地不是目标）照样拒 | `kbd.rs::foreground_settles_to`（type/press/paste 三入口） |
| `segment_chars` | 32 | `ACTL_SEGMENT_CHARS` | type 键盘通道分批大小（停止响应粒度 = 本值；段间焦点复查） | `input.rs::send_in_segments` |
| `modifier_wait_ms` | 1500 | `ACTL_MODIFIER_WAIT_MS` | 注入前等物理修饰键空置的上限（超时 NOT_ACTIONABLE） | `kbd.rs::wait_modifiers_clear` |
| `lock_wait_ms` | 3000 | `ACTL_LOCK_WAIT_MS` | 会话输入互斥锁 `actl-input-lock` 的等待上限（他实例占用 → TIMEOUT） | `input.rs::InputLock::acquire`；例外见 §9 |
| `settle_ms` | 250 | `ACTL_SETTLE_MS` | `wait --stable` 采样间隔（稳定判据：连续两次样本相等，稳定窗 = 2×250ms） | `locate.rs` |
| `fast_poll_ms` | 25 | `ACTL_FAST_POLL_MS` | 可见性/门控快速轮询拍（desktop/display_gate/window_ready 探测） | 显示端就绪等待、前台观察、`wait_iconic` |
| `ready_poll_ms` | 50 | `ACTL_READY_POLL_MS` | `launch --wait-window` 主发现轮询 | `window_ready.rs` |
| `vis_probe_ms` | 1000 | `ACTL_VIS_PROBE_MS` | window_ready 首帧可见性探测窗；SetForegroundWindow 后的有界前台观察 | `window_ready.rs` |
| `handoff_poll_ms` | 100 | `ACTL_HANDOFF_POLL_MS` | 桌面交接 PENDING 态轮询间隔 | `handoff.rs` |

## 3. 协议常量（不可 env 覆盖）

| 常量 | 值 | 作为哪个默认 | 说明 |
|---|---|---|---|
| `EXPECT_MS` | 2000 | `click/press --expect` 的隐式后置等待（`--timeout` 可覆写） | help 契约 |
| `WAIT_MS` | 5000 | `wait --timeout` 默认 | help 契约 |
| `WAIT_WINDOW_MS` | 5000 | `launch --wait-window` 就绪超时默认（旗标限 1..=60000） | help 契约 |
| `DISPLAY_READY_MS` | 4000 | `display-start` 可见渲染帧等待（help 承诺 "up to 4s"） | 显示端握手窗 |
| `DISPLAY_ACK_MS` | 3000 | 每动作预告确认等待 | 显示端握手窗 |

## 4. CLI 超时旗标（clap 面）

| 旗标（命令） | 默认 | 取值范围 | 背后常量 |
|---|---|---|---|
| `--timeout`（click、press；配合 `--expect`） | 2000 | 无限制 | `EXPECT_MS` |
| `--timeout`（wait） | 5000 | 无限制 | `WAIT_MS` |
| `--timeout`（launch，需 `--wait-window`） | 5000 | 1..=60000 | `WAIT_WINDOW_MS` |
| `--idle`（wait） | 必填 | 1..=86_400_000（24h） | 阈值（非超时） |
| `--duration`（preview） | 3000 | 500..=30000（实现在 preview.rs 校验） | 预览窗寿命 |
| `--limit`（snapshot） | MAX_ELEMENTS | 1..=5000 | 遍历预算 |
| `--depth`（snapshot） | MAX_DEPTH | 0..=40 | 遍历预算 |
| `--limit`（find / history） | 20 | — | 计数 |

## 5. 使用点硬编码时序

### 5.1 观察与窗口（actl-uia）

| 参数 | 值 | 位置 | 控制什么 |
|---|---|---|---|
| 桌面根瞬态重试 | 3 次 / 200ms 间隔 | `observation.rs`（MAX_ATTEMPTS / RETRY_INTERVAL_MS） | 只重试 StaleRef 的枚举竞态吸收 |
| `verify-window` 沉降窗 | 15000 / `poll_ms` 轮询 | `window_check.rs` | ASSERTION_FAILED 重读（Office 拆窗滞后 3-15s） |
| 最小化/还原视觉等待 | 2s 上界 / 25ms 轮询 | `desktop.rs::wait_iconic` | `minimize-window`/`restore-window` |
| 快照保留数 | 20 个（按 mtime） | `window.rs`（SNAPSHOT_KEEP） | @eN ref 的"寿命"——无时间 TTL，靠驱逐/窗口 RuntimeId 变化失效 |
| 截图保留数 / 像素上限 | 20 / 50,000,000 | `capture.rs` | 本地 shots 目录 |
| 预览观察/泵循环 | 50ms | `preview.rs`、`preview_choice.rs` | 目标移动检查拍 |

### 5.2 verify 沉降家族（actl-cli `file_check.rs`，见 [70 §2](70-debug-playbook.md)）

| 命令 | 沉降窗 / 轮询 | 吸收什么 |
|---|---|---|
| verify-file | 5000 / `poll_ms` | 已知 mismatch 的重读 |
| verify-rows | 5000 / `poll_ms` | 行树未渲染（缺失改报 NOT_FOUND 入沉降） |
| verify-clipboard | 5000 / `poll_ms` | 复制送达与可读的竞态 |
| verify-closed | 5000 / `poll_ms` | 进程退出重读 |
| verify-window | 15000 / `poll_ms` | 窗口拆除滞后（见 5.1） |

### 5.3 输入（actl-uia）

| 参数 | 值 | 位置 | 控制什么 |
|---|---|---|---|
| `type --verify-value` 读回 | 2000 硬上限 / `poll_ms` 轮询 | `input_verify.rs` | 只重读从不重写 |
| 焦点启发式 DFS 深度上限 | 12 | `action.rs` | 可编辑元素查找 |
| ScrollIntoView 后重绘等待 | 1×`poll_ms`（100ms） | `pointer.rs::ensure_visible` | 离屏滚入后可点性 |
| `wait --stable` 样本数 | 2（STABLE_SAMPLES） | `locate.rs` | 稳定判据 |

### 5.4 显示端与交接

| 参数 | 值 | 位置 | 控制什么 |
|---|---|---|---|
| 伴侣 WM_NULL 存活探针 | 150ms | `display_gate.rs` | 诊断前探针 |
| RECOVER_MESSAGE 发送超时 | 500ms | `display_gate.rs` | 陈旧伴侣清理 |
| CAPTURE_HIDE 同步隐藏确认 | 1000ms | `display_support.rs` | 截图隐藏确认 |
| 确认后预告停留 | 12×`fast_poll_ms` = 300ms | `display_gate.rs` | 保持预告可读再投递 |
| 交接查询 SendMessageTimeout | 500ms | `handoff.rs` | REQUEST/CHECK 往返 |
| 交接 PENDING 总界限 | 25s | `handoff.rs` | 用户应答窗，超时 `handoff_pending` |
| 显示租约/确认新鲜度 | 1500ms | `actl-core display.rs` | "活着且兼容"判定窗 |
| 交接请求新鲜度 | 2000ms | `handoff.rs::Request::fresh` | 忽略陈旧盘上请求 |
| 同意沉降宽限（GRACE_MS） | 750ms | `handoff.rs` | 吸收授权点击自身的抬手/手抖 |
| 同意总量/闲置过期 | 600,000 / 60,000 | `handoff.rs` | 过期型授权生命周期 |
| 赦免（pardon）默认 | quiet 4000ms / 预算 2（范围 1000..=10000 / 1..=5） | `handoff.rs`、`bin/signal/pardon.rs` | 误触自动恢复；`ACTL_HANDOFF_PARDON=0` 停用 |
| 赦免后自动续跑节流 | 5000ms（前置：暂停已沉降 ≥2000ms 且安静达标） | `bin/signal/pardon.rs` | Path B 自动继续 |
| 暂停"已沉降"延迟 | 800ms | `bin/signal/flow_ui.rs` | Continue 按钮就绪门 |
| 伴侣替换关闭 | 2000ms 确认 + 4000ms 退程等待 | `bin/signal/shell.rs` | 旧构建关停宽限 |

### 5.5 流程引擎（actl-flow）

| 参数 | 值/范围 | 位置 | 控制什么 |
|---|---|---|---|
| `retry: read_only` 尝试 | 3 / 间隔 200ms | `engine.rs` | 只读自动重试 |
| `retry: stale_read` 尝试 | 3 / 间隔 200ms / observation_retries ≤32 | `recovery.rs` | 仅 StaleRef |
| `recovery:` 预算 | max_retries 1..10、timeout_ms 1..60000、interval_ms 1..timeout_ms（**YAML 逐步骤必填**，无引擎默认） | `bounded.rs` | 文档惯用 3 次/15s/1.5s 是 workflow 作者取值，非引擎默认 |
| 恢复退避切片 | ≤25ms 片睡到 interval_ms | `bounded.rs` | 退避期间保持暂停/停止可响应 |
| `max_age_ms`（输出新鲜度） | YAML 逐输出必填，min 1 | `freshness.rs` | 绑定业务值的墙钟寿命 |
| attempt_errors / 事件环 / observation_retries | 8 / 64 / 32 | `engine.rs`、`bounded.rs`、`recovery.rs` | 有界记录 |
| 可恢复序列组大小 | 1..16 调用 | `sequence.rs` | run.sequence 上限 |

**引擎没有步骤级超时**：步骤时长由其内嵌 actl 命令自身的等待旗标约束；暂停/停止在步骤边界
和各等待轮询内经 wait_control 观测（每轮文件存在性检查，无定时器）。

### 5.6 actl-signal 伴侣（显示端托盘应用）

| 定时器 | 值 | 位置 | 控制什么 |
|---|---|---|---|
| 主 UI tick | 100ms SetTimer | `shell.rs` | 交接 + 外壳 tick |
| 状态读取线程 | 100ms 轮询 | `shell.rs` | calls/*.json → UI 缓存 |
| 历史清理线程 | 15 天 sleep | `shell.rs` / `history.rs::RETENTION_INTERVAL` | 保留扫描循环 |
| 原始输入监视 | 100ms SetTimer / 健康 1000ms | `input_watch.rs` | 物理输入采样与监控存活判定 |
| 控制面板刷新 | 500ms | `controls.rs` | 控件窗 |
| 动作标记动画 | 20ms / 自动隐藏 400ms（preparing 600ms） | `overlay.rs` | 覆盖层 |
| 心跳重发 | 1000ms | `signal_session.rs` | actl-signal-heartbeat |

### 5.7 会话、历史与急停

| 参数 | 值 | 位置 | 控制什么 |
|---|---|---|---|
| 会话状态文件 TTL | 60,000 | `state.rs` | 每调用 calls/*.json 驱逐 |
| 历史日志保留 | 7 天 / 3 GiB | `history.rs::maintain` | CLI 与伴侣共用，过期判定与容量预算 |
| 任务报告保留 | 30 天 / 32 MiB | `history.rs::archive` | 任务报告 |
| 保留扫描节流 | 15 天（跨进程互斥 + 间隔标记） | `maintenance.rs` / `history.rs::RETENTION_INTERVAL` | 防重复扫描；首次/失败/坏标记/时钟回退例外 |
| journal 事件上限 | 1,048,576 | `history.rs` | 有界 journal |
| 急停检查 | 无轮询间隔——每次守卫调用、每个等待循环每轮做文件存在性检查 | `stop.rs` | 停止的即时性 |

## 6. 物理输入为什么没有"点击时长/双击间隔/按键时长"

**不存在 dwell / press-ms / 双击间隔 / 字距延迟这类参数**。所有鼠标与键盘注入都是每个动作
**单批原子 SendInput**（`dwFlags.time = 0`）：click = move+down+up；双击 = 一批内两次按压
（系统按双击时序识别）；三击 = 一批三次；拖拽 = move+down+move+up；按键 = down+up 单批；
修饰组合键 = 修饰 down → N 次点击 → 修饰 up 一批完成。因此"点击时间"不可调也不需要调——
唯一可调的输入节奏参数就是 §2 的分段/焦点/修饰键等待。滚轮粒度固定 `WHEEL_DELTA = 120`/格
（`scroll --down/--up N` 按格数）。

## 7. 调优证据与纪律

- **sweep 结论**（[timing-2026-09](timing-2026-09.md)，2026-09-26）：poll 100ms、segment 32 字符、
  焦点链 120ms×3 维持现值；环境陷阱登记（慢日/锁屏/受保护视图）。
- **复现入口**：`cargo xtask timing profile | sweep`（原生 devtools 夹具，2026-09-30 落地）。
  前置：交互桌面 + 显示端就绪（写入动作经 display gate 预告）+ 无既有记事本窗口；
  产物落 `docs/timing-data/<kind>-<UTC 时间戳>.md` 与 `target/timing-*/report.json`，
  逐调用留痕在同级 `call-*.{capture.json,stdout.json,stderr.log}`。
  **入库策略**（2026-10-03 起）：`docs/timing-data/` 下的机器产物默认被 .gitignore 忽略
  （本地留存）；curated 结论写 `docs/timing-*.md` 或验收文档后引用数据文件——
  需要入库的数据按需 `git add -f`，此前的历史数据文件不受影响。
- **timing 纪律**：性能数字只在同环境头内比较，慢日绝对值不跨日（AGENTS）。
- gate 各步耗时随 stderr 输出（`· <step> Nms`），慢环节头内比较。
- 生态对照见 [13 §4.2](13-realtime-state-maintenance.md)：poll ~100ms、窗口/元素超时 3-5s、
  文本退避 200ms→1s、settle ~500ms。

## 8. 非时序行为开关（env）

| 变量 | 作用 |
|---|---|
| `ACTL_TRACE` / `ACTL_TRACE_FILE` | 阶段耗时打点 / 落盘到 history/executor-*.trace |
| `ACTL_TASK_ID` / `ACTL_TASK_CONFIG` | 任务关联与流程配置 |
| `ACTL_EXECUTION_GENERATION` | 停止代际绑定（排队子调用不复活旧代） |
| `ACTL_FLOW_REQUEST_ID` / `ACTL_FLOW_REQUEST_SOURCE` | 请求关联 |
| `ACTL_HANDOFF_PARDON=0` | 赦免策略总停用 |
| `ACTL_CAPTURE_TEST` / `ACTL_LITERAL` / `ACTL_PREVIEW_CLI` / `ACTL_DEVTOOLS` | 测试/开发通道 |

## 9. 已知不对称与待统一项（现状登记）

1. 以下值既不可 env 调也无旗标覆写：`--verify-value` 2000ms、verify-window 15s、verify 四族
   5s、`wait_iconic` 2s、交接 PENDING 25s、赦免 quiet 4000ms（YAML 可逐流程覆盖）。其中 verify
   族的窗口数值已被 after_help 承诺为契约（"5-second/15-second settle window"），可调化必须连
   help 措辞与契约 golden 一起改（`spec` 提交）；交接 25s 是 docs/32 的人机协议，建议保持钉死。
   已登记 ROADMAP。
2. 引擎无步骤级超时——步骤时长上限完全由内嵌命令的等待旗标决定（设计如此，登记在此防误判为缺陷）。

## 10. 维护规则

- 协议默认值（§3）变更 = `spec` 提交：golden + docs/06 + CHANGELOG 同 PR（硬规则 8）。
- `Timing` 增删字段：同 PR 更新 timing.rs 单测、本文 §2 与 CHANGELOG。
- 使用点硬编码值变动（§5）：同 PR 更新本文对应行。
- 新增硬编码时序：先问能不能进 `Timing`（§9 的不对称清单只增不减视为劣化）。
