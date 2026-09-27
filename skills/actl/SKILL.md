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

## 任务启动与桌面整理

开始桌面任务可先调用 `actl display-start`：启动同目录的 actl-signal 并等待可见画面。
`display-status` 只查询。观察类命令可独立运行；改变应用/输入/剪贴板的动作由程序检查显示就绪、
确认本次动作预告后执行，skill 不需要轮询进程。`NOT_ACTIONABLE` 且说明显示不可用时，
检查配套版本与状态条是否被隐藏；不要绕过检查。启动显示端不清除停止标志，`ABORTED` 时等待用户恢复。

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
4. **AMBIGUOUS 不是失败**:错误里 `evidence.candidates` 带可重放 ref,直接选一个
   `click @eN --app X` 重试,不必重新 snapshot。
5. **ref 跨步骤会漂移**:同窗口内 @eN 稳定;窗口重开/切换后 ref 失效 → STALE_REF → 重拍快照。
   长流程建议 `--snapshot <id>` 校验窗口实例。
6. **人在用电脑时**:注入会被焦点护栏拒绝(PERM_DENIED/部分送达 stopped_early)——
   这是保护,不是 bug;等空闲再试。紧急情况人按 Ctrl+Alt+F12(actl-signal),你的注入会收到 ABORTED。

## 命令速查

感知:snapshot(--skeleton/--view content) find(--name/--role/--id/--near) get list-windows status commands
执行:click(--physical/--expect/--near) right-click double-click hover drag scroll type(--paste)
     press(--expect) key-down key-up set-value
窗口:launch focus-window close-window resize-window wait(--element/--window/--gone/--stable) batch
桌面:minimize-window restore-window desktop-prepare desktop-restore display-start display-status
验证:verify(--exists/--value/--contains/--checked) extract(--table)
系统:get-clipboard set-clipboard

目标语法:`@eN`(重放,须 --app)· `name:<子串>` · `id:<AutomationId>` · `role:<Type>` ·
`role:<Type>#<序数>` · `id:<AutomationId>#<序数>`(多文档页同名 id) ·
`xy:<x>,<y>`(坐标,须 --app + --physical,窗口边界守卫);窗口锚定 `--app` 可用
`hwnd:<n>`(SPA 跳转改标题时);
多候选消歧:`--near <锚点>`。

## 弹窗处理协议

弹窗分三类,处理方式不同;判定入口是 `list-windows`(看 `foreground`/`owner_title`/`topmost`):

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

- 用同一个 `ACTL_TASK_ID` 关联任务中的调用（1–80 位 ASCII 字母、数字、`-`、`_`）。
  在当前任务的进程环境中设置，避免修改用户全局环境或混用其他任务的 ID。
- `actl history --task <id> --limit 20` 查询事件与 call_id。记录不含输入正文、窗口标题、截图，
  仅覆盖 CLI 成功解析后的调用；没有结束事件可能是仍在运行、被中断或记录失败。
- 验证最终结果后，用 `actl task-archive --file report.json` 归档简短报告，格式见 docs/06 §7.6。
  分别写清结果、验证依据、部分完成项和反馈；推测标为未验证，用户反馈与智能体判断分开。
  报告是调用方陈述，不会自动证明任务成功；截图/文档只记引用，不自动复制。
- 重复提交、发送、写入的结果不明确时先核实，不盲目重试；定位失效可以重新观察再判断。
- 用户接管或停止后，先确认在途操作的结果。当前没有跨调用暂停恢复协议，不宣称“可以操作”
  或自动清除停止标志；用户明确允许后重新观察窗口与焦点，再决定下一步。
- 不自动回写剪贴板或恢复焦点，以免覆盖用户的新操作。只恢复仍符合桌面恢复记录的窗口。
- 经验先作为候选归档，经复现确认后再更新应用 pack；通用流程进 skill，缺陷进代码与测试。
  成功时简报成果和位置；部分成功明确数量及未完成项。普通步骤不反复通知。

## 应用路线(Office 实测背书)

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

- **全新上下文的 web 任务**(抓取、批量表单、headless、测试):用 playwright/
  browser-use 类工具——CDP 通道,DOM 级选择器,那是它们的主场。
- **actl 的浏览器领地**:**用户真实登录态的会话**(企业 SSO 门户、已登录的
  网页应用——Chrome 136 起默认 profile 禁 CDP,Playwright 接管不了,UIA 是唯一
  非视觉通道)+ **浏览器内外联动**(下载→文件对话框→桌面应用、地址栏/标签/
  扩展弹窗等 chrome 层操作)。

浏览器内操作要点(详见 `apps/browser.yml`):树形观察用
`snapshot --view content --skeleton`(chrome 整枝丢弃);读正文用
`get role:Document --property text`(Document 的 Value 是 URL);滚动/导航后用
`wait --element role:Document --stable` 判定加载完成(不要 sleep);地址栏输入
走 `press ctrl+l` → `set-clipboard` + `paste`。

规则:**两工具可组合**——playwright 做研究/采集,actl 把结果"落地"到用户真实
会话或桌面应用(与下方 collector 分工同构)。

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
