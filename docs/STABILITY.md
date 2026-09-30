# 稳定性设计总览（STABILITY）

> 建立于 2026-09-30。回答"**每一个步骤靠什么保持稳定**"。各项机制的设计与验收细节散在
> 各编号文档里；本文做跨层地图：每个机制给出它保护的环节、怎么起作用、实现在哪、详见哪篇。
> 与 [FALLBACK.md](FALLBACK.md)（失败后的退路）互补，本文讲"尽量不失败、失败了不放大"。

## 1. 设计立场

1. **可验证优先于盲目重试**：每个动作都有配套的核验手段（`--expect`、`--verify-value`、
   verify 家族、`wait` 谓词）。重试只给"确认瞬态"的错误码；结果不确定时先核验再决定，
   禁止盲目重放。
2. **fail-closed**：观察失败≠空结果；读失败不换通道；截断不证缺席。宁可失败并留证，
   不产出貌似成功的错误事实。
3. **写操作永不自动重放**：自动重试只开放给只读调用；写的恢复路径是"先核验后置条件，
   人工授权才重放"（`--retry-step`）。
4. **一切换路必须可见**：兜底路径通过 `resolved_by`/`via` 等字段上报（见 FALLBACK §13），
   稳定性不靠静默降级堆出来。
5. **失败要留证、要可归因**：结构化错误码 + 恢复提示 + 证据载荷 + 三通道留证；
   同错三连识别路线缺陷，防止重试掩盖坏路线。

## 2. 一次桌面操作的护栏链（按步骤生命周期）

### 2.1 启动与就绪（"进程起来了 ≠ 窗口可操作"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| `launch --wait-window` | 等窗口真实出现（可见/可交互），失败附 `spawned_pid`+`process_started`，不盲目重启 | `window_ready.rs::wait_window_ready` | [06](06-design-blueprint.md) §3.4 |
| 就绪目标分级 | 窗口 → 前台 → 业务控件出现且稳定，才允许输入（防止启动成功后直接假定页面可输入） | flow 模板 `ready_target`（必填） | [66](66-application-readiness.md) |
| focus-window 诊断阶梯 | 激活失败逐项归因：禁用/模态/无响应/隐藏/离屏/前台未取得 | `window_ready.rs::activate` 前置条件 | [32](32-desktop-handoff.md) |
| 挂起窗口唤醒 | UWP 挂起进程先有界唤醒，避免 DFS 逐调用 stall | `ensure_window_responsive` | FALLBACK §4 |
| `wait --idle` | 用户输入静默门（组合模板首步），避免与用户抢输入 | `actl-uia/src/idle.rs` | [68](68-combined-batch-first-round.md) |

### 2.2 观察与定位（"看得到、看得全、看得真"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| 观察完整性契约 | 截断快照显式标记 + 拒绝据此裁决唯一/缺席 | `observation.rs`（`truncation_reasons`） | [37](37-uia-observation.md) |
| 桌面根瞬态重试 | 吸收无关窗口创建/销毁与枚举的竞态（仅 StaleRef，3×200ms） | `with_transient_retry` | FALLBACK §4 |
| 原生标题优先 | 已知窗口的定位不被无关应用的整桌面 UIA 故障阻断 | `native_windows::title_elements` | [30](30-window-infrastructure.md) |
| 定位分层链 + 上报 | 精确→模糊→虚拟化，实际层级可归因 | `locate.rs`（`resolved_by`） | [06 §5](06-design-blueprint.md) |
| 沉降（settle）家族 | 断言失败不单发判死：verify-window 15s、verify-file/rows/clipboard/closed 5s 内重读 | `window_check.rs`、`file_check.rs::settle` | [70 §2](70-debug-playbook.md) |
| `wait --stable` | 连续两次样本相等才算内容稳定（250ms 采样，与存在性轮询分开） | `locate.rs`（`STABLE_SAMPLES=2`） | [16](16-browser-scenarios.md) |

### 2.3 身份与焦点（"打的就是它，且它有焦点"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| `--snapshot` 绑定 + RuntimeId 校验 | ref 只对快照来源窗口有效；窗口实例一变即 STALE_REF，不静默换目标 | `window.rs::check_snapshot_freshness`、`identity.rs` | [31](31-element-identity-input-focus.md) |
| 重放护栏 | ref 重放须绑定 `--snapshot`+`--app`；REF 目标拒绝"当前前台"路径 | `interact.rs::replay_guard` | [31](31-element-identity-input-focus.md) |
| 显式 `--app` 绑定 | 键盘/坐标类操作必须声明目标窗口，无前台兜底 | `interact.rs::require_app` | [09](09-input-backends-review.md) |
| 焦点双核验 | UIA SetFocus 后用**物理** GetGUIThreadInfo 核验（UIA 焦点视图在 WinUI 上会说谎） | `action.rs::ensure_element_focus`、`kbd.rs::focused_root_window` | [31](31-element-identity-input-focus.md) |
| 执行前复核 | UIA 语义动作预告后按 RuntimeId/PID/角色/名称/AutomationId/原生宿主复核，身份变化/禁用即拒绝 | feedback 复核钩子 | [06](06-design-blueprint.md) 附录 |
| 目标预览 | 歧义目标交用户显式选候选，不做自动纠正 | preview 子命令族 | [46](46-target-preview.md) |
| 控件重建后再观察 | 菜单/控件变化后重新观察定位，不凭旧快照硬打 | AGENTS"目标与焦点" | [31](31-element-identity-input-focus.md) |

### 2.4 输入执行（"注入原子、护栏齐全"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| 输入占用锁 | actl 实例间输入互斥；前任崩溃自动接管（ABANDONED） | `input.rs::InputLock` | [09](09-input-backends-review.md) |
| 修饰键护栏 | 注入前等物理修饰键空置；部分 SendInput 失败报"状态未知勿重试" | `kbd.rs::wait_modifiers_clear` | [09](09-input-backends-review.md) |
| 双通道无静默降级 | 修饰键组合强制 VK；不可映射即 PROTOCOL，不猜 | `kbd.rs::compile_key_spec` | FALLBACK §6 |
| 分段投递 + 段间焦点复查 | 长文本分段打、失焦即停在段边界并如实报 partial | `input.rs::send_in_segments` | FALLBACK §6 |
| 命中检查 | 物理点击前确认落点属于目标窗口（遮挡/越界/漂移拒绝） | `pointer.rs` hit-check | [06 §7](06-design-blueprint.md) |
| `--no-activate` WAL journal | 后台点击样式位崩溃可恢复，残位每次锁获取时清扫 | `bg_style.rs::NoActivateGuard` | [06](06-design-blueprint.md) 附录 |
| 预告-确认投递 | 显示端确认后才投递动作；确认丢失=动作未投递 | `display_gate.rs::wait_presented` | [18](18-display-layer-implementation.md) |

### 2.5 结果核验（"报错 ≠ 没生效"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| `--expect` 后置断言 | 动作后等窗口/元素物化；新窗口基线防同题旧窗冒充 | `interact.rs::expect` | [06 §3.6](06-design-blueprint.md) |
| `type --verify-value` | 输入后读回证明（≤2s）；身份变化 STALE_REF、值不符 ASSERTION_FAILED，从不重写 | `input_verify.rs` | [31](31-element-identity-input-focus.md) |
| verify 家族 | 文件/剪贴板/行数/窗口状态/进程关闭的显式断言 + 沉降重读 | `file_check.rs`、`window_check.rs` | [70 §2](70-debug-playbook.md) |
| 哪些证据才算数 | 字段写入/文件保存/记录创建/应用启动各自的"真正证明"分层 | flow verify 步设计 | [55](55-resumable-desktop-tasks.md) |

### 2.6 流程推进（"只读自动重试，写要授权"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| 有界恢复预算 | 观察瞬态在 3 次/15s 级预算内自动重查，假阴性不再停整轮 | `bounded.rs`（`recovery:`） | [64](64-bounded-observation-recovery.md) |
| uncertain 先核验 | 恢复时先重跑被打断写的 verify；未确认 → NeedsReview，禁自动重放 | `engine.rs` | [20 §4](20-yaml-workflow-protocol.md)、[39](39-flow-observation-recovery.md) |
| `--retry-step` 人工授权 | 只有确认 ASSERTION_FAILED 的 uncertain 写才可显式重放 | `engine.rs`、CLI | [20](20-yaml-workflow-protocol.md) |
| 恢复核验顺序 | reload plan → resume_check → before → verify uncertain，四道再入闸 | `engine.rs::resume` | [55](55-resumable-desktop-tasks.md) |
| 输出新鲜度 | 绑定值带 `max_age_ms`，过期/时钟回拨即 Paused，重试不吃陈旧业务值 | `freshness.rs` | [38](38-flow-output-bindings.md) |
| 路线缺陷识别 | 同写步骤同错三连 → `route_defect_suspected`，提示勿盲目重试 | `route_defect` | [68](68-combined-batch-first-round.md) |
| 段隔离续跑 | `on_failure: skip_section` 一段失败不堵死其余应用，整轮 needs_review | `engine.rs` | [20](20-yaml-workflow-protocol.md) |
| 可恢复操作组 | run.sequence 1-16 调用、必须以 verify 收尾、无回滚不假装有 | `sequence.rs` | [06](06-design-blueprint.md) 附录 |
| 声明式资源 | 初始化失败保留不覆盖不自动重试 | `resources.rs` | [20](20-yaml-workflow-protocol.md) |

### 2.7 人机协作（"让行优先，意图不可抵消"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| 急停代际 | 停止是持久代际，旧调用不得复活；新任务可正常请求开始 | `stop.rs`（generations） | [47](47-execution-recovery.md) |
| 交接授权生命周期 | 25s 应答窗、60s 闲置/10min 总量过期、settled 后才可继续 | `handoff.rs` | [32](32-desktop-handoff.md) |
| 误触赦免（pardon） | 纯鼠标移动撤销授权时静默 4s 内自动恢复（每任务 2 次）；键盘/点击永不赦免；用户暂停永不自动继续 | `bin/signal/pardon.rs`，`ACTL_HANDOFF_PARDON=0` 停用 | [32](32-desktop-handoff.md) |
| 输入监控双源冗余 | WH_KEYBOARD_LL/WH_MOUSE_LL + Raw Input 互备；1s 无心跳 → `input_monitor_unavailable` 如实降级 | `bin/signal/input_watch.rs` | [32](32-desktop-handoff.md)、[44](44-phase2-acceptance.md) |
| 前台资格租约 | 每动作守卫转移/归还前台权，PID+同文件规范化路径验身份 | [62](62-foreground-handoff.md) | |
| 暂停/停止分离 | 人工让行和主动暂停不写急停 | AGENTS"暂停与停止分离" | [47](47-execution-recovery.md) |

### 2.8 失败处置与清理（"失败可归因，现场可恢复"）

| 机制 | 作用 | 实现 | 详见 |
|---|---|---|---|
| 三通道留证 | 步骤失败瞬间落整屏 PNG + 原生窗口清单 + 定向 UIA 快照；吞错不改流程结果 | `flow.rs::capture_failure` | [68](68-combined-batch-first-round.md) |
| 错误码 + 恢复提示 | 每个错误码带面向 agent 的英文恢复建议（见 §3） | `actl-core ErrorCode::recovery_hint` | [06 §4](06-design-blueprint.md) |
| attempt_errors 环 | 步骤记录最近 8 次错误码，失败可归因 | `engine.rs` | [68](68-combined-batch-first-round.md) |
| 优雅关闭 + 归属核对 | 清理只对核实过归属（HWND/PID/任务令牌）的窗口，多标签只清本任务标签；禁 taskkill | close-window 族 | [30](30-window-infrastructure.md) |
| 桌面整理 journal | 最小化前先落盘，崩溃可恢复；恢复跳过身份存疑窗口 | `desktop.rs` | [06](06-design-blueprint.md) 附录 |

## 3. 错误协议与 agent 自愈

每个错误 envelope 附 `recovery_hint`（`actl-core/src/lib.rs`），把"下一步该做什么"编码进协议，
agent 不需要猜：

| 错误码 | 恢复提示（要点） |
|---|---|
| STALE_REF | run `snapshot` to refresh refs, then retry |
| NOT_FOUND | relax the selector or run `find` to inspect candidates |
| PERM_DENIED | 目标完整性级别更高，请用户提权重启——**不要重试** |
| TIMEOUT | re-evaluate preconditions; the UI may be busy or slow |
| AMBIGUOUS | tighten the selector or pick by index |
| NOT_ACTIONABLE | scroll or focus first; 写值考虑 `set-value` |
| ASSERTION_FAILED | inspect the prior step's result; 期望状态未物化 |
| PROTOCOL | fix the command arguments |
| INTERNAL | **retry once**; 持续出现携 evidence 报 issue |
| ABORTED | 被停止；核验既有结果后新起调用或显式恢复；**绝不自动重放** |

## 4. 稳定性不变量（违反即破坏上述设计）

1. stdout 只输出 JSON envelope，诊断进 stderr——agent 的解析链不能被污染（ADR-002）。
2. 运行路径禁 `unwrap/expect`（ADR-004）。
3. 写操作不自动重放；PERM_DENIED 不重试；INTERNAL 只建议重试一次。
4. 观察失败不转空/成功；截断不证缺席/唯一。
5. 兜底换路必须带可观测字段。
6. 留证与诊断吞错，不改流程结果。
7. 物理通道必须显式 `--physical`（ADR-007）——稳定性不允许靠静默提权换。
8. GUI 应用禁 taskkill；清理前核对资源归属。
9. 时序数字只在同环境头内比较（timing 纪律），调参结论须有 sweep/profile 证据（[timing-2026-09](timing-2026-09.md)）。

## 5. 诊断与观测入口（稳定性出问题时怎么看）

- `ACTL_TRACE=1` / `ACTL_TRACE_FILE`：调用内阶段耗时打点，看停在哪层（[65](65-observation-blocking-diagnosis.md)）。
- `history --timeline`：任务全部 journal 事件合并为单一时间序列，重建暂停/交接/失败过程。
- `display-status`：显示端就绪/租约/诊断 taxonomy。
- `execution-status` / `execution-recover`：损坏或旧格式停止状态的诊断与修复（[47](47-execution-recovery.md)）。
- 调试三段法与症状速查见 [70](70-debug-playbook.md)。

## 6. 维护规则

- 新增稳定性机制：同 PR 在本文对应生命周期小节登记一行（机制/作用/实现/详见）。
- 机制行为或契约变化：更新对应编号文档 + 本文行；协议面变化按硬规则 8 走 `spec` 提交。
- 已知薄弱点与待办登记 [ROADMAP](ROADMAP.md)，本文只记现状。
