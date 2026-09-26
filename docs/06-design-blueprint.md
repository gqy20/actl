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
- **命名四规则**:① `动词` 或 `动词-宾语`,小写连字符;② 读操作前缀固定:`get-*` / `find` / `list-*` / `inspect` / `extract`;③ 位置参数只放目标(@ref 或选择器),其余全 flag,谓词统一 `--expect`/`--predicate`、超时统一 `--timeout`;④ 全局 flag 极小:`--dry-run` `--timeout` `--verbose` + 体积控制(`--skeleton` `--max-depth`)。
- **自发现**:`actl commands` 输出全部命令 + 参数 schema 的 JSON(等价于 MCP `tools/list` 的 CLI 形态);人类 help 文本与机器 schema 同源生成,不漂移。agent 冷启动一条命令拿全能力清单,不依赖读文档。
- **可理解性验收标准**:任一命令的 help + envelope,不看文档即可推断四件事——输入什么 / 副作用是什么 / 返回什么 / 失败码指示的下一步动作。

### 3.2 分层模型:哪些统一,哪些适配

| 层 | 内容 | 时机 |
|---|---|---|
| **L1 通用原语**(统一) | 只依赖 OS 契约(accessibility 树 / 窗口系统 / 剪贴板),一套命令吃遍所有应用;**动词集合封闭**(~45 封顶,新增动词需 spec 提交评审) | M1–M2 全量 |
| **L2 语义投影**(机制统一,内容是应用数据) | 同一命令的视图参数(`--skeleton`、`--max-depth`、未来的 `--view grid`),解决大 UI 的 token 膨胀;投影规则是**数据**(pack.toml,参考 desktop-cli 的 application packs),不是代码 | M2 做 skeleton;packs 视真实使用数据再定 |
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
find --role button [--name "确定"]   # 条件查元素(带 ref);与 snapshot 的分工见 §3.6
                                     # 已物化树 0 命中且可映射精确属性时走 L4 虚拟化容器(§5)
get @e3 [--property p] [--raw]      # 读元素属性;--raw 全量含 pattern 支持(合并原 inspect)
                                     # property 白名单含 text(TextPattern 文档正文专用通道,
                                     # 与 value 分工:Chromium 的 Document Value=URL 会掩盖正文)
list-windows / list-surfaces        # 窗口/虚拟桌面枚举(list-surfaces 未实现)
                                     # list-windows 附物理层叠放事实:foreground / topmost /
                                     # minimized / visible / z_order(0=最顶) / rect / owner_title
extract @ref [--table]              # 结构化提取(表格→JSON 数组),@ref 指定容器
                                     # GridPattern 直读优先(source:"grid-pattern",带 row/col
                                     # 语义),树扁平化兜底(source:"tree-flatten");来源随结果上报
```

#### 交互(interact)
```
click @e3 [--expect P]     right-click / double-click
type @e5 "text"            set-value @e5 "v"    # UIA Value pattern 优先
press ctrl+s               key-down shift / key-up shift  # 状态保持键必须成对
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
wait --element @e3 --predicate actionable [--timeout 5s]
wait --window "保存" | --gone @e3 | --property @e3 value="OK"
verify --exists @e3 / --value @e3 "x" / --checked @e8 / --order ...
batch --json '<commands>'   # 单进程内顺序执行,共享 snapshot
```

### 3.5 帮助系统规范(`-h` 的设计与层级)

**双受众、三级层级**(扁平命令面保证 help 两级封顶):

```
L0  actl -h           人类全景:六组分段、一行一命令、≤80 列,30 秒看懂命令面
L1  actl <cmd> -h     人类详情:USAGE / ARGS+FLAGS / EXAMPLE / ERRORS / SEE ALSO
L2  actl commands     机器 schema:全命令+参数的 JSON(agent 自发现入口)
```

- **协议例外(ADR-002 修订条款)**:数据命令 stdout = envelope;`--help/-h/--version` 为元命令,按 POSIX 惯例输出人类文本到 stdout;`commands` 为 agent 服务,输出 envelope 包裹的 schema。由此 stdout 形成不变量:**要么 envelope JSON,要么 help/version 文本,无第三种形态**。
- **三处输出同源生成**:clap doc comment 一处维护,L0/L1/L2 三处渲染,禁止手写 help 字符串。
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
   `anchor` | `ref-replay` | `item-container`),全部失效才报 `STALE_REF`(@eN)/`NOT_FOUND`(语义选择器);
4. 重定位在同 snapshot_id 内自动进行(agent 无感),跨 snapshot 必须重取——契约简单严格,优于模糊缓存。

## 6. token 控制

- `snapshot` 默认输出紧凑模式(role + name + ref + actionable,一行一元素);
- `--skeleton`:大 UI 折叠非交互容器(agent-desktop 实测 78–96% token 削减);
- `find`/`inspect` 是"按需深查",鼓励 agent 先窄后深,反对全树 dump。

## 7. 安全边界

- 物理指针注入(真实移动/点击鼠标)必须显式 `--physical`,默认仅 UIA 后台调用;键盘注入的授权是强制 `--app` + 护栏 + 输入占用锁(见 §2 规则 2);
- 管理员窗口/跨完整性级别受 UIPI 限制时返回 `PERM_DENIED` 而非静默失败;
- 敏感命令白名单(clipboard-set 到密码框等场景)留给上层 agent 策略,CLI 不做内容审查,但 `--dry-run` 全局可用。

## 7.5 显示信号协议(v0.1.1;docs/11 显示层 / docs/13 拉取哲学的落地)

stdout envelope 保持纯净(不塞进度文本);状态走旁路四原语,消费端全拉取、零订阅:

1. **状态快照** `%LOCALAPPDATA%ctl\state.json`:命令开始/结束/batch 步间写入
   `{pid, command, app, step, phase(running|done|error|stopped), ts_ms}`;
2. **停止标志** `stop-requested`(粘性,存在即停止):输入锁获取点与 batch 步间
   检查 → `ABORTED`;只有显式动作清除(actl-signal 点击);
3. **输入占用锁** `actl-input-lock`(命名互斥体):"正在操控物理输入"的权威信号,
   外部程序可 OpenMutex+Wait(0) 观测;
4. **失联判定**:状态心跳 >10s → 消费端如实显示"失联",不显示成功。

伴随程序 `actl-signal`(actl-uia 第二 bin,纯 Win32 零新依赖):置顶状态条
(`WS_EX_NOACTIVATE` 不抢焦点)+ 托盘 + **Ctrl+Alt+F12 紧急停止**(被占时降级为
点击停止);状态灯取市场通行色:绿=执行中 · 琥珀=注入 · 红=停止 · 灰=空闲 · 暗=失联。

## 8. 里程碑

各阶段范围、工期、验收的**唯一来源**:[ROADMAP](ROADMAP.md)。本文不重复维护版本计划。

## 9. 明确不做

非目标清单**唯一来源**:[PRD §1.3](PRD.md);背景论证见 [02 §4](02-gap-analysis.md)。
