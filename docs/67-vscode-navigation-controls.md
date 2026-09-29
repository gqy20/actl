# VS Code 控件导航与时序核验

## 当前结论

新版隔离 VS Code workflow 已通过 26/26 步，磁盘全文和正常关闭核验通过。根因是语义动作错误地要求控件矩形保持不变；现按控件及宿主身份复核，预览期间的布局变化不会单独拒绝操作。完整证据与未覆盖范围见文末。

## 现场与原因

2026-09-29，复用隔离任务 vscode-ready-fixed-20260929，HWND 7146252、PID 45296，程序路径、profile 和进程实例沿用 docs/66 并重新核验。证据位于 target/vscode-navigation，先前失败不覆盖。

- 诊断初期 Ctrl+G 调用已送达；后续改用 UIA，无 Ctrl+G 导航。
- status.editor.selection 是 Group，内部还有真正的 Button。外层 Invoke 返回成功不能证明跳转框打开；以内部 Button 为目标后观察到行号 Edit。
- 结果项 Invoke 返回后，立即核验仍读到旧行号，随后变为目标行号。UIA 调用完成与应用处理完成不同步，原先立即断言存在竞态。
- 输入框出现期间会调整位置，旧版 set-value 将矩形变化判为目标变化而拒绝。曾尝试增加稳定等待，最终改为修正语义护栏，详见下文。
- 现场曾发生窗口移动和设备输入，另有观察时窗口失去前台、Quick Input 消失。不能将所有历史失败归因于同一个原因；docs/66 的 NOT_FOUND/phase=window 尚不能由行号时序解释。

## 修正与组件验证

用户确认暂不操作桌面后，连续执行行号 1、2、1、2 共四轮：点击状态栏内部按钮，等待行号框稳定，设置 ValuePattern 并读回，等待唯一结果稳定，Invoke 后等待目标行列按钮稳定。全部通过，见 rounds.jsonl；不修改正文。

workflow 使用以上路线；保存、撤消、恢复、替换通过精确菜单项执行；文件切换点击“打开快速访问”。菜单名称中的 Ctrl 字样只是标签，不发送快捷键。保留定向正文编辑所需的 Ctrl+Home、Ctrl+A 和粘贴通道；这些仍经过窗口及具体焦点检查，不能声称完全没有快捷键或系统热键冲突风险。

原 26 步及四软件组合 68 步结构不变，组合模板同步。现有契约检查调整为只约束 ListItem 结果必须有容器，同时禁止导航步骤注入 F1、Ctrl+G、Ctrl+P、Enter。代码门禁与真实桌面运行串行。

## 验收

组件验证和完整流程验收分开记录。本文末尾记录新版 26/26 完整通过，历史失败记录保留。

## 新 profile 的语言差异

首次完整新版任务 vscode-controls-20260929 到 insert-1 的保存菜单失败，phase=match：现场菜单实际为 File/Edit，流程为中文选择器；不是不存在的窗口，也不是丢失正文。完整 TextPattern 核验输入仅一次，通过英文 File → Save 菜单保存，磁盘全文核验通过，原 flow 失败状态保留。

修正首次打开与重开均显式 --locale zh-cn，并在正文写入前等待中文 File 菜单对应的“文件”控件。未满足语言前提时停在就绪阶段，避免输入后才发现菜单路线不适用。新的冻结计划使用任务 vscode-controls-zh-20260929 验证。

--locale zh-cn 在本机新 profile 下仍未得到中文菜单，语言检查如期阻止正文写入。最终验收模板改用内置 --locale en（首次与重开一致），控件名来自同一新 profile 的实际快照：File/Edit、Open Quick Access、Go to line、Find / Replace、Replace All、Close。不安装语言包、不改用户现有 profile。英文控件探查结果见 en-line/en-edit/en-replace2/en-quick；本轮中文正文内容与磁盘精确核验保持原样。

## 全桌面 UIA 枚举故障

固定英文任务 vscode-controls-en-20260929 完成 6/26；redo-1 sequence_index=3 报 observation_failed/children、native_code=-2146233083。已恢复正文但保存前的标题定位失败，不重放 redo。en-run.trace 保留了全桌面 children 枚举错误，作为本次修正前的真实复现。

定位实现改为先枚举可见原生标题候选，再仅对匹配候选调用 ElementFromHandle，复核 UIA PID 与标题。普通定位与严格组合选择器共用这条候选路径，唯一性裁定沿用原实现。无原生候选仍走原 UIA 发现，保留非原生/虚拟窗口；目标 provider 错误不吞掉，不按不存在处理。该修正不保证所有 UIA 同步调用有界，也不据此宣称历史 60 秒等待全部解决。

## 布局稳定与写入护栏

原生定位版 vscode-controls-final-20260929 连续通过 22/26，包括菜单撤销/恢复、重开和跳转行，find-replace 的首次 set-value 在动作准备阶段被 NOT_ACTIONABLE 拒绝。Find 值仍为空，未重放正文。检查发现 wait --stable 只比较内容；曾尝试补充 RuntimeId、矩形和 enabled 完整样本比较；该实验现已撤回，因为它没有解决语义动作不应依赖矩形不变的问题。不能仅凭旧错误消息证明所有目标变化都由动画导致，后续实测仍需验证。

诊断版本的目标变化错误提供 reason=target_bounds_changed/target_disabled、前后矩形及 enabled；直接 CLI 可区分几何变化与禁用，workflow 按既有隐私规则保留证据。观察属性失败仍返回错误，不转换为 enabled=false。

## 语义动作与位置解耦

vscode-controls-verified-20260929 在 goto-line 的 set-value 前捕获 enabled=true，矩形从 [1355,307,2493,357] 变为 [1338,308,2510,359]，reason=target_bounds_changed。这证明该次失败由几何护栏触发，不证明移动由动画还是其他环境因素造成。

修复移除语义动作的矩形相等要求，改为严格读取并比较组件身份与最近原生宿主身份，仍检查 enabled；准备后重新获取所选 Pattern。wait --stable 恢复原内容语义，默认两次样本间隔 250ms，不宣称稳定窗 500ms。窗口语义动作也按实例复核；键盘与物理路径不放宽。历史失败和用户暂停的流程保留，不自动续跑不确定写入。新二进制完整真机验收见下节。

## 修复后完整验收（2026-09-29）

- 任务：vscode-semantic-20260929；直接 flow-run workflows/vscode-edit.yml，新隔离 profile，--locale en，未调整 ACTL_SETTLE_MS。
- 执行文件：C:\Users\gqy17\.cargo\bin\actl.exe；SHA256：42BDE424A591EABA7D75F58FC00DE52C282F72423FF121B903211C98E3CD6CB1。
- 结果：26/26 completed，所有实际执行步骤 attempts=1；跳转行、替换、切换至第二文件再切回、保存及关闭均通过。耗时 108232ms，只代表本环境本轮，不作跨环境性能结论。
- 结束后独立读取磁盘：主文件精确为 `Replacement 中文 final\nGOTO-Number 987654321\n`；第二文件精确为 `Second owned file\n`（仅规范化 CRLF）。两项均通过。
- 收尾：reopened-owner HWND=8258286、PID=51140、进程实例时间=2026-09-29T11:57:36.3248550Z；归属校验后正常关闭，verify-closed 通过。结束后未发现携带本任务 profile 参数的 Code.exe。此前失败、用户暂停的其他任务未恢复或关闭。
- 证据：target/vscode-navigation/semantic-{binary,run,acceptance}.json、semantic-run.trace；逐调用记录及冻结计划在 LOCALAPPDATA/actl/flows/vscode-semantic-20260929。首次 gate 的缺失窗口契约用例返回 INTERNAL，独立复查通过；原日志 semantic-gate-first.log 保留，随后完整 cargo xtask gate 通过，见 semantic-gate.log。提交前再次执行完整门禁通过，见 commit-gate.log。

本轮覆盖正常连续运行，不替代暂停/继续、用户接管和四软件组合流程的独立验收。pack 中未按原样执行的通用 ref 路线仍保留 manual。
