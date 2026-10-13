# 任务起止与异常提示

中央短时提示负责让用户感知任务开始、完成或被打断；普通步骤和单次动作继续由原状态条展示，不逐步弹出中央提示。只消费既有 workflow 状态，不增加协议字段、依赖或 WebView2 实例。

## 触发规则

按 workflow 的 run_id 去重：首次 running 和 completed 分别播报开始与完成；failed、stopped、异常 paused、needs_review 分别播报失败、停止、暂停和待核对。同一状态连续刷新不重复提示，普通步骤切换及继续运行不重复播报开始。正常起止只展示一行 `ACTL + 具体任务名称`，由光带颜色与状态图形区分开始/完成；异常在同一行追加“失败 / 已停止 / 已暂停 / 待核对”。

任务名称读取该 run_id 的冻结 `plan.json.name`，不使用步骤标题或最近其他任务的名称。队列事件携带各自任务 ID，名称仅在弹出时读取；缺失或不可读显示“任务名称未确认”。空白字符压成单个空格，长名称省略，异常后缀预留宽度而不被名称挤掉。

`start_paused`/`checkpoint` 是流程主动检查点，不额外弹出中央暂停提示。暂停与待核对不当作任务结束。快速流程首次观察到完成时保留开始和结束两条事件，依次展示；早于显示端启动的快照及保留的历史快照不回放。单独执行的 CLI 调用不触发该提示。

去重游标保存在显示端进程内，不是持久化的通知账本。显示端重启后，若旧流程以新的 CLI 调用恢复且未被当前游标观察过，可能再次提示开始；这部分恢复场景仍待真机验收，不宣称跨显示端重启去重。

## 视觉和动效

- 用户选定 B 开放光带：左侧自由曲线汇入下方细光束，无环绕面板的边框或独立图标框。沿用现有界面的炭灰底色、银灰文字与淡蓝色；完成绿色、失败红色、停止/暂停/待核对暖黄色。深灰承托层两端渐隐，中央文字区保持较高不透明度以应对终端背景。
- 单行 ACTL 为 24 DIP，任务名 30 DIP、600 半粗，异常后缀 26 DIP；按实际字宽安排间隔，并共用基线。常规宽度 760 DIP，画布 112 DIP 高，其中承托层 72 DIP；小屏依据工作区宽度收窄，长名称省略。位置在主屏工作区高度约 40% 处。
- 横向入场 600ms：中心从工作区宽度 1/3 处减速到中央；停留 1800ms；退场 500ms：加速向 2/3 处淡出，总时长 2.9 秒。异常停留 3000ms，总时长 4.1 秒。文字不缩放、不模糊。
- 下方光点约 32ms 更新一帧，带短尾光，单次从左扫向右并停住。文字和曲线只栅格化一次，后续只在光束局部按预乘 Alpha 合成光点；帧参数不变时不重复提交。减少动态时中央静态展示同样时长，无位移、缩放、淡入淡出或光点运动。
- 保持穿透、置顶、不激活与截图排除。窗口失败记入 stderr，只清理自有窗口，不修改授权、执行代次或流程状态。

## 验证与演示

`cargo test -p actl-uia --bin actl-signal` 覆盖任务级去重、事件任务归属、步骤变化不弹、暂停/待核对、快速完成、旧状态不重播、冻结计划名称、三分之一到三分之二的动效边界、减少动态，以及 96/120/144/192 DPI 的透明边缘、长名称与异常后缀保留、光点不修改文字像素和预乘 Alpha。

`cargo test -p actl-uia --bin actl-signal export_flash_previews -- --ignored` 导出正式渲染位图至 `target/ribbon-review/`，属于离屏预览，不是现场截图。此前小提示的证据保留于 `target/flash-review/`，不覆盖旧记录。

桌面演示复用 `fixtures/flows/inspect.yml`：读取系统状态、检查点、观察窗口列表。两个只读步骤不注入输入、不修改用户应用；检查点后使用同一 flow ID 继续。常驻显示端以既有 `--replace` 正常替换，先核对 PID、路径、任务与输入占用，随后用 display-status 检查就绪。不写一次性启动脚本、不用假事件模拟执行。

原生查询演示使用 `skills/actl/workflows/inspect-desktop-native.yml`：读取系统版本和 Win32 原生窗口清单，导出窗口数量，无 UIA 依赖、无需应用资源或输入。它验证只读查询与显示层起止，不代表 UIA 窗口观察验收通过。窗口清单不证明资源归属，不据此关闭任何用户窗口。

## 证据（2026-10-02）

- 前一版原生样式门禁曾通过，日志 `target/flash-review/gate.log`；该结果不代替当前任务级实现的验证。
- 旧独立预览入口门禁曾在核心 `fast_actions_survive_poll_gaps_are_deduplicated_and_not_replayed` 失败（20 条而非 32 条），日志保留 `desktop-preview-gate.log`；该项单独重跑通过，整轮仍记失败。旧独立桌面预览 45 秒通过，记录 `live-preview.stdout.log` / `live-preview.stderr.log`，入口已移除；临时 PS 启动脚本已删除。
- 当前实现的本层测试、门禁和真实 YAML 演示分别保留记录，不以编译或启动成功代替完整验收；多屏切换仍未验收。

### 前版任务级提示验证

- 本层测试 74 项通过、7 项交互/导出用例忽略，记录 `target/flash-review/task-notice-tests.log`。全量 `cargo xtask gate` 通过，记录 `task-lifecycle-gate.log`；前述失败记录仍保留。
- 被测显示端：`target/validation/release/actl-signal.exe`，SHA-256 `B3FFEF51068450632FDB74F16C6DF4C95A85B6738DA00EFED8BE30C0F77A6922`。CLI 为同目录 `actl.exe`，SHA-256 `7A17F2842285F6C86404779C2A56C204F02E926F91275E16B90DAA773A351CD6`。
- Windows 桌面正常替换旧显示端 PID 44092，新显示端 PID 53420、HWND 34735800、协议 v11；display-start/display-status 均 ready，stop_requested=false。无强制终止、无授予桌面权限或续跑旧任务。
- 复用原有 `fixtures/flows/inspect.yml`，flow ID `notice-demo-20261002-01`。flow-run 完成 system 后在既定 checkpoint 暂停（`demo-run.json`）；同 ID 的 flow-resume 完成 windows，最终 completed、runner_active=false（`demo-resume.json`）。模板指纹未变化，两个步骤各执行一次。
- 演示仅查询系统与窗口，不创建或修改用户应用，没有应用资源需要关闭；更新后的常驻显示端按用户请求保留。流程记录保留。物理视觉效果由用户现场查看，本次未捕获排除截图的中央提示；异常状态经过单测，未人为触发真实异常。日志和原始 JSON 均位于 `target/flash-review/`。

### B 开放光带验证（2026-10-02）

- 先保留旧提示尺寸不达标的复现失败 `target/flash-review/ribbon-before.log`。本层 78 项测试通过、7 项交互/导出用例忽略（`ribbon-tests.log`），另原生渲染导出通过（`ribbon-export-final.log`）。预览原始 BGRA、尺寸和动效参数在 `target/ribbon-review/`；PNG 由原生位图转换，`*-dark.png` 仅叠加统一深灰底供离屏检查，不是桌面截图或生成式效果图。
- `cargo xtask gate` 全量通过（`target/flash-review/ribbon-gate.log`）。首轮测试常量断言的 Clippy 拒绝保留于 `ribbon-gate-attempt1.log`，修正后重跑，不覆盖失败记录。没有增加前端依赖、WebView2 实例或运行协议字段。
- Windows 10.0.26200.0；被测显示端 `target/validation/release/actl-signal.exe`，SHA-256 `DE5A1922EE3B66BF7C1B1B1CD3CBB01229F7EF19620F69107E8C8E07D37AA2A4`。同目录 CLI SHA-256 `7A17F2842285F6C86404779C2A56C204F02E926F91275E16B90DAA773A351CD6`。
- 核对原发布显示端 PID 53420、路径及执行状态后，以既有 `--replace` 正常换为调试显示端 PID 50156，释放发布文件用于门禁构建；随后同样正常换为最终发布显示端 PID 53764、HWND 36835316、协议 v11。`ribbon-display-before.json`、`ribbon-debug-display.json`、`ribbon-display-start.json`、`ribbon-display-after.json` 分别留证，最终 ready/responsive=true、stop_requested=false。`ribbon-signal.stderr.log` 为空。
- `ribbon-execution-before.json` / `ribbon-execution-after.json` 的停止代次均为 `s18daa5225ceedbdc0`，state=ready、old_calls_resume=false；未修复或删除急停记录、未授予桌面权限或续跑旧任务。常驻显示端保留；无应用窗口需要清理。
- 原 `fixtures/flows/inspect.yml` 的新任务 `ribbon-demo-20261002-01` 在 checkpoint 后继续时，UIA 根窗口枚举返回 `INTERNAL / observation_failed / children / native_code=-2146233083`，对应 `0x80131505`（Operation timed out），因此最终为 paused/action_failed。保留 `ribbon-demo-run.json` / `ribbon-demo-resume.json` 与旧任务，不重写冻结计划；显示层消费了真实异常暂停状态，该轮不记成功。
- Win32 原生清单观察成功（`ribbon-native-inventory.json`），执行状态与正常前台窗口均可读取。为了完成视觉演示，新增可复用只读原生查询 YAML，记录路线变更原因而不把 UIA 超时当作修复。校验成功（`ribbon-native-validate.json`）；新任务 `ribbon-native-20261002-01` 两步各执行一次、completed、runner_active=false、window_count=574、模板未变化（`ribbon-native-run.json`）。
- 用户要求再试原 UIA 路线后，复用未修改的 `fixtures/flows/inspect.yml` 新建任务 `ribbon-uia-retry-20261002-01`。同一被测显示端与 CLI、无重启或修复执行状态；checkpoint 后继续，system/windows 各执行一次，最终 completed、runner_active=false、模板未变化（`ribbon-uia-retry-run.json` / `ribbon-uia-retry-resume.json`）。本次未复现超时，不据此判定原超时根因已修复；原失败任务和证据继续保留。
- 当前真实桌面验证涵盖正常启动/完成与一次 UIA 查询导致的异常暂停；屏幕中部提示仍排除截图，视觉观感由用户现场查看。完整失败/急停/接管恢复、多屏 DPI 与真实减少动态设置矩阵仍待验收，不能由本轮结果推断全部通过。
- 2026-10-12 增补：异常 paused（action_failed,确定性 NOT_FOUND 演示 `fixtures/flows/notice-fail.yml`）的中央提示经用户目视确认出现,ASCII 与中文任务名各一轮,中文名「中央失败通知演示」渲染正常无乱码;首轮同构造未目击,复跑即现,归因时机错过不立 bug。急停/停止路径的显示语义随真按钮轮收口（[35](35-stop-lifecycle.md) 2026-10-12 节）。仍待：接管恢复、多屏 DPI、真实减少动态设置。
