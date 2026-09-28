# 停止代次与运行恢复

2026-09-29。用户要求根治历史急停阻塞后续任务，并删除无效兼容路径。

## 状态模型

停止是对一轮调用的取消事件。`stop-generation` 保留最新代次；CLI 入口捕获代次，后续等待、输入锁、桌面动作、batch/flow 边界核对。
急停在跨进程文件锁内更新代次，再原子写入 v2 停止记录；中途失败报告错误，不假定写入未生效。
新开始只确认自己观察到的代次，不能清除更新的停止。确认保留代次，修复生成新代次；旧调用两种情况下都不能恢复。

| 操作 | 行为 |
|---|---|
| 新纯读取或预览 | 历史停止不阻断；运行期间再次停止则预览/等待结束 |
| 新桌面写操作 | 正常展示开始提示；点击一次确认该代次并授予该任务权限 |
| 旧 flow 的核对并继续 | 显式恢复，保留 resume_check/uncertain 核验，不自动重放 |
| 正常关闭/重启显示端 | 撤销内存权限；不制造或清除急停，不自动续跑 |
| `execution-status` | 报告 ready/stopped/repair_required，状态损坏仍可诊断 |
| `execution-recover` / 修复运行状态 | 归档旧普通文件、生成新代次、恢复可发起状态；无授权、无任务派发 |

核心记录、请求代次和显示协议一起升级。CLI 的 `ACTL_EXECUTION_GENERATION` 用于绑定显示端排队启动的子调用；父请求以后发生急停/修复，子调用不能重新捕获代次并冒充新工作。
详情窗口确认通过 WM_APP+50 传给主显示端，核对请求时效、代次和持久流程状态，一次性消费请求；旧关闭/解除消息不再支持。

## 删除的兼容及边界

- 不再把 v1、裸时间戳、空文件或损坏 JSON 解释为当前协议；显示 repair_required，并提供修复入口。
- 移除直接 clear_stop 方法及 WM_APP+49 解除前握手。恢复不依赖旧显示端响应；正在运行的不兼容显示端须先更新/关闭。
- 明确的确认/修复会保留原始停止记录于 stop-history，未做不相关历史数据批量删除。
- 修复拒绝目录或符号链接状态路径；不会递归删除未知资源。
- 同步 UIA/COM 调用仍不能强行中断，已送达动作不能撤回。变化在下一检查边界生效。
- 键鼠物理兜底仍要求原有显式参数与输入护栏，没有新增绕过开关。

## 验收记录

初始复现：`target/stop-generation-before.log`，新代次/恢复接口尚不存在时契约测试失败。
核心针对性测试见 `target/stop-generation-core.log`；集成更新中的失败保留在 `target/stop-tests-first.log` 与 `target/stop-tests-second.log`。
`target/stop-tests-third.log` 为该轮 workspace 测试通过记录；不覆盖后续改动或真机验收。

最终门禁 `target/stop-generation-gate-complete.log` 通过：Rust 296 项通过、17 项忽略，另含 pack、回归脚本护栏和打包检查。
其中覆盖运行中 CLI 在急停后立即修复仍 ABORTED、排队子进程拒绝采用新代次、同任务旧授权撤销、旧代次确认不能清除后续停止。
Windows 10.0.26200.0 交互桌面上，`target/stop-generation-live.log` 为 12/12 通过；历史停止下新预览成功并保留停止记录，再次停止仍结束正在运行的预览。
逐调用 HWND/退出码/前台与清理证据位于 `target/preview-lifecycle/`。全部为自有原生测试窗口，已清理；不外推到各软件业务流程。

`target/recovery-cli-smoke-20260929-014446/` 保存 release CLI 对隔离旧记录的真实调用：repair_required → execution-recover → ready，原文归档，permission_granted=false、resumed=false。
不把这项文件状态验证当成真实授权按钮联测。普通 gate 只编译交互桌面 ignored 用例，本轮另行显式运行上述 12 项。

### 本机安装

2026-09-29 01:45（Asia/Shanghai），替换前确认无 actl/actl-signal 进程。原两份程序备份在 `target/execution-upgrade-20260929-014525/`。
成对更新 `C:\Users\gqy17\.cargo\bin\actl.exe` 与 `actl-signal.exe`，逐文件与门禁 release 哈希核对一致：

- actl.exe：`D285DC020A65D30D470C0B5423DB2EFF18C07EF0185EC307E80C9C1649C53AB0`
- actl-signal.exe：`AD4B664609746AF925E368F67364FF9D661F320338A97FB99FD866D313F0587C`
- 原生测试程序：`6CEBE7053C6126DE65BBB50328E8690E3BA43A7EEBD95CD0CE5B865BAB85680E`，完整路径见 `target/stop-generation-hashes.json`。

安装后 execution-status=ready、stop=null；display-start/display-status 确认协议 7、ready=true、responsive=true、stop_requested=false。
显示端 PID 47260，路径核对为上述安装位置；保留运行，未授权或恢复任何旧任务。用户目录替换前即无停止标记，本轮未运行针对用户状态的恢复清除。

### 剩余验收

真实用户点击开始/继续、详情跨进程授权、快捷键急停与系统重启的完整交互链仍需联测；不以纯逻辑授权测试或上述原生观察测试代替。
未承诺中断在途 COM，也未完成多显示器、真实应用 provider 重建或其他 Phase 2 发布门槛。
