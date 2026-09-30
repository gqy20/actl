# 桌面交接与窗口就绪

## 使用方式

### 本仓库已授权的 computer use 代操作

用户已明确允许 agent 在既定测试与排障范围内，通过 computer use 实际点击 actl 的“开始/继续”，无需每次请求用户点击。先核对显示端窗口身份、任务 ID、目标和下一步动作；点击后读取 flow/display 状态，恢复仍经过现场检查和 uncertain 核验。用户明确暂停、稍后、停止或正在接管时不自动继续；原因未知时先诊断，不反复点击。

这是调用方的代操作授权，不新增 YAML 跳过确认字段，不改变显示协议、输入监听或停止代次。不能写内存/授权文件或伪造事件替代可见按钮。computer use 产生的其他输入可能触发让行，点击交接后应退出桌面操作，由 actl 执行；持续让行要查原因，不能循环抢回控制权。代点击的验收记录须注明 computer use，不计为用户亲自点击或真实人工接管验收。下文历史人工验收结论保持原样。

诊断默认使用 actl 的局部观察、日志及截图；需要独立对照或上述代操作时引入 computer use。先做局部复现，再进行整轮验收。flow-validate 只预检资源声明，不创建资源或 paused flow；执行时直接 flow-run，只有确需预先登记并等待时才使用 --paused。

所有经过动作反馈门禁的写操作，先向现有 actl-signal 申请桌面交接。
提示窗口显示任务和当前目标，说明可能切换窗口、使用键鼠与剪贴板。
用户或已获授权的 agent 实际点击「开始 / 继续」后才允许执行；点击「稍后」或 25 秒内未响应，命令返回
`NOT_ACTIONABLE`，不执行请求的动作。弹窗使用不激活窗口样式，不主动移走当前输入焦点。
只读查询不申请授权；观察窗口时不再隐式 ShowWindowAsync。

一组 CLI 命令使用同一 `ACTL_TASK_ID`，flow 使用 run ID，才会复用任务授权。
共享 PowerShell fixture 为同次场景自动生成任务 ID，尊重调用方已有 ID；交接中断后保留窗口，不继续清理。
普通 CLI 没有任务 ID 时仅当前调用有效。授权保存在显示进程内存中：空闲 60 秒、总长 10 分钟、
显示端重启、紧急停止或外部键鼠输入均会使其失效。调用方不得伪造点击或写文件授予授权。

显示协议 v8 支持 `handoff: {approval: once_per_run}`：同任务不因空闲 60 秒或总长 10 分钟失效。
首次提示明确说明本任务确认一次，人工输入/停止/监听异常仍撤销授权。默认 `expiring` 保持旧超时。
flow 在顶层设置 handoff；普通脚本设置 `ACTL_TASK_CONFIG` 指向含 version: 1 和 handoff 的 YAML，
并为整轮生成唯一 ACTL_TASK_ID，不能跨运行复用。配置只描述策略，不保存已授权状态。
`skills/actl/workflows/verify-applications.yml` 将应用流程组织为同一 flow，共用 once_per_run 交接策略；出现交接中断后暂停并保留现场。
现有 `--physical` 显式要求不变；点击交接按钮不代替 `--physical`。

### 误触自动恢复（pardon_input，2026-09-29）

外部输入撤销授权后，若撤销源及其后所有输入都是**纯鼠标移动**（无任何按键、点击、滚轮），
且静默窗内无新输入、前台窗口与出卡时一致，显示端在预算内自动重新授权，不需要用户点击。
任何键盘事件永远不赦免——单个按键完全可能是真人接管的开始，自动恢复的失败方向必须
是"多停一次"而不是"在人还在场时注入"。待执行动作为键盘通道（type/press/key-down/key-up/paste，
种类未知同样按键盘处理）时不自动恢复。因交接暂停（reason=handoff_pending）且执行器已退出的
流程满足同样条件时自动发起续跑；用户暂停、检查点、待核对与停止状态永不自动继续。

策略在 YAML 的 handoff 块配置，逐流程优先于全局默认：

```yaml
handoff:
  approval: once_per_run
  pardon_input:
    enabled: true          # 缺省 true
    quiet_ms: 4000         # 静默窗 1000..=10000，缺省 4000
    budget: 2              # 每任务自动恢复次数 1..=5，缺省 2；耗尽回到显式确认
    require_foreground: true   # 恢复前核验前台未变，缺省 true
    keyboard_safe_only: true   # 键盘通道动作永不自动恢复，缺省 true
```

每次自动授权记录 consent_decision（reason=input_pardoned），预算按任务计、不跨任务共享；
`ACTL_HANDOFF_PARDON=0` 环境变量可全局停用（紧急回退，优先于 YAML）。
预算与策略在显示端内存中维护，显示端重启后预算重新计数（journal 保留历史记录）。

### 预授权写入（显示协议 v10，2026-09-30）

针对"重复点击开始太烦"的体验问题，提供两条**由用户动作授予**的常驻信任通道（`Approval::Standing`）：

1. **托盘开关「预授权」**：右键 actl 托盘图标 → 预授权 → 开/关。开启后所有桌面写入免出卡（REQUEST 即刻 ALLOWED，审计 reason=preauthorized），横条常显「预授权·」前缀。开关状态持久（`%LOCALAPPDATA%\actl\preauth.json`，仅显示端 UI 写入），关闭立即撤销常驻同意。注意：预授权只去掉点击，**用户正在输入时仍会让行**（安静后自动继续）——注入永远不与真人抢桌面。适合 timing sweep/regress 这类开发诊断与本机自动化常驻场景。
2. **流程声明 `approval: standing`**：flow YAML 的 handoff 段写 `approval: standing`——声明不构成授权，首次仍点一次「开始」，之后本会话内该流程常驻（闲置/时长/外部输入都不再要求点击）。适合"这个流程我信任，一次就行"。

Standing 语义与安全补偿：不过期；用户输入计数变化时**让行而非撤销**（安静后自动通过，无需点击）；急停、输入监听不可用、停止代次变化照常杀死信任；动作预告照常上屏；横条指示常在。协议版本 9→10，actl 与 actl-signal 需成对升级。

**有界等待安静（显示协议 v11，2026-10 批次 3）**：v10 的 Standing 让行是"该次调用立即失败"——流任务表现为 action_failed 暂停，误触容忍反而低于默认模式的 pardon。v11 起 CHECK 响应区分两种让行：授权仍在、仅输入活跃 → 新响应 `YIELD_BUSY`（9），客户端在有界窗口内（**3 秒、150ms 轮询**）等待安静——安静即通过、误触无感；等待期间**急停/停止即时生效**；超时仍活跃才按让行失败（证据带 `yield=busy_timeout` 与 `waited_ms`）。授权真被撤销/过期/任务变更 → 维持 YIELDED，立即失败不走等待（默认模式体验不变）。BUSY 中间响应不落 consent_decision 审计，终态才记一次。键盘通道动作等待通过后仍走原有焦点复核。协议 10→11 需成对升级。

### 修复检测与 auto_resume（2026-10 批次 4）

action_failed 暂停期间，显示端以 **5 秒节奏静默运行阻塞步骤（首个未完成步骤，与引擎口径一致）的 before/verify**（均为只读；子进程带 `ACTL_SILENT_PROBE`，不发布调用状态——横条不闪"后台处理"），结果记 `repair_probe` 流程事件。暂停超 10 分钟停止探测。**双判据**（防代做双重生效）：

| 探测结果 | 横条 | 含义 |
|---|---|---|
| before 过、verify 未过 | 「障碍已清除 · 可重试」 | 点继续将重发动作，大概率成功 |
| verify 过（无论 before） | 「结果已存在 · 继续前请核对」 | 可能被手动完成；重发有双重生效风险 |
| 都未过 | 无提示 | 不念叨"还没修好" |

**auto_resume 策略**：流程 YAML 显式声明 `auto_resume: action_failed`（默认关，与预授权正交——授权通道与重试策略分离）时，Retriable 判定下显示端自动派发一次 flow-resume，预算 **2 次/运行**（伴侣内存计，重启重置——保守方向）；预算耗尽或未声明时降级为高亮提示。派发前核验停止代次健康且无急停；user_paused/checkpoint/needs_review 永不自动续跑；每次自动续跑记 `auto_resume` 流程事件。

按流程指纹（冻结计划哈希）持久信任的「始终信任此流程」（跨重启、模板变更重询）为后续方向，见 ROADMAP。

## 人工接管

显示端在独立消息线程安装 WH_KEYBOARD_LL / WH_MOUSE_LL，避免绘制和文件 IO 阻塞回调，被动统计输入序号，不拦截输入，
不存储按键、文本或鼠标坐标。仅忽略带本工具标记且 Windows 标记为 injected 的事件；
其他自动化程序发出的输入也触发让行。每个注入批次前同步向显示端核验授权，
同一输入框内的人机竞争也能触发中断。

失效返回 `evidence.stage=handoff`，`reason=human_takeover_or_grant_expired`。
单条 CLI 结束，flow 保存为 paused，不确定写步骤保留 uncertain；恢复先验证结果，
不能把暂停理解为尚未执行，也不会自动重放。分段输入错误附 `delivered_before_error`
和 `failed_segment_size`，后者的实际送达量可能未知。

用户输入与 SendInput 之间仍有竞态，已经进入系统的一个批次无法撤销；这不是全局输入锁。
Windows 对低级 hook 有超时移除机制，没有直接的 hook 存活查询。现增加 Raw Input 键鼠注册作为
独立的物理活动通道，只累计 WM_INPUT 次数；注册发生在显示程序的专用消息线程，不污染库调用方。
每次交接核验检查两个设备类仍绑定本监听窗口，以及消息泵的 100ms 心跳未超过 1000ms。
注册丢失、线程退出或心跳过期返回 `input_monitor_unavailable` 并撤销授权，恢复健康后仍需重新人工授权。
不会注入假按键探测 hook，也不会循环重装 hook 或自动续跑。

这降低了对单一低级 hook 的依赖，不证明所有输入都可检测：第三方合成输入仍依赖低级 hook；
真实键鼠、远程桌面、锁屏、设备热插拔及 hook 超时现场尚待交互验收。原有停止、前台和焦点护栏继续生效。
依据：[低级键盘 hook](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)、
[Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input)。本轮证据见 [44](44-phase2-acceptance.md)。

## 软件没弹出来时

`focus-window` 先诊断窗口是否有效、禁用/有模态阻挡、无响应、隐藏或离屏，
对最小化窗口请求恢复，再发一次 SetForegroundWindow 请求，有界轮询原生前台（默认 1 秒）。
显示协议 v9 在任务授权通过后转交前台资格，动作最外层结束后归还，支持连续 CLI 及进程内下一动作；核对转交与归还结果，不把 once_per_run 当作永久前台资格。六步实测与未验收边界见 [62](62-foreground-handoff.md)。
返回成功必须对应目标根 HWND。不会通过模拟 Alt、反复切焦点规避系统前台限制。
UIA 属性调用本身受 provider 调度影响，轮询超时不等于整个 COM 调用可被硬中断。

`launch` 默认仍只启动进程，增加 `window_ready:false`，不暗示应用已可操作。
可用 `launch --wait-window <标题子串> --timeout 5000 <程序> [参数]` 等待可见窗口，
并执行上述恢复/激活核验；选项须放在程序名前。匹配多个窗口返回 AMBIGUOUS。
启动器可能转交到已有进程，因此返回匹配的 HWND/PID，标注 `matched_by:title`，
不声称它一定属于刚启动的 PID。失败证据保留 `spawned_pid` 与 `relaunch:false`，
不重复启动。已有窗口应直接使用 HWND 定位的 focus-window。

`display-status` 结合原生窗口与租约返回原因：缺失、版本不匹配、无响应、心跳过期、
隐藏、遮挡/离屏、截图暂时隐藏、未完成首帧、身份不匹配或 ready。
`display-start` / 动作门禁会对兼容显示端尝试一次无激活恢复和位置校正，仍不可见则拒绝。
旧版显示端须显式正常替换，不自动清急停，也不强杀进程。显示 v6 将普通关闭与急停分离，解除急停前同步撤销主显示端授权；生命周期见 [35](35-stop-lifecycle.md)。
菜单/详情创建失败与前台激活失败写入 `popup.json`，由 display-status 返回最后一次诊断
及时间戳；历史 ready 不是当前弹窗仍存在的证明。

## 协议与验收

显示协议提升为 v4，拒绝旧显示端跳过交接。短期请求文件只含 call/task/target/timestamp，
调用退出清理，显示端不接受超过 2 秒未刷新的请求。授权没有磁盘标记。
此变化需要随 spec 提交，包含 docs/06、CHANGELOG 和契约 golden。

自动化覆盖：任务隔离、显式授权、外部输入撤销、时钟回退、超时、注入事件分类、
交接错误 envelope、flow 暂停后不重放，以及分段输入送达量。
真机已验证（target/handoff-live）：
- 旧显示端报告 version_mismatch；正常替换后 v4 ready，急停始终 false。
- 用户确认点击「开始」后又操作键鼠，日志记录 allowed → human_takeover_or_grant_expired，
  在 action preparing 阶段退出，目标程序未启动（pending.stdout.json）。
- 未点击的请求返回 handoff_pending，未发起目标动作，请求文件自动清理（unanswered.stdout.json）。
- Computer Use 读到了任务、目标和两个按钮；截图发现高 DPI 布局偏小，已补缩放/字体及原生控件测试。
- 完整软件任务在人工交接后的成功路径仍需下一轮真机验收；不能用自动化点击「开始」代替人工。



## 暂停显示与继续入口（v5）

- 蓝色「后台处理 · 可操作」：只读查询且没有其他调用占用桌面。
- 黄色「桌面操作 · 暂勿操作」：正在准备/执行界面写操作，右侧为暂停图标。
- 黄色「等待交接／已暂停／已让行 · 可操作」：显示具体原因；有可恢复 flow 时右侧为继续。
- 条件或显示故障显示「无法继续／条件未满足」，详情保留原因。
- 普通失败不再覆盖人工接管状态。诊断调用结束也不会挤掉正在等待恢复的任务。

右侧继续只绑定屏幕所显示的流程，先重新读取状态，再授予该任务桌面权限并继续。
已进入系统的输入仍不能撤回；恢复先验证 uncertain 步骤。明确 action_started:false 的交接拒绝
保留 pending，可直接继续到未开始的步骤。单 CLI 和 PowerShell 脚本保留现场，由调用方接续，
当前版本不提供假继续按钮。紧急停止独立于暂停，不会自动解除。

流程绑定横条的时效（2026-09-30 修订）：人工暂停（paused）与停止（stopped）持久保留继续入口；
**待核对（needs_review）只在 30 分钟内钉住横条**——超时无人核对即视为结果已被人工消化，
横条回空闲（状态文件与 `flow-status` 核对/继续入口不受影响，只是不再霸屏出按钮）。
没有这条时效，历史待核对流程会永久占据横条的「检查并继续」按钮。

需要界面继续的验证使用 `skills/actl/workflows/vscode-resume.yml`：动作在执行前登记为持久化 flow，暂停后由绑定 run ID 的继续按钮恢复，不重新运行准备脚本。核验判据与当前迁移范围见 [55](55-resumable-desktop-tasks.md)。

重复提示诊断记录在本地 history 的 consent_decision：区分实际 shown、用户 granted 与 reused，附带策略、失效原因和两路活动计数，不记录输入内容。Raw Input 只排除无设备且携带 actl 注入标记的自身事件，其余来源或解析失败仍让行。复现和验收见 [54](54-repeated-handoff.md)。


v5 验证：`target/cooperation-gate-final.log` 完整门禁通过；模型覆盖人工让行文案、后台可操作、
正确 flow 绑定、急停禁止继续、查询不覆盖暂停、单调用不提供假恢复；引擎覆盖未开始步骤继续与
不确定写步骤不重放。`target/cooperation-live/display-status.json` 确认新版 v5 显示端就绪。
此次启动验收发现实际急停已开启，保留原标记；桌面点按和恢复业务软件的正向路径未继续验收。

状态条悬停仅高亮右侧操作按钮，不替换主状态文字；点击暂停后鼠标停留原位仍显示正在暂停、已暂停、待核对等当前状态。按钮图标随当前可用操作变化，详细操作说明保留在详情中。
