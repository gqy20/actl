# 06 · 设计蓝图:Windows AI-native 桌面操作 CLI

状态:现行命令语义与设计 · 实现记录见 CHANGELOG，待办见 ROADMAP · 前置:[01 调研](01-research-landscape.md) · [04 选型](04-tech-decision.md) · [05 估算](05-implementation-estimate.md)

> **维护契约**:本文只记**现行态**——每份契约在所属章节保有一处权威位置,修改直接编辑该章节。
> 变更叙事(日期、实证过程、事故编号、提交哈希)一律进 CHANGELOG 与提交信息,不在此重复;
> 禁止在文末追加带日期的"增补节"(2026-10-11 整理裁定:彼时尾部增补曾累积至全文 37%,
> 与 CHANGELOG/git 三层冗余)。

## 1. 定位

实验说明：`prototypes/browser` 独立验证 CDP 会话、CSS/XPath、iframe 和写后核验。
它没有新增 actl 命令、目标语法或 YAML 字段，报告文件不属于本协议；范围与验收见 [34](34-browser-connection-prototype.md)。

一句话:**给 AI agent 用的 Windows 桌面原子操作 CLI——accessibility(UIA)驱动、原子命令可独立调用、输出机器友好。** 类比:Playwright 之于浏览器,本工具之于 Windows 桌面。

平台范围：Windows；Linux/macOS 执行后端暂不考虑实现。MCP 计划作为独立适配服务，
在被控设备本地启动，供其他设备调用本设备；尚未实现，规划见 [ROADMAP](ROADMAP.md)。

- 脑(agent 的工具循环)与手(本 CLI)彻底分离;
- 协议蓝本:agent-desktop(Apache-2.0,直接沿用其 envelope/ref/错误码契约);
- UIA 实现参考:terminator(MIT,Rust);
- 可靠性工程借鉴:RPA 20 年经验(见 [03](03-rpa-vs-quicker.md))。

**命名**:bin = `actl`(agent control)。crate 前缀 `actl-*`。

## 2. 架构原则

1. **无状态原子命令**:每次调用独立进程,无隐藏会话;状态只存在于两个地方——屏幕本身,和 agent 的上下文。
2. **UIA 优先,物理兜底**:默认走 UIA pattern 调用(后台执行、不抢物理鼠标键盘)。物理模拟(SendInput)按输入通道分治授权:**键盘**(press/type 本体即注入)的授权 = 强制 `--app` 绑定 + verify-then-inject 护栏 + 进程间输入占用锁,不另设 `--physical`;**指针**(真实移动/点击鼠标,独占用户的物理指针)必须显式 `--physical`(`click --physical`,后续 right-click/drag/hover 同)。
3. **为自愈设计**:错误返回携带恢复线索(变化线索、evidence 引用),目标是最小化 agent 下一轮恢复成本。求韧不求稳。
4. **token 是一等约束**:默认返回紧凑语义树;大 UI 用 skeleton 投影;不做"全树 dump"这种默认行为。

## 3. 命令面设计

### 3.1 设计原则(agent 认知友好)

- **扁平动词式**:不嵌套分组(对比 `actl interact click`),动词即语义——省 token、零结构记忆负担;分组只用于 help 分段标题。
- **一个命令 = 一个认知动作**:命令语义对齐 agent 循环的四步心智模型——**感知**(`snapshot`/`find`)→ 决策(LLM)→ **执行**(`click`/`type`)→ **确认**(`--expect`/`verify`)。禁止复合语义命令(如 click-and-type):失败无法归因,中间决策被吞掉。
- **命名四规则**:① `动词` 或 `动词-宾语`,小写连字符;② 读操作前缀固定:`get-*` / `find` / `list-*` / `inspect` / `extract`;③ 位置参数只放目标(@ref / 语义选择器 / `xy:x,y` 坐标——坐标为物理专用寻址,见 §7;`focused:` 焦点元素,见下条),其余全 flag;`--app` 窗口锚定接受标题子串或 `hwnd:<n>`(HWND 是窗口身份,SPA 整页跳转会改标题),谓词统一 `--expect`/`--predicate`、超时统一 `--timeout`;④ 全局 flag 极小:`--dry-run` `--timeout` `--verbose` + 体积控制(`--skeleton` `--max-depth`)。
- **焦点元素寻址(`focused:`)**:`focused:`(裸)或 `focused:<Type>`(角色校验)解析到 OS 键盘焦点元素——**树对 name:/id:/role: 不可见但焦点可达的应用**(Electron 懒树)的唯一元素寻址通道。语义与守卫:①焦点唯一,无序数形式(`focused:Edit#2` PROTOCOL 拒绝);②属主 fail-closed——给了 `--app` 时物理焦点根(GUITHREADINFO,非 UIA focused 视图——WinUI 实测会谎报)HWND 必须即 app 窗口,否则 NOT_FOUND 并附 focused_window/app_window 证据;③`focused:<Type>` 角色不符 → NOT_FOUND 附 expected/actual;④resolved_by="focused"。**与 type 的组合即 burst 打字**:`type focused: <词> --app <win>` 走既有逐字键盘通道(批间前台复核),单次调用完成全词注入。元素型命令(type/verify/get/set-value/find --target 等)统一经 locate 获得该通道。
- **裸 `e<N>` 引用别名(2026-10-12)**:与 `@e<N>` 同义同约束(需 --app、ref 从 e1 起、同 `--snapshot` 新鲜度核验)。动机:PowerShell 把裸 `@eN` 当 splat 语法吞掉(agent 实测两次踩坑),`e<N>` 是免引号等价形态;该形态此前为未知目标报错,无回归面。
- **自发现**:`actl commands` 输出全部命令 + 参数 schema 的 JSON(等价于 MCP `tools/list` 的 CLI 形态);人类 help 文本与机器 schema 同源生成,不漂移。agent 冷启动一条命令拿全能力清单,不依赖读文档。
- **可理解性验收标准**:任一命令的 help + envelope,不看文档即可推断四件事——输入什么 / 副作用是什么 / 返回什么 / 失败码指示的下一步动作。

### 3.2 分层模型:哪些统一,哪些适配

#### 容器局部快照

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

#### 独立目标预览

`preview <target> --app <window> [--snapshot <id>] [--message <text>] [--duration <ms>]`。
只展示已定位 UIA 元素的边框和调用方提供的动作说明，不执行目标动作、输入、剪贴板修改或授权。
框渲染的是 **UIA provider 报告的操作矩形**（`--physical` 点击落点为其中心，pointer 物理链同源），
与视觉边框的偏差是**物理点击风险信号**而非渲染缺陷；核对用 `screenshot --rect` 区域模式
（窗口模式抓目标窗口表面，拍不到独立置顶的预览框）。
默认说明为预览提示；说明须为 1–160 字符且无控制字符，duration 为 500–30000ms（默认 3000）。
坐标目标拒绝；ref 必须绑定 snapshot，语义目标每次定位。支持组合选择器，歧义沿用 AMBIGUOUS，不自动选择候选。
临时窗口不激活、不修改 flow 的显示会话；可关闭或超时结束。不允许作为持久化 flow 的 run/check，不构成业务结果验证。

成功 data 为 `{previewed:true,executed:false,exit_reason:"expired"|"dismissed",target,window,bounds,resolved_by,message}`；
bounds 是屏幕物理像素 `[left,top,right,bottom]`。成功表示显示帧已提交，不能证明用户已经看到或认可。
目标离屏、窗口最小化、无有效边界或中心点遮挡返回 NOT_ACTIONABLE；查询失败保留原错误。
预览期间重查同一元素的身份和边界；变化返回 STALE_REF（reason=preview_target_changed），停止返回 ABORTED，均销毁本次窗口。
中心点命中检查不证明控件全区域无遮挡。同步 COM 调用无硬超时，窗口变化与下一次采样之间存在竞态。
命中检查双判据（2026-10-14 托盘轮）：根 HWND 相等（空 Win32 标题窗——任务栏标题空串、UIA Name 才是「任务栏」，标题比对结构性失配，HWND 是唯一可靠通道）或根标题相等（WinUI 三件套跨 HWND 同窗）；两者都不满足才判 occluded。元素路径的空标题目标不再报 no native handle——句柄在则照做。
显示渲染的 unsafe 限定在 actl-uia，复用现有依赖。设计与验收见 [46](46-target-preview.md)。

候选纠正：`preview @e3 --candidate @e7 [--candidate @e9] --app <window> --snapshot <id> --duration 30000`。
仅在提供 candidate 时启用，含位置参数共 2–20 个不同的 @eN，必须来自同一快照；坐标、语义选择器、重复候选、缺失 snapshot 或超限均 PROTOCOL。候选由调用方从 snapshot/find 结果筛出，不自动截断或猜测歧义结果。
用户可切换上一个/下一个、选择此目标或关闭。所有候选先绑定身份和边界，当前候选持续核验，切回候选时复验；选择时重新解析 ref 并比较原身份/边界，失效拒绝。期间不注入输入，不滚动/激活目标、不授予执行权限；后续动作仍用返回的 ref 和 snapshot 重新核验。
候选对话框初次显示不抢焦点，用户点击时可聚焦对话框本身；底层目标应用不因此获得主动激活。单目标预览继续保持不激活。
选择模式成功 data 为 `{previewed:true,executed:false,exit_reason:"selected"|"expired"|"dismissed",snapshot_id,candidate_count,selected,selection,requires_revalidation:true}`。
明确选择才有 `selected:true` 和 `selection:{index,target,window,bounds,identity}`，index 从 1 起；关闭或超时返回 `selected:false,selection:null`。时长覆盖各候选的累计展示，不随切换重置。原单目标输出不变。

#### 结构化 UIA 组合定位

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
  标题发现先筛选可见原生窗口，仅读取匹配候选的 UIA provider 并复核 PID/标题；没有原生候选时保留 UIA 虚拟窗口发现。目标 provider 失败仍返回错误，不能转为不存在。

- 组合查询不执行模糊、序数、距离锚点或 ItemContainer 虚拟化回退，也不自动展开控件。
  不能与 --near/--snear/--to-near 混用，也不能作为这些锚点参数。未物化控件需调用方先显式操作并重新观察。
- provider 枚举或必要属性读取失败返回 INTERNAL，不能证明不存在。每个查询阶段最多 5000 节点、40 层；
  超限 NOT_ACTIONABLE（reason=selector_budget_exceeded），不使用截断结果确认唯一性。COM 调用仍可能阻塞。
- get/click/type/set-value/select/toggle/指针命令/press --target/extract/screenshot --element/
  verify/wait 的元素目标均接受该形式，动作与输入护栏不变。物理鼠标仍要求原命令的显式授权。
  wait 对组合定位的观察错误直接传播，不将其当作消失；NOT_FOUND 仍表示尚未出现或已消失。
- `find --target <uia:JSON>` 用于唯一定位诊断，与旧过滤 flags（含 --capable）及 limit 互斥；输出
  `{window:{title},total:1,elements:[{ref:null,role,name,automation_id,target}],resolved_by:"uia-composite"}`，
  不返回 snapshot_id。带 resolved_by 的命令响应同样标识 uia-composite。
- flow 在执行任何步骤前校验各目标位置及 element: 后置条件里的组合查询，正文参数不按选择器解释。
  运行时每次重新定位，不将 UIA 对象或临时 ref 存进计划。示例见 fixtures/flows/calculator-composite.yml。

| 层 | 内容 | 时机 |
|---|---|---|
| **L1 通用原语**(统一) | 只依赖 OS 契约(accessibility 树 / 窗口系统 / 剪贴板),一套命令吃遍所有应用;**动词集合封闭**(~45 封顶,新增动词需 spec 提交评审) | M1–M2 全量 |
| **L2 语义投影**(机制统一,内容是应用数据) | 同一命令的视图参数(`--skeleton`、`--max-depth`、`--view content`),解决大 UI 的 token 膨胀;投影规则是**数据**(pack.toml,参考 desktop-cli 的 application packs),不是代码 | `--skeleton` 与 `--view content` 已落地;packs 视真实使用数据再定 |
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
                                     # 判定;窗口无 Document → NOT_FOUND,不静默回退
find --role button [--name "确定"]   # 条件查元素(带 ref);与 snapshot 的分工见 §3.6
                                     # 已物化树 0 命中且可映射精确属性时走 L4 虚拟化容器(§5)
                                     # --capable click|type:role 表派生的建议性动作兼容过滤
                                     # (type=Edit/Document 文本载体,click=可交互减文本载体)——
                                     # 零额外属性读,滤空即如实空结果;实际能力仍以动作时
                                     # pattern 判定为准(NOT_ACTIONABLE),分类不构成承诺
find --role edit --property value    # 读批(2026-10-14):limit 内全部命中逐个读该属性并
                                     # 按属性名附加到条目——N 字段表单一次往返全回(此前
                                     # find 枚举+逐 get 的 O(N) 往返收口);白名单与 get 同源;
                                     # 单元素读失败降级 read_error(code)/property_unreadable,
                                     # 不毁全批(批量采集语义:一条虚拟化重建/离屏/通道不支持
                                     # 是元素级事实);每元素一次定位+读是诚实成本(原生窗
                                     # 百 ms 级);--property 与 --target 互斥(单目标读走 get)
get @e3 [--property p] [--raw]      # 读元素属性;--raw 全量含 pattern 支持(合并原 inspect)
                                     # property 白名单含 text(TextPattern 文档正文专用通道,
                                     # 与 value 分工:Chromium 的 Document Value=URL 会掩盖正文)
                                     # 与 rect(物理像素包围盒 {x,y,width,height},PMv2 口径——
                                     # 元素坐标的权威通道,2026-10-12 交互轮补)
list-windows [--native [--hwnd <n>]] / list-surfaces # 窗口/虚拟桌面枚举(list-surfaces 未实现)
                                     # list-windows 附物理层叠放事实:foreground / topmost /
                                     # minimized / visible / z_order(0 = 最顶) / rect / owner_title
                                     # (rect 为物理像素,PMv2 口径——与 screenshot/xy: 同坐标系)
                                     # 附 monitors[] 显示器拓扑(EnumDisplayMonitors:物理 rect+主屏+有效
                                     # DPI)——跨调用 diff 该数组即可检测显示配置变更(分辨率/DPI/拔插),
                                     # 提前重观察,不拿陈旧坐标撞守卫(2026-10-14 显示变更现场实证)
extract @ref [--table]              # 结构化提取(表格→JSON 数组),@ref 指定容器
                                     # GridPattern 直读优先(source:"grid-pattern",带 row/col
                                     # 语义),树扁平化兜底(source:"tree-flatten");来源随结果上报
collect <容器目标> [--role 子串]     # 虚拟化容器有界全量收集(引擎有界动词,非 flow 循环):
      [--rounds N] [--max-items N]  # 先语义复位到顶(收集语义=整表,不受用户遗留滚动位置
                                     # 影响),再滚动(每轮 1 LargeIncrement 页级,语义
                                     # ScrollPattern,零注入——物理轮组合留给调用方显式
                                     # scroll --physical,ADR-007)×每轮新鲜观察×差分
                                     # 去重累积;完成判据 = 纯经验(空连两轮,只当启发
                                     # 不当判据);**ItemContainer 枚举模式实证只盖物化窗,
                                     # 不采用**;items 不带 @eN(虚拟化条目跨轮
                                     # 无 ref 稳定性,重定向走 find);截断如实:
                                     # complete=false + truncation_reason(max_
                                     # items/max_rounds/scroll_reset_failed/
                                     # scroll_pattern_missing/scroll_call_failed/
                                     # container_rebuilt)
find-image --template <png> [--app X] [--threshold 0.85] [--all] [--limit N] [--scales]
                                     # 确定性模板匹配定位(传统 RPA 图色,非语义视觉):
                                     # ZNCC 像素相关在窗口表面定位参考 PNG,返回物理
                                     # 像素坐标(PMv2,与 screenshot/xy: 同系)直接喂
                                     # click xy: --physical。用于 UIA 失明目标:自绘
                                     # 菜单、frameless 标题栏按钮、canvas 内容。诚实
                                     # 语义:score<threshold 一律 NOT_FOUND(不猜);多
                                     # 命中无 --all 报 AMBIGUOUS 附候选;均匀模板
                                     # (零方差)ZNCC 无定义→PROTOCOL;--scales 搜
                                     # 0.5×–2× 六档(DPI/缩放失配兜底,慢 ~6×)。模板
                                     # 像素精确:目标机 screenshot --element/--rect
                                     # 裁取,主题/DPI 变更需重采。零新依赖(WIC 解码
                                     # 复用截图编码同栈)。算法在 actl-core(纯逻辑
                                     # 单测),原生胶合在 actl-uia。**搜索范围可视化
                                     # (2026-10-14)**:搜索期间目标窗口画红色边框
                                     # (best-effort,绘制失败不阻塞匹配),结束自动
                                     # 擦除;window 字段附 origin(窗口客户面原点,
                                     # 物理像素)——失败信封可完整重建已扫窗面
preview-map --app <X> [--duration 3000]
                                     # 全窗组件画框(「先看见再设计」视图,
                                     # 2026-10-14):轻量树遍历只读
                                     # BoundingRectangle+ControlType,layered
                                     # overlay 按物理 rect 绘框——交互元素
                                     # 橙色 3px/其他暗青 1px,点击穿透不抢
                                     # 焦点,--duration(500..30000)后自动消散;
                                     # 只读零授权。预算 1000 元素/40 深(与观察
                                     # 上限同深),truncated:true 如实标注截断;
                                     # 信封带 elements/interactive 计数。用法
                                     # =UIA 盲区判读(空区=自绘,走截图路线)
                                     # 与选择器设计的现场可视化
```

#### 交互(interact)
```
click @e3 [--expect P]     right-click / double-click
click xy:1420,856 --app X --physical   # 坐标寻址(视觉兜底的手;§7 守卫)
click xyw:300,120 --app X --physical   # 窗口相对寻址(Fix B):原点=目标窗左上角
                                      # 与 screenshot --app 同锚——截图像素位置直接
                                      # 可点,零换算;守卫在换算后的屏幕坐标上执行
click @e3 --physical --no-activate     # 抑制激活的物理点击(§7.1;指针族通用)
type @e5 "text"            set-value @e5 "v"    # UIA Value pattern 优先
press ctrl+s               key-down shift / key-up shift  # 状态保持键必须成对
focus @e5 --app X          # 建立键盘焦点(SetFocus+读回核验,零键注入;补 press
                           # --target 只断言不建立的空档;语义见 §5.2)
paste --app Excel                                 # ctrl+v 到当前选中(无元素落点;pack 路线原语)
select @e7 "选项"          toggle @e8
scroll @e9 --down 3        drag @e3 @e8                   # drag/hover 无 UIA pattern,默认物理执行
                                                           # scroll 位移核验:uia-scroll 报 Ok 但前后
                                                           # vertical percent 零变化且不在目标边界
                                                           # (0%/100%)=假位移,降级物理滚轮;percent
                                                           # 不可读维持原判(诚实边界;Chromium 页实证)
hover @e3                  (其余物理兜底:同参数 + --physical)
key-down space --app X --duration 800             # 长按:按下→持有→自动抬起(50-60000ms;
                                                   # 急停即释放并 ABORTED)
drag xy:120,340 xy:540,360 --app X                # 坐标框选/画布直连:两端窗口边界+遮挡守卫
drag xy:100,100 xy:400,300 --app X --path 200,200 --path 300,150
                                                   # 轨迹拖拽:路径点(绝对物理px,≤64)8px 线性
                                                   # 插值成一批 MOVE(画布/签名);无 --path 保持
                                                   # 既有直线四事件行为零变更(docs/74)
drag xy:100,100 xy:400,300 --app X --human [--seed N]
                                                   # 人类轨迹拖拽(2026-10-14,滑块解题的物理
                                                   # 轨执行面):actl-core 轨迹整形器(纯函数,
                                                   # 种子确定)替换路径——峰值速度 ~72% 行程+
                                                   # 过冲回拉+y 轴臂漂移 ±2px+事件间隔按轨迹
                                                   # 时间轴(≤50ms 分片睡眠,每片急停核验;
                                                   # 中断即抬起已按下的键并 ABORTED 如实报);
                                                   # 终点精确落位;与 --path 互斥,需 --app
```

#### 断言(assert)
```
verify --exists @e3 / --value @e3 "x" / --checked @e8 / --order ...
verify --value <target> <expected> --property name|value|text [--contains] [--normalize-lines]
                                     # 分别严格读 Name/ValuePattern/TextPattern;省略 property
                                     # 保持 Value→Text 兼容策略;--normalize-lines 全文比较前
                                     # 转换 CRLF/CR 为 LF、去除尾部 LF(与 contains 互斥,
                                     # 供 Word TextPattern 行结束符差异使用)
verify --value TARGET EXPECTED --url-query KEY [--app APP]     # URL 查询参数断言(§7.7)
verify-file / verify-rows / verify-owner / verify-closed / verify-clipboard   # 只读断言族(§7.7)
verify-window --app <锚> --state <exists|absent|foreground|minimized|visible>  # 窗口态断言(§7.7)
```

#### 窗口/应用(window)
```
launch <app> [--args ...] [--wait-window <标题> --timeout ms]  focus-window w-4521
resize-window / close-window
close-window --hwnd w-4521 --system-close  # 投 WM_SYSCOMMAND/SC_CLOSE(=点标题栏 X);
                                           # 对 WM_CLOSE 只收窗不退程的应用(Excel 后台
                                           # 正确关闭文档后,裸框架 WM_CLOSE 隐藏、SC_CLOSE
                                           # 才退出),作为进程收尾手段
close-window 存活复核:消息投递成功≠已关——模态未清时窗口存活,
  投递后有界等待(2s)窗口销毁;仍存活返回 NOT_ACTIONABLE,evidence 携带同 pid
  归属弹窗清单(title/class/hwnd),调用方直接看到挡路者(docs/81 §8)。
envelope 附 pid + instance(进程创建时刻,与 verify-owner/verify-closed 同表示)
  ——close→verify-closed 配对不再依赖事先的 verify-owner 记录(2026-10-12 交互轮)。
shell 承载面硬护栏:explorer.exe 的任务栏/桌面类窗(Shell_TrayWnd/Progman/WorkerW)
  PERM_DENIED 结构拒绝——WM_CLOSE 到它们可拖垮整个桌面,ROADMAP 实录此前
  "拒绝属 AMBIGUOUS 安全巧合"升级为结构判定;资源管理器文件夹窗不受影响
launch --wait-window:等待成功新增 window 对象{hwnd,pid,title,foreground:true,
  matched_by:"title"},window_ready:true;匹配不证明窗口来自新 PID(§7.5.2)
launch 解析优先级(程序名形态):app-registry 别名(%LOCALAPPDATA%/actl/
  apps.json,app-register/app-locate 维护——**pack 的 launch 字段只写别名,
  机器路径不入 pack**——pack 可移植,其他用户机器跑一次 app-locate 即本地化)
  → App Paths 注册表(HKLM→HKCU)→ 字面路径 → PATH;Store 版本目录漂移用
  --pattern glob 条目(resolved 缓存失效自动重解析)。
app-locate <名> [--register]:软件位置五通道只读探测(①别名→②App Paths
  →③常见安装目录 Program Files×2/%LOCALAPPDATA%\Programs→④**开始菜单
  .lnk 递归 ≤3 层含中文名,IShellLink 解析 TargetPath**——非标准盘符安装
  的唯一系统入口,2026-10-12 腾讯会议教训:曾因漏此通道误判「未安装」→
  ⑤PATH);去重后按通道优先序返回存在的候选+first,**卸载类快捷方式与
  可执行永不入列**(first 候选指向卸载器是注册事故);有界(≤800 .lnk、
  每目录 ≤64 条目);零候选 NOT_FOUND。--register 把 first 写入 apps.json
  (探测本身不启动应用);绿色软件/无开始菜单入口的怪形态走显式 app-register。
**manual 保护(用户显式指定 vs 自动发现)**:`app-register` 写 manual 条目,
  `app-locate --register` 写 auto 条目——**auto 永不覆盖 manual**(同名 manual
  在场时 locate 照常返回候选但注记 `kept_manual`+现有路径,别名继续解析;
  register() 层第二道防线拒绝 auto 覆盖)。改用户指定必须走显式 app-register
  /app-remove;手改 JSON 省略 discovered 字段按 manual 解析(人手写的条目
  就是显式意志,天然受保护);pattern 条目的运行时重解析只动 resolved 缓存,
  永不改 path/pattern 本体。
list-windows 前台兜底:UIA 桌面子树在 provider 楔死时漏窗(无标题保存框实证),
  前台窗缺失时用 Win32 直读回填——枚举不失明于"当前正挡着什么"
UIA 看门狗(引擎行为):定位/树观察/焦点查询区段超过 ACTL_UIA_WATCHDOG_MS
  (默认 20000,0=禁用)时进程带退出码 125 自毙——无界 COM 挂起从"永久僵尸"
  变"限内诚实失败"(docs/81 §8 僵尸事故根治)
**区段口径**:树扫描(collect 前沿循环)逐节点 `rearm()` 重臂——区段语义为
  "单次跨进程调用"的有界性(冷 provider 合法慢扫描在整段口径下会被 20s 误杀);
  单槽约束:嵌套 enter/drop 会清零外层区段——嵌套守卫调用之后不得再依赖
  外层区段(scroll_element 两段式先例)。
**子树物化(方案2——观察面的 N→1 往返收口)**：树观察的缓存请求
  `TreeScope=Subtree`——`build_updated_cache` **一次调用**把整棵子树+属性+父子链接
  拉进本地缓存,树遍历走 `get_cached_children()`(纯本地,零跨进程往返)——N 次
  children 往返变 1 次物化调用(屏幕阅读器同款技术)。collect 循环原封不动
  (上限/顺序/refs 编号语义逐字节保留)。物化是单次长调用,
  UIA 区段 20s 限即其界(巨树 `ACTL_UIA_WATCHDOG_MS` 放宽);个别 provider 对
  Subtree 缓存支持差或对照实验时 `ACTL_WALK_LEGACY=1` 回落逐节点 RPC 遍历。
**find --id provider 快路径**:单一精确条件(--id,无 --name/--role)
  时走 Subtree FindAll + AutomationId 等值条件——单次跨进程调用替代全树遍历。
  id 为精确等值语义与全树过滤等价不漏报;命中走 L4 virtual_hit 同形态(`ref:null` +
  `target` 选择器语义 + `resolved_by:"provider-exact"`,无快照绑定);
  无命中回落原全树路径(保快照面与 L4 兜底)。**--name/--role 为子串语义,
  UIA 属性条件只支持等值,不走此路径(诚实边界)**。
  命中结果不赋 @eN ref 是形态而非缺陷——元素命令直接吃 target 形态
  (`click id:<X> --app ...`),envelope 的 `target` 字段即动作参数;
  需要 ref/快照绑定就走全树路径(--name 等值子串或 snapshot)。
**析构纪律(尾段挂起族)**:UIElement 的 Drop(跨进程 Release)与
  读取/调用同族无界——看门狗是单槽时间戳不能嵌套,guard 声明在资源之后会因
  反序析构让 Release 裸奔。两条铁律:① **成功路径硬退出**——envelope 落盘 flush 后 `std::process::exit(0)`,
  与错误路径对称,析构链整个跳过(状态落盘不依赖 Drop:finish(phase) 显式调用、Journal 每事件即 flush);② **COM 元素不出 actl-uia 域**——跨函数返回携带
  `Located` 会让调用方裸奔 Release;结果结构携带 `ResolvedInfo`
  (`Located::into_info()` 消费式转换,**调用点必须已在看门狗区内**)。
wait --element 瞬态重试:STALE_REF/INTERNAL 观察瞬态按"尚未出现"继续轮询
  (wait --gone 同款语义),超时如实失败;非瞬态立即上抛
```

#### 系统(system)
```
get-clipboard / set-clipboard   # 遵守 get-*/set-* 命名规则
clipboard-save <id> / clipboard-restore <id> / clipboard-formats
                                     # 剪贴板全格式备份/恢复/清单(2026-10-13 剪贴板轮,
                                     # RPA 经典坑:粘贴通道毁掉用户剪贴板——type --paste 的
                                     # EmptyClipboard 清全部格式,用户复制的文件列表/图片一并
                                     # 丢失;UiPath "Restore clipboard" 同款解法)。save 备份
                                     # 全格式(文本/CF_HDROP/位图/私有格式,延迟渲染格式如实
                                     # 登记 skipped_delayed)到状态目录 clipboard/<id>.json;
                                     # restore 逐格式回放(覆盖性写,与 set-clipboard 同副作用
                                     # 面)。**恢复是编排层显式决定,paste 永不自动恢复**——
                                     # agent 粘贴后用户新复制的内容会被静默覆盖回旧值,自动
                                     # 恢复反而制造数据丢失事故。Win+V 历史/云同步把恢复表现
                                     # 为新条目,不伪造历史回滚(诚实边界)。formats 只读枚举
                                     # 当前格式清单(诊断,不读数据内容)。
get-selection [--app X] [目标] [--max-chars N]
                                     # 读当前用户选区:TextPattern 范围优先(kind=text,逐范围
                                     # 文本;UIA 退化插入点读为空串被过滤——折叠光标如实
                                     # no_selection)→ SelectionPattern 容器(kind=items,逐项
                                     # role/name/automation_id)→ 如实 unavailable
                                     # (no_selection/no_selection_support)。纯读取零注入;
                                     # 目标缺省 focused:(选区随键盘焦点所在控件)。
                                     # 不做物理 Ctrl+C 通道(须显式物理授权,调用方组合)
list-notifications              # 系统通知读取
```

#### 浏览器会话(browser,CDP 通道 v1)
```
browser-launch --browser-executable <abs> [--url U] [--profile name] [--proxy host:port] [--user-agent UA] [--headless] [--disable-extensions] [--timeout ms]
                                     # 启动 actl 自有的隔离浏览器(系统 Edge/Chrome 二进制,
                                     # 专用 user-data-dir——默认 profile 被 Chrome/Edge 136+
                                     # 禁调试端口,永不附着用户会话;用户日常浏览器的插件/
                                     # 设置/cookie 属另一数据世界,互不影响)并绑定一个 tab;
                                     # **默认有头**(可见窗口):常规 UA 过 UA 壁垒站点、用户
                                     # 可全程旁观(非隐身执行立场)、持久 profile 人工登录就绪,
                                     # 且浏览器 chrome 层的 UIA 兜底面在场;--headless 才是无
                                     # 窗口后台形态(HeadlessChrome UA 会被部分站点拒服务,
                                     # 无 UIA 面,代价自担;**--user-agent 覆盖 UA 字符串**
                                     # (可打印 ASCII,1-512 字节,入口前置校验)是 headless
                                     # UA 壁垒的通行道——只覆盖 UA 本身,不伪装指纹面
                                     # (canvas/WebGL 仍如实应答 headless);复用活跃会话时
                                     # 不重申);--profile 持久 profile(一次性
                                     # 人工登录复用),**同名再启=复用活跃会话**(reused:true,
                                     # 陈旧记录自动接管);扩展策略:ephemeral 恒禁用,
                                     # 持久 profile 默认保留(合法自动化资产),--disable-extensions
                                     # 显式禁;--proxy 裸 HOST:PORT 显式代理(浏览器 DoH 会
                                     # 绕过系统 TUN fake-ip 路由);DevToolsActivePort 出现
                                     # **且端点应答**=启动成功(持久 profile 的陈旧端口文件
                                     # 会假阳,先清再用),否则 fail-fast 回收自启进程;
                                     # stdio 继承守卫防管道挂死
browser-navigate --session <id> --url U  # 重核绑定 target 存活→导航→等主 frame 的
                                     # Page.frameNavigated(读线程事件,导航竞态判定地基);
                                     # 网络级加载失败(chrome-error)显式 INTERNAL+net_error
                                     # 证据,不静默当成功
browser-read --session <id> (--css S | --xpath X) [--property P] [--iframe]
                                     # 唯一匹配读文本(0→NOT_FOUND,>1→AMBIGUOUS 带 matches 证据;
                                     # --iframe 经子 frame 执行上下文,跨源同权);文本 64KiB 截断;
                                     # --property 白名单读属性:value/href/name/id/checked/tag
                                     # (value 覆盖输入框——textContent 对 input 为空)
browser-read ... [--all [--limit N]]
                                     # 枚举形态:返回全部匹配——列表语义,零命中=诚实空集
                                     # total:0(非 NOT_FOUND,与 find 枚举口径一致);--limit
                                     # 裁返回面(默认 50,上限 500),total 保真 +
                                     # truncated_by_limit 如实标记;唯一模式的硬判定只
                                     # 作用于非 --all 调用,两形态共存
browser-read ... [--deep] [--wait-for N]
                                     # --deep:shadow DOM 穿透——在 document 与每个开放
                                     # shadowRoot(open 模式)内递归执行选择器,收集全部命中;
                                     # closed shadow 对页面 JS 不可见,是如实边界(不伪装穿透);
                                     # 遍历预算有界:节点≤50000/收集≤5000,触顶
                                     # traversal_capped=true 如实上报;payload 附 deep:true
                                     # --wait-for N:沉降谓词——命中数 ≥N 才返回,内建 500ms
                                     # 轮询共用药洞超时,超时 TIMEOUT 附实测命中数
browser-type --session <id> (--css S | --xpath X) --text T [--iframe] [--allow-popups]
                                     # 守卫输入写:input/textarea 唯一匹配→native setter 赋值
                                     # +input/change 事件(React 兼容)→读回 value 必须与请求一致
                                     # (受控组件改写→ASSERTION_FAILED);输入内容不回显 envelope/
                                     # evidence(长度核验),自证用 browser-read --property value。
                                     # **弹层门禁(默认开)**:派发前扫页内可见弹层(默认选择器
                                     # 同 --no-popups),命中→NOT_ACTIONABLE 拒发附样本与专用 hint
                                     # ——模态态会静默吞输入,派发成功≠语义生效;--allow-popups
                                     # 是判读样本后的显式放行(流内静态 role=dialog 误报面)
browser-click --session <id> (--css S | --xpath X) [--iframe] [--expect-text substr] [--allow-popups]
                                     # 守卫写:停止代次核验→绑定重核→弹层门禁(默认开,同上)
                                     # →唯一匹配→派发→读回;--expect-text 断言被点击元素自身文本
                                     # (ASSERTION_FAILED);效果断言用 browser-read;失败不盲重放。
                                     # dismiss 路径:点弹层自身关闭键需 --allow-popups(门禁对
                                     # 弹层内控件同样拒发),关后 browser-verify --no-popups 复核
browser-close --session <id>     # CDP Browser.close 优雅关闭自有浏览器+注销会话文件;
                                     # ephemeral profile 删除、persistent 保留;幂等(端点已死→如实行)
browser-list                      # 只读枚举活跃会话(新→旧):session_id/target/url/pid/mode
browser-verify [--newest-alive | --none | --session <id> --alive|--gone | --session <id> (--css S|--xpath X) [--contains T] | --session <id> --no-popups [--popup-selector S]]
                                     # 只读断言族(flow verify 位专用):launch 后 --newest-alive、
                                     # close 后 --none、按会话 --alive/--gone;**登录态谓词**
                                     # --session+--css/--xpath [--contains]:选择器唯一解析
                                     # (+文本包含)——登录暂停点的 resume_check 判据;值级
                                     # 断言在写动词上(--expect-text)。**页内弹层谓词
                                     # --no-popups**:枚举可见弹层表面(默认 dialog[open]/
                                     # role=dialog/alertdialog/aria-modal,零布局盒不计;
                                     # 站点自绘浮层走 --popup-selector 显式声明),0=通过、
                                     # >0=ASSERTION_FAILED 带 total+样本(≤5 条、text≤200
                                     # 字符)——flow condition 位映射 false,when/unless 弹层
                                     # 分支可组合(原生弹层门禁结构性不覆盖页内 DOM 弹层;
                                     # 写步的弹层门禁默认拒发,本谓词是复核/分支判据——
                                     # dismiss 后收口、登录墙 when 分支等);选择器真语法错误
                                     # PROTOCOL(浏览器 tokenizer 对未闭合 [ 按 EOF 闭合
                                     # 是如实语义)
browser-collect --session <id> (--css S | --xpath X) [--property P] [--deep]
                [--interval-ms N] [--max-rounds N] [--max-items N] [--timeout ms]
                                     # 懒加载列表全量收集(浏览器轨对齐 UIA collect
                                     # 语义):读全部匹配(复用 --all/--deep 枚举)→
                                     # 指纹差分去重累积→页滚一屏(CDP 写,声明动作卡
                                     # kind browser-collect)→沉降(--interval-ms 默认
                                     # 500,100..=5000)→再读;**收敛=空连两轮零新增**
                                     # (converged_by:"empty-streak")——触底信号不作
                                     # 完成判据(懒加载页 scrollHeight 会增长,UIA collect
                                     # 位置信号否证同款教训);预算 --max-rounds(默认
                                     # 100,上限 200)/--max-items(默认 2000,上限
                                     # 5000),触顶 complete:false + truncation_reason
                                     # (max_items/max_rounds/scroll_failed——滚动
                                     # evaluate 失败=软截断);读失败原样上抛(导航离开
                                     # 是调用方现场);items 逐条 64KiB 截断同 read,
                                     # new_items_per_round 逐轮留痕
browser-netlog --session <id> [--filter S] [--window ms] [--max-entries N]
                                     # 有界网络请求快照(深水层首动词,只读;Network
                                     # 域经 chromiumoxide 同步门面桥,docs/72):
                                     # enable→等窗口(100..=60000ms,默认 5000,含
                                     # 300ms 订阅沉降)→逐事件上报 url/method/
                                     # resource_type;--filter URL 子串过滤(≤2048
                                     # 字节),--max-entries(1..=2000,默认 500)触顶
                                     # truncated:true 诚实截断;会话侧 enable 收尾
                                     # 必 disable;**无响应体/头捕获**——URL 清查面
                                     # (懒加载/XHR 端点/资源风暴定位),不是内容转储;
                                     # read_only 分类(可作 flow 观察步);错误面:
                                     # PROTOCOL 预算 | NOT_FOUND 会话/目标失效 |
                                     # TIMEOUT 窗口或 attach 超预算
browser-targets --session <id>   # CDP target 只读枚举(--target 寻址的发现
                                     # 通道):绑 tab 置首(bound:true)+其他
                                     # tab+扩展 popup(打开期间才有 target)+
                                     # service_worker(标注 dom:false,不可 DOM
                                     # 读写);扩展弹层写链第一步=物理开弹窗
                                     # (UIA --physical)→此处枚举→--target
                                     # <target_id> 读写其 DOM
browser-screenshot --session <id> [--css S | --xpath X] [--iframe] [--target T] [--out P] [--timeout ms]
                                     # 像素捕获(只读观察,同 read 分类):clip=
                                     # 元素 bbox(CSS px,视口求交,唯一匹配
                                     # 硬规则;--iframe 进跨源 iframe);省略=
                                     # 整视口。envelope 只带 path+clip+bytes
                                     # (视觉消费是调用方的决定:decide 视觉
                                     # 咨询文档/challenge-solve/人工复核);
                                     # 图像像素尺寸随设备缩放,**不承诺与
                                     # clip CSS 尺寸 1:1**——消费侧自标定
                                     # (challenge-solve 内建 图像宽/clip宽
                                     # 自标定)。默认落 shots 目录(keep-20)
browser-drag --session <id> --from X,Y --to X,Y [--human] [--seed N] [--target T] [--timeout ms]
                                     # CDP Input 拖拽(写操作:停止核验前置
                                     # +动作卡):坐标=视口 CSS px(与定位链
                                     # 同系)。默认直线插值(机器形态,如实);
                                     # --human 走 actl-core 轨迹整形器
                                     # (峰值速度 ~72% 行程+过冲回拉+臂漂移
                                     # ±2px+事件间隔按时间轴,种子确定可回放);
                                     # 效果归调用方谓词,本命令只报送达
browser-verify --session <id> --challenge
                                     # 挑战存在性断言(2026-10-14):扫描页面
                                     # (title/script src/iframe src/DOM 标记)
                                     # 对已知签名表分型——cloudflare 全页/
                                     # turnstile/geetest/TCaptcha/aliyun/
                                     # dingxiang/yidun/recaptcha/hcaptcha/
                                     # arkose/datadome;零命中=通过,命中=
                                     # ASSERTION_FAILED 带 families 证据
                                     # (when/unless 组「检测到挑战→解/停」
                                     # 分支)。**只识别不断言**:命中=形似
                                     # 已知挑战,未命中≠无挑战(自绘在表外)
challenge-solve --session <id> --track-css S --piece-css S --handle-css S --success-css S
               [--template P] [--threshold F] [--attempts 1..=2] [--no-human] [--seed N] [--iframe] [--target T] [--timeout ms]
                                     # 滑块解题循环(写+读复合):检测(上下文)
                                     # →截轨道/件元素(ZNCC:件=模板,轨道=
                                     # 搜索面;件当前邻域自匹配过滤)→位移
                                     # (图像→CSS 自标定)→人类轨迹拖拽→
                                     # --success-css 存在性轮询(≤4s)→有界
                                     # 重试(默认 1,上限 2——失败回灌风控)。
                                     # 证据截图随信封;**边界:只解「呈现在
                                     # 面前的挑战」**(真实浏览器+真实输入
                                     # 事件+确定性感知),不做隐身/指纹伪装
                                     # /打码服务;失败如实上报带证据
--target <target_id>             # browser-read/type/click 的寻址覆盖:默认会
                                     # 绑 tab;指定后绑定重核/弹层门禁全部落在
                                     # 覆盖目标(写护栏不因寻址放宽);id 来自
                                     # browser-targets;覆盖时 envelope 回显
                                     # target 字段
```
**显示层协同**:browser 的写与生命周期命令(launch/navigate/click/type/collect/close/drag)在信号流声明动作卡(kind/effects/target)——**不经显示租约门**(CDP 写非桌面物理输入,展示是透明性而非权限门;急停仍经停止代次核验压过一切写);`InputEffects` 全空如实申报;target 只带 `cdp:<session>` 标记,不含选择器/URL/文本(动作日志内容省略纪律)。观察命令(read/list/verify/netlog/targets/screenshot)不声明动作,与 UIA 轨观察同规。
**YAML 流程接入**:browser 命令进 flow 的治理模型走三件套——① `browser-launch` 是生命周期命令,不能带 outputs(outputs 只允许只读生产者);② 会话句柄经 `browser-list` 以观察者身份产出(outputs.pointer=/sessions/0/session_id),后续步骤 `bind` 注入 `--session` 值位(下标须指向值字面量,不能指到旗标上);③ 有副作用的步骤必须带 verify——`browser-verify` 族即为此而生(launch→--newest-alive,click→--session X --alive,close→--none)。
**背景观察步**(`background: true`,2026-10-13):只读观察命令(如 `browser-netlog` 的窗口期)与后续步骤**并发**的流程一等形态——引擎派发后不等待,流程退出前统一收割(join),结果入 StepRecord.background_result(状态文件可查,bind 结构性不可用)。语义边界:run 必须只读,禁 verify/outputs/pause_before/ensure/resume_check/otherwise/recovery/retry(观察者不是流程控制点),违反即 PROTOCOL;收割失败=步骤失败,Completed 改判 Failed(`background_failed:<step>`)。背景观察不受暂停观察点约束(有界自终),中断恢复时重派发即重开窗。
**持久 profile 治理**:flow 中 `browser-launch --profile <name>` 必须在
`resources.browser_profiles` 声明(未声明即 PROTOCOL 拒绝)——凭据资产显式归属;该资源创建幂等、永不随任务删除。**登录暂停点**:`pause_before: true` + `resume_check: browser-verify --session X --css <登录标记>`(用户在可见窗口完成登录后恢复;谓词不满足则保持 paused,`reason: resume_check_failed`)。示例流程 `skills/actl/workflows/browser-login-session.yml`。
会话状态落 `%LOCALAPPDATA%\actl\browser\`(sessions/*.json + profiles/);错误码复用全集,无新增码。
安全边界(docs/72 §8):浅层客户端只实现 Target/Runtime/Page/Browser 白名单域;深水层(chromiumoxide 同步门面,2026-10-13)类型面全域可达,**命令面**守 cookie 誓言——无 cookie 导出/转储动词,负面契约测试钉死;不附着用户已有浏览器。stdio 继承守卫保证调用方管道在 actl 退出后正常 EOF。已知边界(v1):写入的 uncertain 状态不跨进程持久化(崩溃后由调用方 browser-read 裁决);值级断言动词扩展、batch 组合待后续 spec。
设计全文见 [72](72-browser-cdp-integration.md)。

#### 可靠性(reliability,RPA 借鉴)
```
wait --element @e3 [--timeout 5000]                # 等元素可解析(存在性)
wait --element @e3 --property value="OK" [--contains]   # 谓词等待:value/name/checked
wait --element @e3 --stable                        # settle 等待:text>value>name 通道内容
                                                     # 连续两次采样不变
                                                     # (settle_ms 默认 250ms，至少跨一个采样间隔);
                                                     # 滚动/导航后的加载判定,evidence 带
                                                     # samples/changes;与 --property 互斥
wait --window "保存" | --gone @e3                        # 等窗口出现 | 等元素消失
wait --idle 8000 [--timeout 120000]              # 等系统空闲:全系统键鼠无输入 ≥N ms
                                                  # (GetLastInputInfo;"人不在"门——长批任务的
                                                  # 前置闸。与 --element/--window/--gone 互斥;
                                                  # 只观测物理输入,UIA 语义注入对它不可见)
verify --exists @e3 / --value @e3 "x" / --checked @e8 / --order ...
batch --json '<commands>'   # 单进程内顺序执行,共享 snapshot;每步结果与耗时进 steps[]
                             # (i/cmd/ok/duration_ms/data——步级回传该步 envelope 的 data,
                             # snapshot 类步附 snapshot_id;失败步 error 与单调用信封
                             # 同形:code/message/hint 恒在、evidence 有则保留,顶层错误
                             # hint 继承首错精确恢复路径),首错即停时
                             # 已执行步随错误 evidence 带出(部分结果不丢失);
                             # 命令字符串支持 shell 式双/单引号包裹含空格值
                             # (反斜杠保持字面——Windows 路径;未闭合引号 PROTOCOL);
                             # 本轮新开窗口差集上报 windows_created(title/pid/hwnd,
                             # 清理地基;生命周期归编排层,CLI 不代管);
                             # 步前弹窗闸门:前台非基线且不匹配本批 --app → steps 附 blocked_by(warning)
```

**find --id provider 快路径、析构纪律与子树物化等引擎行为契约见 §3.4 窗口/应用组。**

窗口诊断与批处理基线：

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
| `find` vs `collect` | 定向找**一个**(唯一性纪律)vs 定向要**全部**(虚拟化列表的有界收集,条目无 ref) |
| `find` vs `find-image` | UIA 树内定位(语义选择器,provider 暴露即可达)vs 像素模板匹配(UIA 失明目标的兜底:自绘/frameless/canvas;返回坐标非 ref,须 `click xy: --physical` 消费) |
| `scroll` vs `press pgdn` | 容器内滚动(UIA Scroll pattern)vs 焦点键滚动 |
| `select` vs `set-value` | 离散选项 vs 文本值(combo box 用 select)。select 选项未物化时**自动展开重试一次**(2026-10-14:容器支持 ExpandCollapse 则零注入展开→沉降→重扫;evidence 区分 `expand_unsupported`(Win32 原生 ComboBox 不暴露 ExpandCollapse,agent 走 press f4/alt+down 或物理点击)与 `expanded_retried`(展开过仍无此项);子串回退大小写不敏感(显示文本语义,Win32 CB_SELECTSTRING 惯例),多命中 AMBIGUOUS 附候选) |
| `toggle` vs `click` | Toggle pattern 直调(无点击副作用、可读状态)vs 点击语义 |
| `launch` vs `focus-window` | launch 默认启动**新实例**(不做已运行检测);前置已有窗口是 focus-window 的职责 |

### 3.7 观察与读取语义(UIA 轨现行契约)

- `get` 保留成功读取的空字符串；`value` 仅在 ValuePattern 明确不支持时回退 TextPattern，读取失败不回退。
- `get --property` 白名单含 `enabled`（UIA IsEnabled）：禁用态此前只在写动作失败后可见，门户预填锁定字段的写前判读现为只读一步。
- `get --property selected`（2026-10-13 VMware 向导轮实证补）：RadioButton/CheckBox/MenuItem 选定态读回——SelectionItem `IsSelected` 优先、无该 pattern 时回退 ToggleState（`On`⇒true）；两 pattern 均不可用为 None（非可选控件，诚实降级不伪装）。选择/勾选动作后不再需要截图确认。
- **坐标系不变量（2026-10-13 进程级 PMv2 修复兑现）**：actl.exe 全进程 Per-Monitor V2 感知——`get rect`、find 证据、preview bounds、focus-report、snapshot 树几何、`click xy:` 与 `screenshot --rect` 全部同为**物理像素**，缩放屏无逻辑/物理混排。修复根因：双 200% 缩放屏实测 `get rect` 曾回逻辑坐标（与 list-windows/find-image 恰差 2×），"PMv2 物理"宣称在 rect 读取路径为假；现进程入口一次设置（signal 显示端同款先例），散点线程 guard 保留为无害冗余。设置失败仅 stderr 诊断，坐标回退逻辑值不静默。
- **窗口相对寻址 `xyw:` + `screenshot --wrect`（2026-10-13 Fix B，docs/83 §8——"截图看到的像素位置直接可点"）**：原点=目标窗口 DWM extended bounds 左上角（与 `screenshot --app` 截图口径同锚）。`click xyw:x,y --app W --physical` 把全窗截图上第 (x,y) 像素直接换算成屏幕坐标注入，守卫/遮挡检查/急停全复用，envelope 附 `origin`（窗口原点）与换算后 `point` 可判读；`screenshot --wrect X,Y,W,H --app W` 同锚裁模板（自绘控件全链：全窗截图→--wrect 裁模板→find-image→xyw 点击，缩放屏零手工换算）。窗口相对偏移恒非负（负值 PROTOCOL——是调用方算错的信号）；xyw 现限 click/指针族（drag 端点暂拒，PROTOCOL 指引用元素或 xy:）。
- **窗口几何读写闭环（2026-10-13 窗口几何轮，Selenium setRect 同款部分字段语义）**：`resize-window` 锚面扩 `--hwnd`（标题漂移后仍稳），`--x/--y` 可选——缺省=保位置（向后兼容），全给即闭环：`get --property rect` 的输出改一改就是 `resize-window` 的输入，零单位换算（全物理像素）；最小化窗口先还原（SetWindowPos 对 iconic 未定义，Selenium "Restores the window and sets the window size" 同款）；尺寸下限 120px 防零尺寸不可逆。`maximize-window` 新增，与 minimize/restore 三态对称（SW_MAXIMIZE，窗口管理器语义=填满工作区不遮任务栏；幂等 already:true）；**应用内全屏（F11/VMware 全屏）是应用状态非窗口状态，不在窗口动词管辖（诚实边界）**。评测场景的"标准化摆放"即闭环自然结果：pass^5 重跑前把目标窗 set 到固定 rect，find-image 模板与 xyw 坐标跨 run 稳定。
- 默认 `get` 的 data 增加 `unsupported` 属性名数组（如 `["value","text"]`），表示已确认不支持的读取通道；空数组表示本次没有不支持项。其他读取错误使命令失败，不能输出看似完整的成功结果。
- 显式 `get --property value|text`、verify 和属性等待遇到不支持通道返回 NOT_ACTIONABLE，evidence.reason=`pattern_unsupported`；value 的 get/自动等待允许上述双通道，而 `verify --property value` 严格读取 ValuePattern。
- UIA 元素失效映射 STALE_REF，访问拒绝映射 PERM_DENIED，其余 provider/属性读取错误映射 INTERNAL；evidence 带 channel、native_code（原生错误存在时）。观察失败不能等同空值、断言不满足或稳定。
- `wait --stable` 只在明确不支持时按 Text → Value → Name 取样；`type --verify-value` 复用读取状态并在错误中保留 delivered/value_verified/retry，禁止重放输入。
- 等待谓词无空期望语法；证据记录见 [37](37-uia-observation.md)。

**观察完整性**：

- snapshot、旧语义定位与表格树回退使用显式 RawView 的可失败枚举；子级为空与枚举错误分开。必要属性读取失败按 channel/native_code 报错，成功空名称与 ID 保留为空字符串。
- snapshot 增加 `truncation_reasons` 数组，完整时为 `[]`；节点/深度预算实际省略内容时分别记录 `node_limit` / `depth_limit`，并设 `truncated=true`。恰好达到上限但无未观察节点不算截断。属性或枚举失败整次报错，不提供携带属性错误的部分树。
- snapshot/find 信封附 `materialization`(2026-10-12):`subtree_cache`(单次 provider 物化成功)| `node_by_node_fallback`(物化被 provider 拒答回落逐节点——Word a11y 缓存回归实证,回落态文本通道可能为空)| `node_by_node_legacy`(env 强制)。过滤式 find、旧语义定位/ref 重放和表格树提取拒绝不完整范围，返回 NOT_ACTIONABLE，evidence 为 `{reason:"observation_incomplete",causes:[...]}`。长观察的中途可见性（2026-10-12）：逐节点遍历路径每节点核对 stop/pause（急停与流程暂停可中断长扫描）并间隔 2s 向 stderr 发 `[actl-progress]` 人读进度行（诊断增强，不承诺格式，stdout 信封不受影响）；**物化路径为单次 provider 调用无中途点**——中断归看门狗（区段上限），慢完成归 `[actl-slow]` 事后报告。content 投影在截断范围内未见 Document 时也不能声称 NOT_FOUND。成功采样仍不是原子快照，控件可能在调用之间变化。
- 所有元素等待仅将 NOT_FOUND 解释为缺席；STALE_REF、权限、provider 错误及超限立即传播。虚拟化查询区分成功空结果与原生失败，多个匹配拒绝为 AMBIGUOUS；provider 对查询本身的结构性拒绝（`E_INVALIDARG`，Chromium 对 ItemContainer 形态）归并 NOT_FOUND 并附 evidence.reason=`virtualized_probe_rejected`（channel/native_code），不冒充 INTERNAL——元素缺席是诚实判定，探测拒绝细节留证即可。
- extract 仅在 GridPattern availability 明确为 false 时回退树提取，输出 `fallback_reason:"grid_pattern_unsupported"`；Grid 直读时为 null。Grid 属性、单元格失败不回退；合法空表和空单元格保留。负行列数报 INTERNAL；超限或前后行列数变化报 NOT_ACTIONABLE（causes 为 `table_limit` / `table_changed`）。树回退只反映已物化 Text/Edit 单元格，不保证与逻辑表格的列结构一致。
- 窗口清单、窗口 RuntimeId 基线、等待新窗口及按 PID 定位也使用可失败的 RawView 枚举；根、子窗口、分类及必要 name/class/pid/hwnd/runtime_id 读取失败传播原错误，不输出成功的部分清单，不用漏掉候选后的结果证明唯一性。成功空标题仍按清单约定过滤。
- `wait --idle` 的 GetLastInputInfo 失败返回 INTERNAL，evidence 为 `{reason:"input_observation_failed",api:"GetLastInputInfo"}`，不把未知状态当空闲；每次轮询先检查 flow 暂停/停止。Rust `idle_ms` 接口相应返回 Result。
- 输入交接增加独立 Raw Input 键鼠活动计数。注册丢失、消息泵心跳超过 1000ms 或监听线程退出返回既有 `input_monitor_unavailable`，撤销旧授权；健康恢复不恢复授权。不存储键值或坐标，不发送探针输入，不替代 `--physical` 参数或人工授权。
- 预算约束的是遍历节点/深度与表格规模；单个同步 COM 调用仍可能阻塞，不宣称硬超时或全局观察原子性。

## 4. 输出协议

**stdout 不变量**:要么 envelope JSON,要么 `--help/--version` 文本,无第三形态。
诊断旁路两条,均不碰 stdout:① 结构化字段随 envelope 返回(`duration_ms`/`wake_ms`/
`waited_ms`/`resolved_by`/`via`/`input_effects`/batch `steps[]`——机器消费的常态数据);
② `ACTL_TRACE=1` 时引擎阶段耗时打点(`[actl-trace] locate.window 2036ms` 形态,stderr,
人类诊断按需启用;慢现场把命令总耗时分解到窗口解析/唤醒/树遍历/L4/pattern 各段)。scope 进入时先输出 `<stage> begin`，阻塞未返回时仍可辨认当前阶段；组合选择器额外区分桌面枚举、HWND 转换、候选查询与属性读取，详见 docs/65。

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
| `NOT_FOUND` | 选择器无匹配 | 用 find 放宽条件；evidence.reason=`virtualized_probe_rejected` 时为虚拟化探测被 provider 结构性拒绝（按缺席处理，可 snapshot 复核，勿原选择器重试） |
| `PERM_DENIED` | 权限不足(UIPI/管理员窗口) | 提示用户以管理员重开,勿重试 |
| `TIMEOUT` | wait/谓词或 UIA 提供程序明确超时（`UIA_E_TIMEOUT`） | 重估前置状态 |
| `AMBIGUOUS` | 匹配到多个元素(附候选 evidence,含可重放 @eN;**重复标签附 `context`**=最近具名祖先文本——所在行/容器,8 跳有界 120 字符截断;无具名祖先时为 null。唯一标签不带 context 控制证据体积) | 收紧条件 / `--near` 锚点 / `role:X#N` 序数 |
| `NOT_ACTIONABLE` | 元素存在但不可操作(offscreen/通道不支持),或动作准备期校验拒绝。**同码分型**（2026-10-13 Broadcom 门户轮收口：一码三态曾共用一句 message,只读 message 的 agent 会误诊）:`target_disabled`=预填锁定字段不可写(message="target is disabled at action time",写前可 `get --property enabled` 判读,重试无效);`target_identity_changed`=准备期身份漂移(message="target changed during action preparation",重拍快照重解析);display `version_mismatch`=companion 版本失配(hint 指向 display-stop/start 配对重启)。各分型 hint 独立给恢复路径 | 按 message/evidence.reason 选恢复路径,勿盲重试 |
| `ASSERTION_FAILED` | `verify`/`--expect` 断言未成立。**读回核验三态**（2026-10-13 VMware 密码框轮收口）：evidence.`readback`=`mismatch`（读回可读但不等，真失败——检查 expected/actual）；`empty`（读回恒空，reason=`readback_empty`——密码框/provider 背书 Edit 不暴露内容，写入可能已生效，hint 指向 screenshot 核验，勿盲重放）；通道不可读为 NOT_ACTIONABLE(`pattern_unsupported`) | 按证据三态选恢复路径:mismatch 查差异;empty 走截图/他通道断言;不可读换通道 |
| `PROTOCOL` | 协议/参数错误(不可恢复) | 修正命令参数 |
| `INTERNAL` | 内部/UIA/系统层错误(非协议非目标问题) | 重试一次;持续出现附 evidence 报 issue |
| `ABORTED` | 本调用停止代次已失效，或流程被明确暂停/停止 | 核验结果后新建调用或显式恢复流程；新写操作在开始/继续时确认，禁止自动重放 |

退出码:`0` 成功 · `1` 业务失败(可恢复) · `2` 协议/参数错误(不可恢复)。

## 5. ref 机制与元素身份

### 5.1 定位链 + fallback(RPA 借鉴核心)

1. `snapshot` 深度优先遍历,仅对**可交互**元素编号 `@e1..@eN`,scoped 到本次 `snapshot_id`;
2. 每个 ref 同时生成定位链,执行时逐条回退:
   - primary:UIA 路径(AutomationId / ControlType + Name 精确);
     `id:` 精确匹配走 provider 侧属性条件(UIA FindAll,单次往返;恰一命中直接用,
     多命中回退全遍历算 ref 候选——候选 ref 编号仅在多命中时需要);
   - fallback-1:Name 模糊匹配 + 相对锚点(父容器定位);
   - fallback-2:role + 同级序数;
   - fallback-3(L4):**虚拟化容器**——L1/L2 在已物化树中空手且目标可
     映射为精确属性(name 完整值 / AutomationId)时,询问窗口内 ItemContainer 容器
     (`FindItemByProperty`,provider 侧命中未物化条目)。**精确匹配,无子串语义**——`name:`
     进 L4 的值必须是完整名称(如完整文件名);命中元素不在快照树内,无 `@eN` ref,
     重放走同一 name/id 查询;`Role`/`@eN` 不参与(无属性查询语义);
3. 回退结果写入返回(`resolved_by` 字段,全集:`primary` | `fuzzy-name` | `role-ordinal` |
   `id-ordinal` | `anchor` | `ref-replay` | `item-container` | `provider-exact` | `focused` | `uia-composite`),全部失效才报
   `STALE_REF`(@eN)/`NOT_FOUND`(语义选择器);
   `id:<Id>#N`与 role 序数同机制——多文档页(iframe/扩展弹层)
   同名 AutomationId 的确定性寻址,不走 provider 快路径(序数需计数);
4. 重定位在同 snapshot_id 内自动进行(agent 无感),跨 snapshot 必须重取——契约简单严格,优于模糊缓存;
   **已知 hazard**:ref 校验的是窗口实例(RuntimeId)而非树内容,
   弹层/模态开合会插入/移除子树——若插入点在 DFS 序上先于既有内容,后续 `@eN`
   整体平移而校验照样通过(静默错位)。纪律:**覆盖层开/关后先重拍快照再用 ref**;
   树前缀比对(根治)留待完整会话设计合并。
5. **@eN 编号 = 全树 DFS 交互序数,与投影无关**:投影(skeleton/keyword/content)只过滤可见性、不改编号——投影输出中编号有洞(如 @e3、@e7),但同一窗口的任何投影与全量快照对同一元素使用**同一编号**;输出编号、持久化身份列表、`click @eN --snapshot` 的回放计数(find 同口径)三者对齐。跨投影 ref 混用因此安全(同编号空间)。

### 5.2 快照身份与输入焦点

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
  包括空串；换行在比较前折叠为 \n（\r、\r\n、\n 三种表示在剪贴板/UIA 提供方之间等价），语义为行内容精确比较、表示无关，不重复输入。成功新增 `focus_verified:true`，`value_verified:true`；
  未要求读回时后者为 null。读回不符返回 `ASSERTION_FAILED`（evidence.`readback`=`mismatch`）；读回**通道工作但恒空**
  （密码框/provider 背书 Edit，reason=`readback_empty`，hint 指向截图核验——2026-10-13 VMware 向导轮实证，写入可能
  已生效勿盲重放）；通道不可读为 `NOT_ACTIONABLE`；实例改变返回 `STALE_REF`。
  错误 evidence 含 delivered/value_verified/expected/actual/comparison（newline_folded_exact）；输入可能已经生效，调用方先核实，不能直接重放。
- `press <keys> --app X [--target <selector>] [--snapshot ID]`：`--snapshot` 要求 `--target`。
  指定目标时只断言其已拥有焦点，不切换窗口、不 SetFocus、不猜测首个 Edit；不符返回
  `NOT_ACTIONABLE`，尚未发键。目标歧义遵循现有 `AMBIGUOUS`，ref 可绑定快照身份。
  成功新增 `focus_verified`，有目标为 true、无目标为 false。原 `--expect/--timeout` 继续适用。
  无目标的 press 保持现有窗口级行为。
- `focus <target> --app X [--near <sel>] [--snapshot ID]`：**建立**键盘焦点,
  补 `press --target` 只断言不建立的空档(树对 name/id 不可见但焦点可达的元素,或 click 后焦点
  未落在预期控件的场景)。语义:UIA `SetFocus` 调用后读回核验(GUITHREADINFO 物理焦点根,3×120ms 退避),RuntimeId 与目标
  不符返回 `NOT_ACTIONABLE` 且 evidence 带 expected/actual(焦点去了别处,先读证据再决定下一步,
  禁盲目重放)。**零键注入**——只建立焦点,不发送任何键盘事件(input_effects.keyboard:"none"),
  与 type/press 的注入语义严格分离;后续注入由 press/type 既有护栏接管(verify-then-inject、
  前台复核)。目标歧义遵循 `AMBIGUOUS`;`--snapshot` 走 ref 身份校验链;成功 envelope 含
  `focus_verified:true` 与 resolved_by。flow 中定位为**写步**(改变焦点状态,非只读),verify 伴随
  要求与 click 同规。
- `set-value <target> <value> --app X [--near <sel>] [--snapshot ID] [--verify-value <expected>]`：
  Value pattern 直写（无键盘、无前台门）。**RangeValue 回退（2026-10-14 Slider 轮）**：Value pattern
  不可用时落 RangeValue——Slider/进度条类控件（交互角色白名单在册但此前无写通道）的数值语义写入：
  非数值 PROTOCOL、只读诚实拒绝、越界钳到 [min,max]（envelope 附 `range:{value,minimum,maximum}`，
  实际写入值可判读 step 网格钳制）；`--verify-value` 走精确 f64 比对（step 落差如实 mismatch 附
  expected/actual）；`get --property value` 同步三级回退 RangeValue（`value_via:"range-value"`，
  ms-settings 主音量滑块真机实证：此前写能进读不出的一致性缺陷）。可选 `--verify-value` 在原控件上读回核验（与 type 同通道：
  同实例身份复核、最多 2000ms 轮询、换行折叠精确比较；不符/不可读返回 `ASSERTION_FAILED`，实例改变返回
  `STALE_REF`，绝不重放写入；成功 envelope 带 `value_verified:true`，未要求时为 null）。**已知边界（2026-10-12
  Zotero 导出轮实证）**：provider 背书的系统对话框 Edit（如公共另存对话框文件名框）读回恒空——`--verify-value`
  会诚实失败而非假通过，那类目标改走 `type --verify-value`（键盘路径读回可达）；勿以 get 读回空判"写入丢失"。
  **宿主容器信号 `host_control`（2026-10-13 docs/83 Phase 2，VMware 硬件页实证）**：目标是 ComboBox 的子 Edit
  时 envelope 附 `host_control:"ComboBox"`——直写子 Edit 不一定被宿主采纳（Win32 形态焦点离开才提交
  ValuePattern）；值未生效先 `press tab`（焦点离开触发提交）或改用 `select`。非 ComboBox 子 Edit 时
  字段缺省（写入即生效）。
- 键盘沿用 §2 的显式 `--app` 授权；鼠标物理路径仍要求 `--physical`。

### 5.3 语义动作的执行前复核

click（Invoke/SelectionItem/Toggle/ExpandCollapse）、set-value、select、toggle 在预览前后核对组件 RuntimeId、PID、角色、AutomationId、名称，以及最近原生宿主的 RuntimeId/PID。祖先查询有界，provider 错误保留；无法确认宿主则拒绝。矩形只服务预览，布局移动不单独阻止语义操作。窗口语义动作复核窗口实例，不以矩形不变为条件。

执行前重新获取已选 Pattern，不因能力变化切换到其他动作或物理点击。组件禁用或身份改变返回 NOT_ACTIONABLE，evidence.reason 分别为 target_disabled / target_identity_changed，action_started=false；metadata 保留原因与送达事实，不包含组件正文。组件重建后需重新观察，不自动重放写入。键盘动作仍核验实际焦点，物理动作仍核验位置与遮挡。wait --stable 只证明连续内容样本一致，不证明可写或业务完成；动作后仍需业务后置核验。

## 6. token 控制

- `snapshot` 默认输出紧凑模式(role + name + ref + actionable,一行一元素);
- `--skeleton`:大 UI 折叠非交互容器(agent-desktop 实测 78–96% token 削减);
- `--view content`:内容域投影——Document 子树外的 chrome 整枝
  丢弃,浏览器/编辑器形态的主削减通道;与 `--skeleton` 组合时折叠仅在 Document 内部
  生效(chrome 交互元素不再强制保留——纯 skeleton 对浏览器仅削 ~36%);
- `find`/`inspect` 是"按需深查",鼓励 agent 先窄后深,反对全树 dump。

## 7. 安全边界

- 物理指针注入(真实移动/点击鼠标)必须显式 `--physical`,默认仅 UIA 后台调用;键盘注入的授权是强制 `--app` + 护栏 + 输入占用锁(见 §2 规则 2);
- **坐标寻址(`xy:x,y`)的守卫(视觉兜底闭环的"手")**:无元素
  可校验的物理专用通道——①强制 `--app` + 显式 `--physical`;②**窗口边界守卫**:
  注入瞬间坐标必须落在 `--app` 窗口当前 bounds 内(PMv2 物理像素,与 screenshot
  同坐标系),窗口移动/最小化即拒(防"截图后窗口漂移点进窗外");③复用遮挡
  检查(WindowFromPoint 命中窗口须属目标)。坐标来源是调用方:`find-image`
  (确定性模板匹配,核心内)或 screenshot+VLM/人(语义视觉,核心外);core 不做
  **语义**视觉(PRD §1.3 非目标不破——find-image 是像素相关非识别);click 家族 + hover 适用,drag 暂不含;
- 管理员窗口/跨完整性级别受 UIPI 限制时返回 `PERM_DENIED` 而非静默失败;
- 敏感命令白名单(clipboard-set 到密码框等场景)留给上层 agent 策略,CLI 不做内容审查,但 `--dry-run` 全局可用。

### 7.1 后台物理点击 `--no-activate`

指针族物理动作（click --physical / right-click / double-click / middle-click / triple-click / hover，
含 xy: 坐标寻址）可加 `--snapshot <id>`（ref 重放的新鲜度核验，与 click 同机制）与 `--no-activate`：注入前给目标窗口根 HWND 临时置 `WS_EX_NOACTIVATE`，
点击按坐标正常落地（命中/边界守卫照走），但**不激活、不抢前台、不改 Z 序**；结束后只清
自己置的那一位（期间其他 exstyle 位的合法变更不被吞掉），并把光标送回原位（drag 除外——落点有语义）。
`input_effects.activation` 上报 `suppressed|normal`。

- **键盘不适用（fail-closed）**：窗口无焦点时 type/press 会打进用户当前前台——press/type 无此旗标，
  传入即参数错误。后台化只覆盖指针；占键盘的操作仍需前台 + 信号条预告。
- **置位失败 → PERM_DENIED**（UIPI 拒绝提权窗口）：承诺不激活就不静默破坏承诺；提示去掉旗标重跑。
- **崩溃残留兜底（WAL journal）**：置位前先落盘 `%LOCALAPPDATA%\actl\no-activate-journal.json`
  （hwnd/pid/进程启动时刻）；任何物理路径入口（输入执行锁获取处）扫描 journal，**pid 已死或启动
  时刻不符（pid 复用）→ 清位 + 删条目**（清位前复核窗口仍归属该 pid，防句柄复用误伤）；pid 活着
  → 不动。恢复方向永远是"清位"——让窗口回到正常行为，是安全方向。
- 与 `wait --idle` 组合即"人不在时干活"的两件套（docs/09 §6.2）；扫描/采集类长任务的会话切换
  不再打断用户当前窗口。

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
   **唯一例外是 `display-stop` 生命周期动词**：优雅关闭本工具显示端（WM_CLOSE=普通关闭路径——撤销交接授权、发布 closed 租约、不写急停、不动停止代次），只接受经租约 pid 双核验的自家显示窗（不构成通用关闭旁路），幂等（不在场=already_stopped），有界等待 ≤4s 如实报 stopped:false。它是 AGENTS"显示端占用构建文件"常设授权的执行实体；显示端关闭期间写路径照常 fail-closed（副作用动作要求 ready 租约）。远端排除（MCP 排除表）：显示端生命周期归本地人。

附带修正：`key-up` 将 release 方向传递到输入执行体；带修饰键的物理 click 在
stdout `input_effects.keyboard` 如实返回 `injected`。

### 7.5.1 显示就绪与桌面整理（协议 v3+）

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

| 命令 | 参数 | data |
|---|---|---|
| display-status | 无 | ready, lease（可空）, stop_requested |
| focus-report | 无 | uia_focused（可空）, physical_focus_root（可空）, foreground（可空）, focus_root_processes_agree |
| display-start | 无 | ready, lease, stop_requested |
| display-stop | --reason <≤200字符> | stopped, already_stopped, reason, pid, waited_ms, stop_requested |
| minimize-window / restore-window | 标题、--pid、--hwnd 三选一 | hwnd, window, minimized, input_effects |
| desktop-prepare | --all 或重复 --hwnd | session, results[] |
| desktop-restore | --session ID | session, results[] |

桌面准备只最小化普通可见、非置顶、非工具、无 owner、有最小化能力的窗口；排除系统桌面与显示端。
更改前在 `%LOCALAPPDATA%/actl/desktop/<session>.json` 持久化身份（hwnd/pid/UIA RuntimeId/class）与
WINDOWPLACEMENT，操作后保存确认状态。失败 evidence 带 session 和已完成结果；恢复只处理仍处于
记录状态的同一实例，改变/关闭/未确认的窗口报告 skipped，恢复成功记录不会重复执行。
恢复窗口状态和位置，不承诺精确 Z 序或前台焦点，也不推断中间发生但最终回到同一状态的用户操作。
最小化并非关闭或保存文档。停止标志仍阻止桌面整理及恢复，用户需显式恢复后再调用。

错误：PROTOCOL 参数或记录不合法；NOT_FOUND 窗口/会话不存在；AMBIGUOUS 目标不唯一；
NOT_ACTIONABLE 显示或窗口未就绪；ABORTED 用户停止；INTERNAL 持久化或原生调用错误。

### 7.5.2 桌面交接与前台资格（协议 v4+）

工作流资源初始化契约见 docs/20 的“声明式资源初始化”：resources 存在时展开输入默认值中的受限运行上下文，完整预检后由 flow-run 创建独占资源目录，flow-resume 不重新初始化。flow-validate 在原 envelope 的 data 中增加 resources 清单（仅资源模板返回），使用 validation-preview 上下文且不创建文件；普通模板输出不变。

任务交接策略（显示协议 v8）：flow YAML 可增加 `handoff: {approval: once_per_run}`。
调用方若已获得用户明确的代操作授权，可通过 computer use 核对任务后实际点击可见的开始/继续，具体边界见 docs/32；这是操作者约定，不改变以下协议、授权生命周期或结果核验，也不提供程序化跳过确认参数。
省略时保持 `expiring`（空闲 60 秒、总长 10 分钟）。once_per_run 在同任务内不因这两种超时失效，
首次仍需用户确认，人工接管、暂停撤销、停止代次变化、显示端重启和监听故障仍撤销授权。
策略变化必须重新确认，不能升级已有授权；窗口、焦点与物理输入参数要求不变。
普通脚本通过 `ACTL_TASK_CONFIG` 指定 YAML 文件，格式为 `version: 1` 加同样的 `handoff`；
必须同时设置每次运行唯一的 `ACTL_TASK_ID`，不同运行不得复用 ID。配置严格拒绝未知字段、重复键与未知策略，
不存在 authorized/跳过确认字段。授权仅在显示端内存中，文件不保存授权。flow 以保存的 plan 策略为准。
完整规则见 [32-desktop-handoff.md](32-desktop-handoff.md)。

动作反馈门禁在展示动作前要求人工点击任务交接窗口，宽限期未响应（默认 5s,`ACTL_HANDOFF_GRACE_MS` 可调,2026-10-12 从 25s 收紧——CLI 不陪等人类;flow 转暂停+显示端「继续」恢复）返回 NOT_ACTIONABLE；ACTL_TASK_ID / flow ID 范围内
可短期复用，输入活动、60 秒空闲、10 分钟总长、急停及显示端重启撤销。无任务 ID 只限单调用。
物理鼠标仍须显式 --physical。只读观察不唤醒窗口。
错误 evidence.stage=handoff，reason 为 handoff_pending / user_deferred /
human_takeover_or_grant_expired / companion_missing / companion_unresponsive /
request_rejected / missing_session / handoff_panel_unavailable / input_monitor_unavailable；retry_input=false。
flow 因交接中断保存 paused，写步骤 uncertain，恢复先验证而不自动重放。
分段注入失败保留 delivered_before_error（已确认的前缀）及 failed_segment_size。

**前台资格链**：focus-window 成功须验证原生前台根 HWND；错误按 window_prepare 阶段报告具体 reason，
前台激活超时附 `foreground_before`、`activation_requested`、`activation_returned`（未请求时为 null）、`caller_pid`、`waited_ms`。
前台锁被拒（调用进程非前台进程，全屏窗覆盖/Electron 抢回形态）时执行一次最小化→还原强制激活
（窗口管理器合法路由，与 restore-window 同机制，非按键合成），失败证据附 `min_restore_retry:true`（2026-10-14 微信 AppEx 窗实证）。
`activation_returned` 是 SetForegroundWindow 的原始布尔结果，不等于最终前台核验结果；不读取该 API 未定义的 LastError。
显示端只在任务授权通过后向同目录 actl.exe 的当前调用 PID 请求 AllowSetForegroundWindow；每调用一次，失败不伪装成拒绝任务授权或成功激活。history 的 foreground_transfer 记录 request_call/caller_pid/accepted/failure。Windows 前台资格可随输入失效，不能据 once_per_run 推定永久可激活。
同目录执行器身份比较支持规范化后的同文件路径，解析失败不放行不同路径。flow 默认 metadata 错误证据保留 window_prepare/window_discovery 的阶段、已知原因、HWND、激活结果及 process_started/relaunch；不保留窗口标题或输入正文。
显示协议 v9：动作 guard 在最外层退出时（包括失败路径）向核验过的显示端 PID 归还前台资格，history 记 foreground_return。进程内下一动作通过 FOREGROUND_MESSAGE 重新检查当前任务授权并领取；嵌套动作不提前归还。旧显示端必须正常替换，不能混用。
CHECK_MESSAGE 查询超时允许在复核停止代次后对同一显示窗口重查一次，每次上限 500ms，记 handoff_query_retry；不重试开始请求、资格请求或任何桌面动作。最终失败 evidence 附 query_message/query_timeout_ms/query_return。
前台不匹配属于 NOT_ACTIONABLE，不代表权限不足。

launch 新增 --wait-window <标题子串> 与 --timeout <1..60000 毫秒>（默认 5000，要求前者），
选项置于程序之前。默认结果新增 window_ready:false；等待成功新增 window 对象
{hwnd,pid,title,foreground:true,matched_by:"title"}，window_ready:true。
匹配不证明窗口来自新 PID；失败 evidence 保留 spawned_pid/process_started/relaunch:false。
不自动重启、重复注入、清急停或跨权限强行激活。

display-status 新增 reason/hwnd/responsive/popup，ready 同时校验真实可见窗口、进程身份及租约。
focus-report：只读诊断，三视图并列——UIA 焦点元素（name/role/automation_id/process_id）、物理焦点根窗（GetGUIThreadInfo→GA_ROOT 的 hwnd/title/pid）、物理前台窗（hwnd/title/pid），附 focus_root_processes_agree（两视图进程一致性的事实信号，非裁决）。用途：元素选择与焦点争议的交叉比对（WinUI 上 UIA focused element 实测会谎报，以物理层为权威；判定树见 docs/31）。观察命令，不强制启动显示端。
front-report --app <窗口>：只读报告前台引导层——原生 owned 可见弹窗（标题/可见/前台）与树内子窗表面（文本全文+按钮清单），附朴素分类提示（error/save/confirm/update/unknown，不裁决）；front_clear=true 表示无引导层。观察命令，不点击任何东西。判读纪律见 docs/31 前台弹层优先。**锚窗非前台时走原生快门**：in_tree 置空并附 `in_tree_skipped=anchor_not_foreground`——树内面板用户不可见，不构成"最前方的弹窗引导"语义（docs/31），全树 walk 在后台懒激活态撞 provider 慢墙；owned 原生弹窗层照报（廉价枚举不经 provider），并自 2026-10-13 起携带**子窗文本 texts/按钮 buttons**（EnumChildWindows+GetWindowText 直采——#32770 类弹窗全文在子窗 Static/Button,owned 只带标题曾是 decide 判读置信贫困的根因,A/B 实证同崩溃框 conf 0.32→0.622 跨门槛）；前台态恢复自动回全量路径。
popup 读取 popup.json，但记录的 pid 已退出时视为陈旧证据返回 null。
display-start 可对兼容显示端无激活恢复及位置校正；版本不符仍拒绝并保留停止状态。
显示 v4 租约 golden 替代 v3。
wait --window 的标题出现轮询改用原生可见顶层窗口枚举，避免 UIA provider 阻塞协作暂停；不激活窗口，失败不伪装成空列表。

**授权余韵宽限与快捷键**：授权动作自身的余韵（点击/按键的抬起、手部后摇）
不视为外部接管——`Consent` 自授予起 750ms（`GRACE_MS`）内允许输入序列号变化并重设基线，
窗外恢复严格相等判定。授权通道含全局快捷键 **Ctrl+Alt+Y**（与"开始/继续"按钮等价；
Ctrl+Alt+F12 仍为紧急停止），快捷键注册被占用时静默退化为仅按钮。

### 7.5.3 停止代次与暂停恢复（协议 v5+）

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
两条管理命令不能成为持久 flow 步骤。v6 显示端不能与新 CLI 混用。
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

### 7.5.4 预授权写入（协议 v10+）

新增 `Approval::Standing`（常驻信任）与两种用户授予通道，目标是在不引入"隐身执行"的前提下消除重复点击：

- **显示端托盘开关「预授权写入」**：用户点开即对所有桌面写入生效（持久，`state/preauth.json`，由显示端 UI 独占写入；CLI/agent 无 sanctioned 写入通道）。开启后 REQUEST 即刻按 Standing 授予并返回 ALLOWED，不出卡；显示端横条常显「预授权·」前缀作为可见性补偿；关闭开关立即撤销常驻同意。
- **流程声明 `approval: standing`**：声明只是意图，不构成授权——首次仍需用户点一次「开始」，之后本会话内该流程常驻（不因闲置/时长/外部输入撤销）。与 `once_per_run` 的区别：跨调用任务切换与输入让行后无需再次点击。
- Standing 的 check 语义：不过期；输入计数变化时**让行而非撤销**（reason=input_active，基线重置，安静即自动通过）；急停、监听不可用、代次变化照常杀死信任。审计记录 reason=preauthorized。

安全不变量：急停在任何模式下立即生效；动作预告照常上屏；授予动作只能来自显示端 UI 或用户点击（YAML 声明永远不自动生效）。actl 与 actl-signal 需成对升级。按流程指纹（冻结计划哈希）持久信任的「始终信任此流程」为后续方向（ROADMAP）。

CHECK 响应含 `YIELD_BUSY`——Standing 授权仍在、仅输入活跃时返回，客户端有界等待安静（3 秒/150ms 轮询、急停即时、超时按让行失败并带 waited_ms 证据）；授权撤销/过期维持 YIELDED 立即失败。

### 7.5.5 信号条文案规范

显示层（actl-signal）全部用户可见文案遵循以下规则；新增 reason 枚举必须同步 `ui_text::reason` 映射（交付清单项，缺失即落"需要处理"泛化兜底视为缺陷）：

- **分词**：**核验**=机器动作（verifying→"核验中"、verified→"有核验记录"、"恢复核验未通过"）；**核对**=人的动作（needs_review→"待核对"、指引中的"请核对"）。不混用。
- **横条后缀约定**："· 可操作"=用户可正常使用桌面；"· 暂勿操作"=正在注入桌面写入。
- **复合 reason 前缀族**：popup_pending:\<step\>/ambiguous_target:\<step\>/budget_exhausted:\<子类\> 按前缀匹配给专属文案（"前台有弹窗 · 处理后继续"/"多个候选 · 待指定目标"/预算提示），不用泛化"待核对结果"——这三态的正确动作不是核对结果。
- **停止族标准句**："旧调用不会自动恢复；可核对后继续此任务，或发起新任务。"——stop_requested/流程已停止各处统一；run 级停止="流程已停止"，执行级="急停已启用"，两概念文案刻意区分。
- **reason 是纯枚举**：不在 reason 值里拼接提示长句（提示归显示层映射与调用方 hint）。
- **继续按钮三元规则**：needs_review→"核验并继续"；paused+action_failed→"已修复，重试继续"（点继续会自动重发该步动作）；其余暂停→"检查并继续"。按钮文案说真话，不泛化为单一"继续"。
- **英文屏蔽**：执行器返回的英文 envelope message 在横条折叠为"操作未完成，请展开技术详情"，错误码进详情层。

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
actl history --task <id> --timeline [--limit <1..2000>]  # 单一时间线，默认保留最新 20 条事件
actl task-archive --file <report.json>          # UTF-8 JSON，最大 64 KiB
```

history 的 data 为 `{calls:[{complete,integrity,events:[...]}],incomplete_files,logging_health}`。
complete 要求 JSON 可解析、身份和版本一致、seq 连续且调用首尾边界有效；完整性在步骤过滤前检查。
integrity 区分 complete、unfinished、malformed_json、empty、missing_start、invalid_identity、
identity_or_version_mismatch、sequence_gap_or_duplicate、invalid_boundary。
unfinished 不证明进程仍在运行或已经崩溃。incomplete_files 统计本次扫描到的空/损坏文件及
返回的无终态调用；过期记录可能尚未清理。非法 ID/limit 返回 PROTOCOL，读取失败返回 INTERNAL。
`--timeline` 把任务（或全部，不建议）的 journal 事件按 ts_ms/seq 合并为单一时间线，
data 为 `{events,total,truncated,incomplete_files}`，旧事件在前，超限时保留最新窗口；
指定 task 时先核对有效的调用首条身份，非目标调用不继续解析尾部；incomplete_files
统计目标调用以及首条无法判定归属的文件，无 task 查询仍检查全部文件。
有效首条的 task_id、文件大小及修改时刻缓存在 `history/cache/task-headers.json`（最大 8 MiB），
后续按 task 查询可跳过未变化的非目标文件。文件变化、未知首条、缓存缺失/损坏均回到原始日志；
目标调用始终读取原始事件。缓存仅加速读取，不提供完整性或防篡改保证，不修改原日志或执行状态。
用于重建暂停、交接与步骤失败过程，免去手工拼接多个调用文件。显示端触发的执行器通过
`ACTL_TRACE_FILE` 把阶段 trace 追加写入 `history/executor-<request_id>.trace`，stdout 契约不变。

`history --step <步骤>` 与 `--reports` 均要求 --task；先按步骤过滤再应用调用条数限制，
返回匹配步骤事件及 call_started/call_finished。流程上下文事件含 run_id/step_id；
data.action_id 在 call_id 内唯一。`--reports` 返回任务级报告及查询时的证据可用性，不代替业务核验。
task-archive 支持 artifacts、feedback.kind/evidence_ids；报告分类和引用检查见 docs/19 §6。

UIA 观察默认增加 `observation_started` / `observation_finished` 和预算达到时的
`observation_budget` 事件，使用原有 call_id/task_id/run_id/step_id 关联，不新增命令。
每调用前 32 个观察区间保存开始和结束，之后最多保留 8 个耗时 ≥250ms 或失败的结束事件；
`started_recorded:false` 表示该区间未保留开始事件，不代表调用没有执行。
阶段为 window_inventory/automation_initialize/desktop_root/children/find_all_build_cache/array_length/array_elements/window_handle/window_role/window_properties。
children 查询另含 query_scope（desktop_children 或 element_children）；原生 FindAllBuildCache、
数组 Length 与逐项 GetElement 分别记录首尾，children 保留外层区间。完成事件另含
result_count/processed_count/failed_index：数组长度读取成功后才知总数，转换阶段记录已取得的元素数
及零起始失败索引，无适用值或未知为 null，成功空数组明确为 0。原生查询阶段不额外读 Length
填计数，目标身份仍只从已有缓存和原生查询取得。
叶阶段计时排除自身诊断采样与写日志，外层计时包含嵌套诊断成本；嵌套阶段不可相加。
这些区间共用既有 32/8 预算，预算外失败仍保留最内层错误阶段链接，不保证所有成功区间完整留证。
数组负长度返回 INTERNAL，不把无效提供程序数据作为空树；GetElement 失败不返回已转换的部分结果。
事件含 span_id、已缓存的目标 HWND 及其原生 PID（未知为 null）、duration_ms、outcome、error_code/native_code；
前后端点含 ts_ms、foreground_hwnd/foreground_pid、last_input_tick 及输入诊断采样。
session_input_changed / foreground_changed 是端点差异（观察失败为 null），不证明人工操作或错误因果。
错误 evidence 增加 observation_stage/observation_span_id/observation_duration_ms，保留最内层失败，
flow 默认 metadata 仅放行规定阶段及无符号数值，不保留输入或错误正文。

显示端后台每 200ms 替换 `observation-input.json`（独立诊断采样 v1，非显示授权协议），
含显示端 pid/epoch、ts_ms、输入监听健康和 device/foreign_injected/own_injection 的 signals/last_ms。
Hook 与 Raw 可重复观测同一次输入，signals 不是键数或点击数；不记录键值、坐标、设备句柄或业务正文。
读取校验显示端身份、版本和 <1500ms 新鲜度；missing/unreadable_or_invalid/invalid/stale/
display_unavailable_or_changed/monitor_unavailable 不作为“没有输入”的证明。
sampled_input_delta 仅在两个健康采样身份一致且计数未回退时提供，from_ms/to_ms 是采样区间，
可滞后于观察边界；零差值和不变的输入 tick 均不能证明用户未操作。
采样与日志故障不改变命令结果或授权，写入/线程启动失败尽力记 log-health；
诊断不改急停、暂停边界、授权或重试，不强行取消同步 COM 调用。

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

默认保留：history 7 天/3 GiB，tasks 30 天/32 MiB；单调用日志最大 1 MiB。
history 全目录清理检查每 15 天一次，CLI 与显示端后台共用跨进程间隔标记；首次、标记损坏、
时钟回退或前次清理失败时可重新检查。7 天是检查时的过期判定，不保证到期立即删除；
3 GiB 是检查时的容量预算，不是实时硬上限，近期与活跃文件保护仍生效。
CLI 写入前与显示端后台共用 history 检查，任务报告仍在归档写入前机会式清理；保护最近一小时
文件及具有新鲜运行心跳的调用。并发/在途增长可暂时超限，无法释放容量时报告失败；不清理 desktop 恢复记录。
日志不保证防篡改或断电完整性。完整设计见 [19](19-history-and-user-experience.md)。

### 流程事件字段

执行型 flow 的 call_finished 在已有调用 phase/error_code 之外增加 workflow_status、
workflow_completed、workflow_reason、workflow_error_code。done 只表示调用处理完毕；
是否完成流程以 workflow_completed 为准。没有可用流程上下文时不输出这些字段。
workflow_invocation 逐次记录 run/before/verify/resume_check 的 started/succeeded/failed、
attempt 和错误码，继承 call_id/run_id/step_id；不含命令参数和业务内容。
flow_resume_requested 记录请求 run_id、retry_step 及来源 cli/signal_control/unknown，
source_verified=false。显示端通过 ACTL_FLOW_REQUEST_SOURCE=signal_control 标记子进程；
该信息仅供诊断，不证明用户点击或授予权限。未标记旧显示端也会归入 cli。

## 7.7 YAML 顺序流程 v1

flow-v1 Schema：`schemas/flow-v1.schema.json`；字段诊断含 `error.evidence.field/step_id`。
完整字段、状态与边界以 [20](20-yaml-workflow-protocol.md) 为本节规范性扩展。

有界观察恢复扩展：步骤 `recovery: {max_retries, timeout_ms, interval_ms}` 与 retry 互斥，仅额外重查可恢复的只读观察；预算跨暂停/重启持久化，继续不重置。StepRecord 增加可选 recovery 及观察事件，历史增加 workflow_recovery；参数范围、错误分类、同步超时限制和 UI 语义以 [64](64-bounded-observation-recovery.md) 为规范性扩展。

简单条件扩展：步骤可声明互斥的 `when`/`unless` 即时只读断言和可选 `otherwise` 动作，未选中且无备用路线时跳过。两条路线均预检；条件步骤不声明 outputs。成功为真，ASSERTION_FAILED 为假；严格 UIA 存在性断言仅在已确认窗口/容器、完整零命中时将带明确查询证据的 NOT_FOUND 判假，其他观察错误以 condition_observation_failed 停止。branch=run/otherwise/skip 在派发前持久化，未派发恢复重判，不确定步骤保持原路线先核验。完整字段、执行顺序及模板见 [63](63-simple-workflow-conditions.md)。

`verify-window --app <标题片段|hwnd:句柄> --state <exists|absent|foreground|minimized|visible>`：成功 data 为 `{action:"verify-window",state,matched:true,window}`，window 为 `{hwnd,pid,title,foreground,minimized,visible}` 或 null；已知不成立返回 ASSERTION_FAILED，evidence 保留对应 matched=false 数据。多匹配返回 AMBIGUOUS，查询错误原样传播，无匹配仅 absent 成立。窗口存在与前台判断不证明资源归属、无遮挡或进程退出。

新增 flow-validate/run/status/pause/resume，完整字段、状态与边界以
[20](20-yaml-workflow-protocol.md) 为本节规范性扩展。

### 重新观察与恢复核验

`retry: stale_read`：只读调用遇到 STALE_REF 最多调用三次，写调用始终单次；
state.steps[].observation_retries 保存阶段、错误码和重试序号（最多 32 条），暂停/急停阻止后续重试。
可选 `resume_check`（verify/wait）：恢复时每个未完成步骤先核对上下文，再核验 uncertain 结果。
失败保存 resume_check_failed，保留原步骤阶段；不会自动导航、切换目标或重新授权。
恢复写步骤的结果核验若无法观察，保存 needs_review/result_observation_failed，
即使指定 --retry-step 也不重放；只有明确 ASSERTION_FAILED 加显式重试才可进入写步骤。
完整约束见 [20 §9](20-yaml-workflow-protocol.md)，实现与验收矩阵见 [39](39-flow-observation-recovery.md)。

紧邻检查复用：恢复执行时，只有仍为 pending 的步骤，且解析后的 resume_check 与 before 完全相同时，成功的恢复检查才直接满足此次前置检查。两者之间仍检查暂停/停止；不跨步骤、跨暂停或跨执行器缓存。uncertain/running/verifying 步骤仍先核验结果，显式重试前重新执行 before。不同参数或不同命令不合并。

### 输出绑定

可选 outputs（只读 run 的 JSON Pointer＋显式类型）与 bind（前序步骤输出替换完整参数）。
字段和实际 CLI 参数分别预检，输出随步骤状态持久化，恢复不重跑已完成步骤；写步骤不导出数据。
state/run 响应的步骤可带 outputs，显式保存的业务数据不受 error evidence 脱敏策略影响。
缺失/类型/绑定错误为 PROTOCOL；绑定解析失败保存 paused/failed、reason=binding_validation_failed，动作次数不增加。
输出可声明正整数 `max_age_ms`；步骤保存 `outputs_captured_ms`（读取调用开始时间）。
消费步骤入口与每次 run 调用前检查有效期，超限、缺失时间或当前时间早于采集时间时，
保存 PROTOCOL 与 reason=output_freshness_failed，按 on_failure 暂停或失败，保留已有步骤阶段。
诊断只包含输出名称、来源步骤和时间，默认 metadata 也保留；不自动刷新或重放已完成步骤。
完整语法、预算、负号参数限制及恢复语义见 [20 §8](20-yaml-workflow-protocol.md)，测试记录见 [38](38-flow-output-bindings.md)。

### 可复用流程输入

`flow-validate` 和 `flow-run` 支持 `--inputs values.yml`（UTF-8 YAML 或 JSON 字符串映射）。
流程顶层 `inputs: {app: {}, text: {default: hello}}` 声明必填参数或字符串默认值。
四种调用阶段的 args 可用 `{input: app}` 替换一个完整参数，不做 shell 求值、拼接或环境变量展开。
缺少必填值、未知参数、重复键、非字符串、NUL、超过 16 KiB 或以 `-` 开头的输入返回 PROTOCOL。
最多 128 个声明；名称沿用 1–80 位 ASCII ID。所有解析后的命令仍须通过 CLI 和流程策略预检。
仅参数值可绑定，command、策略、步骤 ID 等不可参数化；命令选项应明确写在流程中。

`--set key=value`（可重复，flow-validate/flow-run/flow-revise）内联覆盖声明输入——换词重跑免值文件：
与 `--inputs` 值文件合并后 **set 优先**（同名键 set 赢）；缺等号、空键、`--set` 同键给两次返回 PROTOCOL；
未声明键与值约束沿用 inputs 统一裁决。覆盖值不落盘，随 run 计划冻结。

```powershell
actl flow-validate --file skills/actl/workflows/verify-text.yml --inputs target/readback-inputs.yml
actl flow-run --file skills/actl/workflows/quark-search.yml --id search-01 --set app=夸克网盘 --set query=今日天气
actl flow-run --file skills/actl/workflows/verify-text.yml --inputs target/readback-inputs.yml --id readback-001 --paused
actl flow-resume --id readback-001
```

运行前解析并保存具体 plan，保存内容没有 inputs 或绑定对象；恢复不重读模板/参数文件。
参数仍会进入本地运行计划，不能因错误证据脱敏而认为计划也已脱敏。
独立测试文件、Office 种子文档及 profile 的准备仍由验收工具负责；引擎不执行任意 PS。

### 操作组与只读断言族

`run: {sequence: [...]}` 将 1–16 个结构化 invocation 作为一个持久化步骤。每个子调用先预检，禁止嵌套、batch、管理命令、按键按住和子调用输出绑定。必须有最终 verify，禁止 read_only 重试及 outputs。组内沿用同一 task/flow ID，在每个子调用前检查停止与暂停；不是事务，没有回滚。

组开始前写入 running；任何已送达写入使整个组成为 uncertain。后续子调用即使在注入前让行，也不能将整个组改回 pending。恢复先执行 resume_check，再核对最终结果；完成则跳过，部分完成/未知保持待核对，不自动重放。仅明确的 --retry-step 可重放整组，界面继续不携带该参数。workflow_sequence 日志记录子调用序号、命令和错误码，不记录输入正文。

`flow-validate --emit-plan` 可返回解析、预检后的 plan，包含完整输入值。资源准备工具可将多份计划前缀化、合并并再次预检；不是运行中的嵌套流程。保存后的 plan 固定，不跟随源模板变化。

只读断言（可用于 before/verify/resume_check，以及只读 run）：

- `verify-file --path <path> --kind text|json|docx|docx-styles|meta|xlsx (--expected <JSON> | --expected-file <utf8文件>) [--subset]`：文本按 CRLF→LF 读取；JSON 按类型比较；docx 返回正文 text 与显式加粗 bold；docx-styles 返回逐段 `{"paragraphs":[{text,style,bold}]}`（style 为 w:pStyle 的 OOXML styleId，无显式样式为空串即 Normal；bold 为段内"有文本 run 且全部显式加粗"，不推断样式继承）；**meta 返回 `{"size","mtime_unix_ms"}`（元数据读取不受 Office 排他锁影响，是文档打开期间"保存确实落盘"的唯一断言通道），expected 支持关系键 `size_min`/`mtime_min`（unix 毫秒，单调下界断言；关系键不参与通用比较，剔除后空对象+subset=纯关系形态）**；xlsx 返回 sheet1 单元格的 value/formula。**--expected-file 从 UTF-8 文件读期望（与 --expected 互斥，二选一必填）——长/中文 JSON 经 shell 传参的引号/编码坑类消灭（docs/81 §8）**。subset 仅忽略未声明的对象键，数组长度及顺序仍须一致。流程中的路径必须为绝对路径。docx 的 bold 指每个含文本 run 均显式声明加粗，不推断样式继承。**--discover <通配>（2026-10-12 增补）**：--path 为目录时，取其中匹配锚定通配（仅 `*`、大小写不敏感、跨点号）的 mtime 最新文件断言，envelope 带 `discovered` 实名——保存对话框按类型过滤器补扩展名（输入 `.csl.json` 落盘 `.csl.json.bib`），按预设精确名断言会静默落空（Zotero 导出轮实证），产物断言走发现形态；无匹配报 `ASSERTION_FAILED`（io_kind=no_match），与"文件可能稍后出现"同语义、参与 5s settle 窗。
- `verify-rows --app ... --prefix <parent-id>-row- --expected <JSON string array>`：完整快照中必须有唯一父树，比较 TreeItem 行名集合（排序后保留重复项），截断或父树缺失拒绝。
- `verify-owner --app ... --program <absolute exe> --token <absolute task path> [--instance ... --hwnd ... --pid ...]`：核对唯一可见原生窗口、进程可执行路径、命令行中的完整任务路径参数，并返回 HWND、PID、进程创建时间。可选身份字段必须全部匹配；不把标题匹配单独当归属证据。返回含 `hwnd_anchor`（`hwnd:<n>` 就绪字符串）——outputs/bind 直填后续任意 `--app` 参数槽,动态标题窗口（标题随曲目/脏标记变）静态计划锚定无须标题插值：**归属步产锚 → 后续步绑锚**的惯用法。
- `verify-closed --hwnd ... --pid ... [--instance ...]`：观察原窗口和原进程实例；窗口消失但进程仍在不算通过，PID 复用也不对新进程执行操作，状态矛盾/查询失败保留未知。**--instance 可选三分支（2026-10-12）**：pid 消失→决定性通过（复用 pid 属别的进程，与 instance 无关）；窗口仍在→决定性失败（原实例必然还在）；pid 活而窗不在→NOT_ACTIONABLE 求签（复用与无窗原实例不可区分，instance 取 verify-owner 或 close-window 信封）。
- `verify-clipboard --expected ... [--normalize-lines]`：精确比较剪贴板文本；显式规范化转换 CRLF 并去除尾部 LF。

文件/行集合/关闭断言对已知不匹配在 5 秒观察窗口内复查，未知错误不转为空或成功。文件内容上限 4 MiB、单个 XML part 上限 4 MiB、expected 参数上限 16 KiB。OOXML 由 zip 解压指定 XML part，再经 Windows.Data.Xml.Dom 只读解析；进程身份通过 Win32 句柄读取，完整命令行通过 WMI 读取。运行时不启动 PowerShell，不使用 Office COM。XML 禁用 DTD/外部实体，最大深度 128；拒绝重复/缺失单元格地址及越界 shared string 索引。进程退出检查不读取命令行。WMI 枚举等待上限 5 秒，但连接/COM 调用没有独立可中断的硬超时，因此整个断言仍不保证 5 秒硬实时。依赖与 unsafe 登记见 TECH_STACK §3。

### 计划治理

**flow-status 陈旧计划提示**：`flow-run` 冻结计划时旁车记录源模板字节指纹（`flows/<id>/source.json`：路径 + FNV-1a 双通道指纹；记录失败不阻断）。
`flow-status` 的 data 增加 `template` 字段：重读模板重算比对——不一致时
`changed:true` 且 hint 提示"模板已改，重试仍跑冻结的旧路线；验证新修复需新任务"；
早于本特性创建的流程无 source.json 时为 null（零噪音）；模板不可读时
`changed:null` 并说明原因，不臆断。

**flow-clean**：`flow-clean [--older-than-minutes N] [--dry-run]` 批量归档历史非完结流程
（paused/needs_review/中断态）。默认仅处理最后更新早于 60 分钟的流程；
`completed` 不动（保留在 flows/ 作证据保留窗）；running.lock 记录的存活执行器
在跑的不动（进程已死视为过期锁）。归档即整目录 `rename` 到 `flows-archived/`，
全部证据保留、不删除任何文件；`--dry-run` 仅列候选不移动。
输出 data 为 `{archived, kept, errors[], detail[], dry_run, older_than_minutes}`。
非 flow 目录与损坏状态跳过不报错。

**flow-revise**（计划版本化修订）：`flow-revise --id <id> --file <新.yml> [--inputs <值.yml>] [--message] [--accept-rerun <step>]... [--resume]`——--file 收完整模板形态（resources 块经 prepare 剥离、`{"input":...}` 引用经 defaults 或 --inputs 解析、pack 级 popup_ignores 注入与 load_plan 同口径）。新 YAML 解析后与冻结计划做**步骤级 diff**（按位+id 对齐：unchanged/modified/added/removed，规格相等 = 规范化 JSON 一致）。处置政策（全部显式）：完成步规格未变 → 结果保留（StepRecord 带 `plan_version` 溯源，Run 记录当前 `plan_version`）；完成**读步**规格已变 → 重置 Pending（无副作用）；完成**写步**规格已变 → 必须 `--accept-rerun` 点名否则 PROTOCOL 列名拒绝；uncertain 步规格已变 → 重置 Pending——**修订本身即该步的重试授权**；uncertain 规格未变 → 保持（仍走 `--retry-step` 原语义）。持久化序：旧计划字节归档 `revisions/vN.plan.json`（永不覆盖）→ 新 plan.json → 新 state → `revisions/index.json` 追加版本事件（version/fingerprint/message/diff/revised_ms）；flow-status 展示版本谱系（revisions 数组）。默认修订后**不自动续跑**（`--resume` 才链式恢复，同 flow-resume 的停止/锁检查）；续跑前置由既有 freshness/resume_check 重立。探索 run 拒绝修订（PROTOCOL）。调节粒度 = 步边界（不做指令级打断）；冻结计划四不变量保持——「冻结」升级为「版本化」。

**ensure 幂等**：`ensure: {check: <只读调用>}` 步骤——Pending 步 check 通过 → 直接 Completed+verified，run 不派发（重跑/修订续跑的环境准备前缀自动短路）；不通过 → 走正常 run+verify；check 观察错误如实传播（与 when/unless 同族）；与 outputs 互斥（满足路径不产出绑定值）。

**AMBIGUOUS 目标串联（pick）**：run 相定位**多命中**（AMBIGUOUS、目标在 args[0]、有 `--app` 锚、证据含候选）时不走通用失败语义——引擎立即锚定一份**骨架快照**（候选 ref 的身份来源），候选证据**全量保留**（不走默认 redaction——候选是应答面），run 进 needs_review（reason=`ambiguous_target:<step>`）、步保持 **Pending**（locate 拒绝=零派发，无 uncertain 语义）；快照失败则落回通用失败语义。应答走 **`flow-resume --id <id> --pick @eN`**（与 --decision/--retry-step 互斥，探索 run 拒绝）：引擎校验 ref ∈ 候选集（唯一待应答暂停）后置位 picked，续跑时 args[0] 重写为选中 ref 并追加 `--snapshot <锚定 id>`——身份核验交给既有 ReplayGuard（窗口重建/元素漂移即 STALE_REF 拒绝派发，不消费重写）；步骤目标经修订已变（陈旧 pick）→ PROTOCOL 引导重新判读或 flow-revise。flow-status 附 **ambiguous_hint**（候选清单+可复制应答命令）。pick 是**运行时纠正**，不改 plan_version、不触碰冻结计划四不变式；步骤的 AMBIGUOUS 记录随修订重置不迁移。**自动缩小查询不做**（引擎侧静默改写选择器违背冻结计划的显式变更纪律，缩小路径 = 调用方 pick 显式选择或 flow-revise 显式修订）。

**for_each 数据驱动迭代**（列表驱动的有界循环，非通用循环）：`run: {for_each: {from: "stepid.output" [, max: N] [, field: name]}, body: [<调用>…]}`——迭代对象**必须是前置步骤 outputs 保存的 array**（前向引用 PROTOCOL 拒绝）；每项执行一轮 body（1..=16 子调用），子调用 args 中以 **`@each` 子串占位**（`name:@each` 派发时替换为项标量；`field` 给定时从对象项取成员，缺省项即标量；项值守卫与 bind 同规——非选项/非瞬态 ref/无 NUL/≤16KiB）。**有界纪律**：迭代次数=列表长度，`max`（1..=256，默认 32）显式兜底，超限 PROTOCOL **不静默截断**；项间检查停止/暂停（看门狗心跳在项边界）；迭代体只许语义选择器（占位替换值守卫拒绝 `@eN`——每轮重新观察，不携带跨轮 ref）。约束：for_each 步禁 outputs、禁 `retry: read_only`、verify 必填（每轮后置核验走外层步 verify）；body 禁嵌套 for_each/sequence、禁 bind。失败语义：第 i 项失败 → 步 Uncertain（写）/Failed（只读），`for_each_item`/`for_each_child` 位置证据随错误携带——已完成项的重放幂等依赖调用方在 body 内配 `ensure`/幂等设计。冻结计划四不变式全保留：for_each 是静态结构，列表是运行时数据。嵌套分支无真实场景不立项。

### 运行时治理

**① failure_bundle 失败采集包**：uncertain/门禁阻断时 capture_failure 自动归集四工件——全屏截图、原生窗口清单、按 `--app` 定向的 UIA 快照、**失败瞬间前台弹层快照**（front-report 全文+分类），外加 bundle.json 采集清单（只列实际存在的工件，不伪造）；`flow-evidence --id <id> [--step <sid>]`（只读命令）索引采集包。每个 popup_pending 阻断同样落 failure_bundle。

**② 写步步前弹层门禁**：写步派发前经 `Host::front_layer` 观察其 `--app`（含 sequence 内嵌锚）的前台引导层：非 clear 且存在未声明处理的面板 → run 进 needs_review、reason=`popup_pending:<step>`、步保持 **Pending**（未执行无副作用，恢复走普通 flow-resume，门禁复查通过即续跑）；flow-status 附 popup_hint（判读入口+续跑命令）。读步与无 `--app` 步不设防；观察失败放行（门禁是阳性检测，写步自身 fail-closed 守卫兜底）。

**③ popup_handlers 声明**（AGENTS 人肉弹窗白名单的机制化）：flow 顶层声明 `popup_handlers: [{app, classification, name_contains?, click}]`：classification 仅允许 error/save/confirm/update/**notice**（**未声明的 unknown 永远上抛**，不可直接声明 unknown）；匹配面板经既有 click 全护栏（守卫/预告/审计）自动处置并逐次 popup_event 留痕，处置后复查清层才放行，未清或处置失败一律 popup_pending。**notice 分类**（最窄形态）：认领 naive 词表落 unknown 的 INFO 级流程自产表面（无按钮/非 error 语义的通知类滞留）；`name_contains` **必填**（按名认领，schema 层拒绝空/缺失——不存在「任意 unknown 一律处置」的宽形态），匹配语义 = unknown 面板且 name/text 含认领词；已分类面板不被 notice 吞并（分类等值匹配优先）；审计面 popup_event 记 `classification`（面板本体）+ `handler`（认领声明分类）双字段。词表演进纪律：更宽分类等真实滞留事故再扩，不预置。**锚输入引用**：`app` 字段支持 `{input: name}`——模板锚为运行输入（如 `${run.id}` 前缀形态）时可静态声明；在 load_plan 输入解析期展开为具体字符串（flow-run/validate/revise 三路同口径），引擎 `app.contains` 匹配语义不变。

**④ popup_ignores 常驻表面认领**：门禁观察面（front-report 树内 role:Window 表面）分不清「常驻部件」（停靠窗格/折叠挂件）与「真瞬态弹窗」——裁定为**声明制**（词表/形态启发已退役，provider 可见性路线被否证）。flow 顶层 `popup_ignores: [{app, name_contains}]`——匹配表面（app 锚包含+surface name/text 含认领词）是**声明已知的常驻部件**，门禁三处同口径跳过（步前进入判定/pending 过滤/处置后复查）。`name_contains` 必填（禁止「忽略该 app 全部表面」的宽声明——那会掏空门禁）；同一表面（同 app+同认领词）同时声明 handler 与 ignore → PROTOCOL（处置与放行语义互斥）；与 handler 双层共存合法：常驻表面认领放行+真弹层声明处置，两不相扰。`app` 同样支持 `{input: name}` 引用（解析口径同③）。**pack 级来源**：`apps/*.yml` 顶层可声明 popup_ignores（与 plan 级同构——pack 作者列全该应用的锚形态，如 Word 家族 "Word"/".docx" 两种）。CLI 层 `load_plan`（flow-run/validate/revise 共用）解析计划时自动并入（与模板级声明并集去重，幂等），注入来源留 stderr 痕；注入用行级状态机提取而非完整 YAML 解析（pack 是松散人读文档）；注入失败不阻断（lint-packs 检查每条必带非空 app+name_contains，是 pack 声明的守门员）。release 态注入路径=exe 旁 skills 目录。其他应用模板按各自实证增量补（先探测再写死，不预置）。

**⑤ 步骤 evidence 脱敏**：步骤 evidence 默认 metadata，可选 full；StepRecord 可含 error_evidence，旧运行记录可读取。metadata 隐去期望值、实际值和目标；full 超过 8 KiB 时只记 omitted。

**⑥ 进程看门狗**：**`actl` 进程级 wall-clock 看门狗**（actl-core `watchdog`）——UIA 跨进程挂起族的进程级防护。语义是**「无进展」而非「总时长」**：进展点（流程步派发、等待命令每轮轮询、恢复观察每次重试、录制 worker 每批事件、探索引擎每轮观察/动作、**树扫描逐节点**）重置期限；期限耗尽（**默认 120s**，`ACTL_WATCHDOG_MS` 可调、0=关）= 进程卡在任何不可恢复的等待里，**自我终止**：exit 1 + stderr `[watchdog]` 诊断行（stdout envelope 纪律不破坏——主线程已卡本就不会有 envelope）。心跳在各循环内逐轮/逐批重置,树扫描逐节点心跳——合法长路径(全树扫描分钟级、长录制、慢沉降)全部带心跳不受影响,心跳停止的单点卡死才触顶。恢复语义复用既有机制：flow 执行器被杀 → run 留中断态 → `flow-resume` 的 uncertain 重放核对接手。测试缝 `ACTL_TEST_HANG_MS`（确定性无进展挂起）。UIA 区段看门狗（20s/单调用）覆盖 pattern 派发与元素 Release 等 COM 区段；看门狗击杀进程而非步骤——被杀 runner 的状态如实滞留,由 flow-resume 接手。

**⑦ 前台快拒绝（后台懒激活慢墙的收口）**：Word 类 provider 在锚窗非前台时懒激活节流(同命令前台 ~2s/后台 14-22.6s),全树观察里单次 COM 调用撞 20s UIA 看门狗即击杀进程。两处收口：**type/press 输入快门**——昂贵 locate 之前经廉价原生枚举核对锚窗前台态:非前台先两级激活（原生 SetForegroundWindow[最小化先还原；进程有前台权时成功，如显示端授权点击后]→锚窗**单元素** UIA SetFocus[单次 COM 调用不做树遍历；Office 认 SetFocus,用户活跃于他窗时亦常成功]）,仍非前台 → NOT_ACTIONABLE 让行（守卫语义同既有 set_focus+settle,拒绝点提前到慢墙之前;OS 拒绝恰说明用户正在别处活跃,让行正确）。锚窗不存在不抢答（NOT_FOUND 留给原路径）；无锚（默认聚焦窗）与 click 不设此门（语义 Invoke 后台窗合法）。**front-report 原生快门**——锚窗非前台时 in_tree 置空+`in_tree_skipped` 原因、owned 原生层照报（见 §7.5.2 front-report 条目），门禁据此以 owned 层判定，写步随后被输入快门快拒，runner 全程存活。**如实边界**：观察类命令（wait/verify/find）在后台锚窗的慢 provider 上慢（单次定位 14-22s）是设计内边界——读步不激活窗口（§7.5.2"只读观察不唤醒窗口"承诺），wait 超时语义兜底；不为此设门（触发条件与候选方案见 ROADMAP 长期观察项）。

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

## 7.8 MCP 适配服务（actl-mcp）

独立 crate `actl-mcp`（bin `actl-mcp.exe`，**不依赖任何 actl workspace crate**——纯薄壳；async/tokio 仅限该 crate，ADR-001 定界修订，crate 边界由编译器强制）。协议实现用官方 rmcp SDK（协议层不手写——用户裁定：依赖数量不构成手写理由）。设计全文 [82](82-mcp-adapter-research.md)。

**传输模式 A**：`actl-mcp --listen <addr:port> [--token <t>]`——默认 127.0.0.1（fail-safe 防意外暴露），LAN 显式 0.0.0.0；`--token` 缺省生成 256-bit 并打 stderr 配对横幅（token + 客户端 .mcp.json 行 + 局域网警告——人工抄 token 即设备主人知情授权的配对仪式）；axum 壳层 Bearer 门禁（401 无 id JSON-RPC 错误体）+ Origin 校验（规范 MUST，跨源 403）；明文 HTTP 仅限可信局域网，跨公网不支持。SSH/stdio（rmcp 现成）与官方 tasks 扩展为后续可选。

**子进程派发模型（安全穿越的结构性保证）**：每次 tools/call spawn 真实 `actl <命令>` 子进程（`ACTL_TASK_ID=mcp-<hex>` 审计映射；`kill_on_drop`——请求取消/HTTP 断连即杀子进程，run 留 interrupted/uncertain，恢复走 flow-resume 既有语义）。预告、开始/继续本地放行、急停、看门狗、输入锁在被控端本地原样生效——**远端结构性无法绕过任何一环**；急停后 spawn 的子进程绑停止代次，远端调用自动 fail-closed。stdout envelope 是唯一权威结果；子进程非零退出或 stdout 不可解析（clap 参数错/看门狗自毙）时合成诚实错误 envelope（保留退出码与 stderr 尾巴，绝不伪造 ok）。

**能力面（三层懒加载，上下文经济与本地 agent 同构）**：① 通用工具 `actl`（{command, args[]}，描述携带九族速查 ~700 tokens 一次性）；② 资源模板 `actl://help/{command}`（单命令 -h 四要素文本，按需 ~300-500 tokens/次；排除动词模板不存在）；③ 全量 commands schema 不进资源面（客户端自动读取会全员买单 15-25K tokens）——作为普通命令显式调用。支持面 = 全部命令 − 排除表：`execution-recover`（停止状态修复）、`display-start`（显示端生命周期）、`desktop-restore`（初版保守排除）、`record-worker`（内部进程缝）——排除原则：操作本地监督与恢复机制本身的动词出局；用户可见副作用不出局（信号条预告管辖）；排除不静默（明确 PROTOCOL 错误 + 原因）。envelope → tool result 映射：`ok:false` → `isError:true`，code/hint/evidence 全保留进 structuredContent——远端 agent 拿到的失败信息与本地完全同构。

**测试**：库单测（envelope 直通/clap 错合成/参数透传/映射语义）+ HTTP 契约 7 项（真实服务进程 + 裸 TCP 线格式：401×2/403 跨源/工具调用真实派发/排除动词明确错误/unknown 方法不崩服务存活/真命令帮助四要素/排除与未知命令无伪造帮助）。规范实证：2026-07-28 无状态契约由 rmcp 强制（请求须带 _meta protocolVersion + clientCapabilities；Mcp-Name 头与 body 一致）。真机跨设备验收（真实 MCP 客户端接入）待用户场景。

## 7.9 探索模式与决策卡

Plan 新增可选 `explore` 段；存在时 `steps` 必须为空（可省略键），运行走探索引擎而非静态步骤引擎。spec 四要素：`task`（≤160B 一句话）、`app`（≤80B 窗口锚）、`keywords`（1..16 个、各 ≤64B——词表投影与候选评分共用）、`goal`（只读断言族调用，verify/wait/verify-file 等；禁 click 等写命令、禁 bind）。`budgets` 缺省 {actions:12, decisions:6, wall_ms:600000}；auto_resume 与 explore 互斥。

探索循环：`snapshot --app <app> --keywords K1,K2 --skeleton`（零命中放宽一次全 skeleton，记 `widened`）→ 与上一帧差分（S1 配对）→ 前沿评分排序（词表命中 × 身份稳定 × 角色先验 × 新鲜加成，整数分稳定排序）→ 熵门：非危险候选头部领先次名 ≥2 分自动执行，否则外抛；词法闸（删除/发送/支付等中英模式，子串大小写不敏感）命中的候选永不自动执行——全部走决策卡。每个候选先过 `find` 只读探测（probe-before-act）：死候选记 `tried=dead`，不消耗动作预算；点击成功后按帧变化归因 `reveals`/`no_effect`。目标核验完全本地谓词（`goal` 断言），LLM 永远不被问「这步成功了吗」。

决策外抛：run 置 `needs_review`、`reason="decision_pending"`（复用 NeedsReview，不加状态顶点）；决策卡自包含、序列化硬上限 **4 KiB**、候选 **Top-K≤8**、卡内只带判别性字段（id/selector/role/截断 name/效果标注/分数/危险位），差分只报计数 + ≤4 条样本名；压缩阶梯（丢样本名→截名字→Top-4→Top-2）保证上限，仍超即 PROTOCOL。卡经 `flow-status` 的 `data.decision_card` 暴露，`data.explore` 携带动作/决策/观察计数、tried 映射、goal_met 与 token 审计（observation_bytes/card_bytes）。全部状态持久化于 `flows/<id>/explore.json`（256 KiB 上限，原子写）。

应答协议：`flow-resume --id <id> --decision <choice_id>`，choice 必须属于卡内候选 id 集，否则 PROTOCOL；`--confidence`（≤32 字符，requires --decision）仅入报告。应答后引擎按「首选 + 卡内其余非危险候选按序」组成排序回退队列——首选无效果时自动推进，不产生新卡（危险候选永不自动回退，显式选择即授权）。静态流程传 --decision 为 PROTOCOL；探索运行传 --retry-step 为 PROTOCOL；悬挂卡未带 --decision 续跑为 PROTOCOL。

**`flow-run --auto-decide`**（2026-10-13 用户授权，PRD §1.3）：run 停 `decision_pending` 时 actl 自问配置的决策 provider（decision.json）并逐次回填续跑直至终态。授权红线与手工应答完全一致——危险候选构造前剥离（provider 物理不可见）、任何弃权（dangerous_only/provider_none/low_confidence）停止循环交回人（run 留 decision_pending，decision_card 照常暴露）、回填走 explore::execute 同一校验链（成员校验/预算/急停/暂停逐应答边界让行）、decisions 预算封顶循环（防御 cap=预算+1，撞上 INTERNAL 如实报 bug）、每答审计 `responder=auto_decide`。预检先于 run 创建：静态流程 PROTOCOL（无决策卡语义）、与 --paused 互斥（暂停让行是人保留位）；provider 配置缺失/不可达如实传播，绝不静默降级为无模型行为。实现 `crates/actl-cli/src/flow/auto.rs`；真机 A/B 双臂验收（confident 自动完成/弃权交回人工 resume）见 CHANGELOG。

终态语义：goal 满足 → completed；预算耗尽 → needs_review（reason `budget_exhausted:actions|decisions|wall`）；无候选（放宽后仍空）→ needs_review `no_candidates`；观察失败 → needs_review `observation_failed` 且错误如实传播（错误码原样返回调用方）；目标断言本身报错 → needs_review `goal_check_failed`。急停/暂停在任何边界生效（先持久化后返回），与静态流程同语义。显示端 needs_review 动作对探索显示 resume（静态流程仍为 recheck）。

### 路线回写、先验与目标重试

`explore.priors`（可选,YAML 映射）:选择器 → 分数调整(-4..=4,≤32 项),调用方从 pack quirks / 上轮回写的 suggested_priors 注入——已知死/无效果/窗口重构类候选被压低,已知有效候选提升;分数下限 0。引擎不读 pack,先验显式入参(分层约束:核心 CLI 永不读 pack)。

`explore-writeback --id <run> [--out <file>]`(只读):从 goal_met=true 的完成运行抽取 pack 格式路线片段——status **恒 unverified**(verified 迁移必须人工复核,PACKS.md 修订纪律;自动化只做证据收集);步骤 = acted 点击序列 + 终态 goal 断言,`--app <锚点>` 精确值替换为 `{window}`;死候选生成 quirk 片段,tried 效果映射输出 suggested_priors(dead=-4,no_effect=-2)供下一轮 priors。证据头注释携带 run id/动作/决策/观察计数与 token 审计。action_log 达上限(可能截断)或 goal 未达成即拒绝——不完整路线不导出。片段由调用方合并进 `apps/<slug>.yml`(actl 不写 pack 文件),`cargo xtask lint-packs` 为最终校验;重放由 agent 执行步骤并留证据,PR review 即迁移复核。

goal 断言瞬态错误(NOT_FOUND/TIMEOUT,如提供程序抖动/窗口短时未物化)有界重试 2 次(300ms 沉降,重试计入 goal_retries 不计 errors);耗尽或非瞬态错误维持失败闭合,错误码原样传播。flow-status 的 explore 块新增 goal_retries 与 priors。

### goal 折叠进观察帧

goal 目标选择器的值(id 精确/name 子串,镜像 verify 定位语义)并进观察关键词,使 goal 元素始终入帧——存在型断言与 name 通道值断言(`--value ... --property name [--contains]`)在帧内本地判定,**逐轮消掉一次 verify 采集**。折叠仅覆盖可精确镜像的形态;目标缺席/id 歧义/其他通道(--property value、--normalize-lines、--url-query)回退权威 COM 调用,错误码语义不变。spec 词表达 16 上限时折叠词机会性放弃。goal 核验通道由 condition 改为 execute_output 直判,保留 ASSERTION_FAILED(未满足)/NOT_FOUND(目标缺席)两态:存在型缺席=未满足继续探索,值断言缺席=有界重试;wait 族超时=未满足。goal 元素入帧使同窗差分归因能看到 goal 通道的变化,reveals/no_effect 标注更准。

### 输入原语与跨窗 goal 探测

`explore.texts`(可选,YAML 映射):完整语义选择器(id:/name:)或 `focused:`/`focused:<Role>`(2026-10-12 增补:键入 OS 键盘焦点元素——焦点是运行时状态不入帧,合成候选 **dangerous 标注永不自动执行**,只能经应答者显式裁决授权;不参与投影关键词与定位预检,可达性由 CLI type focused: 的角色核验 fail-closed)→ 待输入文本,≤8 项、值 1–4096B、词表+texts 键 ≤16。id:/name: 键的选择器值并入观察关键词(种子+评分激活 Edit——Edit 角色先验为零,靠 texts 键命中得分);候选命中即执行 `type <selector> <文本> --app <锚点>`(复用键盘护栏与 --app fail-closed),probe-before-act 与授权/急停/暂停护栏同 click。**输入通道按内容自动选择**(与录制编译器同口径的内容子句):文本含非 ASCII 字符 → 注入 `--paste` 走剪贴板通道,ASCII 保持键盘——引擎是发起方观察不到 IME 键,故只取内容判据;flow YAML 的 type 步骤本就直接透传 `--paste`,此规则只覆盖引擎自建 invocation 的 texts 原语。type 动作的文本记入 action_log(serde `text` 字段),writeback 导出为 `["type", selector, 文本, (--paste), --app, "{window}"]` 步骤(非 ASCII 文本注入 `--paste`——导出路线重放不走键盘退路)——重放可用。

跨窗 goal 探测:goal 目标在锚窗 NOT_FOUND 时,先做同进程窗口探测——`list-windows` 枚举 pid 等于观察窗(快照 window.pid)的其他有标题窗口(排除当前窗与同题窗),把 goal 的 `--app` 逐个改写为候选窗标题重试(≤3 个/轮);命中即记录 `goal_window`(转移)并判定 goal 达成。探测预算:每运行 ≤2 轮(每轮一次枚举),无 --app 的 goal(如 verify-file)不探测;枚举失败不传播(探测是机会性增强,主错误语义留在 goal 调用)。观察窗与跨窗身份(window_pid/window_title/goal_window/anchor_transfers/probe_episodes)入 explore.json 与 flow-status 的 explore 块。v1 边界:转移只在 goal 命中时发生(观察锚点不随弹窗转移——在弹窗内继续点击属后续);goal_window 已转移而其窗后续缺席时回落锚窗重探。

### 批量探测通道

Host 的 `execute_probes` 批量只读通道**保持串行**逐个执行(同进程多线程并行 UIA find 经真机证伪——并发读对桌面根/UIA 提供程序存在上限,并发即抖动,本栈上既有害又无益;浏览器轨 CDP 读天然可并行,通道为将来预留,语义契约[同序结果、单失败不影响其他]已经 MockHost 钉住)。引擎两个接入点:①**卡构建预检**——外抛前对 Top-K 候选批量探测,确定性死亡的候选不进卡(应答者不看到死控件,卡体积与 token 双省),全前沿死亡回循环重算(有界,最终落 no_candidates);②**应答队列预检**——卡发出到应答之间世界可能已变,应答时批量探测把死项出队,每个死项省一整轮观察。死活分类规则:仅确定性错误(NOT_FOUND/AMBIGUOUS/NOT_ACTIONABLE/PERM_DENIED)标 dead 并计 errors;瞬态错误(TIMEOUT/INTERNAL/ABORTED)不标记——留待动作时单点 probe 权威复核(过渡期错误不代表控件不存在)。观察∥goal 并行按分析不做:goal 折叠已消掉大半核验采集,残余并发读与抖动窗口重叠,风险大于收益。

### 观察与 goal 的瞬态重试语义

观察的瞬态错误(INTERNAL/TIMEOUT——菜单开合/窗口重构过渡期提供程序短暂锁住,紧随动作的观察失败形态)有界重试 2 次、500ms 沉降,重试计入 `observation_retries` 不计 errors;耗尽后维持原语义(协议错误 → Failed spec_rejected,其余 → needs_review observation_failed 且错误码原样传播)。窗口缺失(NOT_FOUND,结构性)不重试。与 goal 有界重试同构:瞬态先给恢复机会,终态保持诚实闭合。

goal 核验的查询类故障（INTERNAL）**不判死运行**：计 errors 如实记录、按未满足继续探索。理由：goal 核验先于动作，持续故障会饿死本该执行的探索动作；预算有界兜底，永久故障最终以 budget_exhausted 终止且 errors 计数反映每轮故障。INTERNAL 与 NotFound 一样先经同 pid 跨窗探测（机会性）；NotFound 的存在性/有界重试/失败闭合语义不变，PermDenied/PROTOCOL 等非查询类致命错误维持立即 goal_check_failed。

### 帧背书与 ref 绑定动作

**帧背书原则**:本轮刚观察的帧是引擎对候选存活性的权威视图——find/click 的 item-container 定位对 flyout 项**间歇性失明**(同现场 0 结果与命中交替,快照树稳定可见,两通道可靠性倒挂)。规则:选择器能在当前帧内解析(id 精确/name 子串,镜像选择器语义)即帧内候选——①**跳过动作前 COM 探测**(帧即新鲜度证据);②**动作失败不判死**(计一次动作预算,有界重试,下轮前沿重导;真死候选在预算内自然耗尽,终态诚实)。非帧内候选(应答回退队列等陈旧上下文)保持 probe-before-act 原语义。卡构建**不做**批量预检——卡与观察同轮,帧即新鲜度。

**动作经快照 ref 身份绑定**：引擎的动作用帧内 ref 走捕获身份解析,不再经选择器重定位——`click/type @eN --snapshot <snapshot_id> --app <锚点>`(docs/31 身份纪律:跨调用使用 ref 必须绑定 snapshot_id)。机制:①Host 的 `execute_snapshot` 快照通道(返回 snapshot_id + data);②引擎观察时把快照 id 与帧同轮记录(explore.json 的 `snapshot_id`);③act() 对帧内元素构造 ref 绑定动作——身份来自**看见它的那条通道**(快照捕获身份+RuntimeId 核对+窗口新鲜度检查),绕开间歇失明的 item-container 定位通道。无 ref/无快照 id(历史语义)回退选择器路径;应答回退队列中的陈旧 ref 自然失效(STALE_REF),按帧内候选的有界重试语义下轮重新观察取新 ref。STALE_REF(元素重建)不计死亡、计一次动作预算——与"控件重建后重新观察"纪律同构。

### decide:决策卡的模型应答（System One provider）

`actl decide [--card <文件>] [--run <id>] [--task <意图>] [--config <文件>] [--endpoint <URL>] [--min-confidence F64] [--timeout-ms N] [--dry-run]`（只读，不做任何桌面动作；read_only 分类入 flow 观察步）。actl 命令面上唯一的出站网络触点：读入决策卡，向显式配置的 System One 决策端点提问，返回应答或弃权。**判别式决策模型协议**（choice/confidence 类型化问答，Jev/Kev/laya/openjev 族 `/v1/systemone`）——非生成式 LLM，「不内置 LLM 客户端」的 PRD 非目标边界不变；出站载荷=决策卡本身的判别性字段（截断名/无全树/无窗口标题之外的额外数据），防注入固定行随 state 携带。

输入六通道：**裸决策卡 JSON** / **flow-status envelope**（`data.decision_card`，`actl flow-status --id <run> | actl decide` 管道）/ **`--run <id>`**（从 run 状态文件直读悬挂卡 `flows/<id>/explore.json` 的 `pending.card`——flow 步无 stdin 管道，这是应答循环 YAML 化的卡源通道；id 白名单 `[A-Za-z0-9_-]`，与 --card/--task 互斥）/ **AMBIGUOUS 错误信封**（`error.code=="AMBIGUOUS"` + `evidence.candidates`，须配 `--task` 意图——错误信封不带意图，缺 --task 即 PROTOCOL 附 `task_missing` 证据与专属 hint；docs/12 §4 结构化候选的消费闭环：模型免重取 snapshot 直接裁决）/ **front-report 咨询信封**（`actl front-report --app X | actl decide --task '<流程意图>'`，弹窗判读咨询）/ **视觉咨询文档**（`{state, images: [本地 PNG 路径], questions: {qid: {type: choice|noul, instructions, criteria}}}`——`--card`/stdin 按形状派发，配 `--task` 为 PROTOCOL（意图在文档 state 内），协议见下段）。AMBIGUOUS 形态映射为合成卡：候选 id c1..cN 按证据序、dangerous 由词法闸独立重算、`context` 消歧上下文行进候选判别描述；应答与低置信 top_choice 附 **resolved 回显块**（`{ref,role,name,automation_id,context}` 可重放元数据，调用方不必回读错误信封）。卡形态配 --task 为 PROTOCOL（意图只对消歧/判读形态有意义）。

**front-report 判读咨询**（popup 门禁的判读辅助，纯咨询不执行任何东西）：owned/in_tree 表面渲染为有界 state 行（name/buttons/text 各截断）+ 任务意图 + 防注入固定行，**family 分类与 blocking 概率并行一问**（System One 多问题原生形态）。家族集与门禁 handler 词表同义（crash/error_notice/save/confirm/update/notice）加 benign_surface（应用自身常驻部件——计算器门禁误报轮的判读形态）与 unknown（交回人）。输出 data.family + data.blocking；conf < 门槛 → abstain `low_confidence`，top_choice（家族倾向）与 blocking 照报——**判读建议不冒充可信结论，处置仍归白名单与 unknown 上抛纪律**。`front_clear=true` 短路免网（family=front_clear）；零表面（`in_tree_skipped` 形态）abstain `no_surfaces` 同样免问；幻觉家族 → PROVIDER_UNAVAILABLE `choice_not_offered`。无桌面实机烟雾（本地 Kev-4B）：常驻表面→benign_surface、Fail Fast 崩溃框→crash 方向均正确；真实信封 conf 0.32-0.37 低于 0.5 门槛，弃权而 top_choice 照报——嘈杂真实载荷上门槛拒绝自动断言（探针合成载荷同题 0.95；owned 子窗文本增强后 A/B 实证同崩溃框 conf 0.32→0.622 跨门槛,信号差已收口——2026-10-13 front-report 信号增强）。

**视觉咨询文档形态**（2026-10-14 spec，PRD §1.3 受限判别式视觉咨询 carve-out）：图像由 CLI 本地读取（PNG 魔数 + 单张 10 MiB 校验；base64 为手写 RFC 4648 纯函数，零新依赖），wire 请求携带 `images: [PNG data URL]`（实测 LM Studio `/v1/systemone` + clef-flash：单图/双图、确定性逐位复现、单次前向多问题并行——决策头整输入单物理批，服务端 ubatch 即单请求预算上限，文档形态的上限设计与该预算对齐）。协议纪律与卡形态同源：choice 强制注入 `none`（文档自带 none 键 PROTOCOL `criterion_key_reserved`）、应答幻觉拒绝（`choice_not_offered`）、置信/领先裕度双门槛**逐问题**适用、防注入固定行随 state 携带。应答面：`data.answers` 逐问题结果（choice → `{choice, confidence, probabilities}` 或 `{abstain, confidence, top_choice}`；noul → `{noul}`）；恰一个 choice 问题时另镜像顶层 `choice/abstain/confidence/top_choice`。上限：4 图 ×10 MiB、4 问题、choice 16 选项、state 2000/instructions 256/选项描述 128 字符、qid 与选项键 32 字符。**图像负载永不入 dry-run 输出**（路径+字节数替代 base64）**与审计行**（vision 摘要：state 截断 160 + 问题骨架 + 图像元数据；屏幕内容是待观察数据，且 4 KiB 行预算装不下 base64）。

安全不变量（结构性，非提示词约定）：**危险候选在请求构造前剥离**——模型物理不可见，dangerous_only 卡直接 abstain 不触网；`none` 强制入选项（模型有真实弃权出口）；provider 返回非 offering 集的 choice → PROVIDER_UNAVAILABLE（`choice_not_offered`，模型不能幻觉出目标）；confidence < 门槛（默认 0.5）→ abstain `low_confidence` 附 top_choice。**领先裕度双门槛（2026-10-13，fastjev-egolite 同款第三重守卫）**：conf 只答「头名多可信」不答「头名比次名可信多少」——p1-p2 < MIN_LEAD_MARGIN(0.3) → abstain `low_margin` 附 top_choice/confidence（探针实证形态：t_g 大卡 0.604/0.228 薄裕度恰是过 conf 门槛但次名有实质权重的答案；none 不参与裕度；provider 未返回分布时裕度让位，conf 门槛独立工作——旧后端兼容）。**AMBIGUOUS 截断护栏（2026-10-13，third-hand §13.2.1 召回保护的 decide 侧收口）**：`ambiguous_error` 候选超 8 截断时证据带 `total/returned/truncated:true`，decide 见标志 abstain `candidates_truncated` 不触网——在残缺选项集上问模型=把「正确候选恰好被裁掉」变成强迫硬选，正解是收窄选择器/--near。**弃权不是错误**——run 保持 needs_review，裁决权交回人/上层 agent。decide 永不代答危险候选、永不 auto-resume：回填仍走 `flow-resume --decision`，授权语义一寸不移动。

配置 `%LOCALAPPDATA%\actl\decision.json`：`{endpoint, api_key?, api_key_env?, timeout_ms?(10000), min_confidence?(0.5)}`（与 apps.json 同目录同治理）；`--endpoint` 显式给出且无配置文件时免配置直答（本地 llama-server/LM Studio 无鉴权场景）。key 解析序 env > 明文。审计 JSONL 追加于同目录 `decision-audit.jsonl`（>4 MiB 轮换 .old）：卡 id/choice 或 abstain/confidence/provider_ms/model/被剥危险数/responder(CLI vs auto_decide)/**完整卡嵌行**（≤4KiB 卡协议上限兜底体积；每行=请求+应答配对，是模型换版回归与阈值校准的直接语料——docs/08 §7 校准债的偿还通道，front 形态无卡不带）。`--dry-run` 打印请求不触网。

输出 envelope data：`{card_id, choice?, abstain?, confidence?, top_choice?, candidates_sent, dangerous_filtered, provider_ms?, model?, endpoint}`。abstain 值域 `dangerous_only | provider_none | low_confidence`。错误：PROTOCOL（卡/config 不可读、参数坏，退出码 2）| PROVIDER_UNAVAILABLE（不可达/超时/HTTP 非 200/负载不合规/幻觉 choice，退出码 1，hint 指向端点检查；原子命令与流程引擎不受影响）。测试：契约 decide_contract（本地 std 桩 HTTP 全离线，危险剥离/none 强制/门槛/幻觉拒绝/envelope 双形态均 golden）；真网行为属真机验收轮，不进 CI。

## 7.10 GUI 操作录制

录制 = "人演示 → YAML"入口，与探索模式（agent 自主 → YAML）在回写层汇合。R1 = 捕获 + trace 证据 + 机械编译初版 YAML（"录完即可跑"下限）；R2 编译增强（延迟分析/泛化）、R3 agent 改写轮（skill 层）、R4 显示端菜单（[79 §8](79-quicker-recording-research.md) 触发面）。会话有界：显式 start/stop + 时长上限，**非常驻**。设计 docs/76 + 调研 docs/79。

### 命令族（4 个，stdout 一律 envelope）

- `record-start [--id <id>] [--max-seconds <N>]`：spawn 分离的 `actl record-worker --id … --max-seconds …`（std Command + `CREATE_NO_WINDOW` + stdio 全 NUL；**spawn 前先 `process_io::clear_stdio_inheritance()`**——worker 继承调用方管道写端会让 `$()`/ReadToEnd/管道读阻塞到 worker 退出）。默认 id = `rec-<unix ms>`（valid_id 规则）；默认时长上限 600s。启动后等 state.json 出现（≤3s）确认后返回：data `{id, status: "recording", pid, dir, trace, max_seconds}`（路径为相对 %LOCALAPPDATA%\actl\ 的 records/<id> 形态）。已存在活跃会话（state=recording 且 pid 存活）→ **PROTOCOL**；id 已被用过 → PROTOCOL。落盘：`%LOCALAPPDATA%\actl\records\<id>\{state.json, trace.jsonl, stop-requested, draft.yml}`。
- `record-worker --id <id> [--max-seconds <N>]`（内部会话进程，由 record-start spawn；直接调用同语义，stdout envelope 便于测试与诊断）。捕获循环 → 终态时编译 draft.yml 并返回 `{id, status: "completed", stop_reason: "requested"|"duration_cap", events, steps, todos, yaml, trace, draft, dir}`。钩子安装失败 → status `failed` + 错误码如实。
- `record-stop [--id <id>]`：写 stop-requested 标记 → 轮询 state.json 至终态（≤15s，超时 INTERNAL 如实）→ 返回终态摘要（与 worker 终态同形，含 yaml 全文）。无 --id 时按活跃会话发现（records/ 扫描 + pid 存活，**唯一才执行，歧义即 PROTOCOL 要求 --id**）；会话已 completed → 幂等返回摘要不报错；**abandoned 会话（worker 已死）做数据收口**：编译既有 trace 产 draft（stop_reason=abandoned），不复活进程；未知 id → PROTOCOL。
- `record-status [--id <id>]`（只读，进 main 的免信号名单）：有 id → 完整会话状态 + trace 统计（按 kind 计数 + masked/injected + 行数）；无 id → `{active: [ids], sessions: N}`。state=recording 而 pid 已死 → 派生态 `abandoned`（如实报告；收口走显式 record-stop，不自动恢复）。

### 捕获语义

- **钩子回调极简**：WH_MOUSE_LL + WH_KEYBOARD_LL 挂在专属线程（消息泵），回调只做"时间戳+事件入队"；**一切 UIA/Win32 反查在工作线程**（钩子超时被 Windows 回收；UIA 反查不可进回调）。
- **只录语义事件**：鼠标按下（左键 kind=click；右/中键记 button 字段，编译为 TODO）；移动/滚轮不录。键盘只记 down（按键重复去重：按住期间同 VK 只记首个）。
- **注入事件**（LLMHF_INJECTED）**全量捕获但不参与编译**：与人工事件同路径反查/焦点身份/值读回（trace 是"发生过什么"的完整证据），编译侧一律排除（`injected:true`）——录制的是人的演示，机器注入不是演示内容。密码脱敏对注入键同样生效。
- **actl 自家窗口过滤**：鼠标事件的 WindowFromPoint 命中窗 pid ∈ {worker 自身, actl-signal} → 整事件跳过（计数入 state 的 skipped_own）。
- **点击瞬间反查**：工作线程对每个点击事件做 `element_from_point` + 根窗元数据（GetAncestor GA_ROOT → hwnd/标题/rect/DPI/pid/进程名）。反查失败如实记 `lookup:"failed:<code>"`，坐标与窗口元数据仍在（编译为 TODO，不猜测）。
- **焦点身份与密码脱敏**：键事件经 GetGUIThreadInfo 取物理焦点 hwnd；hwnd 变化或 burst 首键时做一次 `get_focused_element`（identity: automation_id/name/control_type + `is_password`）。焦点元素是密码框期间的全部键事件 → trace 只记 `{kind:"key", masked:true}`（**VK 都不落盘**）；脱敏不可关闭。焦点元素身份同时服务 type 步骤的目标选择器。
- **IME 标记**：VK_PROCESSKEY(0xE5) 出现 → 该键运行标记 `ime:true`（编译为 TODO：中文文本走 R2/--paste 通道，键序列重放必错）。
- **DPI 口径**：会话线程设 `SetThreadDpiAwarenessContext(PER_MONITOR_AWARE_V2)`——钩子事件坐标是物理屏幕坐标,反查 API 按调用线程 DPI 感知解释,多显示器混合 DPI 下必须对齐。
- **键盘注入标志位**：`KBDLLHOOKSTRUCT` 的 INJECTED 是 **0x10（LLKHF_INJECTED）**——0x1 是 EXTENDED 位;鼠标侧 LLMHF_INJECTED=0x1。
- **trace = JSONL 逐事件追加**（崩溃安全）：首行会话头 `{kind:"session", id, started_ms, max_seconds, pid}`；事件行含 t/kind/坐标/窗口元数据/元素身份/标志（键事件另携带 mods 快照与 focus/focus_element）。trace 是 agent 改写轮的原料（本地文件，非 token 载荷；按行索引消费）。

### 机械编译（R1 规则，编译器在 actl-flow，纯函数）

trace 事件 → flow-v1 兼容 YAML。flow schema 要求写步骤必带 verify 且 retry ≠ read_only（默认）——**机械验证 + `retry: never` 恒配**，效果级验证留给 R2/agent 改写轮：

| 事件形态 | 编译产物 | 机械 verify |
|---|---|---|
| click 且元素有 AutomationId/name | step `click 'id:<aid>' --app <根窗标题>` | `verify --exists <同 selector> --app <标题>`（目标仍在） |
| click 无可用身份（反查失败/无名无 id/右中键） | **TODO 注释**（非步骤，文件保持可验证）——不猜测目标；注释含 trace 行号证据引用 | — |
| 键事件 = ctrl/alt/win 修饰 + 单键 且焦点元素已知 | step `press '<mods>+<key>' --app <焦点窗标题>` | `verify --exists <焦点 selector>`（编辑框类快捷键通常成立） |
| 快捷键无焦点身份 / 键名不可解析 | TODO 注释 | — |
| 键运行 = 纯可打印 ASCII 且焦点元素已知 | step `type '<selector>' '<text>' --app <焦点窗标题>` | `verify --value <selector> <text> --property value`（读回核验——真后置条件） |
| 终结键（Enter/Tab/Esc/F 键） | **TODO 注释**——终结键恰恰改变焦点/关窗，对旧焦点断言存在是系统性错误猜测（不猜测纪律）；笔记含完整表达式供人工/agent 补 verify | — |
| 键运行含 IME 标记 | TODO 注释（中文文本，--paste 通道） | — |
| 键运行被脱敏 | TODO 注释（密码字段，内容永不落盘） | — |

修饰键自身事件是噪音（组合语义由被修饰键的 mods 字段携带——捕获侧回调的 held 集快照，免 keyup 跟踪）。每步 `# evidence: trace:<行号>` 注释 = 指回 trace 的证据引用（agent 改写轮的锚点）。**编译器不提前丢信息**：未编译的事件全部保留在 trace。

v1 边界（如实记录）：空事件会话的草稿 `steps: []` 会被 flow schema 拒绝（"expected 1-256 steps (empty only with explore)"）——空录制没有可运行内容，如实失败不是编译缺陷。

### R2 编译增强

- **延迟分析 → wait 步骤**（docs/76 §4 规则的落地）：与上一用户动作间隔 ≥2s **且目标换窗** → 在点击前编译 `wait --window <新窗标题> --timeout <间隔+1000ms>`（"等应用加载"形态）；同窗长间隔是"人在犹豫"，不出步。wait 只读——无 verify、默认 retry 可重放。时间轴只记用户动作（点击/按键），值事件（机器读回）不入轴。
- **值读回事件（kind=value）**：键入 burst 的三个终点——静默 ≥300ms / 下一个点击事件 / 会话终点——捕获层对当前焦点元素做 `get_focused_element` + ValuePattern 读值，事件 `{t, kind:"value", value, focus, element}` 先于点击事件入 trace。守卫：焦点根窗须与运行一致（不一致 = 焦点已换，读到的是别的元素）；**密码焦点不读**（脱敏不可关闭）；ValuePattern 缺失如实返回无事件。**值读回回退链**：Value 读回失败后回退 Text 文档区全文读回（与 verify 的 value/text 通道同族；超长文档的截断边界留给改写轮）。
- **编译配对与 --paste**：值事件与 pending 键入运行配对（窗口一致 + 身份一致或单侧未知）→ `type '<selector>' '<提交文本>'`，**提交文本覆盖 VK 推导**（IME 候选/自动纠正的真值）；IME 键出现过或文本含非 ASCII → 加 `--paste`（剪贴板通道，中文 IME 下键盘通道不可靠——type 命令既有语义）。无值读回的 IME 运行维持 TODO。
- **{window} 泛化**：步骤与 verify 里 `--app <记录时标题>` 的字面标题收编为 flow inputs——`window`/`window_2`/…（按首现顺序），默认值 = 记录时标题，args 替换为 `{input: windowN}`。重放前一处可调（多机/窗口改名场景），与 explore-writeback 的 `{window}` 同一泛化思想；TODO 注释里的标题保持字面（注释不执行）。文本字面值的参数化留给 agent 改写轮（数据 vs 常量是语义判断）。

已知边界（如实）：值读回依赖事件到达时机（工作线程异步），焦点快速转移时可能错过（运行退回 VK 路径或 TODO，不虚记）；wait 只覆盖"换窗+间隔"形态，"UIA 树变化但同窗"的等待待 agent 改写轮补。

### 测试缝与错误码

- 契约测试离线：`ACTL_RECORD_DISABLE_HOOKS=1` 跳过钩子线程（会话循环/终态/编译/文件协议全真），CI 无需交互桌面——与"uia 交互用例默认 ignore 不占门禁"同纪律；**测试缝不掩盖真机行为**，钩子真机验收另轮。
- 编译器单测（actl-flow）：事件流 → YAML → `serde_saphyr` 往返解析为 Plan（与 route.rs 渲染同验证口径）；覆盖上表全部形态 + 空事件 + 注入跳过 + 证据行号正确。
- 契约 golden：record-start/status/stop/worker 的 envelope（duration_ms 剔除后全文比对）；错误侧断言 error.code 与退出码 2。

错误码——PROTOCOL：已存在活跃会话 / 未知 id / 活跃会话歧义（无 --id）/ 非 actl 会话目录形态。INTERNAL：spawn 失败、state.json 超时未现、stop 等待超时（15s）、trace/草稿写失败。worker 钩子失败 → 会话 status `failed` + 原错误码入 state（record-start 已返回的启动成功不回改——异步失败由 record-status 如实暴露）。

## 8. 里程碑

各阶段范围、工期、验收的**唯一来源**:[ROADMAP](ROADMAP.md)。本文不重复维护版本计划。

## 9. 明确不做

非目标清单**唯一来源**:[PRD §1.3](PRD.md);背景论证见 [02 §4](02-gap-analysis.md)。
