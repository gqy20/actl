# 目标预览与用户纠正

2026-09-29：用户授权先更新计划并开始实现。

## 使用者

- 用户看到目标边框、目标应用与调用方提供的动作说明，可关闭预览；无需理解 UIA 属性。
- agent 获得目标身份、边界、定位路径和退出原因；预览成功不代表点击、输入或业务动作完成。
- 开发者通过已有 snapshot/find/get 诊断属性；复杂选择器编辑器不属于首批。

## 第一批边界

`preview <target> --app <window> [--snapshot <id>] [--message <text>] [--duration <ms>]`。
仅预览单一 UIA 目标；ref 必须绑定快照。坐标预览及歧义候选选择留后续，不自动猜测目标。
时间范围 500–30000ms，默认 3000ms；说明最多 160 字符，不允许控制字符。

由当前 CLI 创建临时、无激活的原生预览窗口，绘制边框与说明，用户可点击关闭区域。
不模拟鼠标键盘、不改变剪贴板、不调用控件动作、不启动或授权旧 flow。
预览与 actl-signal 的执行反馈分开，避免把“正在展示”误作“正在执行”；复用现有 Win32/UIA 依赖。
首批不允许作为持久化 flow 步骤，不把预览当作业务 verify。

退出时销毁本次预览窗口。超时和用户关闭属于正常结束；目标失效、边界变化、离屏或系统停止则停止并返回错误。
同步 provider 调用仍可能阻塞，时间参数不表示 COM 硬超时。
显示帧成功仅证明本机渲染，不证明用户看过或认可；自动化执行前仍须重新定位并遵守原有授权。

## 验收

- 离线：参数边界、ref 快照要求、坐标拒绝、flow 拒绝、输出 envelope 和 help 契约。
- 真机：独立测试窗口边框、消息、自动消失、关闭、焦点保持、移动失效、停止与资源清理。
- 多显示器、缩放、遮挡和 provider 重建分别记录，不以离线通过替代真机通过。
- 现有 10 个软件档案逐步复验见 [45](45-app-pack-refresh.md)，此次预览实现不将其自动改为 verified。

## 2026-09-29 实现与验收记录

首批代码已实现，尚未发布。CLI 参数预检、原生无激活边框与说明窗口、超时/关闭退出和资源回收已接通。
每轮观察检查目标身份、边界、离屏状态及窗口可见性；遮挡检查仅覆盖目标中心点，不能证明整个区域未被遮挡。

环境：Windows 10.0.26200.0，当前交互桌面；沿用仓库锁定工具链，未新增依赖。测试不注入鼠标键盘，不解除已有停止状态。

| 验证 | 结果与边界 |
|---|---|
| 参数及协议 | 3 项 CLI 契约测试，覆盖 ref 快照、空应用、坐标拒绝、时长/说明边界、help、golden、持久化 flow 拒绝 |
| 纯逻辑 | 2 项单测，覆盖时长/字符上限和身份/边界变化；不替代真实窗口移动测试 |
| 原生窗口 | 前台窗口保持；向自有说明窗口投递 WM_CLOSE 后退出，边框与说明均清理 |
| UIA 调用 | 自有测试窗口通过严格 UIA 目标定位，预览 500ms 后自动结束；前台保持、窗口清理 |
| release CLI | 实际 release 二进制执行同一自有目标预览，核对 JSON 的 executed=false 与 expired，核对前台及清理 |

本地证据：`target/preview-before.log` 保留实现前失败；`target/preview-gate.log` 为首轮门禁；最终门禁见 `target/preview-gate-final.log`。
最终真机测试逐项结果见 `target/preview-live-final.log`；二进制路径及 SHA-256 见 `target/preview-binary-hashes.json`。
最终 `cargo xtask gate` 通过，3 项本机测试全部通过；更新后的 10 份应用档案共 294 个步骤参数检查无错误。
被测 release：`E:\explore\computer_cli\target\validation\release\actl.exe`，SHA-256 `3B6866C5F07234B3B0ECF2210C361D6429D779BD38C9C53C3F3729B88B7ACC20`。
被测原生测试程序：`E:\explore\computer_cli\target\debug\deps\actl_uia-107b82953eeb6492.exe`，SHA-256 `040FD7C8FF6E1DF9D78CC3AD4D60317CC79F187CD3F31EDF3CA30E16345265D9`。
原生说明窗口截图 `target/preview-notice.png` 已人工式图像检查，文字可读；这张截图不证明边框全场景可见。
测试只创建并清理自有 HWND，未关闭用户应用，未操作已有 flow 或授权状态。

剩余验收：实际鼠标点击关闭、真实目标移动/重建时失效、急停触发、不同 DPI/多屏、遮挡变化和并发预览。
候选切换与用户纠正、坐标预览未实现。全部应用 YAML 真机复验及 Phase 2 发布门槛仍按 ROADMAP 保留。

## 第二批：失效与退出回归

将原生预览测试拆到 `crates/actl-uia/src/preview_window/tests.rs`，新增 `tests/lifecycle.rs`。
后者通过真实 release CLI 与自有 Win32 测试窗口，等待该 CLI PID 的两扇预览窗口可见后再改变目标。
测试分别覆盖移动、改名、隐藏、销毁、同名同位置重建、中心点遮挡、停止和关闭消息退出。
每例核对前台窗口、CLI 输出、该 PID 的预览窗口清理及自有目标/遮挡窗口清理，原始输出保存在 `target/preview-lifecycle/`。

停止用例使用子进程专用 LOCALAPPDATA；只写该目录内的停止标记，并断言退出后仍保留。
运行前检查真实桌面停止状态，不读取或清除用户停止标记内容。此用例验证预览对停止状态的响应，不替代显示端急停按钮端到端验收。
关闭用例向自有预览的说明窗口投递 WM_CLOSE，验证 JSON 为 `dismissed` 且 `executed=false`，仍不代表实际鼠标关闭通过。
全部用例以 ignored 标记，普通 gate 只编译它们；需交互桌面显式运行，不加入无桌面 CI 执行轨。

### 原始结果与限制

- `target/preview-lifecycle-first.log`：首次 6 项通过。
- `target/preview-lifecycle-second.log`：扩大到 8 项时，6 项通过、销毁及重建 2 项失败。二者的前台保持和清理均成功，但测试错误地限定返回 STALE_REF。
- 失败输出显示 UIA 身份查询返回原生错误 `-2146233083`，经现有规则保留为 INTERNAL，包含 `observation_failed`、属性通道及原生错误码。协议要求保留查询错误，因此未改生产错误映射。
- 测试修订为销毁/重建允许 STALE_REF，或带明确观察失败证据的 INTERNAL；其他错误不放行，不允许成功退出。原失败日志和 `target/preview-lifecycle-second-hashes.json` 保留。
- 最终整轮：`target/preview-batch2-live.log` 为 11/11 通过（本批 8 项、原有 3 项）；`target/preview-batch2-gate.log` 为门禁通过。该结果属于修订后的新一轮，不覆盖前述失败轮次。
- Windows 环境仍为 10.0.26200.0。被测 `target/validation/release/actl.exe` 的 SHA-256 为 `53BB84D00D73A53EA483931996FD1CFCB14E59797C5FD42809513DDE49F01CD8`；测试程序 `target/debug/deps/actl_uia-107b82953eeb6492.exe` 为 `DAFFC0A640E896BB8A3857A8E8F242668AF330E6BCD7768B1BE70DEA37D1D105`，绝对路径另存 `target/preview-batch2-hashes.json`。
- 8 项逐调用记录均确认前台保持、该 CLI PID 无预览残留、自有目标/遮挡窗口已关闭；停止用例的隔离标记保留在本地证据目录。未修改生产预览行为或错误码映射。

复验命令（PowerShell，使用已经构建的 release）：

```powershell
$env:PATH='C:\Users\gqy17\scoop\shims;'+$env:PATH
$env:ACTL_PREVIEW_CLI=Join-Path $PWD 'target/validation/release/actl.exe'
cargo test -p actl-uia preview_window::tests -- --ignored --test-threads=1
Remove-Item Env:ACTL_PREVIEW_CLI
```

目标是自有原生 STATIC 窗口，结论不外推到 Office、浏览器或其他 provider 的重建行为。
后续仍需实际鼠标关闭、多屏缩放、并发预览、真实应用控件重建以及显示端急停联测。
