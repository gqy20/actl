# 基建第一批：窗口基线与关闭判定

## 解决的问题

延续 [Office 深测](29-office-deep-tests.md) 暴露的窗口基线遗漏，以及此前计算器的空 UIA 列表问题。
本批补独立原生观察通道，不调整前台护栏，不清除停止状态，不修改应用配置。

先写复现再修实现：

- CLI 离线测试复现枚举错误被吞成空基线、`hwnd:42` 被当作标题匹配。
  原始失败：`target/window-baseline-red.log`，两项均失败。
- Office 离线测试模拟 UIA 空结果、原进程仍存活、原生探测失败。
  旧函数错误地标记 `closed_verified`，断言失败；修正后保留未知状态。

## 原生窗口通道

`actl list-windows --native` 使用 Win32 EnumWindows，包含隐藏和无标题的顶层窗口，
与默认 UIA 枚举独立。保留 HWND、PID、类名、标题、前台、可见性、矩形及 owner_title；
不把枚举序号当作 Z 序，原生模式省略 z_order。完整枚举排除显示端窗口；指定 HWND 查询直接观察目标。
EnumWindows 失败返回错误；采样期间已销毁或 PID 改变的窗口不输出为可靠条目。

默认 `list-windows` 仍保留 UIA 语义；控件树、子控件身份和 UIA 缺失原因尚未由本批解决。
原生枚举覆盖顶层 HWND，不能以列表未包含某子 HWND 证明它已销毁。
因此补充 `list-windows --native --hwnd <n>`，直接查已知 HWND，覆盖 CoreWindow 这类子窗口。
不存在返回空列表；原生查询错误、采样期间仍存在但身份改变，均不能按不存在处理。

## batch

- 初始基线和结束差集都走原生通道，避免 UIA 初始化前后出现大批“新窗口”。
- 初始枚举错误立即返回，不执行步骤；不再 `unwrap_or_default()`。
- 最终枚举失败保留步骤结果，在 data/error.evidence 中报告带错误码的
  `window_observation_error`，省略 `windows_created`。调用方必须保留未知状态。
- `hwnd:` 与十六进制 HWND 目标按句柄判断，普通标题继续子串匹配。
- 差集只表示本次观察中新增的 HWND，不证明窗口由本任务创建；仍需核对进程、文件、配置目录和实例。

新增 golden 覆盖原生枚举 envelope 与最终观察失败的数据形状。协议和帮助同步到 docs/06。
新 unsafe 仅位于 actl-uia/native_windows，理由登记在 TECH_STACK；没有新增依赖、线程或异步运行时。

## 清理

系统 fixture 发送关闭之前和之后均按指定 HWND 原生查询核对目标。发送前仍核对 PID、进程启动时间、
窗口类与任务标记；资源管理器的单标签归属检查保持不变。

Office 等待关闭也使用指定 HWND 查询。仅在已发送关闭、且之后确认该句柄不再存在时，
才认定窗口已关闭。探测错误保留未确认状态，不重复发送关闭。
保留空宿主的分支另核对进程启动时间、不可见状态、空 UIA 框架及文件独占读取。
等待过程中检查停止状态；未创建窗口的场景仍可在停止时退出 77，零 CLI 调用。
Excel 与 Word 场景报告同时保存 owned 元数据，便于后续核对原窗口身份。

Excel 完整 UIA 快照包含精简快照省略的系统 MenuBar。旧空宿主判定未允许该节点，
导致文档关闭后仍等待超时。补充失败复现后允许 MenuBar；工作表、正文等内容节点仍阻止空宿主判定，
且仍需核对隐藏状态、进程身份和文件释放。

## 验证记录

- `cargo xtask gate` 全部通过，日志 `target/window-infrastructure-gate-final.log`。
  过程中修正过新增测试模块的位置，以满足 Clippy，不跳过检查。
- 当前桌面只读探测 10/10 轮通过：每轮 `batch ["status"]` 均无新增窗口、无 blocked_by。
  使用新构建的 debug 二进制，报告保存路径与 SHA256：
  `target/window-baseline-probe-e8c95121f0cc406bb5a2d493b0ff1284/result.json`。
  原生清单 1013 项、UIA 清单 16 项；前者含大量隐藏窗口，不能将数量差理解为 UIA 丢失了 997 个可见窗口。
- 原生枚举、最终观察失败 golden、HWND 匹配及场景停止状态检查纳入离线门禁。
- 计算器 3/3 轮计算通过，但首轮关闭检查发现 CoreWindow 是子 HWND，顶层枚举不包含它。
  报告 `target/regress-851b7364cf574546973d30e8900c5004/report.json` 保留为失败。
  补直接 HWND 查询后，已准确读到原实例，并核对原 PID、启动时间、类名后发送关闭。
  后续 UIA 不再列出该窗口，进程 MainWindowHandle 为 0，但原生 CoreWindow 仍存在。
  因此没有宣称句柄已销毁，也未强杀进程；独立清理证据为
  `target/calculator-native-cleanup-6a258f215509498a90fe22f389bb5df0/result.json`。
  新代码在这种情况明确记为 `retained_close_pending`；UWP 宿主生命周期仍待专项处理。
- Excel 本轮 8/8 内容检查通过，原始运行因空宿主判定超时而失败，保留原报告：
  `target/excel-table-054b22cb6fe94abeb94b0817f5285e3b/result.json`。
  修复 MenuBar 判定后，独立只读确认得到 `document_closed_host_retained`：文档已关闭、
  文件可独占读取，隐藏空宿主仍存在。此确认没有再次发送关闭，也不将原始运行改为通过。
  证据：`target/excel-table-054b22cb6fe94abeb94b0817f5285e3b/cleanup-confirmation-058234654127483aa93035937afa8b88/result.json`。

## 后续范围

本批尚未实现控件 RuntimeId 绑定、具体输入框焦点断言、统一业务等待、owner HWND 输出或
窗口事件历史。HWND 复用与在两次采样间创建又关闭的窗口也不由差集解决。
下一批优先控件身份及焦点，以 Zotero 引用漂移和夸克双输入框为验收场景。
