---
name: actl
description: Operate Windows desktop applications (click, type, read state) via the actl CLI - UIA-precise, JSON-verifiable, no screenshots. Use when the task involves native Windows apps: calculators, notepads, file dialogs, installers, legacy Win32 tools.
---

# actl - Windows 桌面操作(agent skill)

actl 通过系统无障碍树(UIA)精确定位控件,像 Playwright 操作 DOM 一样操作桌面应用。
每条命令输出 envelope JSON;失败带错误码 + 恢复建议 + 结构化 evidence,按建议自愈即可。

## 心智模型

**感知(snapshot/find)→ 决策 → 执行(click/type/press)→ 确认(verify/--expect)**。
禁止跳过确认:ok:true 只代表命令完成,不代表任务达成。

## 综合任务与业务流程

actl 是统一技能入口。收到包含多步决策或跨应用操作的任务时，先查
[workflows/README.md](workflows/README.md) 的流程索引，再按需读取 `apps/` 中的应用知识。
可复用业务 YAML 放在 `workflows/`，不为每个业务任务新建独立 skill。声明式输入用 `--inputs values.yml` 传入 flow-validate / flow-run；恢复沿用保存的具体计划，不重新生成任务。
应用档案描述软件的操作特点，流程描述完整业务目标的步骤、核验与恢复。

先核对流程适用条件、必需信息、用户授权和验证状态，再按现行协议准备计划并运行
`actl flow-validate --file <计划文件>`；校验通过不代表真实应用验收通过。
当前没有已交付的业务流程。只读步骤支持显式 `outputs` 导出和后续完整参数 `bind`，
用法见 [流程使用说明](references/flows.md)；变量插值和子流程调用仍未实现。
个人任务值和运行状态不回写技能文件，测试计划仍放 `fixtures/flows/`。

## GUI 录制与草稿改写

用户要求"演示一遍就自动化"或提供人工演示时：`actl record-start --id <id>` 开始录制
（用户正常操作，会话有界，密码字段键不落盘），`actl record-status --id <id>` 查看，
`actl record-stop` 停止并返回机械草稿 `records/<id>/draft.yml` + 证据文件 `trace.jsonl`。
草稿可 flow-validate，但目标选择、等待与验证是保守机械形态，TODO 项以注释携带
trace 行号证据。改写草稿（选择器稳健化、TODO 修复、参数化、噪音清除）与重放对比
验收的完整程序见 [录制草稿改写说明](references/record-rewrite.md)；改写产物留在本地
`records/<id>/rewritten.yml`。录制与探索模式互补：录制是"人演示→YAML"，
探索是"agent 自主→YAML"。

## 任务启动与桌面整理

开始桌面任务可先调用 `actl display-start`：启动同目录的 actl-signal 并等待可见画面。
`display-status` 只查询。观察类命令可独立运行；改变应用/输入/剪贴板的动作由程序检查显示就绪、
确认本次动作预告后执行，skill 不需要轮询进程。`NOT_ACTIONABLE` 且说明显示不可用时，
检查配套版本与状态条是否被隐藏；不要绕过检查。`ABORTED` 表示本次调用不能再用，核对已完成结果后发起新调用。
历史停止不阻断新观察；新写操作在开始/继续时确认，明确授权的 computer use 代操作边界见末尾“桌面交接”。`execution-status` 区分 stopped 与 repair_required；
明确请求修复时使用 `execution-recover`，它归档原记录并废止旧调用，不授予权限或自动恢复任务。不要手动删停止文件。

整理桌面是可选步骤，仅在任务需要或用户要求时使用：

- 单个窗口：`minimize-window --hwnd <句柄>` / `restore-window --hwnd <句柄>`。
- 指定窗口并记录恢复信息：`desktop-prepare --hwnd <句柄>`（可重复 `--hwnd`）。
- 用户明确要求整理整个桌面：`desktop-prepare --all`。系统/工具/置顶/owned 窗口及显示端被排除。
- 保存返回的 `session`（失败时检查 `error.evidence.session`），结束后运行
  `desktop-restore --session <session>`，逐项检查 restored/skipped。只恢复实例、位置与最小化状态仍匹配的窗口。

不要用 `Win+D` 切换代替恢复记录；不要为清理桌面关闭用户窗口。任务新建窗口仍按下文规则优雅关闭。
进程中断时恢复记录保留；未确认完成的最小化记录会跳过，需重新观察。当前记录不能追踪用户先恢复再最小化回
完全相同状态的历史，也不承诺恢复精确 Z 序或用户原先焦点。

## 执行循环

### 向用户预览目标

当前源码新增 `preview <target> --app <window> --message <动作说明> --duration 5000`。
多个合理候选需要用户纠正时，从同一 snapshot/find 结果筛出 2–20 个不同 ref，运行 `preview @e3 --candidate @e7 --snapshot <id> --app <window> --duration 30000 --message <动作说明>`。仅 selected=true 表示用户选了目标；关闭/超时不替用户默认选择。使用 selection.target 与 snapshot_id 重新核验后再执行；executed=false，选择不替代开始/继续授权。候选过多先缩小范围，不任意截断。
先用 `actl commands` 确认所用二进制支持。它只显示目标边框和说明，用户可关闭，超时自动结束；不点击、不输入、不授予桌面权限。
`@eN` 必须传同次观察的 `--snapshot`。目标变化时停止预览并重新观察；不要将成功预览解释为用户确认或操作完成。
技术诊断由 agent 使用，给用户说明拟执行的业务动作，不默认展示 UIA 树。首批无候选选择，不得把歧义中的第一个目标当作用户选择。

```bash
actl snapshot --app <窗口标题子串> --skeleton   # 感知:拿 @eN 引用(省 token 用 --skeleton)
actl click @e7 --app <app>                       # 执行:引用重放
actl type role:Document "文本" --app <app>       # 输入:role/name/id 语义定位均可
actl verify --value <target> "<期望>" --app <app> # 确认:立即断言
```

## 关键纪律(违反会产生错误操作)

1. **键盘命令必须 --app**(press/type/key-down/key-up):fail-closed,无 --app 直接 PROTOCOL 拒绝。
2. **中文输入法环境**:文本输入加 `--paste`(剪贴板通道,绕过 IME 改写)。
3. **效果断言用内容,不用窗口标题**(标题滞后/只取首行,实测会说谎):
   `verify --value` 读 UIA 值,或磁盘终验。
4. **AMBIGUOUS 必须收紧定位**：优先使用 `uia:{"match":{"role":"Button","name":"确定"},"within":{"id":"panel"}}`。
   match/within 各条件为 AND，容器和目标均要求唯一；name 精确、name_contains 子串，scope 为 children/descendants。
   `find --target <uia:JSON> --app X` 可先核验，返回稳定查询、不返回 ref；组合查询错误的候选也不含 ref。
   普通快照 ref 只在重新观察并绑定 snapshot 后使用，不依据候选序号盲点。
5. **ref 是树遍历序号，动态界面会漂移**:即使同窗口，通知/弹层/列表变化也可能让 @eN 指向另一控件，
   不保证报 STALE_REF。变化后重新定位并核对名称、角色和内容；写操作优先稳定语义选择器。
   `--snapshot <id>` 现在同时核验窗口与实际命中控件的 RuntimeId/PID/角色/名称/AutomationId。
   旧快照缺少元素记录时拒绝 ref 重放，重新 snapshot；不带 --snapshot 的 ref 仍只有序号语义。
   `type --verify-value <完整期望值>` 在同一控件上读回验证；失败可能已输入，不自动重试。
   `press enter --target <输入框> --app X` 核对现有焦点，不主动切换输入框；可加 --snapshot 绑定 ref。
6. **人在用电脑时**:注入会被焦点护栏拒绝(PERM_DENIED/部分送达 stopped_early)——
   这是保护,不是 bug;等空闲再试。紧急情况人按 Ctrl+Alt+F12(actl-signal),你的注入会收到 ABORTED。
7. **PowerShell 里 `@eN` 引用必须加引号**:PowerShell 把裸 `@e7` 当 splat 语法吞掉
   (实测报"required arguments not provided"),脚本中写 `'@e7'`;bash 无此问题。
   读回恒空不等于写入丢失:系统对话框 Edit(provider 背书)的 get value 可能恒空——
   写通道核验用 `type --verify-value`(键盘路径可达),勿以读回空判失败。

## 命令速查

感知:snapshot(--skeleton/--view content/--keywords K1,K2/--within uia:JSON --depth N --limit N) find(--name/--role/--id 或 --target uia:JSON) get list-windows status commands

已知容器时，使用 `snapshot --app <window> --within 'uia:{"match":{"id":"panel"}}' --depth 3 --limit 100` 减少无关输出。
已知任务词汇时，用 `--keywords 'save,导出'` 只保留命中的交互元素+祖先链（零命中返回空，不静默回退全量——这是任务投影，探索模式的地基）。
容器必须唯一，根节点深度为 0；检查 truncated/truncation_reasons，不把截断解释为不存在。
局部 ref 从 @e1 重新编号，后续命令必须传同次返回的 `--snapshot <snapshot_id>`；不要混用全窗口或其他局部快照的编号。
容器变化后重新观察；没有唯一容器时先 find/inspect，不用扩大输出代替定位。此功能不保证降低总扫描耗时。
验收输出削减使用仓库 `cargo xtask diagnose local-observation --hwnd HWND --within SELECTOR`：传已核对的 HWND 和严格容器选择器。
只在完整/局部观察均未截断、前后内容和身份稳定时比较；分别检查 elements 与完整 envelope 字节，
不把字节等同 token。深树定位优先提供精确 id/name：由原生查询完整候选再核验唯一性。
已知 role 可走原生候选查询；只有 name_contains 等宽泛查询仍有遍历预算；原生候选超限或 provider 失败须保留错误，不能按首个候选继续。
执行:click(--physical/--expect/--near/--no-activate) right-click double-click hover
     drag(--to/--path X,Y 轨迹/xy: 框选两端守卫) scroll type(--paste)
     press(--expect) key-down(--duration 长按自动释放) key-up set-value
浏览器:browser-launch(--profile/--proxy/--headless/--browser-executable 接受别名) browser-navigate browser-read(--property/--iframe)
     browser-click(--expect-text) browser-type browser-verify(--newest-alive/--none/--css) browser-close browser-list
窗口:launch(接受 app-registry 别名) focus-window close-window resize-window wait(--element/--window/--gone/--stable/--idle) batch
桌面:minimize-window restore-window desktop-prepare desktop-restore display-start display-status execution-status execution-recover
验证:verify(--exists/--value/--contains/--checked) extract(--table)
系统:app-register(--pattern) app-list app-remove get-clipboard set-clipboard
流程:flow-validate flow-run(--paused) flow-status flow-pause flow-resume(--retry-step/--decision) flow-revise(--accept-rerun/--resume,修订续跑) flow-evidence(--step,失败采集包) flow-clean
录制:record-start(--id/--max-seconds) record-worker record-stop(--id) record-status(--id)——草稿改写见 references/record-rewrite.md

## 外部 UIA 对照诊断

控件缺失、定位歧义、值或焦点矛盾时，可复用本机 `winapp ui` 做有界只读对照。
先查 `winapp --version` 和子命令 `--help`；用 `list-windows -a <进程> --json`
核对唯一目标，再 `inspect -w <本次 HWND> --depth 5 --json`，按需 get-property/get-value/get-focused。
同一现场比较树视图、深度、过滤、角色、ID、父容器和读取通道；winapp 使用 Control View，
不能仅凭节点数量断言 actl 漏项。保留退出码、stderr 和原始 JSON，失败不转成空树。
winapp 的 slug 不作为 actl ref 使用；回到 actl 后重新观察并绑定 snapshot。
不通过外部工具绕过急停、让行、权限或焦点护栏；截图/录制可能改变窗口状态，不按纯查询处理。

## 假成功应用的操作范式(UIA 撒谎时)

部分自绘框架(mmui/Qt 系,如微信 4.x)的元素声明支持 Invoke/Select,调用返回 ok 却不产生
任何效果——UIA"假成功"。标准循环是**UIA 优先 + 物化验证 + 受控降级**:

```bash
# 1. UIA 优先,带后置条件(不是新窗口,是"目标元素应可解析"):
actl click id:session_item_张三 --app 微信 --expect element:id:chat_message_list
#    通过 → 导航真实发生,继续。
#    ASSERTION_FAILED → 假成功暴露(evidence.cause 区分"假成功"与"目标错误")
# 2. 降级一次物理(全部门禁照走:命中检查/输入锁/信号条预告):
actl click id:session_item_张三 --app 微信 --physical
actl wait --element id:chat_message_list --app 微信 --timeout 4000
```

要点:`--expect element:<target>` 已存在即成立(后置条件,非"新出现");不要跳过第 1 步
直接 --physical(每次都占物理输入);也不要在假成功被暴露后盲目重试 UIA(它是确定性谎言)。

## 人在场与后台执行

- `wait --idle 8000 --timeout 120000`:等全系统键鼠静默 ≥8s——长批/采集任务的**前置门**,
  "人不在"才开工;只观测物理输入(UIA 注入对它不可见)。
- 指针族 `--no-activate`(click --physical / right-click 等):点击落地但目标窗口**不抢前台、
  不改 Z 序**,结束光标送回(drag 除外)。与空闲门组合 = 采集类任务两件套;键盘无此模式
  (窗口无焦点,键会打进用户当前前台——CLI 直接拒绝)。
- 人突然回来:注入段会被焦点护栏/输入锁拦下,或人按 Ctrl+Alt+F12 紧急停止;样条仍有动作预告。

目标语法:`@eN`(重放,须 --app)· `name:<子串>` · `id:<AutomationId>` · `role:<Type>` ·
`role:<Type>#<序数>` · `id:<AutomationId>#<序数>`(多文档页同名 id) ·
`xy:<x>,<y>`(坐标;click 须 --app + --physical;drag xy:→xy: 框选须 --app,两端窗口边界+遮挡守卫);
窗口锚定 `--app` 可用 `hwnd:<n>`(SPA 跳转改标题时);
多候选消歧:`--near <锚点>`。
`drag --path X,Y`(可重复 ≤64 点,绝对物理像素)走 8px 插值轨迹(画布/签名);
`key-down --duration <ms>`(50-60000)为长按:按下→持有→自动释放(急停即释放)。

## 弹窗处理协议

弹窗分三类,处理方式不同;判定入口是 `list-windows`(看 `foreground`/`owner_title`/`topmost`):

窗口清单与画面矛盾时，补 `list-windows --native`：原生顶层 HWND 清单包含隐藏和无标题窗口，
不依赖 UIA 树就绪。它不能证明任意子 HWND 已销毁；不要仅凭默认 UIA 清单缺项宣称窗口关闭。
确认已知窗口是否销毁应使用 `list-windows --native --hwnd <n>`，支持计算器这类子 HWND。
batch 的 `windows_created` 是原生前后差集，不是任务归属证明。出现
`window_observation_error` 时，已执行步骤仍有效，但新增窗口情况未知；先观察，不自动清理或重放写操作。

1. **宿主弹窗(另存为/打开等)**:owner_title 非空 = 挂靠某窗口的对话框。直接
   `snapshot --app <弹窗标题>` → 定位控件操作(文件名走 `set-value`);用
   `press ctrl+s --app X --expect window:<弹窗标题>` 断言其出现。
2. **消息框(确认/警告)**:普通窗口 + 按钮元素,`click name:确定`(或 是/否/取消)
   即可;中文按钮名可直接定位。
3. **意外弹窗(更新提示/浮窗)**:batch 的 steps[] 会带 `blocked_by`(前台窗口既
   不在批前基线、也不匹配本批任何 --app)——这就是元凶。处理:读 blocked_by 的
   title → snapshot 它 → 关闭/点击 → 重试原步骤。**不要盲目重试**:type 被抢焦点
   会停在批边界并如实上报 delivered/stopped_early,先处理弹窗再续。

收尾：检查 `windows_created`（data 或失败 evidence），逐个判断用途。成果窗口（文档、报表、
结果页面）默认保留；仅关闭本任务创建且已不再需要的临时窗口。用户原有窗口保留，不能仅凭
“本轮新建”就关闭。清理使用 `close-window --pid/--hwnd` 优雅关闭（禁 taskkill）。
有未保存提示时先判断成果是否需要保留；不要为了清理自动丢弃内容。

## 任务记录与反馈

复杂且可预先确定步骤的任务可先写 YAML，使用 `flow-validate --file` 全部预检，再
`flow-run --file <file> --id <id>`；可加 `--paused` 先保存待运行计划。编写与恢复见 [流程使用说明](references/flows.md)。
使用语义选择器和参数数组；写操作必须有业务充分的 verify，禁止自动重试。
通过 `flow-pause --id` 请求暂停，以 `flow-status` 的 paused 确认；在途调用可能仍未结束。
`flow-resume --id` 先核对未确认步骤，needs_review 时先观察实际结果；只有明确需要重复时
才使用 `--retry-step <step-id>`。运行/恢复自动以 run ID 关联历史。
修改源 YAML 不改变已有运行；**步骤级修复走 `flow-revise --id <run> --file <修后.yml>`**
（版本化修订原地续跑：完成步规格未变保留结果，写步变更需 `--accept-rerun` 点名，
uncertain 步变更即重试授权，谱系经 flow-status 的 revisions 可见）；路线级重构或需要
干净对照时才开新 ID。失败现场的采集包用 `flow-evidence --id` 读取，含失败瞬间弹层。

- 用同一个 `ACTL_TASK_ID` 关联任务中的调用（1–80 位 ASCII 字母、数字、`-`、`_`）。
  在当前任务的进程环境中设置，避免修改用户全局环境或混用其他任务的 ID。
- `actl history --task <id> --limit 20` 查询事件与 call_id。记录不含输入正文、窗口标题、截图，
  仅覆盖 CLI 成功解析后的调用；没有结束事件可能是仍在运行、被中断或记录失败。
- 验证最终结果后，用 `actl task-archive --file report.json` 归档简短报告，格式见 [任务记录](references/task-records.md)。
  分别写清结果、验证依据、部分完成项和反馈；推测标为未验证，用户反馈与智能体判断分开。
  报告是调用方陈述，不会自动证明任务成功；截图/文档只记引用，不自动复制。
- 重复提交、发送、写入的结果不明确时先核实，不盲目重试；定位失效可以重新观察再判断。
- 用户接管或停止后，先确认在途操作的结果。YAML 流程支持命令边界暂停恢复，普通单次调用
  不受流程暂停控制；不宣称整个桌面“可以操作”，不自动清除紧急停止。继续前重新观察目标。
- 不自动回写剪贴板或恢复焦点，以免覆盖用户的新操作。只恢复仍符合桌面恢复记录的窗口。
- 经验先作为候选归档，经复现确认后再更新应用 pack；通用流程进 skill，缺陷进代码与测试。
  成功时简报成果和位置；部分成功明确数量及未完成项。普通步骤不反复通知。

## 应用路线(Office 实测背书)

**L0 路线命中重放——任务到达的第一分岔(docs/42 分层 L0,知识飞轮的读侧)**:动手前先读
目标应用的 `apps/<slug>.yml` 路线,按序分岔:

1. **命中 `status: verified` 的路线(goal 与所需输入对得上)** → 直接重放,零探索:
   ≥3 步按验收脚本纪律转成 flow 文件放 `workflows/` 交给 `flow-run`(获得
   verify/暂停/治理),短路线走 batch。路线步骤是语义选择器 + `{window}` 绑定
   (绑定当前窗口锚),pack 不存 ref——refs 永远当轮重新观察;verify 字段即断言,
   如实核对,失败不静默重试。
2. **只有 unverified 或无路线** → 不得直接执行,走探索引擎:flow 文件写 `explore:`
   段(task/app/keywords/goal 四要素,见 docs/06 探索节),`flow-run` 启动;决策卡
   经 flow-status 的 `data.decision_card` 读取,`flow-resume --id <run> --decision
   <choice_id>` 应答。完成后 `explore-writeback --id <run>` 抽路线片段,合并进
   pack(恒 unverified,人工复核转 verified)——下次同类任务即走分岔 1,闭环。

操作 Excel/Word/微信前先读对应档案:`apps/excel.yml`、`apps/word.yml`、`apps/wechat.yml`(含路线
steps、quirks 与解法、验证方法;路线的 `status` 是信任分级,`unverified` 不得直接执行)。
浏览器(Edge/Chrome,知识外溢到 WebView2/Electron 系)读 `apps/browser.yml`。
为其他应用写档案:格式契约、模板与贡献红线见 [PACKS.md](PACKS.md)。要点速记:

- **Excel 单元格**:名称框 `id:1001` 导航 + 剪贴板直灌(`set-clipboard` →
  `paste --app`)——网格单元格不是 UIA 元素,name 是坐标不是内容;表格抠数用
  `extract role:DataGrid`(GridPattern 直读)。
- **Word 正文**:`get role:Document --property text`(中英文逐字)/
  `set-clipboard` + `paste`。
- **保存 = 原地 ctrl+s,不弹窗**:断言走磁盘终验(文件 mtime/重开读回),
  不要等"另存为"窗口。
- **受保护视图**:TEMP/下载来的文件编辑被静默拒绝(ctrl+v ok 不落格)——
  先 `click name:启用编辑`。
- **启动**:Office/商店应用裸名即可(`launch WINWORD`,App Paths 注册表解析)。

## 视觉兜底协议(UIA 失明时)

当 UIA 树不暴露内容(图片消息、自绘菜单、Canvas 画布)时,截图是最后的兜底通道:

```
actl screenshot --element <ref> --sapp <win>    # 元素级裁剪(经定位链;作用域是 --sapp)
actl screenshot --app <win>                     # 整窗(抗遮挡)
actl screenshot --rect <x,y,w,h>               # 区域(配合 right-click 返回的 point)
```

输出为 PNG 文件路径(envelope data.path),把该文件交给多模态模型即可读图;
模型给出坐标后按**标准闭环纪律**落点(坐标是"约"值,精度依赖图像质量):
① `hover xy:<x>,<y> --app <win>` 预探(不破坏)→ ② 截图核对落点 →
③ `click xy:<x>,<y> --app <win> --physical` → ④ `--expect`/`verify` 收尾。
窗口边界守卫:坐标必须在目标窗口当前 bounds 内,窗口移动即拒——重截图再算
(坐标换算基准 = list-windows 的 rect,物理像素,与截图同坐标系)。
截图是**显式请求的兜底**——默认路径永远是 UIA(确定性和 token 效率优先)。
Canvas 类(mspaint)、自绘菜单(微信右键)、图片消息(微信气泡)、UIA 无名按钮
(抖音互动栏)是四个典型场景。

## 工具分工:actl × playwright(浏览器场景路由)

浏览器任务先路由,再动手:

- **全新上下文 + 无需桌面联动的 web 任务**:用 playwright/browser-use——它们
  在无头/批量场景成熟。
- **actl 自有浏览器会话(CDP 通道)**:`browser-launch` 启动隔离浏览器(默认有头、
  常规 UA、可见窗口),守卫写(`browser-click --expect-text` / `browser-type` 读回核验),
  会话生命周期经 `browser-list` / `browser-verify` 管理。持久 profile(`--profile`)
  支持一次性人工登录复用。适用于需要登录态+守卫写的网页任务。
- **用户真实登录态的桌面浏览器**(企业 SSO 门户、已登录的网页应用):UIA 驱动——
  Chrome 136 起默认 profile 禁 CDP,Playwright 接管不了,UIA 是唯一非视觉通道。
  树形观察用 `snapshot --view content --skeleton`;地址栏走 `press ctrl+l` → paste。

浏览器内操作要点(详见 `apps/browser.yml`):树形观察用
`snapshot --view content --skeleton`(chrome 整枝丢弃);读正文用
`get role:Document --property text`(Document 的 Value 是 URL);滚动/导航后用
`wait --element role:Document --stable` 判定加载完成(不要 sleep);地址栏输入
走 `press ctrl+l` → `set-clipboard` + `paste`。

规则:**三通道可组合**——playwright 做研究/采集,actl CDP 做守卫写落地,
actl UIA 操作用户真实会话或桌面应用。

## 工具分工:actl × wechat-ui-collector

同机互补,按任务选工具:

- **actl**(本 skill):**实时操作**——发消息、搜索、传文件、联系人、朋友圈、
  任何桌面应用。UIA 驱动,逐条原子命令。
- **wechat-ui-collector**:**存量检索**——海量聊天记录(数据库直解析,314 万条
  ~17 分钟,UIA 滚动差三个数量级)、联系人画像、公众号文章、导出 Markdown/JSON。
  CLI:`WechatUiCollector.exe`,随包自带 wechat-records skill。

规则:**"现在要做什么"用 actl;"过去说过什么"用 collector**。两者可组合:
collector 检索到结论 → actl 实时发消息回应。

## 更多

错误码与恢复动作全表:[references/error-codes.md](references/error-codes.md)
本地软件位置查找指南:[references/app-locations.md](references/app-locations.md)
流程使用说明:[references/flows.md](references/flows.md)
录制草稿改写说明:[references/record-rewrite.md](references/record-rewrite.md)
MCP 远程使用(被控端启动配对/远端 agent 纪律/故障速查):[references/mcp-remote.md](references/mcp-remote.md)

### 本地应用别名

`launch <别名>` 和 `browser-launch --browser-executable <别名>` 支持经
`app-register` 注册的本地别名(如 `edge`、`paint`),先 `app-list` 查已注册的别名,
避免每次写完整路径。Store 应用用 `--pattern` 通配注册(路径带版本号,更新后自动重解析)。
详见 [app-locations.md](references/app-locations.md)。


### 桌面交接（显示 v5）

开始写操作前告知用户会出现「开始 / 继续」提示，使用同一 ACTL_TASK_ID 标识当前任务。
默认由用户点击；用户明确授权代操作时，可用 computer use 核对任务和目标后实际点击“开始/继续”，不用逐次询问。不得写授权文件、伪造事件或清急停；用户明确暂停、稍后、停止或接管时不得自动继续。代点击须在验收记录中注明，不能作为人工点击验收。handoff_pending / user_deferred /
human_takeover_or_grant_expired 表示让行；停止后续输入，核对部分结果，再等用户交接。
launch 默认只说明进程启动；可用 --wait-window（选项在程序名前），或显式等待后 focus-window。
查看 display-status.reason 区分提示端异常；focus-window 的 foreground_not_acquired 不等于要提权。

状态栏右侧暂停不等于急停；flow 的继续会核对结果，单条 CLI/脚本没有自动续跑入口。不要把 Details 按钮当作 Continue，也不要因人工让行而自动重放命令。
