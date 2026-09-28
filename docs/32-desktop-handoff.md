# 桌面交接与窗口就绪

## 使用方式

所有经过动作反馈门禁的写操作，先向现有 actl-signal 申请桌面交接。
提示窗口显示任务和当前目标，说明可能切换窗口、使用键鼠与剪贴板。
用户点击「开始 / 继续」后才允许执行；点击「稍后」或 25 秒内未响应，命令返回
`NOT_ACTIONABLE`，不执行请求的动作。弹窗使用不激活窗口样式，不主动移走当前输入焦点。
只读查询不申请授权；观察窗口时不再隐式 ShowWindowAsync。

一组 CLI 命令使用同一 `ACTL_TASK_ID`，flow 使用 run ID，才会复用任务授权。
共享 PowerShell fixture 为同次场景自动生成任务 ID，尊重调用方已有 ID；交接中断后保留窗口，不继续清理。
普通 CLI 没有任务 ID 时仅当前调用有效。授权保存在显示进程内存中：空闲 60 秒、总长 10 分钟、
显示端重启、紧急停止或外部键鼠输入均会使其失效。调用方不得伪造点击或写文件授予授权。
现有 `--physical` 显式要求不变；点击交接按钮不代替 `--physical`。

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
对最小化窗口请求恢复，再发一次 UIA SetFocus 请求，最多轮询原生前台 1 秒。
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


v5 验证：`target/cooperation-gate-final.log` 完整门禁通过；模型覆盖人工让行文案、后台可操作、
正确 flow 绑定、急停禁止继续、查询不覆盖暂停、单调用不提供假恢复；引擎覆盖未开始步骤继续与
不确定写步骤不重放。`target/cooperation-live/display-status.json` 确认新版 v5 显示端就绪。
此次启动验收发现实际急停已开启，保留原标记；桌面点按和恢复业务软件的正向路径未继续验收。
