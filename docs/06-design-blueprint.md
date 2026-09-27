# 06 · 设计蓝图:Windows AI-native 桌面操作 CLI

日期:2026-09-26 · 状态:草案(未实现) · 前置:[01 调研](01-research-landscape.md) · [04 选型](04-tech-decision.md) · [05 估算](05-implementation-estimate.md)

## 1. 定位

一句话:**给 AI agent 用的 Windows 桌面原子操作 CLI——accessibility(UIA)驱动、无 MCP、无状态、输出机器友好。** 类比:Playwright 之于浏览器,本工具之于 Windows 桌面。

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
list-windows / list-surfaces        # 窗口/虚拟桌面枚举(list-surfaces 未实现)
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
verify --exists @e3 / --value @e3 "x" / --checked @e8 / --order ...
batch --json '<commands>'   # 单进程内顺序执行,共享 snapshot;每步结果与耗时进 steps[]
                             # (i/cmd/ok/duration_ms,失败步附 error),首错即停时
                             # 已执行步随错误 evidence 带出(部分结果不丢失);
                             # 本轮新开窗口差集上报 windows_created(title/pid/hwnd,
                             # 清理地基;生命周期归编排层,CLI 不代管);
                             # 步前弹窗闸门:前台非基线且不匹配本批 --app → steps 附 blocked_by(warning)
```

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
| `--expect P` | 执行成功后的隐式 `wait P`(默认 2s 轮询),失败 `ASSERTION_FAILED`——"执行+验证"内联但不吞决策 |
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
| `ABORTED` | 用户主动停止(紧急热键/停止文件,v0.1.1) | 清除停止标志(actl-signal 点击或删文件)后重试 |

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
5. **停止**：`stop-requested` 为粘性标志。输入锁等待后、动作准备间隔及执行前
   检查；取消返回 `ABORTED`。UIA 修改和剪贴板写入也在调用前检查。停止请求与
   本次调用确认停止分别呈现，不能承诺撤销已提交的 COM/输入。右侧按钮只请求停止；
   右键“清除停止标志”才允许后续调用。保留 Ctrl+Alt+F12，热键冲突时按钮仍有效。
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

history 的 data 为 `{calls:[{complete,events:[...]}],incomplete_files}`。
complete 只表示事件可解析且有结束记录。incomplete_files 统计本次扫描到的空/损坏文件及
返回的无终态调用；过期记录可能尚未清理。非法 ID/limit 返回 PROTOCOL，读取失败返回 INTERNAL。

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
