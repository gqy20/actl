# 06 · 设计蓝图:Windows AI-native 桌面操作 CLI

更新:2026-09-28 · 状态:现行命令语义与设计；实现记录见 CHANGELOG，待办见 ROADMAP · 前置:[01 调研](01-research-landscape.md) · [04 选型](04-tech-decision.md) · [05 估算](05-implementation-estimate.md)

## 1. 定位

### 流程日志阶段与结果（2026-09-28）

执行型 flow 的 call_finished 在已有调用 phase/error_code 之外增加 workflow_status、
workflow_completed、workflow_reason、workflow_error_code。done 只表示调用处理完毕；
是否完成流程以 workflow_completed 为准。没有可用流程上下文时不输出这些字段。
workflow_invocation 逐次记录 run/before/verify/resume_check 的 started/succeeded/failed、
attempt 和错误码，继承 call_id/run_id/step_id；不含命令参数和业务内容。
flow_resume_requested 记录请求 run_id、retry_step 及来源 cli/signal_control/unknown，
source_verified=false。显示端通过 ACTL_FLOW_REQUEST_SOURCE=signal_control 标记子进程；
该信息仅供诊断，不证明用户点击或授予权限。未标记旧显示端也会归入 cli。

### 流程重新观察与恢复核验（2026-09-28）

新增 `retry: stale_read`：只读调用遇到 STALE_REF 最多调用三次，写调用始终单次；
state.steps[].observation_retries 保存阶段、错误码和重试序号（最多 32 条），暂停/急停阻止后续重试。
新增可选 `resume_check`（verify/wait）：恢复时每个未完成步骤先核对上下文，再核验 uncertain 结果。
失败保存 resume_check_failed，保留原步骤阶段；不会自动导航、切换目标或重新授权。
恢复写步骤的结果核验若无法观察，保存 needs_review/result_observation_failed，
即使指定 --retry-step 也不重放；只有明确 ASSERTION_FAILED 加显式重试才可进入写步骤。
完整约束见 [20 §9](20-yaml-workflow-protocol.md)，实现与验收矩阵见 [39](39-flow-observation-recovery.md)。

### 流程输出绑定（2026-09-28）

flow v1 增加可选 outputs（只读 run 的 JSON Pointer＋显式类型）与 bind（前序步骤输出替换完整参数）。
字段和实际 CLI 参数分别预检，输出随步骤状态持久化，恢复不重跑已完成步骤；写步骤不导出数据。
state/run 响应的步骤可带 outputs，显式保存的业务数据不受 error evidence 脱敏策略影响。
缺失/类型/绑定错误为 PROTOCOL；绑定解析失败保存 paused/failed、reason=binding_validation_failed，动作次数不增加。
输出可声明正整数 `max_age_ms`；步骤保存 `outputs_captured_ms`（读取调用开始时间）。
消费步骤入口与每次 run 调用前检查有效期，超限、缺失时间或当前时间早于采集时间时，
保存 PROTOCOL 与 reason=output_freshness_failed，按 on_failure 暂停或失败，保留已有步骤阶段。
诊断只包含输出名称、来源步骤和时间，默认 metadata 也保留；不自动刷新或重放已完成步骤。
完整语法、预算、负号参数限制及恢复语义见 [20 §8](20-yaml-workflow-protocol.md#8-显式输出与完整参数绑定2026-09-28)，测试记录见 [38](38-flow-output-bindings.md)。

### UIA 读取状态修订（2026-09-28）

- `get` 保留成功读取的空字符串；`value` 仅在 ValuePattern 明确不支持时回退 TextPattern，读取失败不回退。
- 默认 `get` 的 data 增加 `unsupported` 属性名数组（如 `["value","text"]`），表示已确认不支持的读取通道；空数组表示本次没有不支持项。其他读取错误使命令失败，不能输出看似完整的成功结果。
- 显式 `get --property value|text`、verify 和属性等待遇到不支持通道返回 NOT_ACTIONABLE，evidence.reason=`pattern_unsupported`；value 的 get/自动等待允许上述双通道，而 `verify --property value` 严格读取 ValuePattern。
- UIA 元素失效映射 STALE_REF，访问拒绝映射 PERM_DENIED，其余 provider/属性读取错误映射 INTERNAL；evidence 带 channel、native_code（原生错误存在时）。观察失败不能等同空值、断言不满足或稳定。
- `wait --stable` 只在明确不支持时按 Text → Value → Name 取样；`type --verify-value` 复用读取状态并在错误中保留 delivered/value_verified/retry，禁止重放输入。
- 本轮未扩展等待谓词的空期望语法；snapshot/定位/整表提取的后续修订如下，证据见 [37](37-uia-observation.md)。

#### 观察完整性修订

- snapshot、旧语义定位与表格树回退使用显式 RawView 的可失败枚举；子级为空与枚举错误分开。必要属性读取失败按 channel/native_code 报错，成功空名称与 ID 保留为空字符串。
- snapshot 增加 `truncation_reasons` 数组，完整时为 `[]`；节点/深度预算实际省略内容时分别记录 `node_limit` / `depth_limit`，并设 `truncated=true`。恰好达到上限但无未观察节点不算截断。属性或枚举失败整次报错，本轮不提供携带属性错误的部分树。
- 过滤式 find、旧语义定位/ref 重放和表格树提取拒绝不完整范围，返回 NOT_ACTIONABLE，evidence 为 `{reason:"observation_incomplete",causes:[...]}`。content 投影在截断范围内未见 Document 时也不能声称 NOT_FOUND。成功采样仍不是原子快照，控件可能在调用之间变化。
- 所有元素等待仅将 NOT_FOUND 解释为缺席；STALE_REF、权限、provider 错误及超限立即传播。虚拟化查询区分成功空结果与原生失败，多个匹配拒绝为 AMBIGUOUS。
- extract 仅在 GridPattern availability 明确为 false 时回退树提取，输出 `fallback_reason:"grid_pattern_unsupported"`；Grid 直读时为 null。Grid 属性、单元格失败不回退；合法空表和空单元格保留。负行列数报 INTERNAL；超限或前后行列数变化报 NOT_ACTIONABLE（causes 为 `table_limit` / `table_changed`）。树回退只反映已物化 Text/Edit 单元格，不保证与逻辑表格的列结构一致。
- 窗口清单、窗口 RuntimeId 基线、等待新窗口及按 PID 定位也使用可失败的 RawView 枚举；根、子窗口、分类及必要 name/class/pid/hwnd/runtime_id 读取失败传播原错误，不输出成功的部分清单，不用漏掉候选后的结果证明唯一性。成功空标题仍按清单约定过滤。
- `wait --idle` 的 GetLastInputInfo 失败返回 INTERNAL，evidence 为 `{reason:"input_observation_failed",api:"GetLastInputInfo"}`，不把未知状态当空闲；每次轮询先检查 flow 暂停/停止。Rust `idle_ms` 接口相应返回 Result。
- 输入交接增加独立 Raw Input 键鼠活动计数。注册丢失、消息泵心跳超过 1000ms 或监听线程退出返回既有 `input_monitor_unavailable`，撤销旧授权；健康恢复不恢复授权。不存储键值或坐标，不发送探针输入，不替代 `--physical` 参数或人工授权。真实设备接管验收另记。
- 预算约束的是遍历节点/深度与表格规模；单个同步 COM 调用仍可能阻塞，不宣称硬超时或全局观察原子性。

实验说明：`prototypes/browser` 独立验证 CDP 会话、CSS/XPath、iframe 和写后核验。
它没有新增 actl 命令、目标语法或 YAML 字段，报告文件不属于本协议；范围与验收见 [34](34-browser-connection-prototype.md)。

一句话:**给 AI agent 用的 Windows 桌面原子操作 CLI——accessibility(UIA)驱动、原子命令可独立调用、输出机器友好。** 类比:Playwright 之于浏览器,本工具之于 Windows 桌面。

平台范围：Windows；Linux/macOS 执行后端暂不考虑实现。MCP 计划作为独立适配服务，
在被控设备本地启动，供其他设备调用本设备；尚未实现，规划见 [ROADMAP](ROADMAP.md)。

- 脑(agent 的工具循环)与手(本 CLI)彻底分离;
- 协议蓝本:agent-desktop(Apache-2.0,直接沿用其 envelope/ref/错误码契约);
- UIA 实现参考:terminator(MIT,Rust);
- 可靠性工程借鉴:RPA 20 年经验(见 [03](03-rpa-vs-quicker.md))。

**命名**:bin = `actl`(agent control)。查证结论(2026-09-26):crates.io 发布名 `actl-cli` 可用、npm `actl` 可用、无知名同名 CLI。备选 `ctl` 因 [conduktor/ctl](https://github.com/conduktor/ctl)(Conduktor 官方 Kafka CLI)撞名 + npm `ctl`/`ctl-cli` 双双被占而否决。crate 前缀 `actl-*`。

## 2. 架构原则

1. **无状态原子命令**:每次调用独立进程,无隐藏会话;状态只存在于两个地方——屏幕本身,和 agent 的上下文。
2. **UIA 优先,物理兜底(2026-09-26 裁定修订)**:默认走 UIA pattern 调用(后台执行、不抢物理鼠标键盘)。物理模拟(SendInput)按输入通道分治授权:**键盘**(press/type 本体即注入)的授权 = 强制 `--app` 绑定 + verify-then-inject 护栏 + 进程间输入占用锁,不另设 `--physical`;**指针**(真实移动/点击鼠标,独占用户的物理指针)必须显式 `--physical`(`click --physical`,后续 right-click/drag/hover 同)。
3. **为自愈设计**:错误返回携带恢复线索(变化线索、evidence 引用),目标是最小化 agent 下一轮恢复成本。求韧不求稳。
4. **token 是一等约束**:默认返回紧凑语义树;大 UI 用 skeleton 投影;不做"全树 dump"这种默认行为。

## 3. 命令面设计

### 3.1 设计原则(agent 认知友好)

- **扁平动词式**:不嵌套分组(对比 `actl interact click`),动词即语义——省 token、零结构记忆负担;分组只用于 help 分段标题。
- **一个命令 = 一个认知动作**:命令语义对齐 agent 循环的四步心智模型——**感知**(`snapshot`/`find`)→ 决策(LLM)→ **执行**(`click`/`type`)→ **确认**(`--expect`/`verify`)。禁止复合语义命令(如 click-and-type):失败无法归因,中间决策被吞掉。
- **命名四规则**:① `动词` 或 `动词-宾语`,小写连字符;② 读操作前缀固定:`get-*` / `find` / `list-*` / `inspect` / `extract`;③ 位置参数只放目标(@ref / 语义选择器 / `xy:x,y` 坐标——坐标为物理专用寻址,见 §7),其余全 flag;`--app` 窗口锚定接受标题子串或 `hwnd:<n>`(2026-09-27:SPA 整页跳转改标题,HWND 是窗口身份),谓词统一 `--expect`/`--predicate`、超时统一 `--timeout`;④ 全局 flag 极小:`--dry-run` `--timeout` `--verbose` + 体积控制(`--skeleton` `--max-depth`)。
- **自发现**:`actl commands` 输出全部命令 + 参数 schema 的 JSON(等价于 MCP `tools/list` 的 CLI 形态);人类 help 文本与机器 schema 同源生成,不漂移。agent 冷启动一条命令拿全能力清单,不依赖读文档。
- **可理解性验收标准**:任一命令的 help + envelope,不看文档即可推断四件事——输入什么 / 副作用是什么 / 返回什么 / 失败码指示的下一步动作。

### 3.2 分层模型:哪些统一,哪些适配

#### 容器局部快照（2026-09-29）

`snapshot --app <window> --within 'uia:{"match":{"id":"panel"}}' [--depth 3] [--limit 100]`。
也可用 `--hwnd` 绑定窗口；不允许隐式使用前台窗口。within 必须是唯一匹配的严格 UIA 选择器，
不接受 ref、坐标或模糊选择器。depth 为 0–40（默认 40），limit 为 1–5000（默认 5000），
二者只能与 within 一起使用。容器本身计为深度 0 和一个节点；支持继续叠加 skeleton/content 投影。

局部输出增加 `ref_scope:"container"`、投影前的 `observed_nodes` 和 `scope`：
`{version:1,selector,identity,depth,limit}`，identity 包含 RuntimeId/PID/角色/名称/AutomationId。
`truncated` 和 `truncation_reasons` 如实标记深度/数量截断；观察失败保留错误，不伪装为空结果。
定位容器仍需要完整范围内的唯一性核验。严格 UIA 条件含精确 id 或 name 时，使用原生属性条件查询
取回全部候选（优先 id），按原 children/descendants/含根语义查询，保持 RawView，再核验全部组合条件。
候选超过 5000 返回 NOT_ACTIONABLE / selector_budget_exceeded；查询或属性失败直接报错，不回退或取首个命中。
此路径不受无关分支的客户端深度上限阻断；已知 role 也使用原生 ControlType 条件筛选；只有 name_contains 等无原生精确条件时仍按原节点/深度预算遍历。
适用于严格 find、动作定位、局部快照及其绑定重放；局部快照本身的 depth/limit 截断规则不变。
单个原生查询仍为同步调用，可能因 provider 缓慢阻塞，不宣称硬超时或服务端工作量上限。

局部 ref 从 `@e1` 重新编号，必须携带本次返回的 `--snapshot <snapshot_id>` 使用，不能与全窗口 ref 混用。
持久化记录保存容器和预算；重放时按原范围重新观察，核对窗口、容器和具体元素身份。
容器缺失、歧义、变化或记录无效返回 STALE_REF，不回退到全窗口序号；其他 provider 错误保留。
截断快照只允许核验已记录元素，不能据此证明未出现的元素不存在。
未绑定 snapshot 的旧式 ref 仍是全窗口序号语义，无法携带局部范围，应使用绑定方式。
此能力减少输出内容，不承诺降低总扫描耗时；输出字节数不能等同 tokenizer token 数。

#### 独立目标预览（2026-09-29）

`preview <target> --app <window> [--snapshot <id>] [--message <text>] [--duration <ms>]`。
只展示已定位 UIA 元素的边框和调用方提供的动作说明，不执行目标动作、输入、剪贴板修改或授权。
默认说明为预览提示；说明须为 1–160 字符且无控制字符，duration 为 500–30000ms（默认 3000）。
坐标目标拒绝；ref 必须绑定 snapshot，语义目标每次定位。支持组合选择器，歧义沿用 AMBIGUOUS，不自动选择候选。
临时窗口不激活、不修改 flow 的显示会话；可关闭或超时结束。不允许作为持久化 flow 的 run/check，不构成业务结果验证。

成功 data 为 `{previewed:true,executed:false,exit_reason:"expired"|"dismissed",target,window,bounds,resolved_by,message}`；
bounds 是屏幕物理像素 `[left,top,right,bottom]`。成功表示显示帧已提交，不能证明用户已经看到或认可。
目标离屏、窗口最小化、无有效边界或中心点遮挡返回 NOT_ACTIONABLE；查询失败保留原错误。
预览期间重查同一元素的身份和边界；变化返回 STALE_REF（reason=preview_target_changed），停止返回 ABORTED，均销毁本次窗口。
中心点命中检查不证明控件全区域无遮挡。同步 COM 调用无硬超时，窗口变化与下一次采样之间存在竞态。
显示渲染的 unsafe 限定在 actl-uia，复用现有依赖。设计与验收见 [46](46-target-preview.md)。

候选纠正：`preview @e3 --candidate @e7 [--candidate @e9] --app <window> --snapshot <id> --duration 30000`。
仅在提供 candidate 时启用，含位置参数共 2–20 个不同的 @eN，必须来自同一快照；坐标、语义选择器、重复候选、缺失 snapshot 或超限均 PROTOCOL。候选由调用方从 snapshot/find 结果筛出，不自动截断或猜测歧义结果。
用户可切换上一个/下一个、选择此目标或关闭。所有候选先绑定身份和边界，当前候选持续核验，切回候选时复验；选择时重新解析 ref 并比较原身份/边界，失效拒绝。期间不注入输入，不滚动/激活目标、不授予执行权限；后续动作仍用返回的 ref 和 snapshot 重新核验。
候选对话框初次显示不抢焦点，用户点击时可聚焦对话框本身；底层目标应用不因此获得主动激活。单目标预览继续保持不激活。
选择模式成功 data 为 `{previewed:true,executed:false,exit_reason:"selected"|"expired"|"dismissed",snapshot_id,candidate_count,selected,selection,requires_revalidation:true}`。
明确选择才有 `selected:true` 和 `selection:{index,target,window,bounds,identity}`，index 从 1 起；关闭或超时返回 `selected:false,selection:null`。时长覆盖各候选的累计展示，不随切换重置；同步 UIA 调用不保证硬超时。原单目标输出不变。

#### 结构化 UIA 组合定位（2026-09）

元素目标新增 `uia:<JSON>`，CLI 与 flow 的字符串参数共用同一解析器。例如：

```powershell
# PowerShell 7.3+ 标准原生命令参数传递；Windows PowerShell 5.1 建议改用 YAML args。
actl find --target 'uia:{"match":{"role":"Button","name":"确定"},"within":{"id":"ExportOptions"},"scope":"descendants"}' --app MyApp
```

- 顶层字段仅 `match`（必填）、`within`（可选）、`scope`（默认 `descendants`，另可取 `children`）。
- `match` 与 `within` 各含至少一个条件：`role`、`id`、`name`、`name_contains`；条件之间为 AND。
  role 忽略大小写精确比较 UIA ControlType 名称；id/name 区分大小写精确比较；name_contains 为
  区分大小写子串。name 与 name_contains 互斥，id 中的 `#` 是字面量，不表示序号。
- 不支持嵌套 within、未知字段、重复 JSON 键、任意表达式。条件值 1–4096 UTF-8 字节且无 NUL，
  JSON 部分最多 16 KiB。可选字段 null 等价于未提供，空过滤器拒绝。
- `within` 在目标窗口 UIA 原始子树（含窗口自身）中必须唯一。随后在容器内查询目标，排除容器自身。
  children 只查直接子级；descendants 查全部后代。不指定 within 时从窗口查起，descendants 包含窗口自身。
- 组合查询的 --app 标题子串在 UIA 桌面直接子级中要求唯一，不做旧窗口查找的辅助窗口折叠或 owned-dialog 回退；
  有歧义时使用 list-windows 得到的 hwnd:<n>，嵌套对话框用 within 限定。不传 --app 时绑定原生前台窗口。
  HWND 对应窗口存在但 UIA provider 读取失败，保留 INTERNAL；不会声称窗口已消失。
- 最终目标也必须唯一：零命中 NOT_FOUND，多命中 AMBIGUOUS；evidence 含 phase=within|match、
  count、最多 20 个 candidates 及 truncated（窗口标题歧义时 phase=window）。候选仅含属性，不伪造 snapshot ref。
- 组合查询不执行模糊、序号、距离锚点或 ItemContainer 虚拟化回退，也不自动展开控件。
  不能与 --near/--snear/--to-near 混用，也不能作为这些锚点参数。未物化控件需调用方先显式操作并重新观察。
- provider 枚举或必要属性读取失败返回 INTERNAL，不能证明不存在。每个查询阶段最多 5000 节点、40 层；
  超限 NOT_ACTIONABLE（reason=selector_budget_exceeded），不使用截断结果确认唯一性。COM 调用仍可能阻塞。
- get/click/type/set-value/select/toggle/指针命令/press --target/extract/screenshot --element/
  verify/wait 的元素目标均接受该形式，动作与输入护栏不变。物理鼠标仍要求原命令的显式授权。
  wait 对组合定位的观察错误直接传播，不将其当作消失；NOT_FOUND 仍表示尚未出现或已消失。
- `find --target <uia:JSON>` 用于唯一定位诊断，与旧过滤 flags 及 limit 互斥；输出
  `{window:{title},total:1,elements:[{ref:null,role,name,automation_id,target}],resolved_by:"uia-composite"}`，
  不返回 snapshot_id。带 resolved_by 的命令响应同样标识 uia-composite。
- flow 在执行任何步骤前校验各目标位置及 element: 后置条件里的组合查询，正文参数不按选择器解释。
  运行时每次重新定位，不将 UIA 对象或临时 ref 存进计划。示例见 fixtures/flows/calculator-composite.yml。

| 层 | 内容 | 时机 |
|---|---|---|
| **L1 通用原语**(统一) | 只依赖 OS 契约(accessibility 树 / 窗口系统 / 剪贴板),一套命令吃遍所有应用;**动词集合封闭**(~45 封顶,新增动词需 spec 提交评审) | M1–M2 全量 |
| **L2 语义投影**(机制统一,内容是应用数据) | 同一命令的视图参数(`--skeleton`、`--max-depth`、`--view content`),解决大 UI 的 token 膨胀;投影规则是**数据**(pack.toml,参考 desktop-cli 的 application packs),不是代码 | `--skeleton` 与 `--view content`(2026-09-27)已落地;packs 视真实使用数据再定 |
| **L3 应用适配**(受控逃生舱) | 仅当 accessibility 残缺(Electron 无 aria / Canvas / 老 Win32)或语义不可达(Excel cell 语义):以 **strategy 插件 + fallback 定位链**在命令内部实现,agent 无感知;确有必要才允许 `actl app <name> ...` 独立命名空间,与 L1 永不混淆 | M3,按需 |

**核心纪律:适配是数据/策略,不是新动词。** 应用知识(选择器、投影规则、执行策略)永远不侵蚀 L1 动词空间——这是与 RPA "每个应用一套 activity 库"路线的根本分野,也是命令面不随适配数量膨胀的保证。

### 3.3 解耦与整合

**解耦**(允许独立演进的四条轴):感知与执行(`snapshot` 产出 ref,`click` 消费 ref,中间是 agent 的决策,天然支持重试与自愈)· 动词与目标(`click` 的语义由 UIA pattern 分发,不感知控件类型)· 协议与实现(envelope 在 actl-core,uia/physical 是可替换后端)· 应用知识与命令面(L2/L3 是数据,L1 稳定)。

**整合**(必须保持单一心智模型的东西):一个二进制 · 一个 envelope · 一套错误码 · 一个 ref 体系;`--expect` 把"执行+验证"内联为可选原子;`batch` 是唯一的复合逃生舱(同进程共享 snapshot)。

### 3.4 L1 命令清单(31 个子命令:M1 17 + M2 14;含 flag 变体约 45 个操作)

> 阶段归属的唯一来源是 ROADMAP;此处仅按功能组呈现全集。语义近似对的裁定见 §3.6。

#### 元(meta)
```
commands                   # 全命令 + 参数 schema 的 JSON(自发现)
status                     # 版本、权限、屏幕信息
```

#### 观察(observe)
```
snapshot [--app X] [--skeleton]     # 可交互元素树 + @eN ref + snapshot_id
                                     # --view content:内容域投影——只保留 Document 子树
                                     # (网页/文档正文)与根→Document 路径,应用 chrome
                                     # (工具栏/ribbon)整枝丢弃;与 --skeleton 组合时折叠
                                     # 仅作用于 Document 内部,chrome 交互元素不参与保留
                                     # 判定(浏览器形态 skeleton 仅削 ~36% 的根因修复;
                                     # 窗口无 Document → NOT_FOUND,不静默回退)
find --role button [--name "确定"]   # 条件查元素(带 ref);与 snapshot 的分工见 §3.6
                                     # 已物化树 0 命中且可映射精确属性时走 L4 虚拟化容器(§5)
get @e3 [--property p] [--raw]      # 读元素属性;--raw 全量含 pattern 支持(合并原 inspect)
                                     # property 白名单含 text(TextPattern 文档正文专用通道,
                                     # 与 value 分工:Chromium 的 Document Value=URL 会掩盖正文)
list-windows [--native [--hwnd <n>]] / list-surfaces # 窗口/虚拟桌面枚举(list-surfaces 未实现)
                                     # list-windows 附物理层叠放事实:foreground / topmost /
                                     # minimized / visible / z_order(0=最顶) / rect / owner_title
                                     # (rect 为物理像素,PMv2 口径——与 screenshot/xy: 同坐标系)
extract @ref [--table]              # 结构化提取(表格→JSON 数组),@ref 指定容器
                                     # GridPattern 直读优先(source:"grid-pattern",带 row/col
                                     # 语义),树扁平化兜底(source:"tree-flatten");来源随结果上报
```

#### 交互(interact)
```
click @e3 [--expect P]     right-click / double-click
click xy:1420,856 --app X --physical   # 坐标寻址(视觉兜底的手;§7 守卫)
click @e3 --physical --no-activate     # 抑制激活的物理点击(见 §7.8;指针族通用)
type @e5 "text"            set-value @e5 "v"    # UIA Value pattern 优先
press ctrl+s               key-down shift / key-up shift  # 状态保持键必须成对
paste --app Excel                                 # ctrl+v 到当前选中(无元素落点;pack 路线原语)
select @e7 "选项"          toggle @e8
scroll @e9 --down 3        drag --from @e3 --to @e8       # drag/hover 无 UIA pattern,默认物理执行
hover @e3                  (其余物理兜底:同参数 + --physical)
```

#### 窗口/应用(window)
```
launch <app> [--args ...]  focus-window w-4521   resize-window / close-window
```

#### 系统(system)
```
get-clipboard / set-clipboard   # 遵守 get-*/set-* 命名规则(原草案 clipboard-get/set 纠正)
list-notifications              # 系统通知读取
```

#### 可靠性(reliability,RPA 借鉴)
```
wait --element @e3 [--timeout 5000]                # 等元素可解析(存在性)
wait --element @e3 --property value="OK" [--contains]   # 谓词等待:value/name/checked
wait --element @e3 --stable                        # settle 等待:观测样本(text>value>name
                                                     # 通道)连续两次采样不变(settle_ms,
                                                     # 默认 250ms→稳定窗 500ms)→ 内容就绪;
                                                     # 滚动/导航后的加载判定,evidence 带
                                                     # samples/changes;与 --property 互斥
wait --window "保存" | --gone @e3                        # 等窗口出现 | 等元素消失
wait --idle 8000 [--timeout 120000]              # 等系统空闲:全系统键鼠无输入 ≥N ms
                                                  # (GetLastInputInfo;"人不在"门——长批任务的
                                                  # 前置闸。与 --element/--window/--gone 互斥;
                                                  # 只观测物理输入,UIA 语义注入对它不可见)
verify --exists @e3 / --value @e3 "x" / --checked @e8 / --order ...
batch --json '<commands>'   # 单进程内顺序执行,共享 snapshot;每步结果与耗时进 steps[]
                             # (i/cmd/ok/duration_ms,失败步附 error),首错即停时
                             # 已执行步随错误 evidence 带出(部分结果不丢失);
                             # 本轮新开窗口差集上报 windows_created(title/pid/hwnd,
                             # 清理地基;生命周期归编排层,CLI 不代管);
                             # 步前弹窗闸门:前台非基线且不匹配本批 --app → steps 附 blocked_by(warning)
```

窗口诊断与批处理基线（2026-09-28）：

- `list-windows` 保持 UIA 枚举；`list-windows --native` 使用 Win32 EnumWindows，
  不依赖 UIA 初始化，返回包含隐藏、无标题窗口的原生顶层窗口列表；沿用默认模式字段，省略 z_order。
  显示端自身窗口仍排除。子 HWND 不属于这个列表，不能用它证明任意子控件已销毁。
  `--native --hwnd <n>` 改为原生直接查询指定句柄，支持子窗口：存在时返回一项，已销毁时返回空列表。
  该参数必须与 --native 一起使用，格式错误返回 PROTOCOL；查询期间身份变化且句柄仍存在时返回错误。
  `visible` 仅反映 Win32 可见样式，不等于实际屏幕可见或文档仍打开。
- 原生枚举失败返回 `INTERNAL`，不转为空列表。窗口可能在采样期间创建或销毁，列表不是桌面事务快照。
- `batch` 开始与结束均用原生枚举。初始枚举失败时直接返回错误，尚未执行任何步骤。
  `blocked_by` 的 `hwnd:` 目标按句柄比较，普通目标保持标题子串匹配。
- 最终枚举成功时，`windows_created` 为前后 HWND 差集，允许空数组；只表示新观察到的原生窗口，
  不证明任务归属，不足以授权关闭。HWND 生命周期复用及期间创建又关闭的窗口不由差集解决。
- 最终枚举失败时，不输出 `windows_created`，改为 `window_observation_error: {code,message}`。
  已完成步骤仍保留；步骤全成功时该字段在 `data` 中，已有步骤失败时在 `error.evidence` 中，
  不覆盖原步骤错误。调用方必须将窗口结果视为未知，不得重放写操作或自动清理。

### 3.5 帮助系统规范(`-h` 的设计与层级)

**双受众、三级层级**(扁平命令面保证 help 两级封顶):

```
L0  actl -h           人类全景:六组分段、一行一命令、≤80 列,30 秒看懂命令面
L1  actl <cmd> -h     人类详情:USAGE / ARGS+FLAGS / EXAMPLE / ERRORS / SEE ALSO
L2  actl commands     机器 schema:全命令+参数的 JSON(agent 自发现入口)
```

- **协议例外(ADR-002 修订条款)**:数据命令 stdout = envelope;`--help/-h/--version` 为元命令,按 POSIX 惯例输出人类文本到 stdout;`commands` 为 agent 服务,输出 envelope 包裹的 schema。由此 stdout 形成不变量:**要么 envelope JSON,要么 help/version 文本,无第三种形态**。
- **三处输出同源生成**:clap doc comment 一处维护,L0/L1/L2 三处渲染,禁止手写 help 字符串;四要素的 Example/Errors 经 after_help 表注入(derive variant 级属性不传播,build_cli() 单点注入),表与 doc comment 同为 help 单一事实来源。
- **help 文本英文**(docs 保持中文):LLM 对英文 CLI 语义理解与 token 效率最优,与命令名同语言。
- **单命令 help 四要素**(对齐 §3.1 可理解性验收):一行 USAGE / 参数表 / 一条可直接复制的 EXAMPLE / 该命令特有错误码。
- **未知子命令**:stderr 人类可读错误 + exit 2(PROTOCOL)。

### 3.6 语义边界裁定(近似命令对的分工,防止 agent 选择困难)

| 近似对 | 裁定 |
|---|---|
| `type` vs `set-value` | 键盘路径(触发输入事件)vs Value pattern 直写(快、不触发事件);各有不可替代场景,不合并;失败时错误 hint 互相指引(`NOT_ACTIONABLE` → "try set-value") |
| `wait` vs `verify` | 数学上 verify ≈ wait --timeout 0,但认知分工不同:**耐心等待** vs **立即断言**;保留两个动词,断言失败统一 `ASSERTION_FAILED` |
| `--expect P` | 执行成功后的隐式断言(默认 2s,失败 `ASSERTION_FAILED`)——"执行+验证"内联但不吞决策。P 文法 v2:`window:<substr>`(须**新窗口**出现,基线排除既有同名)| `element:<target>`(动作后目标元素**可解析**的后置条件,复用 wait 元素文法;已存在即成立——与 window: 的"新出现"刻意不对称:mmui 类应用 UIA 假成功的验证点是物化,失败 evidence 附 cause 区分"假成功"与"目标错误") |
| `snapshot` vs `find` | **全局认知**(不知道界面有什么)vs **定向查找**(知道要什么,省 token) |
| `scroll` vs `press pgdn` | 容器内滚动(UIA Scroll pattern)vs 焦点键滚动 |
| `select` vs `set-value` | 离散选项 vs 文本值(combo box 用 select) |
| `toggle` vs `click` | Toggle pattern 直调(无点击副作用、可读状态)vs 点击语义 |
| `launch` vs `focus-window` | launch 默认启动**新实例**(不做已运行检测);前置已有窗口是 focus-window 的职责 |

## 4. 输出协议

**stdout 不变量**:要么 envelope JSON,要么 `--help/--version` 文本,无第三形态。
诊断旁路两条,均不碰 stdout:① 结构化字段随 envelope 返回(`duration_ms`/`wake_ms`/
`waited_ms`/`resolved_by`/`via`/`input_effects`/batch `steps[]`——机器消费的常态数据);
② `ACTL_TRACE=1` 时引擎阶段耗时打点(`[actl-trace] locate.window 2036ms` 形态,stderr,
人类诊断按需启用;慢环境把命令总耗时分解到窗口解析/唤醒/树遍历/L4/pattern 各段)。

### 成功 envelope

```json
{
  "version": "1",
  "ok": true,
  "command": "click",
  "data": { "action": "click", "ref": "@e3", "resolved_by": "primary" },
  "snapshot_id": "s8f3k2p9",
  "duration_ms": 42
}
```

### 失败 envelope(核心设计:错误码 + 恢复建议 + evidence)

```json
{
  "version": "1",
  "ok": false,
  "command": "click",
  "error": {
    "code": "STALE_REF",
    "message": "element @e3 no longer resolves",
    "hint": "run `snapshot` — tree changed: window '设置' opened, 2 nodes added near ref path",
    "evidence": { "local_tree": "ev-12", "screenshot": null }
  },
  "exit_code": 1
}
```

### 错误码(初版枚举)

| 码 | 含义 | agent 的恢复动作 |
|---|---|---|
| `STALE_REF` | ref 失效(fallback 链已穷尽) | 重新 snapshot(错误里已附变化线索) |
| `NOT_FOUND` | 选择器无匹配 | 用 find 放宽条件 |
| `PERM_DENIED` | 权限不足(UIPI/管理员窗口) | 提示用户以管理员重开,勿重试 |
| `TIMEOUT` | wait/谓词超时 | 重估前置状态 |
| `AMBIGUOUS` | 匹配到多个元素(附候选 evidence,含可重放 @eN) | 收紧条件 / `--near` 锚点 / `role:X#N` 序数 |
| `NOT_ACTIONABLE` | 元素存在但不可操作(disabled/offscreen) | 先 scroll/focus;hint 会指向 `set-value` |
| `ASSERTION_FAILED` | `verify`/`--expect` 断言未成立 | 检查前置步骤(`--expect` 默认 2s 轮询后判定) |
| `PROTOCOL` | 协议/参数错误(不可恢复) | 修正命令参数 |
| `INTERNAL` | 内部/UIA/系统层错误(非协议非目标问题) | 重试一次;持续出现附 evidence 报 issue |
| `ABORTED` | 本调用停止代次已失效，或流程被明确暂停/停止 | 核验结果后新建调用或显式恢复流程；新写操作在开始/继续时确认，禁止自动重放 |

退出码:`0` 成功 · `1` 业务失败(可恢复) · `2` 协议/参数错误(不可恢复)。

## 5. ref 机制 + fallback 定位链(RPA 借鉴核心)

1. `snapshot` 深度优先遍历,仅对**可交互**元素编号 `@e1..@eN`,scoped 到本次 `snapshot_id`;
2. 每个 ref 同时生成定位链,执行时逐条回退:
   - primary:UIA 路径(AutomationId / ControlType + Name 精确);
     `id:` 精确匹配走 provider 侧属性条件(UIA FindAll,单次往返;恰一命中直接用,
     多命中回退全遍历算 ref 候选——候选 ref 编号仅在多命中时需要);
   - fallback-1:Name 模糊匹配 + 相对锚点(父容器定位);
   - fallback-2:role + 同级序数;
   - fallback-3(2026-09-26,L4):**虚拟化容器**——L1/L2 在已物化树中空手且目标可
     映射为精确属性(name 完整值 / AutomationId)时,询问窗口内 ItemContainer 容器
     (`FindItemByProperty`,provider 侧命中未物化条目;Explorer 文件列表实证:61 文件
     目录树内 0 命中 → 精确查询命中末位文件)。**精确匹配,无子串语义**——`name:`
     进 L4 的值必须是完整名称(如完整文件名);命中元素不在快照树内,无 `@eN` ref,
     重放走同一 name/id 查询;`Role`/`@eN` 不参与(无属性查询语义);
3. 回退结果写入返回(`resolved_by` 字段,全集:`primary` | `fuzzy-name` | `role-ordinal` |
   `id-ordinal` | `anchor` | `ref-replay` | `item-container`),全部失效才报
   `STALE_REF`(@eN)/`NOT_FOUND`(语义选择器);
   `id:<Id>#N`(2026-09-27)与 role 序数同机制——多文档页(iframe/扩展弹层)
   同名 AutomationId 的确定性寻址,不走 provider 快路径(序数需计数);
4. 重定位在同 snapshot_id 内自动进行(agent 无感),跨 snapshot 必须重取——契约简单严格,优于模糊缓存;
   **已知 hazard(2026-09-27 登记)**:ref 校验的是窗口实例(RuntimeId)而非树内容,
   弹层/模态开合会插入/移除子树——若插入点在 DFS 序上先于既有内容,后续 `@eN`
   整体平移而校验照样通过(静默错位)。纪律:**覆盖层开/关后先重拍快照再用 ref**;
   树前缀比对(根治)留待完整会话设计与 §5 既有登记合并。

## 6. token 控制

- `snapshot` 默认输出紧凑模式(role + name + ref + actionable,一行一元素);
- `--skeleton`:大 UI 折叠非交互容器(agent-desktop 实测 78–96% token 削减);
- `--view content`:内容域投影(2026-09-27,spec)——Document 子树外的 chrome 整枝
  丢弃,浏览器/编辑器形态的主削减通道;与 `--skeleton` 组合时折叠仅在 Document 内部
  生效(chrome 交互元素不再强制保留——纯 skeleton 对浏览器仅削 ~36% 的根因);
- `find`/`inspect` 是"按需深查",鼓励 agent 先窄后深,反对全树 dump。

## 7. 安全边界

- 物理指针注入(真实移动/点击鼠标)必须显式 `--physical`,默认仅 UIA 后台调用;键盘注入的授权是强制 `--app` + 护栏 + 输入占用锁(见 §2 规则 2);
- **坐标寻址(`xy:x,y`)的守卫(2026-09-27,spec,视觉兜底闭环的"手")**:无元素
  可校验的物理专用通道——①强制 `--app` + 显式 `--physical`;②**窗口边界守卫**:
  注入瞬间坐标必须落在 `--app` 窗口当前 bounds 内(PMv2 物理像素,与 screenshot
  同坐标系),窗口移动/最小化即拒(防"截图后窗口漂移点进窗外");③复用遮挡
  检查(WindowFromPoint 命中窗口须属目标)。坐标来源是调用方(screenshot+VLM/人),
  core 不做视觉(PRD §1.3 非目标不破);click 家族 + hover 适用,drag 暂不含;
- 管理员窗口/跨完整性级别受 UIPI 限制时返回 `PERM_DENIED` 而非静默失败;
- 敏感命令白名单(clipboard-set 到密码框等场景)留给上层 agent 策略,CLI 不做内容审查,但 `--dry-run` 全局可用。

## 7.5 显示信号协议 v2（独立于 stdout envelope v1）

stdout 保持原有单个 JSON envelope。显示通道是旁路、拉取式；协议 golden 见
`crates/actl-core/tests/golden/signal_v2.json`，实现说明见 [18](18-display-layer-implementation.md)。

1. **调用快照**：`%LOCALAPPDATA%/actl/calls/<call_id>.json`，单调用独占文件，
   原子替换；同时更新兼容入口 `state.json`。字段为
   `{version:2, call_id, pid, seq, command, app, step, phase, started_ms, finished_ms,
   ts_ms, action, recent_actions, error_code}`。
   `phase` 仍为 `running|done|error|stopped`；旧快照缺少新字段时按默认值读取。
2. **持续存活**：命令运行期间标准线程每秒更新 `ts_ms`，不重置 `started_ms`。
   仅运行态超过 10 秒未更新显示“状态未知”；终态不会因静默变成失联。
   `finished_ms` 固定终态耗时；状态通道不推测上层 agent 的思考或整个任务完成。
3. **动作事实**：`action` 为
   `{id,seq,kind,phase,effects,target,ts_ms,expires_ms}`；动作阶段为
   `preparing|executing|delivered|failed|cancelled|partial`。
   `effects={keyboard:bool,pointer:bool,clipboard:untouched|read|write,focus:bool}`，
   表示该动作的输入影响，阶段决定这是预期影响、正在执行还是已发生；不含内容。
   `target={window,point:[x,y]?,bounds:[l,t,r,b]?,destination:[x,y]?}` 使用虚拟桌面物理像素。
   `delivered` 不等于业务结果已经验证。
4. **短动作保留**：每调用 `recent_actions` 保留最近 32 个终结动作；消费者按
   `(call_id,seq)` 去重，2 秒过期，不补播显示进程启动前的动作。只对 delivered
   的空间动作绘制送达反馈。调用文件清理保留 60 秒内最多 128 个终态，运行态由心跳续期。
5. **停止**：使用持久代次区分旧调用与新调用。输入锁等待后、动作准备间隔及执行前
   检查代次；取消返回 `ABORTED`。UIA 修改和剪贴板写入也在调用前检查。停止请求与
   本次调用确认停止分别呈现，不能承诺撤销已提交的 COM/输入。急停由菜单、详情急停按钮
   或 Ctrl+Alt+F12 触发；新任务开始时确认当前代次，旧调用仍失效；损坏状态使用显式修复入口。
   普通关闭显示条撤销授权并使显示租约失效，不写急停；重启不解除已有急停，也不自动续跑任务。
6. **预告**：显示条可见时通过 `display-live` 时间戳声明在线，执行端对已解析动作
   提供约 300ms 可取消准备期，结束后重新校验目标或物理焦点。显示端不存在时不增加
   该等待。预告不构成授权、确认或跨调用任务计划；物理输入授权语义不变。
7. **显示**：`actl-signal` 为 280×34 DIP 单行原生条；统一线性图标，组合显示设备
   影响；详情按需打开。独立非激活穿透窗口负责准备圈、点击波纹和控件边界高亮。
   状态条不做持续动画；减少动态效果时空间反馈为短暂静态标记。
8. **自排除**：显示窗口不进入一般目标枚举，显式 HWND 自动化也拒绝。屏幕截图通过
   同步隐藏确认和 DWM flush 排除本工具显示；不能确认时拒绝捕获，不静默污染截图。

附带修正：`key-up` 将 release 方向传递到输入执行体；带修饰键的物理 click 在
stdout `input_effects.keyboard` 如实返回 `injected`（原先错误地返回 `none`）。

## 7.6 操作历史与任务归档 v1

`ACTL_TASK_ID` 可选，值为 1–80 位 ASCII 字母/数字/`-`/`_`，执行前验证，无效值返回
PROTOCOL。它只关联记录，不提供任务调度或暂停恢复。

每次调用写 `%LOCALAPPDATA%/actl/history/<call_id>.jsonl`。事件字段为
`version:1, call_id, task_id(null 可用), seq, ts_ms, event, data`。
event 包括 `call_started`（command/version）、`step`（command/index/total）、
`action`（action_id/kind/phase/effects）、`call_finished`（phase/error_code/duration_ms）。
动作阶段沿用显示协议。不记录参数、标题、输入正文、剪贴板、截图或心跳。
参数解析失败不建记录；history 查询自身不写历史。日志失败在 stderr 输出 INTERNAL
诊断，不改变原命令结果；缺少终态不能推断为成功或失败。

```text
actl history [--task <id>] [--limit <1..100>]   # 默认 20，最近写入的调用优先
actl task-archive --file <report.json>          # UTF-8 JSON，最大 64 KiB
```

history 的 data 为 `{calls:[{complete,integrity,events:[...]}],incomplete_files,logging_health}`。
complete 要求 JSON 可解析、身份和版本一致、seq 连续且调用首尾边界有效；完整性在步骤过滤前检查。
integrity 区分 complete、unfinished、malformed_json、empty、missing_start、invalid_identity、
identity_or_version_mismatch、sequence_gap_or_duplicate、invalid_boundary。
unfinished 不证明进程仍在运行或已经崩溃。incomplete_files 统计本次扫描到的空/损坏文件及
返回的无终态调用；过期记录可能尚未清理。非法 ID/limit 返回 PROTOCOL，读取失败返回 INTERNAL。

logging_health 包含 status、complete_logging_guaranteed（始终 false）、failure_count、truncated、failures。
status 为 no_recorded_failure/degraded/unknown；未发现失败记录不保证没有丢日志。
日志打开/追加与反馈状态写入失败尽力保存独立 log-health 标记；读取历史失败时健康结果附于错误 evidence。
按 task 过滤，最多返回最近 100 条，默认保留 7 天/8 MiB；目录不可写时仅能报告 stderr。
各标记包含 call_id/task_id/stage/dropped_events/error_kind/native_code/ts_ms/version/status，
不保存错误正文或业务参数。打开失败的 dropped_events=0 表示尚未计数，不表示没有丢失。
完整性与健康记录均不提供防篡改或事务性保证，详见 [41](41-logging-metadata.md)。

任务报告示例：

```json
{
  "task_id": "report-01",
  "outcome": "partial",
  "summary": "已完成 8 项，2 项待处理",
  "verification": "已回读前 8 项；其余结果未确认",
  "call_ids": ["call-1"],
  "feedback": [{"source":"agent","text":"页面切换后需重新定位","verified":false}],
  "evidence": ["results/report.csv"]
}
```

outcome 为 completed/partial/stopped/failed；feedback.source 为 agent/user；
evidence 可省略，其余字段必填，未知字段拒绝。summary/verification 必须非空。
task_id/call_ids 遵循同一 ID 规则，引用不要求仍存在。evidence 只存引用，不打开或复制。
verified 是提交者声明；报告内容由提交者简化，不自动脱敏。

归档成功 data 为 `{archived:true,path:<绝对路径>,source:"caller"}`；文档为
`{version:1,archived_ms,source:"caller",report:<规范化报告>}`，每次新增修订，不覆盖旧报告。
归档成功只确认保存，不核验结论。格式/大小错误为 PROTOCOL，文件与写入失败为 INTERNAL。

默认保留：history 7 天/64 MiB，tasks 30 天/32 MiB；单调用日志最大 1 MiB。
写入前机会式清理，保护最近一小时文件及具有新鲜运行心跳的调用；并发/在途增长可暂时超限，
无法释放容量时报告失败。没有后台清理器，不清理 desktop 恢复记录。
日志不保证防篡改或断电完整性。完整设计见 [19](19-history-and-user-experience.md)。

## 7.7 YAML 顺序流程 v1

简单条件扩展：步骤可声明互斥的 `when`/`unless` 即时只读断言和可选 `otherwise` 动作，未选中且无备用路线时跳过。两条路线均预检；条件步骤不声明 outputs。成功为真，ASSERTION_FAILED 为假；严格 UIA 存在性断言仅在已确认窗口/容器、完整零命中时将带明确查询证据的 NOT_FOUND 判假，其他观察错误以 condition_observation_failed 停止。branch=run/otherwise/skip 在派发前持久化，未派发恢复重判，不确定步骤保持原路线先核验。完整字段、执行顺序及模板见 [63](63-simple-workflow-conditions.md)。

新增只读命令 `verify-window --app <标题片段|hwnd:句柄> --state <exists|absent|foreground|minimized|visible>`：成功 data 为 `{action:"verify-window",state,matched:true,window}`，window 为 `{hwnd,pid,title,foreground,minimized,visible}` 或 null；已知不成立返回 ASSERTION_FAILED，evidence 保留对应 matched=false 数据。多匹配返回 AMBIGUOUS，查询错误原样传播，无匹配仅 absent 成立。窗口存在与前台判断不证明资源归属、无遮挡或进程退出。

新增 flow-validate/run/status/pause/resume，完整字段、状态与边界以
[20](20-yaml-workflow-protocol.md) 为本节规范性扩展。
独立流程层复用原子命令，run/resume 按运行 ID 关联操作历史。
写操作要求后置 verify，自动重试仅限只读；暂停在命令边界确认，紧急停止保持粘性。
未确认动作恢复时先验证，不能直接重放；显式 --retry-step 才允许重复写入。
显示信号可选 workflow 字段为 `{run_id,title,status,reason?,step_id?,error_code?,actions?}`，
新增字段均可缺省，旧信号继续可读。actions 是状态建议，不是授权：pause/stop/resume/recheck/
allow_then_resume；执行时仍重新检查停止标志、运行锁及步骤状态。
流程中的 wait 在每次轮询前检查暂停和停止，当前 COM 调用返回前不能强行中断。
因暂停中断的只读等待保留 pending，不标成超时；已写入后等待验证的步骤仍需恢复前核对。
`actl-signal --controls` 打开可聚焦的原生控制窗口，提供标准按钮及键盘导航；关闭窗口不停止流程。
该窗口不提供输入动作显示租约，也不解除原有显示就绪约束；停止标志只能通过明确操作解除。
JSON envelope 版本保持 1；CLI 使用结构化错误与 evidence 返回运行状态。

## 8. 里程碑

各阶段范围、工期、验收的**唯一来源**:[ROADMAP](ROADMAP.md)。本文不重复维护版本计划。

## 9. 明确不做

非目标清单**唯一来源**:[PRD §1.3](PRD.md);背景论证见 [02 §4](02-gap-analysis.md)。
## 显示就绪与桌面整理增补（2026-09-27）

stdout envelope 继续为 v1，调用记录继续为 v2；显示租约使用独立 protocol=3。
`display.json` 字段：protocol、build（包版本）、pid、epoch（启动实例）、state、ts_ms。
state 为 ready/hidden/capture/closed；仅版本一致、心跳不超过 1500ms、对应显示窗口存活且中心点未被遮挡的 ready 租约
允许开始副作用动作。CLI 从自身目录启动 actl-signal.exe，最长等待 4s；隐藏、缺失或不兼容返回 NOT_ACTIONABLE。
并发启动由会话内单实例互斥体合并。不会清除停止标志，不提供静默绕过。
Windows 锁屏可能保留可见窗口属性，因此另用屏幕命中检查拒绝被锁屏覆盖的状态条；这不证明整条无遮挡或人已阅读。

准备阶段记录存活 6s。显示端优先展示最早等待的 preparing 动作，Direct2D 画面提交及 DWM flush 成功后，
写入 `ack-<call_id>.json`：epoch、call_id、action_id、ts_ms。执行端最长等 3s，身份与时间匹配后再保留约 300ms
可取消预告，然后重验目标并执行。用户悬停不会覆盖待执行的预告文案。确认表示程序提交了画面，不代表人已阅读。
输入分段边界检查停止及显示存活；短暂 capture 隐藏不误判失联，新动作仍等待重新可见。
观察命令（snapshot/find/get/status/list-windows/screenshot 等）不强制启动显示端。

新增命令：

| 命令 | 参数 | data |
|---|---|---|
| display-status | 无 | ready, lease（可空）, stop_requested |
| display-start | 无 | ready, lease, stop_requested |
| minimize-window / restore-window | 标题、--pid、--hwnd 三选一 | hwnd, window, minimized, input_effects |
| desktop-prepare | --all 或重复 --hwnd | session, results[] |
| desktop-restore | --session ID | session, results[] |

桌面准备只最小化普通可见、非置顶、非工具、无 owner、有最小化能力的窗口；排除系统桌面与显示端。
更改前在 `%LOCALAPPDATA%/actl/desktop/<session>.json` 持久化身份（hwnd/pid/UIA RuntimeId/class）与
WINDOWPLACEMENT，操作后保存确认状态。失败 evidence 带 session 和已完成结果；恢复只处理仍处于
记录状态的同一实例，改变/关闭/未确认的窗口报告 skipped，恢复成功记录不会重复执行。
恢复窗口状态和位置，不承诺精确 Z 序或前台焦点，也不推断中间发生但最终回到同一状态的用户操作。
最小化并非关闭或保存文档。停止标志仍阻止桌面整理及恢复，用户需显式恢复后再调用。

新命令错误：PROTOCOL 参数或记录不合法；NOT_FOUND 窗口/会话不存在；AMBIGUOUS 目标不唯一；
NOT_ACTIONABLE 显示或窗口未就绪；ABORTED 用户停止；INTERNAL 持久化或原生调用错误。

### YAML 断言与归档补充契约（2026-09）

- flow-v1 Schema：`schemas/flow-v1.schema.json`；字段诊断含 `error.evidence.field/step_id`。
- verify 新增 `--property name|value|text`（依赖 --value），分别严格读 Name/ValuePattern/TextPattern；
  省略保持 Value→Text 兼容策略。成功含 channel；失败含 target/property/channel/expected/actual，
  比较失败另含 comparison。wait 新增 text 谓词，超时补上述证据及 last_actual。
- 步骤 evidence 默认 metadata，可选 full；StepRecord 可含 error_evidence，旧运行记录可读取。
  metadata 隐去期望值、实际值和目标；full 超过 8 KiB 时只记 omitted。
- history 新增 `--step` 和 `--reports`，均要求 --task；先按步骤过滤再应用调用条数限制，
  返回匹配步骤事件及 call_started/call_finished。流程上下文事件含 run_id/step_id；
  data.action_id 在 call_id 内唯一。
- task-archive 新增 artifacts、feedback.kind/evidence_ids；报告分类和引用检查见 docs/19 §6。
  history --reports 返回任务级报告及查询时的证据可用性，不代替业务核验。

完整 YAML 与写后验证示例见 docs/20 §6 和 fixtures/flows/calculator-verified.yml。


### 控件身份与输入焦点补充（2026-09-28）

- `snapshot`/`find` 在本地快照记录中按同次完整 DFS 的 ref 顺序保存控件 RuntimeId、PID、
  role、name、AutomationId；投影不重编号，输出 envelope 的元素形状保持不变。快照落盘失败返回 INTERNAL。
- `click/type/set-value/get --snapshot <id>`：窗口实例校验后，在实际 locate 命中的 ref 上
  比对上述身份。任一不符或缺少有效 RuntimeId/PID，返回 `STALE_REF`，不自动按名称写入另一个控件。
  旧快照没有元素身份记录时，ref 重放同样拒绝。无 `--snapshot` 的 ref 仍是兼容的序号重放。
  语义选择器每次重新定位；`--snapshot` 与语义选择器组合仅校验窗口，不绑定某个历史控件。
- RuntimeId 是当前实例身份，不是跨启动永久 ID。UIA 树采样和注入不是原子事务，无法消除最后检查后
  的外部竞态，也不能检测 provider 将同一身份和相同元数据复用于不同业务对象。
- `type`：SetFocus 后同时核对 UIA 具体控件 RuntimeId/PID 与现有原生前台、焦点锚点。
  注入边界继续检查两种焦点；provider 无法证明焦点时返回 `NOT_ACTIONABLE`，不降级为仅窗口匹配。
- `type <target> <text> --app X [--paste] [--snapshot ID] [--verify-value <expected>]`：
  可选在原控件上最多等待 2000ms，ValuePattern 优先、不可用时 TextPattern，精确比较完整字符串，
  包括空串和换行，不归一化、不重复输入。成功新增 `focus_verified:true`，`value_verified:true`；
  未要求读回时后者为 null。读回不符/不可读返回 `ASSERTION_FAILED`；实例改变返回 `STALE_REF`。
  错误 evidence 含 delivered/value_verified；输入可能已经生效，调用方先核实，不能直接重放。
- `press <keys> --app X [--target <selector>] [--snapshot ID]`：`--snapshot` 要求 `--target`。
  指定目标时只断言其已拥有焦点，不切换窗口、不 SetFocus、不猜测首个 Edit；不符返回
  `NOT_ACTIONABLE`，尚未发键。目标歧义遵循现有 `AMBIGUOUS`，ref 可绑定快照身份。
  成功新增 `focus_verified`，有目标为 true、无目标为 false。原 `--expect/--timeout` 继续适用。
  无目标的 press 保持现有窗口级行为。
- 键盘沿用 §1 的显式 `--app` 授权；鼠标物理路径仍要求 `--physical`。


### 桌面交接与窗口就绪（2026-09-28，显示协议 v4）

工作流资源初始化契约见 docs/20 的“声明式资源初始化”：resources 存在时展开输入默认值中的受限运行上下文，完整预检后由 flow-run 创建独占资源目录，flow-resume 不重新初始化。flow-validate 在原 envelope 的 data 中增加 resources 清单（仅资源模板返回），使用 validation-preview 上下文且不创建文件；普通模板输出不变。无新命令或任意脚本入口。

任务交接策略（显示协议 v8）：flow YAML 可增加 `handoff: {approval: once_per_run}`。
调用方若已获得用户明确的代操作授权，可通过 computer use 核对任务后实际点击可见的开始/继续，具体边界见 docs/32；这是操作者约定，不改变以下协议、授权生命周期或结果核验，也不提供程序化跳过确认参数。
省略时保持 `expiring`（空闲 60 秒、总长 10 分钟）。once_per_run 在同任务内不因这两种超时失效，
首次仍需用户确认，人工接管、暂停撤销、停止代次变化、显示端重启和监听故障仍撤销授权。
策略变化必须重新确认，不能升级已有授权；窗口、焦点与物理输入参数要求不变。
普通脚本通过 `ACTL_TASK_CONFIG` 指定 YAML 文件，格式为 `version: 1` 加同样的 `handoff`；
必须同时设置每次运行唯一的 `ACTL_TASK_ID`，不同运行不得复用 ID。配置严格拒绝未知字段、重复键与未知策略，
不存在 authorized/跳过确认字段。授权仅在显示端内存中，文件不保存授权。flow 以保存的 plan 策略为准。

完整规则见 [32-desktop-handoff.md](32-desktop-handoff.md)。动作反馈门禁在展示动作前要求
人工点击任务交接窗口，25 秒未响应返回 NOT_ACTIONABLE；ACTL_TASK_ID / flow ID 范围内
可短期复用，输入活动、60 秒空闲、10 分钟总长、急停及显示端重启撤销。无任务 ID 只限单调用。
物理鼠标仍须显式 --physical。只读观察不再偷偷唤醒窗口。

错误 evidence.stage=handoff，reason 为 handoff_pending / user_deferred /
human_takeover_or_grant_expired / companion_missing / companion_unresponsive /
request_rejected / missing_session / handoff_panel_unavailable / input_monitor_unavailable；retry_input=false。
flow 因交接中断保存 paused，写步骤 uncertain，恢复先验证而不自动重放。
分段注入失败保留 delivered_before_error（已确认的前缀）及 failed_segment_size。

focus-window 成功须验证原生前台根 HWND；错误按 window_prepare 阶段报告具体 reason，
前台激活超时附 `foreground_before`、`activation_requested`、`activation_returned`（未请求时为 null）、`caller_pid`、`waited_ms`。
`activation_returned` 是 SetForegroundWindow 的原始布尔结果，不等于最终前台核验结果；不读取该 API 未定义的 LastError。
显示端只在任务授权通过后向同目录 actl.exe 的当前调用 PID 请求 AllowSetForegroundWindow；每调用一次，失败不伪装成拒绝任务授权或成功激活。history 的 foreground_transfer 记录 request_call/caller_pid/accepted/failure，可与调用关联。Windows 前台资格可随输入失效，不能据 once_per_run 推定永久可激活。
显示协议 v9：动作 guard 在最外层退出时（包括失败路径）向核验过的显示端 PID 归还前台资格，history 记 foreground_return。进程内下一动作通过 FOREGROUND_MESSAGE 重新检查当前任务授权并领取；嵌套动作不提前归还。旧显示端必须正常替换，不能混用。
CHECK_MESSAGE 查询超时允许在复核停止代次后对同一显示窗口重查一次，每次上限 500ms，记 handoff_query_retry；不重试开始请求、资格请求或任何桌面动作。最终失败 evidence 附 query_message/query_timeout_ms/query_return。
前台不匹配属于 NOT_ACTIONABLE，不代表权限不足。
launch 新增 --wait-window <标题子串> 与 --timeout <1..60000 毫秒>（默认 5000，要求前者），
选项置于程序之前。默认结果新增 window_ready:false；等待成功新增 window 对象
{hwnd,pid,title,foreground:true,matched_by:"title"}，window_ready:true。
匹配不证明窗口来自新 PID；失败 evidence 保留 spawned_pid/process_started/relaunch:false。
不自动重启、重复注入、清急停或跨权限强行激活。

display-status 新增 reason/hwnd/responsive/popup，ready 同时校验真实可见窗口、进程身份及租约。
display-start 可对兼容显示端无激活恢复及位置校正；版本不符仍拒绝并保留停止状态。
popup 是最后一次菜单/详情诊断，带 ts_ms/pid，不代表当前存在。显示 v4 租约 golden 替代 v3。

wait --window 的标题出现轮询改用原生可见顶层窗口枚举，避免 UIA provider 阻塞协作暂停；不激活窗口，失败不伪装成空列表。

**授权余韵宽限与快捷键（2026-09-28 修订）**：授权动作自身的余韵（点击/按键的抬起、手部后摇）
不再视为外部接管——`Consent` 自授予起 750ms（`GRACE_MS`）内允许输入序列号变化并重设基线，
窗外恢复严格相等判定（`human_takeover_or_grant_expired` 语义不变；此前实现里授权点击自己的
mouse-up 会让授权在第一次 check 即失效，属实现缺陷）。授权通道增加全局快捷键 **Ctrl+Alt+Y**
（与"开始/继续"按钮等价，免鼠标行程；Ctrl+Alt+F12 仍保留为紧急停止），按钮与提示文案同步标注，
快捷键注册被占用时静默退化为仅按钮。


### 暂停与继续控制（显示协议 v5）

当前显示协议为 **v7**：普通关闭（WM_CLOSE / 菜单“关闭显示条”）与停止分离。
关闭前撤销交接授权并发布 closed 租约，执行端在下一护栏检查拒绝后续桌面动作；已经送达的动作不能撤销。
详情窗口单独关闭仍只关闭详情。正常替换仅在输入空闲时允许，不清已有急停。

`stop-requested` 使用严格 JSON v2：`version/generation/requested_ms/source/pid/display_epoch`，`stop-generation` 独立持久保存。
每次急停在文件锁内生成新代次；CLI 入口绑定代次，输入、等待、batch 和 flow 边界核对。旧代次不会因确认开始或修复而复活。
新纯观察和 preview 可在历史停止期间运行；再次急停仍取消正在进行的预览/等待。停止不能中断已经送达的原生调用。
新桌面写操作仍须正常交接：请求携带 generation，用户开始时只确认该代次的停止并授予该任务权限，不恢复旧调用。
停止期间新 flow 可做只读检查；写步骤仍需交接。旧 flow 仅在显式恢复后执行原有 resume_check/uncertain 核验。
详情窗口通过 WM_APP+50 向主显示端请求绑定流程的确认；请求一次性消费且核对代次、时效和流程状态，排队子进程继承代次。
旧 v1、时间戳、空文件、损坏记录不再作兼容解析，返回 execution_state_invalid；不靠升级自动清除。
`execution-status` 无参数，输出 ready/stopped/repair_required、generation/stop（有效时）和固定 new_task_requires_consent=true、old_calls_resume=false。
`execution-recover` 无参数，调用即明确请求修复：归档原始普通文件到 stop-history，生成新代次并撤销停止，返回 recovered/generation/archived/permission_granted=false/resumed=false。
修复不启动任务或授予权限；拒绝目录/符号链接状态路径及已运行的旧协议显示端。状态不可读/不支持或修复失败返回 NOT_ACTIONABLE 并保留诊断。
两条管理命令不能成为持久 flow 步骤。WM_APP+49 解除握手及直接 clear_stop 接口删除；v6 显示端不能与新 CLI 混用。
实现与验收见 [47](47-execution-recovery.md)。

显示租约升级 v5，避免旧版方框按钮仍执行急停。SessionState 新增可选 handoff_reason，
旧记录缺省 None；人工交接事件同时刷新状态和历史，allowed 清除该字段。
右侧按钮按当前状态显示暂停、检查并继续或详情：按下时绑定调用/流程，释放时不改绑其他任务。
暂停只撤销该调用授权（返回 handoff reason=user_paused）并请求 flow-pause，不写 stop-requested；
拖动状态条也只请求暂停。紧急停止由急停按钮、菜单或 Ctrl+Alt+F12 显式触发，正常关闭不触发。

只有 paused / needs_review 流程提供继续；点击前重读存档状态，停止标记存在时拒绝，
同一次点击授予这个 flow ID 的短期桌面授权并执行 flow-resume，不自动清急停。
已完成/已停止/缺失/发生变化的任务不续跑。单 CLI 和普通脚本没有续跑入口，显示详情并说明
需由调用方核对结果，按钮不宣称已恢复。

第一次交接在任何动作开始前被拒绝时，handoff 错误证据增加 action_started:false，flow 保存
步骤为 pending，显式继续后可执行；其他情况保持 uncertain，恢复先验证，不能确认则 needs_review，
不自动重放。状态文案区分后台处理（明确的只读观察）、桌面操作、等待交接、暂停让行与条件阻塞；
UIA 写操作也按桌面操作显示，不能仅因无物理键鼠就声称可并发操作。

继续入口要求暂停状态至少稳定 800ms，避免移鼠标去暂停时触发让行、紧接着的点击却反向恢复；时钟回退时不放行。

### 后台物理点击 `--no-activate`（2026-09-28）

指针族物理动作（click --physical / right-click / double-click / middle-click / triple-click / hover，
含 xy: 坐标寻址）可加 `--no-activate`：注入前给目标窗口根 HWND 临时置 `WS_EX_NOACTIVATE`，
点击按坐标正常落地（命中/边界守卫照走），但**不激活、不抢前台、不改 Z 序**；结束后只清
自己置的那一位（期间其他 exstyle 位的合法变更不被吞掉），并把光标送回原位（drag 除外——落点有语义）。
`input_effects.activation` 上报 `suppressed|normal`。

适用边界与安全设计：

- **键盘不适用（fail-closed）**：窗口无焦点时 type/press 会打进用户当前前台——press/type 无此旗标，
  传入即参数错误。后台化只覆盖指针；占键盘的操作仍需前台 + 信号条预告。
- **置位失败 → PERM_DENIED**（UIPI 拒绝提权窗口）：承诺不激活就不静默破坏承诺；提示去掉旗标重跑。
- **崩溃残留兜底（WAL journal）**：置位前先落盘 `%LOCALAPPDATA%\actl\no-activate-journal.json`
  （hwnd/pid/进程启动时刻）；任何物理路径入口（输入执行锁获取处）扫描 journal，**pid 已死或启动
  时刻不符（pid 复用）→ 清位 + 删条目**（清位前复核窗口仍归属该 pid，防句柄复用误伤）；pid 活着
  （他处会话可能动作段内）→ 不动。恢复方向永远是"清位"——让窗口回到正常行为，是安全方向。
- 与 `wait --idle` 组合即"人不在时干活"的两件套（docs/09 §6.2）；扫描/采集类长任务的会话切换
  不再打断用户当前窗口。

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
独立测试文件、Office 种子文档及 profile 的准备仍由验收工具负责；引擎不执行任意 PS。

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

文件/行集合/关闭断言对已知不匹配在 5 秒观察窗口内复查，未知错误不转为空或成功。文件内容上限 4 MiB、单个 XML part 上限 4 MiB、expected 参数上限 16 KiB。OOXML 由 zip 解压指定 XML part，再经 Windows.Data.Xml.Dom 只读解析；进程身份通过 Win32 句柄读取，完整命令行通过 WMI 读取。运行时不启动 PowerShell，不使用 Office COM。XML 禁用 DTD/外部实体，最大深度 128；拒绝重复/缺失单元格地址及越界 shared string 索引。进程退出检查不读取命令行。WMI 枚举等待上限 5 秒，但连接/COM 调用没有独立可中断的硬超时，因此整个断言仍不保证 5 秒硬实时；原检查器的 20 秒子进程超时已随脚本移除。依赖与 unsafe 登记见 TECH_STACK §3。

### URL 查询参数断言（2026-09-29）

`verify --value TARGET EXPECTED --url-query KEY [--app APP]` 从明确的 Value 通道读取 URL，比较指定查询参数的 UTF-8 百分号解码值。忽略 fragment，保留字面 `+`；参数缺失/值不同为 ASSERTION_FAILED，重复参数、坏转义或非 UTF-8 为 NOT_ACTIONABLE。空 KEY 为 PROTOCOL；与 --property、--contains、--normalize-lines 互斥。结果包含 verified=true、channel=value、comparison=url_query_exact。供夸克搜索核验使用，可作为流程的 before/verify/resume_check；不会发出网络请求。

### 紧邻检查复用（2026-09-29）

恢复执行时，只有仍为 pending 的步骤，且解析后的 resume_check 与 before 完全相同时，成功的恢复检查才直接满足此次前置检查。两者之间仍检查暂停/停止；不跨步骤、跨暂停或跨执行器缓存。uncertain/running/verifying 步骤仍先核验结果，显式重试前重新执行 before。不同参数或不同命令不合并。
