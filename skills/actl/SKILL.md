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

## 核心循环

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

感知:snapshot(--skeleton) find(--name/--role/--id/--near) get list-windows status commands
执行:click(--physical/--expect/--near) right-click double-click hover drag scroll type(--paste)
     press(--expect) key-down key-up set-value
窗口:launch focus-window close-window resize-window wait(--element/--window/--gone) batch
验证:verify(--exists/--value/--contains/--checked) extract(--table)
系统:get-clipboard set-clipboard

目标语法:`@eN`(重放,须 --app)· `name:<子串>` · `id:<AutomationId>` · `role:<Type>` ·
`role:<Type>#<序数>`;多候选消歧:`--near <锚点>`。

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

收尾:batch 结束看 `windows_created`(data 或失败 evidence)——本轮新开的窗口清单,
用 `close-window --pid/--hwnd` 逐一优雅关闭(**禁 taskkill**:记事本/计算器等会话
恢复型应用强杀会复活一堆窗口)。

## 更多

错误码与恢复动作全表:[references/error-codes.md](references/error-codes.md)
