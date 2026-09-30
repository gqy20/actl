# 兜底与降级总览（FALLBACK）

> 建立于 2026-09-30。本文是**跨子系统兜底/降级路线的全量清单**（开发参考），回答"某条主路径
> 失败或不可用后，系统还有什么路可走"，供后续逐项完善与优化。
> 定位链的协议规范以 [06 §5](06-design-blueprint.md) 为准；输入后端选型论证见 [09](09-input-backends-review.md)；
> 本文不重复论证，只登记现状。引用以 crate/文件/函数名为准（行号会漂移）。

## 1. 分类法

| 类型 | 含义 | 进入方式 |
|---|---|---|
| **自动兜底** | 主路径失败即走下一级，无需参数 | 必须通过 `resolved_by` / `via` / `source`+`fallback_reason` 等字段上报实际路径——**兜底不可静默**是可观测性契约 |
| **显式兜底** | 需命令行参数或 YAML 字段声明 | ADR-007：物理通道必须显式 `--physical`；错误信息会枚举可走的显式路线引导 agent |
| **刻意不兜底（fail-closed）** | 明确拒绝降级，直接失败 | 见 §12；"反兜底"与兜底同等重要，是可靠性设计的一半 |

总原则：**观察失败 ≠ 空结果**；兜底换路必须留痕；写操作永不因兜底而自动重放。

## 2. 全景一览

| 子系统 | 主路径 | 兜底路线（按顺序） | 类型 |
|---|---|---|---|
| 元素定位 | `id:` provider 快路径 → L1 精确 DFS | L2 名称模糊（仅 `name:`）→ L4 虚拟化容器；`@eN` 先于一切（不参与模糊）；`--near` 锚点裁决多命中 | 自动 |
| 窗口发现 | 原生标题候选（EnumWindows） | UIA 桌面子枚举 → 深扫 owned dialog；无 `--app` 时聚焦链顶层 | 自动 |
| 观察 | 全量快照 | 预算内部分快照 + `truncated`/`truncation_reasons`；桌面根 StaleRef 瞬态重试 3×200ms | 自动 |
| 窗口清单 | UIA 枚举 | `--native`（原生 EnumWindows）/ `--hwnd`（直读） | 显式 |
| 读值 | ValuePattern | TextPattern（**仅当 value 通道确认缺席**，读失败不回退） | 自动 |
| 表格提取 | GridPattern 直读 | DataItem/ListItem 树展平（仅当 pattern 确认不支持，带 `fallback_reason`） | 自动 |
| 下拉选择 | 精确名称匹配 | 子串匹配 → NOT_FOUND（两层都试尽） | 自动 |
| click | UIInvoke | UISelectionItem → UIToggle → UIExpandCollapse → NOT_ACTIONABLE（错误信息枚举显式路线） | 自动（语义内） |
| scroll | UIA Scroll pattern | **物理滚轮**（pattern 存在但调用失败时）——全系统唯一自动 UIA→物理兜底 | 自动，`via` 上报 |
| 文本输入 | 键盘逐字（分段） | `--paste` 剪贴板通道（中文 IME 下唯一可靠文本通道）；部分投递如实报 `partial` | 显式（paste）/自动（分段降级） |
| 值写入 | `set-value` | 失败提示改走 `type`（键盘路径） | 错误信息引导 |
| 物理指针 | — | `--physical` 显式通道；`xy:` 坐标目标物理专用 + `--app` 边界护栏 | 显式 |
| 截图 | PrintWindow+PW_RENDERFULLCONTENT | 失败退化窗口矩形 BitBlt（遮挡者如实入镜）；DWM 扩展边框 → GetWindowRect | 自动 |
| 窗口关闭 | WM_CLOSE（优雅） | `--system-close` 投 SC_CLOSE（点 X 同命令） | 显式 |
| 关闭证明 | 窗口消失 | 进程身份核验（`verify-closed`；PID 复用永不算关闭） | 显式命令 |
| 显示端 | 复用就绪租约 | RECOVER_MESSAGE 恢复 → 重生 actl-signal.exe → NOT_ACTIONABLE（带诊断 taxonomy） | 自动 |
| 交接查询 | CHECK_MESSAGE 一次 | 慢/超时重发**一次**（绝不重放动作） | 自动 |
| 流程步骤 | 单次执行 | `retry:`（只读自动重试）→ `recovery:`（有界预算）→ `--retry-step`（人工授权重放写） | 显式（YAML/旗标） |
| 段失败 | 整轮失败 | `on_failure: skip_section` 段隔离续跑，整轮 needs_review | 显式（YAML） |
| 分支 | `when` 命中执行 | `otherwise` 备选分支，否则 Skip | 显式（YAML） |
| 失败证据 | 全量结构化证据 | 8KiB 截断 → 脱敏子集 → synthetic 最小证据（带错误原文 200 字） | 自动（阶梯降级） |
| 失败留证 | — | 整屏 PNG + 原生窗口清单 + 定向 UIA 快照三通道互相覆盖，诊断吞错不改流程结果 | 自动 |

## 3. 元素定位（locate 链，`crates/actl-uia/src/locate.rs::locate`）

执行顺序（`uia:{}` 组合选择器除外，见下）：

1. **`@eN` ref 重放优先**：先试 `identity::scoped_ref`（局部快照作用域内重放），再全局 DFS 计数重放；
   数不到 → STALE_REF（"树在快照后已变化"）。**ref 不参与模糊兜底**——ref 漂移就该报
   STALE_REF，模糊化重解析反而可能点进别的元素。
2. **`id:` provider 快路径**（`fast_exact_ids`，TreeScope::Subtree 单次 COM 往返）：单命中直接返回
   `primary`；**确定空手**（id 无模糊层）直落 L4；多命中或快路径不可用 → 全遍历（查询失败不静默降级）。
3. **L1 精确 DFS**（`collect_matches`），与快照 ref 编号同源。
4. **L2 名称模糊**：仅 `name:` 目标且 L1 空手时；大小写/空白不敏感（`actl-core fuzzy_contains`），
   `resolved_by: "fuzzy-name"`。
5. **L4 虚拟化容器**（`locate_virtualized`，ItemContainerPattern::FindItemByProperty）：未物化的
   列表项（Explorer/任务管理器）；仅 Name/Id 目标；多命中 → AMBIGUOUS。
6. **L3 序数**（`select` 内）：`role:X#N` / `id:X#N` 直选索引；越界 → NOT_FOUND。
7. **`--near` 锚点**：多命中时按矩形中心最近距离裁决；裸锚点串按 `name:` 子串语义。

终态：NOT_FOUND（错误信息枚举已试层级，如 "exact, fuzzy and virtualized-container levels"）
或 AMBIGUOUS（附结构化候选，agent 免重取 snapshot 直接裁决）。

**组合选择器 `uia:{...}` 是独立严格路由**（`scoped_locate::locate`）：无模糊/序数/near 兜底，
唯一匹配才通过；provider FindAll 失败绝不落到 DFS 或空结果；遍历预算超限 →
NOT_ACTIONABLE `selector_budget_exceeded`。设计见 [33](33-uia-composite-selectors.md)。

**`find` 命令的虚拟化重路由**（`actl-cli/src/query.rs`）：物化树 0 命中且查询是 name/id 时重走
locate L4；仅当 `resolved_by == "item-container"` 才接受；重路由的 NotFound 吞为"无命中"。

**窗口发现**（`window.rs::find_window`）：`hwnd:` 前缀走严格 scoped 路由 → 原生标题候选
（`native_windows::title_elements`，先于 UIA provider）→ UIA `desktop_children`；根扫描 0 命中才
深扫 per-window owned dialog；WinUI 辅助窗折叠（ApplicationFrameWindow/TitleBar 让位 CoreWindow）；
无 `--app` 时用聚焦链顶层（`top_level_of_focused`），但 **REF 目标拒绝该路径**（fail-closed：
`@eN` 编号只对快照来源窗口的遍历顺序有意义，对着"当前恰好聚焦的窗口"重放会静默点进别的应用）。

## 4. 观察与快照（`crates/actl-uia/src/observation.rs`、`window.rs`）

- **部分快照**：节点/深度预算内收集，超限打 `truncated` + `truncation_reasons`
  （`node_limit`/`depth_limit`）；`require_complete` → NOT_ACTIONABLE"缩小观察范围"。
  `find` 拒绝过滤截断树；**截断结果不能证明缺席或唯一性**（`scoped_locate` 的裁决同样如此）。
- **桌面根瞬态重试**（`observation.rs::with_transient_retry`）：3 次 / 200ms，只重试 StaleRef
  （无关窗口创建/销毁与 UIA 枚举的竞态）；其他错误立即传播；耗尽保留原错误并附 `attempts` 证据。
  窗口子树枚举**故意不做**此重试。
- **原生标题优先**：`title_elements` 用 EnumWindows 在触碰任何 accessibility provider 之前解析
  候选；返回 None 时保留 UIA 路径（虚拟/非原生窗口）；UIA 解析期间身份复核（PID+标题）失败 →
  STALE_REF。背景：已知原生窗口的定位不被无关应用的整桌面 UIA 枚举故障阻断。
- **`list-windows` 双通道**：默认 UIA；`--native` 走原生枚举（含隐藏/无标题窗，不依赖 UIA）；
  `--hwnd` 直读单窗。枚举失败是错误，**绝不返回空列表**（[30](30-window-infrastructure.md)）。
- **挂起窗口唤醒**（`window.rs::ensure_window_responsive`）：WM_NULL 探针（60ms）发现挂起后
  有界等待（≤1600ms）直到恢复泵消息；UIA-only 元素直接放行；上报 `wake_ms`。把逐属性 ~2s 的
  DCOM stall 变成一次有界可上报的等待。
- **快照新鲜度**（`check_snapshot_freshness`）：记录缺失 → STALE_REF"过期或异机"；RuntimeId 不匹配
  → STALE_REF"窗口实例已变，重取快照再重放"；**空的记录 id 永不授权重放**。局部快照重放
  （`scoped_snapshot::replay`）用原选择器/预算重新观察，容器 NotFound/Ambiguous 映射为 STALE_REF。
- **wait 族缺席语义**（`scoped_locate::observed`）：只有 NOT_FOUND 表示缺席，其他错误码传播；
  `wait --gone` 额外把观察期错误（垂死窗口的 StaleRef/Internal）视为"尚未消失"在超时内继续轮询；
  AMBIGUOUS 立即失败。`snapshot --view content` 无 Document 区域 → NOT_FOUND，**不静默回退全树**。

## 5. 读取与断言通道

- **读值通道**（`read_channel.rs::choose`）：ValuePattern 有值 → `value-pattern`；确认缺席
  （Ok(None)）才试 TextPattern → `text-pattern`；**读失败（Err）永不回退**（测试
  `failed_value_read_never_falls_back` 锁死）；空字符串是有效观察，不触发回退；两通道都缺 →
  NOT_ACTIONABLE `pattern_unsupported`。通道经 `value_via` 上报。契约讨论见 [37](37-uia-observation.md)。
- **`verify --property` 缺省通道**："value 然后 Text" 自动顺序；`--value` 时代遗留回退/name/text 可选。
- **`wait --stable` 采样通道阶梯**：text > value > name（`locate.rs::stability_sample`）。
- **表格提取**（`table.rs::extract_table`）：GridPattern 直读（`source:"grid-pattern"`）；仅当
  IsGridPatternAvailable **确认不支持**才树展平（`source:"tree-flatten"`，
  `fallback_reason:"grid_pattern_unsupported"`）；无行 → NOT_ACTIONABLE；中途失败报错、
  **不出半张表**；读取期间行列数漂移 → incomplete `table_changed`；单元格文本 value→name 回退。
- **下拉选择**（`action.rs::select_option`）：精确名（`matched_by:"exact-name"`）→ 子串
  （`"substring-name"`）→ NOT_FOUND（错误信息注明两层都试尽）；多命中 AMBIGUOUS 附候选名。
- **剪贴板争用**（`read.rs::open_clipboard`）：被他进程持有 → NOT_ACTIONABLE"稍后重试"；
  ClipGuard 保证始终关闭。

## 6. 输入子系统（`crates/actl-uia/src/action.rs`、`pointer.rs`、`kbd.rs`、`input.rs`）

- **click 模式派发链**：UIInvoke → UISelectionItem（select）→ UIToggle → UIExpandCollapse，
  实际 pattern 随结果上报（`invoke`/`selection-item`/`toggle`/`expand-collapse`）；耗尽 →
  NOT_ACTIONABLE，错误信息本身枚举显式兜底路线："scroll/focus first, try `set-value`, or
  `--physical` for a real click"。
- **scroll**：UIScrollPattern 优先（容器语义、可后台）；pattern 存在但调用失败（实测记事本
  Document）→ 物理滚轮打在元素 clickable point 上（`via:"physical-wheel"`）。这是**全系统唯一
  一处自动跨 UIA→物理的兜底**，靠 `via` 字段保持可见；两通道都失败 → 底层 NOT_ACTIONABLE。
- **物理指针通道**（ADR-007）：`--physical` 显式 opt-in；元素路径与 pointer 命令（右键/双击/
  中键/三击/hover）走 `pointer_physical`；**`xy:` 坐标目标物理专用**——不带 `--physical` 直接
  PROTOCOL 拒绝，且必须 `--app` 做窗口边界护栏（遮挡/最小化/点越界/准备期目标漂移均拒绝）。
- **点击点退化**：`get_clickablePoint()` 无 → BoundingRectangle 中心（受控退化，仍受命中检查）；
  两者都无 → NOT_ACTIONABLE。
- **滚入视口**（`pointer.rs::ensure_visible`）：离屏时 ScrollItemPattern；失败静默跳过——后续
  clickable point/命中检查会如实失败；结果经 `scrolled_into_view` 上报。
- **命中检查身份回退**：优先爬父链找原生 HWND 严格比对；provider 无 handle（记事本类）→
  按标题重解析窗口比对（WinUI 框架/CoreWindow 的 HWND 分裂使严格比对恒失败，故用根标题）；
  否则 NOT_ACTIONABLE。
- **`--no-activate` 后台点击守卫**（`bg_style.rs::NoActivateGuard`）：设 WS_EX_NOACTIVATE 前先写
  WAL journal（崩溃留恢复数据）；每次输入锁获取时 `recover_stale` 清扫死主残位；目标是提权窗口 →
  PERM_DENIED 提示去掉该 flag 走普通物理点击；点击后光标还原 best-effort。
- **键盘准备链**（`action.rs::prepare_keyboard`/`ensure_element_focus`）：窗口 SetFocus → 元素级
  SetFocus 重试（`focus_retries` 轮）→ 用**物理** GetGUIThreadInfo 核验（UIA 焦点视图在 WinUI 上
  会说谎）；已聚焦则不动（保护多 Edit 应用）；重试耗尽不报错，交给权威的前台护栏做最终裁决。
- **VK/UNICODE 双通道无静默降级**（`kbd.rs::compile_key_spec`）：带修饰键强制 VK 通道；不可映射
  字符 + 修饰键 → PROTOCOL"改用 `type` 发送"（历史上静默 UNICODE 降级曾把 ctrl+s 打成裸 s）；
  纯文本走 KEYEVENTF_UNICODE 绕过 IME；修饰键被物理按住 → 有界等待后 NOT_ACTIONABLE；
  SendInput 部分失败 → INTERNAL"键状态未知，勿盲目重试"。
- **文本输入双通道**：默认键盘逐字、分段投递（`input.rs::send_in_segments`，段间焦点复查，
  失焦停在段边界并如实报 `input_effects.keyboard="partial"` + `delivered`）；`--paste` 显式走
  剪贴板 + ctrl+v（`via:"clipboard-paste"`，中文 IME 下唯一可靠文本通道）。`set-value` 失败的
  错误信息建议改试 `type`；`type --verify-value` 读回核验（≤2s，身份变化 STALE_REF、值不符
  ASSERTION_FAILED，**从不重写**）。
- **输入锁**（`input.rs::InputLock::acquire`）：会话命名互斥体做 actl 实例间输入互斥；
  WAIT_ABANDONED（前任持有者崩溃）**接管继续**（残留修饰键清理由 kbd 侧负责）；
  WAIT_TIMEOUT → TIMEOUT"另一 actl 实例持有输入锁"。

## 7. 窗口生命周期与清理（`crates/actl-uia/src/window.rs`、`window_check.rs`、`desktop.rs`）

- **close-window 双路由**：默认 PostMessageW(WM_CLOSE)——优雅关闭，未保存内容由应用自行弹提示
  （语义关闭，不是强杀）；`--system-close` 投 WM_SYSCOMMAND/SC_CLOSE（点标题栏 X 的同一命令），
  用于"收起窗口但不退进程"的应用（实测 Excel：后台正确关闭文档后裸框架对 WM_CLOSE 只隐藏）。
  无原生 handle → NOT_ACTIONABLE；PostMessage 失败 → INTERNAL。定案记录见 [68](68-combined-batch-first-round.md)。
- **窗口寻址**：标题子串 / `--pid` / `--hwnd`；pid 歧义 → AMBIGUOUS"传 --hwnd 或标题"；
  特殊窗口只能经 `--hwnd` 的 element_from_handle 读到。
- **关闭证明**（`verify-closed`，`actl-cli/src/file_check.rs`）：窗口消失本身不证明关闭——读进程
  创建时间身份做核验；**PID 复用永不算已关闭**；5 秒沉降重读。**GUI 应用禁 taskkill**（会话复活：
  记事本 TabState+云同步、计算器僵尸框架，AGENTS 硬规则），清理一律优雅关闭。
- **verify-window 沉降**（`window_check.rs`）：ASSERTION_FAILED 在 15 秒内重读（带加载项的
  Office 拆窗滞后 3-15s）；AMBIGUOUS 立即失败；`hwnd:` 前缀直读原生不整窗枚举。
- **focus-window**（`window_ready.rs::activate`）：前置条件逐项核验（启用/响应/还原/可见/在屏，
  各带独立失败原因）；SetForegroundWindow **只请求一次**（不合成 Alt 键 hack）；有界前台观察；
  失败 → NOT_ACTIONABLE `foreground_not_acquired` 附激活证据。
- **桌面整理**（`desktop.rs::prepare`/`restore`）：每次最小化**之前**先写 journal（崩溃保留恢复
  数据）；恢复按逆序、逐窗做放置身份复核，跳过 `closed_or_replaced` / `changed_or_unconfirmed`
  的窗口；请求的窗口缺失 → NOT_FOUND 附部分 `results`，保留证据与会话。

## 8. 截图像素通道（`crates/actl-uia/src/capture.rs`）

显式的"UIA 盲区兜底"通道（默认观察仍是 UIA）：四个独立域——`--rect`（BitBlt+CAPTUREBLT，
所见即所得）、整屏（虚拟桌面，失败留证默认视图）、`--app`/`--hwnd`（窗口）、`--element`
（定位+包围盒裁剪）。

- **窗口截图退化**：PrintWindow+PW_RENDERFULLCONTENT（抗遮挡）→ 失败退化窗口矩形 `blt_region`
  （遮挡者入镜，如实行为）；DWM 扩展边框优先，GetWindowRect 回退（坐标点击护栏同款 DWM 优先）。
- 最小化 → NOT_ACTIONABLE"先还原再截"（不隐式还原）。
- `xy:` 坐标目标是该通道的消费端：截图 + 人/VLM 选点，核心自身不做视觉（[16](16-browser-scenarios.md)）。

## 9. 显示端与桌面交接（`crates/actl-uia/src/display_gate.rs`、`handoff.rs`）

- **显示端 ensure 四级**：复用就绪且存活的租约 → 向兼容的在场伴侣发 RECOVER_MESSAGE 恢复 →
  窗口类缺失时重生 `actl-signal.exe`（二进制缺失 → NOT_ACTIONABLE 指明"actl.exe 旁边找不到匹配
  的 actl-signal.exe"）→ 有界就绪等待，超时 NOT_ACTIONABLE"display not ready" 附完整诊断
  taxonomy（companion_missing / lease_missing / version_mismatch / companion_unresponsive /
  lease_stale / hidden / capture_in_progress / covered_or_offscreen / frame_not_ready /
  window_identity_mismatch）。**急停状态跨替换保留**。
- **execution-recover 门**：不兼容伴侣在场时拒绝（先关/升级再恢复）；否则归档原始状态、写新代际。
  契约见 [47](47-execution-recovery.md)。
- **交接查询重试**（`handoff.rs::query`）：CHECK_MESSAGE 响应慢/超时 → 重发**一次**（"重查同意，
  绝不重放动作"）；持续失败 → NOT_ACTIONABLE `companion_unresponsive`。
- **预告确认丢失**：`wait_presented` 超时 → NOT_ACTIONABLE"显示端未确认本次动作预告，动作未投递"
  ——宁可不做，不做猜着投。
- **会话中显示端失联**：`check_active` →"显示端不可用，后续输入已停止"。

## 10. 流程引擎（`crates/actl-flow`）

- **重试梯**（`retry:` 字段）：`never` / `read_only`（3 次尝试）/ `stale_read`（仅只读调用最多
  3 次、间隔 200ms、observation_retries 上限 32）；**写操作结构性不可能自动重试**（引擎 + CLI
  预检双重把关）；重试前复核绑定输出的新鲜度，不让重试消费过期的业务值。规范见 [20 §9](20-yaml-workflow-protocol.md)。
- **有界恢复预算**（`recovery:` 字段，`bounded.rs`）：max_retries 1..10 / timeout_ms 1..60000 /
  interval_ms ≤ timeout；与 `retry` 互斥；可重试码 = StaleRef/Timeout/NotFound/**Internal**
  （docs/70 复盘纳入，冷启动 UIA 抖动）；带 `@` ref 参数的调用、截断/交接证据**不重试**；睡眠前
  先做崩溃安全的重试预约；预算耗尽保留原错误 + 完整事件尾。契约见 [64](64-bounded-observation-recovery.md)。
- **uncertain 写的处置**：恢复时对 Uncertain/Running/Verifying 的写步骤**先重跑 verify**（"被打断
  的写可能已生效，先读后置条件再谈重放"）；已确认 → Completed；未确认 → NeedsReview（reason
  `result_unconfirmed`）；`--retry-step` 必须指名 uncertain 步骤，且仅当 verify 确认 ASSERTION_FAILED
  才授权重放。
- **段隔离**（`on_failure: skip_section`）：失败步骤保留证据，同段其余 pending 步骤标 skipped，
  继续后续段，整轮以 needs_review 收束（绝不 completed）；后续授权重试成功则还原同段重跑。
- **路线缺陷识别**（`route_defect`）：同一非只读步骤最后三次记录尝试同错误码 → run reasons 追加
  `route_defect_suspected`——防止有界恢复掩盖坏路线。
- **分支备选**（`branch.rs::choose`）：条件真 → Run；假 → `otherwise` 备选分支（若声明）或 Skip；
  决策在任何副作用前持久化；**条件观察失败 → Paused/Failed，绝不解释为假**。
- **输出新鲜度**（`freshness.rs`）：绑定的 `max_age_ms` 输出使用前重验；过期/时钟回拨/缺捕获时间
  → PROTOCOL"审核并重备计划"，整轮 Paused/Failed。见 [38](38-flow-output-bindings.md)。
- **证据降级阶梯**：全量证据（策略 full 且 ≤8KiB）→ `{"omitted":"evidence_exceeds_8_kib"}` →
  脱敏 Metadata 子集 → synthetic 最小结构（带错误原文截断 200 字）。
- **资源初始化**：失败/部分初始化**保留不覆盖、不自动重试**；恢复时从不重新初始化。

## 11. CLI 编排层（`crates/actl-cli`）

- **launch 解析链**（`interact.rs::launch`）：App Paths 注册表 → 直接路径 → PATH 扫描补 `.exe`；
  首次 spawn 失败后用注册表全路径做**第二次 spawn 尝试**；最终 NOT_FOUND。`--wait-window` 失败
  附 `spawned_pid` + `process_started:true` + `relaunch:false`——进程起了不等于窗口可用，且不需要
  盲目重启。
- **batch**：首错即停，已执行步骤搭错误证据车；步骤前与失败时采样的意外前台弹窗记为非阻塞
  `blocked_by`；`windows_created` 差集附指定的清理武器提示（close-window --pid/--hwnd）；终局窗口
  清点失败降级为 `window_observation_error`，**绝不转空列表**。
- **`--expect` 后置断言**：`window:<substr>` 用动作前 RuntimeId 基线——预先存在的同题窗口满足
  不了它（`wait_new_window`）；`element:<target>` 是"物化检查"（UIA 动作报成功但无实效的应用），
  失败把 TIMEOUT 重映射为 ASSERTION_FAILED 并保留原因。
- **失败留证三通道**（`flow.rs::capture_failure`）：整屏截图 + 原生窗口清单 + 定向 UIA 快照；
  UIA 快照失败可容忍（另两通道已覆盖）；全部诊断吞错——**留证永远不改流程结果**。
- **fail-closed 的 `--app` 要求**：`type`/`press`/`key-down`/`key-up` 无 `--app` 直接拒绝，没有
  "当前前台"兜底（前台一变就会静默打进别的应用）。

## 12. 刻意不兜底清单（fail-closed）

| 场景 | 行为 | 理由 |
|---|---|---|
| `@eN` 重放数不到 | STALE_REF，不模糊化重解析 | ref 漂移时模糊重解析可能命中错误元素 |
| 读失败（Err） | 传播原错误，不换读取通道 | 通道故障与值缺席是不同事实；静默换通道会掩盖故障 |
| 截断观察 | NOT_ACTIONABLE/拒绝裁决 | 截断树不能证明缺席或唯一性 |
| `snapshot --view content` 无 Document | NOT_FOUND | 不静默回退全树（token 控制契约） |
| PERM_DENIED | 永不重试 | 提权边界不是瞬态 |
| 写操作 | 永不自动重放（引擎与 `--verify-value` 双重锁死） | 重放不可幂等的写 = 双重生效 |
| 修饰键 + 不可映射字符 | PROTOCOL 拒绝 | 静默 UNICODE 降级曾把 ctrl+s 打成裸 s |
| 观察失败 | 报错/未知状态，不转空列表 | 空列表会被当成"确认没有"消费 |
| GUI 应用清理 | 优雅关闭，禁 taskkill | 强杀导致会话复活（记事本/计算器实证） |
| `xy:` 坐标目标 | 无 `--physical` + `--app` 即 PROTOCOL | 坐标点击无边界护栏会打到别的窗口 |
| 条件观察失败（flow 分支） | Paused/Failed | 不把观察失败解释成条件为假 |
| 前台确认丢失（显示端） | 动作不投递 | 宁可没做，不做猜着投 |

## 13. 可观测性：兜底必须可见

每次实际换路都落在结果字段里，agent 可归因、可审计：

- `resolved_by`：`primary | fuzzy-name | item-container | scoped-ref-replay | ref-replay | ...`
- `via`：`invoke | selection-item | toggle | expand-collapse | uia-scroll | physical-wheel |
  clipboard-paste`
- `value_via`：`value-pattern | text-pattern`
- `source` + `fallback_reason`（extract）：`grid-pattern | tree-flatten` + `grid_pattern_unsupported`
- `truncated` + `truncation_reasons`（snapshot/find）
- `wake_ms`、`scrolled_into_view`、`input_effects.keyboard="partial"` + `delivered`

## 14. 维护规则

- 新增/修改兜底：同 PR 登记本文对应小节；自动兜底必须配可观测字段；涉及协议面（错误信息、
  结果字段、旗标）时按硬规则 8 走 `spec` 提交（golden + docs/06 + CHANGELOG 同步）。
- 收紧/删除兜底（把自动改显式、把降级改 fail-closed）同样按 `spec` 处理。
- 待完善项登记 [ROADMAP](ROADMAP.md)，本文只记现状。
