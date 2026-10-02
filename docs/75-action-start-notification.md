# 任务起止与异常提示

中央短时提示负责让用户感知任务开始、完成或被打断；普通步骤和单次动作继续由原状态条展示，不逐步弹出中央提示。只消费既有 workflow 状态，不增加协议字段、依赖或 WebView2 实例。

## 触发规则

按 workflow 的 run_id 去重：running 首次出现提示“任务开始”；completed 为“任务完成”，failed 为“任务失败”，stopped 为“任务已停止”。paused 的人工暂停、输入接管或授权中断提示“任务已暂停”；needs_review 为“任务待核对”。同一状态连续刷新不重复提示，普通步骤切换及继续运行不重复播报开始。

`start_paused`/`checkpoint` 是流程主动检查点，不额外弹出中央暂停提示。暂停与待核对不当作任务结束。快速流程首次观察到完成时保留开始和结束两条事件，依次展示；早于显示端启动的快照及保留的历史快照不回放。单独执行的 CLI 调用不触发该提示。

去重游标保存在显示端进程内，不是持久化的通知账本。显示端重启后，若旧流程以新的 CLI 调用恢复且未被当前游标观察过，可能再次提示开始；这部分恢复场景仍待真机验收，不宣称跨显示端重启去重。

## 视觉和动效

- 20 DIP、600 半粗中文文字，左对齐和垂直居中。左侧约 16 DIP 图标无独立外框：开始浅蓝箭头、完成绿色勾、失败红色叉、停止/暂停/待核对为暖黄色对应图形。
- 深灰面板 44 DIP 高、12 DIP 圆角和弱中性描边，4 DIP 透明边距；实际排版宽度加内边距，整体限制为 196–420 DIP，长文案省略。按窗口 DPI 缩放，位置在主屏工作区中上方。
- 入场 320ms：由下方 12 DIP 轻微上浮，94%→100% 缩放、淡入，小幅回弹后落定；停留 1600ms；退场 180ms：轻微下沉、收缩至 98% 并淡出，总时长 2.1 秒。
- 减少动态时静态展示同样时长。文字和图形只排版一次，动画使用预乘 Alpha 双线性缩放避免黑边；停留阶段不重复提交画面。
- 保持穿透、置顶、不激活与截图排除。窗口失败记入 stderr，只清理自有窗口，不修改授权、执行代次或流程状态。

## 验证与演示

`cargo test -p actl-uia --bin actl-signal` 覆盖任务级去重、步骤变化不弹、暂停/待核对、快速完成、旧状态不重播、动效边界与减少动态，以及 96/120/144/192 DPI 的透明圆角、抗锯齿、长文案和缩放 Alpha。

`cargo test -p actl-uia --bin actl-signal export_flash_previews -- --ignored` 导出正式渲染位图至 `target/flash-review/`，属于离屏预览，不是现场截图。

桌面演示复用 `fixtures/flows/inspect.yml`：读取系统状态、检查点、观察窗口列表。两个只读步骤不注入输入、不修改用户应用；检查点后使用同一 flow ID 继续。常驻显示端以既有 `--replace` 正常替换，先核对 PID、路径、任务与输入占用，随后用 display-status 检查就绪。不写一次性启动脚本、不用假事件模拟执行。

## 证据（2026-10-02）

- 前一版原生样式门禁曾通过，日志 `target/flash-review/gate.log`；该结果不代替当前任务级实现的验证。
- 旧独立预览入口门禁曾在核心 `fast_actions_survive_poll_gaps_are_deduplicated_and_not_replayed` 失败（20 条而非 32 条），日志保留 `desktop-preview-gate.log`；该项单独重跑通过，整轮仍记失败。旧独立桌面预览 45 秒通过，记录 `live-preview.stdout.log` / `live-preview.stderr.log`，入口已移除；临时 PS 启动脚本已删除。
- 当前实现的本层测试、门禁和真实 YAML 演示分别保留记录，不以编译或启动成功代替完整验收；多屏切换仍未验收。

### 当前实现验证

- 本层测试 74 项通过、7 项交互/导出用例忽略，记录 `target/flash-review/task-notice-tests.log`。全量 `cargo xtask gate` 通过，记录 `task-lifecycle-gate.log`；前述失败记录仍保留。
- 被测显示端：`target/validation/release/actl-signal.exe`，SHA-256 `B3FFEF51068450632FDB74F16C6DF4C95A85B6738DA00EFED8BE30C0F77A6922`。CLI 为同目录 `actl.exe`，SHA-256 `7A17F2842285F6C86404779C2A56C204F02E926F91275E16B90DAA773A351CD6`。
- Windows 桌面正常替换旧显示端 PID 44092，新显示端 PID 53420、HWND 34735800、协议 v11；display-start/display-status 均 ready，stop_requested=false。无强制终止、无授予桌面权限或续跑旧任务。
- 复用原有 `fixtures/flows/inspect.yml`，flow ID `notice-demo-20261002-01`。flow-run 完成 system 后在既定 checkpoint 暂停（`demo-run.json`）；同 ID 的 flow-resume 完成 windows，最终 completed、runner_active=false（`demo-resume.json`）。模板指纹未变化，两个步骤各执行一次。
- 演示仅查询系统与窗口，不创建或修改用户应用，没有应用资源需要关闭；更新后的常驻显示端按用户请求保留。流程记录保留。物理视觉效果由用户现场查看，本次未捕获排除截图的中央提示；异常状态经过单测，未人为触发真实异常。日志和原始 JSON 均位于 `target/flash-review/`。
