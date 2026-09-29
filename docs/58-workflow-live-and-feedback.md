# VS Code workflow 真机首轮与状态条优化

日期：2026-09-29；Windows 本机交互桌面，默认时序，源码 c9f95b2。

## 首轮结果：未通过

任务 vscode-edit-24969ba428864428aef58b82519629b7，共 22 步，完成 17 步，当前 needs_review。证据保存在 target/同名任务目录（history.json、status-after-review.json、timings.json、handoff-summary.json）；二进制哈希见 context.json。执行器使用安装路径 C:/Users/gqy17/.cargo/bin/actl.exe，SHA256 EB3002EDE50CC36C7DD4E8F50489869224936C8741480B6B8A5A3BC629B00F7D。显示端为原有安装版本，本轮最初未替换。

- 三轮插入、撤销、重做及全文替换的磁盘断言通过。
- close-before-reopen 首次关闭核验 NOT_ACTIONABLE，恢复后才通过；不能记为无中断关闭通过。原生窗口观察和进程查询的时间差仍需复现。
- 重开、归属及全文核验通过。
- goto-line 的 sequence index 3（type 行号）报 NOT_ACTIONABLE；后续 GOTO- 插入和保存未派发。恢复只核对最终结果，ASSERTION_FAILED 后进入待核对，没有重放该组。
- 只读现场确认行号输入框存在、值为冒号；正文文件有两处真实 LF，截图显示三行。最初将转义输出误读为字面换行的问题已更正，不修改正确的换行内容。
- 用户通过控制面板继续过任务；本轮不据此宣称一次点击全程通过，也未独立完成人工接管、主动暂停和误让行全部验收。
- 保留任务窗口、文件与运行记录；没有自动关闭待核对现场。Word/Excel/Zotero 后续真机尚未执行。

## 速度与显示反馈

同一轮日志统计：12 次 before 合计 15194ms，平均 1266ms；13 次 resume_check 合计 18094ms，平均 1392ms，包含失败后的恢复调用。这些数字不是跨版本性能对比。

发现从 paused 开始执行时，同一个 pending 步骤先执行 resume_check，随后执行完全相同的 before。现只复用同一步骤紧邻成功的相同检查；保留暂停/停止边界，不跨步骤缓存，也不复用 uncertain 的结果核验。定向测试覆盖不同参数不能合并、显式重试前重新核验前置条件。实际端到端提速比例待同环境复验。

状态条原来在鼠标悬停时用操作提示覆盖进度，点击暂停后鼠标不动会继续遮住真实状态。现悬停只改变右侧按钮高亮，主状态持续显示。像素回归先复现失败，修复后通过，覆盖正在暂停、已暂停、运行和待核对。

现有 ACTL_PRESS_FOCUS_MS/ACTL_TYPE_FOCUS_MS 默认 120/150ms。本轮不缩短焦点稳定等待，也不缩短观察超时；先排除真实失败并验收上述重复检查优化。

## 本轮交付验证

- 完整门禁通过：target/live-ux-speed-gate.log；补充检查复用 golden 测试通过：target/check-reuse-golden.log。
- 新执行器与显示端已更新安装文件；当前显示端从 target/live-runtime-b952d94310514deabb18b2f8add22ef5 运行，display-status ready/responsive，证据 target/live-ux-display-status.json。
- 新 CLI SHA256：D3F2ADB57F904BEFB4D6C20FC4013DAD8DE478D4B642A82C5A49FC05A0BB5164；新显示端 SHA256：63890EF7D3E928F9F6B29C513B03A566B9BC69CCD7247CEE4ADA3B845692C222。
- 替换前已无旧显示端进程；仅启动新显示端，未恢复旧任务或授予桌面权限。原任务仍为 needs_review，runner_active=false。
- 悬停像素回归通过、显示端就绪已确认；真实用户再次点击暂停的整轮行为仍待复验。端到端速度暂不宣称提升比例。

复跑任务 vscode-edit-74c4b754d8c24999a69397df4fcb7f92：新版去重执行器完成 12 步，关闭后首次核验再次 NOT_ACTIONABLE。只读复核已确认原进程退出，保留 history-close-failure.json 与 close-readonly-recheck.json。修复关闭核验仅查询进程实例身份，并将原生窗口观察移到慢进程读取之后；复验结果另记。

完整组件核对见 [59](59-vscode-quick-input.md)。补等待后关闭核验通过，但行号仍因 Edit/ListItem 焦点不一致拒绝；已通过实际 ValuePattern/Invoke 组件探查修正路线，旧轮次仍不计通过。
