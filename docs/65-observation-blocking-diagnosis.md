# 单次观察长阻塞排查

## 结论与证据范围

2026-09-29，原始 `recovery-templates-final-20260929` 的最后一个 unless 检查耗时 60101ms 后返回 NOT_FOUND。该区间没有写入派发，也没有人工交接事件；不是 recovery 的等待间隔。旧日志没有内部阶段耗时或失败的 match/window 阶段，无法追溯证明某一个 COM 调用是根因。

本轮增加诊断后没有复现这次 60 秒故障，不能宣称已修复。证据在 `target/blocking-diagnose/`，旧任务 history 保存在 `target/recovery-live-20260929-173716/history-diagnose.json`。仅比较本轮同一桌面、同一 debug 构建的耗时。

## 实测

- 现有 Chrome 的 Document 查询先发现 6 个候选，按现场 RootWebArea 的 id/name 收紧后，8 组标题/HWND 值核验全部通过，共 16 次。标题路径 353–861ms，HWND 路径 121–150ms。原歧义 envelope 保留，没有将其记为通过。
- 用原冻结计划创建 `blocking-repro-20260929`，首次启动成功但切前台失败，保留 repro-run.json。诊断 CLI 位于 debug 目录，显示端位于安装目录；随后使用与显示端配套的已安装 CLI 切前台成功。不能据此把本次失败直接归因为旧 60 秒问题。
- 在上述未成功激活的窗口上，10 组输入框查询返回 NOT_FOUND；完整快照显示 RootWebArea 内容为空。切前台后恢复原流程，先核验不确定步骤，8 步最终全部 completed，恢复调用 7115ms。前台失败记录不覆盖。
- 页面就绪后，对同一个输入框再做 12 组标题/HWND 值核验，共 24 次全部通过。标题路径 346–1429ms，HWND 路径 90–145ms。其中 1429ms 调用有 1090ms 在 session.start；该次控件查询仅 38ms，读值 0ms。不能把命令总时长都算在 UIA 查询上。
- 上述 24 次的分层耗时：标题路径桌面枚举 236–417ms；两种路径的 FindAllBuildCache 为 24–46ms，ValuePattern 读取为 0–1ms。二进制路径及 SHA256 见 binaries.json。
- 标题路径正常时的额外耗时主要在桌面根枚举；HWND 路径绕开该枚举。它证明当前开销来源，不证明历史 60 秒也来自同一位置。已知窗口可绑定经归属核验的 HWND，并保留进程实例与目标身份校验，不能只凭可复用句柄长期认定身份。
- 测试窗口经程序/profile/PID/创建时间及唯一网页标签核验后正常关闭，verify-closed 确认原进程实例退出；用户原有窗口保留。

## 已补齐诊断

`ACTL_TRACE=1` 的 stderr 包含阶段 begin 与退出耗时。新增定位初始化、组合查询窗口解析、桌面枚举、HWND 转换、窗口候选属性、FindAllBuildCache、候选属性匹配、结果属性和 ValuePattern 读取。stdout envelope 不变；未启用时不输出。

复现时保存 stdout 与 stderr，最后一个未配对的 begin 可以定位仍在等待的调用层。trace 不记录输入正文或窗口标题。原有 scope 只有退出时打点，真正阻塞时看不到当前阶段，这一诊断缺口已补齐。

## 尚未证明与下一步

当前依赖的 UIAutomation::new 创建 CUIAutomation，代码没有设置 provider 事务时限。微软文档给出的 IUIAutomation2 默认连接超时为 2 秒、事务超时为 20 秒，但它不是整条命令的硬时限，不能凭 60 秒接近三倍就认定发生了三次超时。来源：[连接超时](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-put_connectiontimeout)、[事务超时](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomation2-get_transactiontimeout)。

下一次慢调用应以新增阶段证据区分初始化、窗口发现、控件查询或读值，再决定是否限制 provider 调用或隔离只读执行。当前没有修改全局超时、切换 COM 客户端版本或把 provider 失败当作空结果；外层 recovery 仍不能强制中断同步调用。

本轮 `cargo xtask gate` 通过（gate.log），发布版 CLI 与显示端同步安装；安装版本哈希见 installed-binaries.json。真机批量数据来自 debug 构建，发布版仅另作只读观察和显示就绪核验，不混为同一轮性能样本。
