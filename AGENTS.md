# AGENTS.md — 协作规范(人与 AI 共用)

操作规则的单一事实来源;背景与理由见 docs/,不在此重复。改动本仓库前必读本文。

## 硬规则(违反 = 拒绝,编号对应 docs/TECH_STACK.md 的 ADR)

1. **stdout 只输出 actl-core 的 JSON envelope**,日志/诊断只进 stderr(ADR-002;唯一例外 `--help/--version` 走人类文本,见 docs/06 §3.5);
2. **`windows` crate 只在根 Cargo.toml 声明一次**,全 workspace 同版本(ADR-003);
3. **失败必须有错误码**:统一走 actl-core 的 Error → ErrorCode 映射,运行路径禁 `unwrap/expect`(ADR-004);
4. **不引入 async/tokio**(ADR-001);
5. **`actl-core` 禁止依赖 windows 相关 crate;`unsafe` 只允许出现在 `actl-uia`**(workspace 已 deny,仅该 crate 豁免并登记理由);
6. **直接依赖总数 ≤ 9**,新增先登记 docs/TECH_STACK.md §3（2026-09-30 用户授权 WebView2 显示层增加 webview2-com；前端构建工具单独登记）;
7. **默认 UIA,物理鼠标兜底必须显式 `--physical`**(ADR-007);键盘按 docs/06 使用显式 `--app` 绑定与输入护栏,人工交接不替代命令参数要求;
8. **协议/命令语义变更(spec 提交)必须同 PR 完成**:契约 golden 测试 + docs/06 + CHANGELOG,三者同步。

## 工作流

- trunk-based:`main` 始终绿;短生命周期分支 `feat/xxx`、`fix/xxx`,合并即删;
- 提交:Conventional Commits `<type>(<scope>): <subject>`,首行 ≤100 字符(hook 校验;type 全集以 `hooks/commit-msg` 正则为最终权威,本列表须与之同步);
  type: `feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert|spec`(spec = 协议变更,触发硬规则 8);scope: `core|uia|physical|cli|protocol|release`;破坏性加 `!`;
- **门禁单点**:`pre-push` hook 与 CI 都执行 `cargo xtask gate`;门禁命令的**唯一实现在 `crates/xtask`**,改门禁只改那一处;
- **push 是阶段性动作**:完成一批相关提交后才 push,不为单个提交即时 push;唯一例外是发版流程(push main → CI 绿 → 打 tag,见 TECH_STACK §6);
- 跳过门禁仅限紧急:`git push --no-verify`,且在 PR 说明。

## 环境

- 工具链锁死于 [rust-toolchain.toml](rust-toolchain.toml)(版本唯一来源,含 msvc target),不要绕过、不要在他处写版本号;
- clone 后:`make setup`(惯例入口,等价 `cargo xtask setup`;一步装 hooks + 激活 pinned 工具链);提交前:`make gate`;
- **Makefile 只是转发层**:全部目标转发到 `cargo xtask ...`,门禁的唯一实现单点在 `crates/xtask`;禁止在 Makefile 写实现逻辑;
- 无 make 的环境(如纯净 Windows)直接用 `cargo xtask setup / gate / fmt`。

## 仓库地图(分层细节见 docs/TECH_STACK.md §2)

`crates/actl-core`=协议(纯逻辑) · `crates/actl-uia`=UIA/COM(唯一 unsafe 豁免) · `crates/actl-physical`=物理兜底 · `crates/actl-flow`=顺序流程与恢复 · `crates/actl-cli`=入口 · `crates/actl-devtools`=原生诊断/夹具/回归/打包（xtask 转发） · `fixtures/`=系统集成测试 · `scenarios/`=真实应用场景 · `hooks/`=git hooks · `docs/`=全部决策文档 · `skills/actl`=agent skill 与应用档案 pack(`apps/*.yml`,契约见 skills/actl/PACKS.md,应用知识不进核心;lint-packs 已入 gate)

业务流程统一维护在 `skills/actl/workflows/`，仍由单一 actl 技能入口引导；应用知识放 `apps/`，引擎测试放 `fixtures/flows/`。目录与维护规范见 [流程索引](skills/actl/workflows/README.md)。

- **应用 YAML 的使用与同步**:操作软件前先读对应 `apps/*.yml` 的路线、已知限制和验证状态，再选择适用的 workflow，避免重复探查已有结论。应用档案供 agent 阅读，`workflows/*.yml` 才由流程引擎执行；当前引擎不自动引用应用档案。修改定位、输入通道或操作路线时，同步检查并更新受影响的应用档案、workflow、输入准备器与回归断言，不能只改档案就宣称执行已修正。
- **修正与验收分开**:按 PACKS.md 维护路线状态，组件探查、静态预检和门禁通过不代表完整业务验收通过。已有 flow 使用保存的冻结计划，不会自动获得 YAML 修正；不得改写旧计划或覆盖失败证据，需要验证新计划时明确记录新任务及变更原因。

## 测试

- 新命令:单测 + envelope 契约 golden 最低配,且 **help 四要素齐备**(Usage/参数/Example/Errors,进 cli.rs 的 after_help 表);fixtures 只用系统自带应用(计算器/记事本/ms-settings);
- **双轨纪律**:CI fixture 轨(系统应用,可复现)与场景实测轨(真实应用,docs/ 场景记录,不进 CI)分开;场景实测结论落 docs 并同步 pack;
- **内环按层选测试**:改 core/flow/uia 只跑本 crate(`cargo test -p actl-core|actl-flow|actl-uia`,uia 交互用例默认 ignore 不占门禁);改 cli 只跑对应契约文件(`cargo test -p actl-cli --test <名>`);涉协议/golden/跨层才跑全量。全量验证交给 pre-push 的 `cargo xtask gate`,不为单文件改动反复跑整套;gate 各步耗时在 stderr(`· <step> Nms`),耗时数字不跨日/跨环境比较(见文末 timing 纪律);
- 修 bug:**先写复现测试再修**,同 PR;
- golden 变更只能随 `spec`/`test` 提交,禁止顺手改;
- **GUI 应用禁 taskkill**(会话复活:记事本 TabState+云同步、计算器僵尸框架)——清理一律优雅关闭(close-window --pid/--hwnd),fixture 无论成败统一进入收尾判定并记录清理状态;仅在无停止/人工接管且资源归属已核实时关闭,否则保留现场并记录原因;
- **资源归属**:使用隔离文件/profile/数据目录;关闭前核对 HWND、PID、进程实例和任务标记。新窗口差集、标题匹配或 UIA 消失不能单独证明归属或已关闭;多标签应用只清理本任务标签。详见 [窗口基建](docs/30-window-infrastructure.md)。
- **验收证据**:代码完成、gate 通过、真机验收分别报告;受阻不记通过,原失败记录保留,后续局部确认不能改写整轮结果。记录被测二进制路径/哈希、环境、逐调用证据及清理状态,产物留本地 target。
- 真机回归:`cargo xtask regress`(需交互桌面;含键盘注入的 fixture 需桌面空闲)。
- 目标预览真机回归:`cargo xtask preview-tests`(需交互桌面;串行跑 preview_window 交互用例,只操作自有预览窗口、不注入输入;活动急停时整轮让行报阻塞,锁屏/遮挡由用例护栏拒绝并如实失败;默认跳过需人工点击的 mouse_selection 用例)。

## 桌面协作与恢复

- **显示端占用构建文件**：用户已授权，当运行中的 `actl-signal.exe` 占用待构建或替换文件时，可直接正常关闭该显示端，完成构建/替换后重新启动，无需再次确认。先核对进程路径、PID 与当前任务状态；有执行中的桌面动作时先等待安全边界。按测试节 taskkill 禁令优雅关闭；保留已有急停、暂停和流程记录，不自动授予桌面权限或续跑旧任务。重启后用 display-status 核验就绪；关闭或重启失败应报告，不循环强制重启。
- **桌面交接与代操作授权**:同一任务复用 ACTL_TASK_ID/flow ID。用户已授权 agent 在本仓库的既定测试与排障范围内仅在 computer use 以**真实输入驱动**时点击 actl 可见的“开始/继续”（2026-10-01 实证：CUA SendInput、PostMessage、PowerShell SendInput 与注入热键均无法驱动面板——合成授予被安全模型拒绝，机械不可达；不得把代点当作依赖，正式 agent 自驱验收须先设计白名单通道并走 spec）；先核对显示端身份、任务 ID、目标和待执行动作，再实际点击并检查恢复结果。不得写授权文件、伪造事件或绕过护栏。用户明确暂停、稍后、停止或接管时优先让行，不以代点击抵消用户意图；不确定写入先核验，不循环点击继续。详见 [桌面交接](docs/32-desktop-handoff.md)。
- **诊断工具选择**:优先用 actl 的状态、局部快照、属性、模式和日志诊断；视觉核对优先用 actl 自带的整窗、元素或区域截图。需要看画面本身不是切换 computer use 的理由。仅在 actl 自身失效、证据矛盾需要独立工具对照，或执行上述交接代操作时使用 computer use，记录原因与结果；同一现场不默认重复观察。UIA 树与截图都不保证完整揭示界面，虚拟列表、隐藏状态、自绘内容和操作后的变化须围绕当前目标补充观察与结果核验，不能将截断或未暴露视为不存在。
- **诊断与执行效率**:先在已有隔离窗口完成最小复现和组件路线验证，再跑整轮；优先限定目标容器，避免反复全窗口遍历。准备资源和静态预检不创建待恢复任务，不为普通诊断额外启动或暂停 flow。仅在流程计划必须变更或验收要求隔离时创建新任务，保留旧失败证据；暂停不是每步确认机制。门禁与真实桌面写入串行，避免输入锁竞争干扰诊断。
- **暂停、停止与恢复**:人工让行和主动暂停不写急停。急停变更持久停止代次，旧调用不得因开始新任务或修复而复活；新任务可正常请求用户开始，纯观察不因历史急停拒绝。损坏或旧格式状态通过 execution-status 诊断，由明确的 execution-recover/修复运行状态操作归档并修复，不手动删标记、不自动恢复旧任务或授予权限。实现契约见 docs/47-execution-recovery.md。
- **结果不确定先核验**:写入、点击、提交或启动报错不等于没有生效;检查送达信息与实际结果后决定下一步,禁止盲目重放。flow 的 uncertain 步骤先验证;单 CLI/普通脚本不宣称具备自动续跑能力。
- **目标与焦点**:跨调用使用 ref 时,支持 `--snapshot` 的命令须绑定 snapshot_id;其他路径重新观察定位。控件重建或菜单变化后重新观察。面向输入框的键盘操作核验具体输入目标与焦点,不能只凭窗口相同放行;拒绝时诊断原因,不放宽护栏。详见 [身份与焦点](docs/31-element-identity-input-focus.md)。
- **就绪与未知状态**:进程启动不代表窗口可操作,租约存活不代表提示可见;结合原生窗口、前台及显示诊断确认。观察失败必须保留错误/未知状态,不转为空列表或成功;恢复有界,不反复启动应用。

## AI agent 附加要求

- 交付前过 gate(唯一门禁,见工作流),如实报告结果(含失败);
- 命令/协议语义以 docs/06-design-blueprint.md 为准,不凭记忆发明;
- 代码与文档同 PR;范围以 docs/PRD.md §1.3 非目标清单为铁律,拿不准就问,不静默扩大;
- 安全:不做任何规避权限/隐身执行的功能;涉及 `--physical` 的改动必须显式提示;用户经显示端 UI 显式开启的预授权（显示协议 v10，docs/32）是合法授权通道——急停、动作预告与审计不变,但 agent 不得代开开关、不得绕过或伪造授予;
- 提交信息自写、按逻辑分组,禁巨型混合提交;工作区有其他改动时按本次授权范围暂存,共享文件按改动块核对;
- **文档分工**:ROADMAP 只维护待办和验收;完成实现进入 CHANGELOG（未发布记 Unreleased）,未验收部分继续保留待办;CHANGELOG 正文中的 `@eN` 等 ref 语法必须置于反引号内（裸 `@` 会被 GitHub 解析为用户提及）。docs 场景记录保存证据与限制,应用知识同步 pack,不复制进 AGENTS;
- **平台与 MCP 范围**:执行端仅 Windows,Linux/macOS 暂不考虑实现。MCP 是计划中的独立适配服务,在被控设备本地启动供其他设备调用本设备;尚未实现,范围以 PRD/ROADMAP 为准;
- **bash 内联命令转义与编码双重污染**：中文参数会 GBK 污染（显示层及应用层均可能）；反斜杠路径与 `$` 变量在工具层+bash 双引号组合下会被吃掉（实证产出过 `X:/.xlsx` 残渣参数）——**含中文、反斜杠路径或变量的命令序列一律写 UTF-8 脚本文件执行，不内联**；脚本内传给原生应用的路径用正斜杠（Rust 侧两种皆收、原生命令行/对话框只认反斜杠，见 docs/70 §3-4）；
- **失败处置协议**:①先读调用留证(result.json 的 error envelope:evidence 与 hint)——错误契约常自带恢复路径(如 `STALE_REF` 指示重取快照后重试),native_code 立即解码,禁止推断先行拿真机验证;②三段法定位:状态类矛盾先查环境(锁屏/慢日/受保护视图/遮挡——护栏拒绝通常是对的),再看窗口实际状态(list-windows/snapshot/get),最后才怀疑代码,截图是最后仲裁;③同一失败签名出现第二次即按**类**修根因,不逐个位置打补丁;诊断脚本不静默吞错,现场未清不重跑;组件选择类问题的判定树见 [身份与焦点](docs/31-element-identity-input-focus.md);
- 性能/耗时数字只在同环境头内比较(慢日绝对值不可跨日,timing 纪律)。
- **前台弹层优先**:应用用最前方的弹窗引导用户(错误框/确认框/保存提示/向导)——动手前先 `front-report --app` 读前台引导层,存在未知弹窗即停下判读,不硬走既定路线;自动点击只允许显式白名单(如清理动词的「不保存」、「找不到文件」+「确定」),弹窗全文随证据落盘(2026-10-01 三次弹窗滞留事故的机制化收口;判定树见 docs/31);


License: MIT(见 [LICENSE](LICENSE));不引入 vendored 外部代码。
