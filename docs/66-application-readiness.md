# 应用目标就绪与 VS Code 复验

## 实现

- ensure-app-window 增加必填 ready_target；窗口存在/启动、前台、业务控件出现及稳定分别作为步骤。选择器应指向页面业务控件，窗口根稳定不能代表网页加载完毕。
- VS Code 原 22 步保留，首次打开与重开各增加前台确认和编辑器就绪步骤，共 26 步。启动只等待原生窗口；随后核验任务进程归属，再前置并等待具体文件编辑器，精确读取全部 TextPattern 内容。稳定采样不能代替可写性和焦点护栏。
- 查找替换先等两个容器内的精确 Edit，写 ValuePattern 后读回验证，再执行限定容器内的替换按钮。文件切换直接 Ctrl+P 打开 Quick Input，保留写值核验、唯一结果与打开后的全文核验。
- 四软件组合模板同步为 68 步；本轮没有运行四软件整批。既有模板一致性及预检测试同步更新，没有新增模拟真机的测试。

## 首轮与恢复

证据目录 `target/readiness-acceptance/`，被测本机 CLI/显示端路径和 SHA256 见 binaries.json。Windows 交互桌面、VS Code 独立 profile 和两个测试文件；普通用户窗口保留。

任务 `vscode-ready-20260929`：首次打开及重开的四个新步骤通过。完成 21/26 后，goto-line 首个 F1 前 companion_unresponsive 暂停。核验完整正文仍为原值且无命令面板后，显式重试该步骤，转到行通过。

find-replace 在 Ctrl+H 后第一次 set-value 的 preparing 阶段失败；现场查找值仍为空。随后用 actl 对查找/替换容器内的两个 Edit 写值并精确核验，执行替换、关闭面板和保存。恢复先核验磁盘全文，跳过已完成的替换，没有重放正文操作。此现象支持增加等待与局部定位，但未证明首次失败的唯一根因。

quick-open-file 在第三个调用出现 children/INTERNAL（native_code=-2146233083），trace 停在桌面枚举阶段后返回错误。恢复观察未找到目标文件编辑器，按契约停在 needs_review，没有把观察错误作为重放授权。随后在同一任务窗口通过 Ctrl+P、ValuePattern 写路径、读回、唯一文件结果 Invoke 完成打开；再次恢复核验全文后正常关闭，最终 26 步 completed。

上述包含恢复与 agent 修复操作，不能记为无人干预的一次连续通过；首轮失败状态、快照、逐调用证据均保留。没有使用 computer use 代点授权；两次查询窗口清单未发现可见 actl 提示窗口，未实施 UI 点击。

## 边界

此次改动复用已有 CLI/YAML 能力，没有新增执行协议。wait 时限仍不能中断在途同步 UIA 调用；历史 60 秒阻塞及显示端瞬态无响应未因此宣称解决。下一轮全新任务使用最终修订模板，不改写首轮冻结计划。

## 显示端历史清理阻塞

第二轮 `vscode-ready-final-20260929` 同样完成 21/26 后，在 goto-line 第一个 F1 前失败。1790678257643ms 记录 companion_unresponsive，1790678257695ms 调用结束；显示端在 1790678258961ms 才记录前台转交成功。没有发送该 F1。源码显示 foreground::transfer 同步调用 Journal::open，后者每 60 秒清理历史目录；现场约 4700 个日志文件。该路径让清理耗时进入了 500ms 的窗口消息响应预算。

修正：显示端的前台转交与授权审计打开日志时不执行清理，独立后台线程运行原清理策略，避免同时阻塞 UI 消息处理和状态读取线程。根据用户要求，全局清理节流和后台线程共用 60 分钟间隔；保留 7 天/64MiB 限制及跨进程锁。现有测试覆盖小时内不重复扫描和到期恢复扫描。低频不能替代线程隔离，二者同时生效。

第二轮失败状态保留；核验所有权、完整正文后正常关闭测试窗口，verify-closed 确认原进程实例退出。此显示端问题与 docs/65 中尚未复现的单次 UIA 60 秒等待分别记录，不能用此次修复宣称后者已解决。

## 新版复验结果

最终 `cargo xtask gate` 通过（`target/readiness-acceptance/final-gate.log`）；已安装新 CLI 与显示端，路径和 SHA256 见 `fixed-binaries.json`，显示端就绪证据见 `installed-display.json`。

第三轮 `vscode-ready-fixed-20260929` 完成 21/26，首次打开和重开就绪检查通过；本轮未在此前 F1 的位置出现 companion_unresponsive。goto-line 已执行到写入行号，随后第 8 个调用（sequence_index=7）读取输入框值时 NOT_FOUND/phase=window，因此保留 uncertain。显示端随后收到一次继续请求，恢复核验发现目标正文尚未完成，停在 needs_review，没有重放输入。该继续请求只按日志记录为 signal_control，不能据此证明来源身份。

局部观察确认测试窗口仍存在（HWND 7146252、PID 45296），完整快照未截断，跳转输入框已消失；TextPattern 精确核验正文仍为 `Replacement 中文 final\nNumber 123456789\n`，尚未插入 GOTO-。观察时前台为 ZCode，此事实不能单独判定失败由人工切换导致。证据为 `fixed-run.json`、`fixed-run.trace`、`fixed-history.json`、`fixed-windows.json`、`fixed-paused-snapshot.json`、`fixed-paused-content.json`。保留测试窗口和恢复状态，不关闭现场、不将本轮记为通过。

历史清理改动已通过门禁并运行于新显示端；一次未复现原交接超时不能证明全部瞬态无响应已消除。跳转组件变化原因及剩余 5 步仍待核查，四软件整批和历史 UIA 60 秒等待仍未验收。
