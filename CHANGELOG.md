# Changelog

本项目的全部显著变更记录于此，release notes 由本文件生成。格式遵循
[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0)，版本语义遵循 SemVer；
0.x 期间破坏性变更在条目中以 **[BC]** 标注。条目只记"变了什么与为什么"；
过程细节与归因在 git 提交信息与 docs/ 里，不在此重复。

## [Unreleased]

### Added

- **焦点元素目标语法 `focused:`（spec，[docs/06 §3 命名规则增补](docs/06-design-blueprint.md)，改写重放轮速度/通道结论的收口）**：`focused:`（裸）/ `focused:<Type>`（角色校验）解析到 OS 键盘焦点元素——树对语义选择器不可见但焦点可达的应用（Electron 懒树，uTools 实证：snapshot/find 全空、焦点可达）的唯一元素寻址通道。守卫 fail-closed：属主（给了 `--app` 时物理焦点根 GUITHREADINFO 的 HWND 必须即 app 窗口——UIA focused 视图 WinUI 实测会谎报，物理判据为准）与角色校验不符均 NOT_FOUND 并附证据；焦点唯一，序数形式 PROTOCOL 拒绝；resolved_by="focused"。元素型命令统一经 locate 获得（type/verify/get/set-value/find --target）。**与 type 组合即 burst 打字**：`type focused: <词> --app <win>` 走既有逐字键盘通道单次注入全词——uTools 真机实测 5 字符 916ms（对比逐键 press 五次调用 ≈8.5s，每键 ~1s 前台/焦点守卫成本），识图实证搜索筛选被驱动（记事本居首高亮，与手敲同效果）；fail-closed 三态真机验证（角色不符/属主不符 NOT_FOUND 带证据、角色匹配 ok）。单测：解析形态与序数/空白拒绝。
- **flow-status/flow-resume 的 retry_hint（DX，[docs/80 改写重放轮 007 误诊教训](docs/80-record-acceptance.md)）**：needs_review 运行含 uncertain 步时信封附结构化 `retry_hint`（step / 可复制的 `flow-resume --id <id> --retry-step <step>` 命令 / "裸 resume 不会重跑 uncertain 写步"说明）——裸 resume 是 no-op 曾被两度误读为"执行了没生效"。单测两条（命中/缺席形态）。
- **GUI 录制 R4 显示端右键触发（[79 §8 R4 落地](docs/79-quicker-recording-research.md)）**：信号条/托盘右键菜单新增录制动态项（恒在：无会话→"开始录制"，有→"停止录制"；序号随前置流程动态项漂移，MENU_RECORD 按菜单打开轮次匹配）；执行走显示端后台线程 spawn `actl record-start/record-stop`（CREATE_NO_WINDOW + envelope 解析；record-stop 等终态 ≤15s 不阻塞窗口线程）；反馈经 tick 进信号条 details 行 + 含 `：` 的错误弹窗（flow_ui 同模式）；**信号条 label 前缀"录制中"**（tick 轮询 records/ 发现，满足 docs/76 §9 反后台录制的"明确提示"，会话结束自动回落）。发现逻辑单测覆盖（recording 状态 + pid 存活 + 目录缺席三分支）。`record-pause` 未实现不设菜单项；右键真机验收与人工演示轮合并进行。
- **GUI 录制 R2 编译增强（spec，[docs/06 R2 增补](docs/06-design-blueprint.md)）**：机械编译器三块能力——① 延迟分析→wait 步骤：与上一用户动作间隔 ≥2s 且换窗 → 点击前编译 `wait --window <新窗> --timeout <间隔+1s>`（只读免验证；同窗长间隔是"人在犹豫"，不出步）；② 值读回事件：键入 burst 的终点（静默 ≥300ms / 点击事件 / 会话终点）捕获层读焦点元素 ValuePattern 提交值入 trace（`kind:value`，守卫：焦点窗一致、密码焦点不读）；编译配对后**提交文本覆盖 VK 推导**（IME 候选/自动纠正的真值），IME 或非 ASCII 文本自动加 `--paste`（type 命令既有剪贴板通道）——中文文本从"TODO 留 R2"变为机械可编译；③ `{window}` 泛化：步骤/verify 的 `--app` 字面标题收编为 flow inputs（`window`/`window_2`/…，默认值 = 记录时标题，args 换 `{input: windowN}`），与 explore-writeback 同一泛化思想，重放前一处可调。已知边界如实记录：值读回受事件到达时机影响（焦点快速转移可能错过，退回 VK 路径不虚记）；同窗内容加载的等待留给 agent 改写轮。
- **GUI 录制 R3 agent 改写轮指引（skills/actl，[references/record-rewrite.md](skills/actl/references/record-rewrite.md)）**：录制草稿改写的完整 agent 程序——TODO 分类修复（unresolved click 现场重定位 / terminator 键按语义补 verify / IME 补 `--paste` / 密码保留占位）、选择器稳健化（id > name，与 pack 同序）、噪音清除（"点错又点回"合并）、字面值参数化为 inputs、敏感步骤 `pause_before`；**重放对比验收**（等价初始状态 → 基线/改写各跑一轮 flow-run → verified 率比较，通过 = 改写 ≥ 基线）；产物约定 `records/<id>/rewritten.yml`（本地，个人任务值不进技能目录）；复核迁移走 PACKS.md 纪律。SKILL.md 新增"GUI 录制与草稿改写"路由节 + 命令速查 + 引用清单。首次实战需真机录制（真机验收轮后）。
- **GUI 操作录制 R1（spec，[docs/06 录制节](docs/06-design-blueprint.md)，PRD §1.3 授权；设计 [76](docs/76-gui-recording.md) + Quicker 调研 [79](docs/79-quicker-recording-research.md)）**：`record-start/record-worker/record-stop/record-status` 四命令——低级键鼠钩子捕获（回调极简入队 + 工作线程 `element_from_point` 点击瞬间反查 + `get_focused_element` 焦点身份）、trace JSONL 证据文件（密码框键 VK 永不落盘、注入事件如实标注不编译、actl 自家窗口过滤）、机械编译初版 flow YAML（写步骤恒配机械 verify + `retry: never`：click→--exists、type→--value 读回；终结键/IME/无身份目标按不猜测纪律落 TODO 注释，每步 `evidence: trace:<行号>` 证据引用）。会话有界（时长上限/停止标记/abandoned 数据收口，非常驻）；worker spawn 前 `clear_stdio_inheritance()`（管道写端继承会让 $()/ReadToEnd 阻塞到 worker 退出——R1 落地实测，docs/72 process_io"兜底一切分离式子进程派生"由此兑现）。测试缝 `ACTL_RECORD_DISABLE_HOOKS=1`（钩子离线，会话协议全真）；编译器 14 单测 + 契约 5 项 golden（含 abandoned 收口 + flow-validate 兼容断言）；真机钩子验收另轮（R1 真机通过后在此补记）。
- **双循环二进制纪律工具化（[docs/70 §8](docs/70-debug-playbook.md)，AGENTS 验收纪律操作化）**：`cargo xtask dev`（诊断循环入口——增量 DEBUG 构建,实测无改动 0.8s/小改动 ~4s,40× 快于 release;计时打 stderr）与 `cargo xtask evidence`（证据轮入口——RELEASE 构建+FNV-1a 双通道 128 位二进制指纹追加落 `target/evidence/binary-log.txt`,调用即进入证据轮的显式声明）。诊断与证据二进制结构性分离;AGENTS 验收纪律同步升级（双循环命令对、一轮多假设、事件等待替代固定 sleep）。Makefile 精简为高频别名（dev/gate/evidence/setup,按频率排序;fmt/regress 低频不再设别名）;agent 直达 `cargo xtask`,make 仅为人类别名层（AGENTS 环境节明确）。
- **探索引擎 L2 原语（spec，[docs/06 L2 增补](docs/06-design-blueprint.md)）**：① 输入原语 `explore.texts`——选择器→文本映射（≤8 项，键值并入关键词激活 Edit 评分），候选命中执行 `type`（键盘护栏/--app fail-closed/probe/授权全复用），action_log 记录文本、writeback 导出 type 步骤，解锁文件名/搜索框/表单类任务；② 跨窗 goal 探测——goal 目标锚窗缺席时枚举同 pid 窗口（快照 window.pid），goal 的 --app 改写为候选窗标题重试（≤3/轮、≤2 轮/运行），命中即转移 goal_window 并完成，"点击→对话框→对话框内断言"链打通（v1 转移仅 goal 侧，观察锚点不随弹窗移动）。测试：type 全链路、跨窗命中/预算封顶、texts schema 校验契约（flow 34 + 契约 9 全绿）。

### Fixed

- **input 绑定渲染与残留守卫（[80 发现 #6/#6b](docs/80-record-acceptance.md)，actl-flow 渲染+引擎双层）**：`yaml_scalar` 会把 `{input: name}` 绑定加引号成普通字符串——inputs 解析只对流映射做替换，字符串原样透传；若 run 与 verify 拿同一字面量会**自洽假通过**（set-clipboard/verify-clipboard 对同一未替换字面量比较为真，用户目击实证）。修复双层：渲染层新增 `arg_scalar`（绑定原样发射，`yaml_scalar` 继续负责数字/布尔/null 引号——发现 #5）；引擎 `resolve` 后扫描残留绑定字符串并 PROTOCOL 拒绝（unresolved input binding）。单测两条（引号包裹拒绝/流映射默认值解析）+ golden 重生成。
- **YAML 纯量数字/布尔/null 形态加引号（[80 演示轨发现 #5](docs/80-record-acceptance.md)，actl-flow 渲染层）**：`yaml_scalar`（route/record 渲染共用）此前不引数字/布尔/null 形态纯量——`--timeout 8022` 裸渲染被 YAML 解析为整数，flow schema 要求 args 全字符串 → 真人草稿 flow-validate 拒绝。修复：解析为 f64 或命中 bool/null 关键字的纯量强制加引号；既往 pack/route 数据无数字参数故未暴露。单测（数字/前导负号/科学计数/布尔/null 家族）+ roundtrip 断言（wait 超时参数必须字符串）钉住。
- **投影快照 ref 编号口径（docs/06 增补，投影 ref 可回放性修复）**：@eN 编号改为**全树 DFS 交互序数**（投影只过滤可见性不改编号，投影后编号有洞）——修复前投影快照按投影后顺序从 @e1 重编，与 `click @eN --snapshot` 回放的全树计数错位，skeleton/keyword/content 投影的 ref **从未正确回放过**（发现 #6 攻坚中实证：keyword 快照的 @e1 回放解析到全树第 1 个交互元素）。修复后输出编号=持久化身份=回放计数三口径对齐，同一窗口跨投影编号稳定（与 find 输出同纪律），投影 ref 混用从隐患变为安全。全量快照编号不变。
- **并行探测回退串行（P3 修正，docs/06 P3 修正节）**：真机复现证伪同进程并行 UIA 读——两个并行 find 必有一个返回 `E_FAIL(0x80004005)`（顺序不定，桌面根/提供程序并发上限）；与此前"并行无净计时收益"互相印证。CLI 侧移除并行分发，`Host::execute_probes` 批量通道保持默认串行；引擎侧卡/队列预检语义不变（提供程序无关的正确性收益），通道为将来可并行的读（浏览器 CDP）预留。契约测试的队列断言同步新语义（ghost 窗口下候选确定性死亡→队列清空）。

### Changed

- **探索引擎 ref 绑定动作（docs/06 增补，发现 #6 的结构性收口）**：动作经快照 ref 身份解析——`click/type @eN --snapshot <id> --app`（捕获身份 + RuntimeId 核对 + 窗口新鲜度检查，docs/31 身份纪律），绕开间歇失明的 item-container 定位通道。Host 新增 `execute_snapshot` 快照通道（返回 snapshot_id + data；默认实现回退选择器路径，CLI Adapter 覆写捕获 envelope 顶层 id）；快照 id 与帧同轮持久化（flow-status 可见）；帧内元素携带 ref 即走绑定路径，无 ref/陈旧上下文回退选择器；STALE_REF（元素重建）不判死、计动作预算有界重试（下轮重新观察取新 ref）。测试：ref 路径全链路（raw action 断言 @eN+--snapshot）、STALE 重试恢复、无 ref 回退（44 项引擎单测）。
- **探索引擎帧背书原则（docs/06 增补，docs/77 发现 #6 收口）**：本轮刚观察的帧是候选存活性的权威视图——find/click 的 item-container 定位对 flyout 项间歇性失明（同现场 0 结果与命中交替，快照稳定可见）。帧内候选：跳过动作前 COM 探测（帧即新鲜度证据）、动作失败不判死（计动作预算有界重试）；非帧内候选（应答队列）保持探测判死原语义。卡构建的批量预检移除（卡与观察同轮，定位失明会错杀卡面）——死候选过滤收敛到应答时队列预检与动作时点击权威。P3 的卡面接入点由此修正，队列预检保留。结构性方向（ROADMAP）：动作改经快照 ref 身份绑定（`click @eN --snapshot`，docs/31 身份纪律），消除选择器重定位的通道分叉。
- **探索引擎观察有界重试 + goal 查询故障语义（docs/06 增补，docs/77 发现 #3/#5 收口）**：① 观察的瞬态错误（INTERNAL/TIMEOUT——菜单开合/窗口重构过渡期提供程序短暂锁住，真机实证 ~1s 后恢复）有界重试 2 次、500ms 沉降（observation_retries 入状态、重试不计 errors）；耗尽维持诚实闭合。② goal 核验的查询类故障（INTERNAL——真机实证：菜单开启态下 item-container 定位持续抛 RPC 服务器异常）不判死运行：计 errors 如实记录、按未满足继续探索（核验先于动作，持续故障会饿死本该执行的菜单项点击），预算有界兜底；INTERNAL 与 NotFound 同样先经同 pid 跨窗探测。两者共同直接针对 L2 验收中阻断跨窗链路的观察/核验瞬态。
- **探索引擎 P3 批量并行探测（docs/06 P3 增补）**：Host 新增 `execute_probes` 批量只读通道（默认串行；CLI 侧对 find 批量经 std::thread::scope 并行——UIA 客户端每线程自初始化 MTA，并行读合法）。引擎两个接入点：卡构建前对 Top-K 候选预检（死候选不进卡——真机验收里 ClearMemoryButton 死候选进过卡的问题就此收口，应答者不再看到死控件）与应答时队列预检（卡到应答间死亡的候选出队，每个死项省一整轮观察）。死活分类只认确定性错误（NOT_FOUND/AMBIGUOUS/NOT_ACTIONABLE/PERM_DENIED）；瞬态错误不标记（docs/77 发现 #3 的经验规则——过渡期抖动不等于控件死亡），动作时单点 probe 权威复核不变。P2（观察∥goal）按分析不实施：goal 折叠已消掉大部分核验采集、残余并发读与提供程序抖动窗口重叠。
- **探索引擎 goal 断言折叠(P1,docs/06 探索节增补)**：goal 目标选择器的值并入观察关键词(goal 元素始终入帧),存在型断言与 name 通道值断言帧内本地判定——逐轮消掉一次 verify 采集(真机实测观察 ~450ms/次,这是无线程的采集并行化);goal 核验改走 execute_output 直判,保留"未满足/目标缺席"两态语义(存在型缺席=继续探索,值断言缺席=有界重试,wait 超时=未满足)。折叠不可镜像的形态回退权威 COM 调用。副产品:goal 通道变化进入同窗差分,reveals/no_effect 效果标注更准(此前显示区不在关键词帧)。

### Documentation and validation

- **改写重放基础设施轮（[80 勘误与 v7](docs/80-record-acceptance.md)，2026-10-04）**：① uTools 应用档案 pack 落地（粘贴死路/树不可见焦点可达/失焦即藏/查询持久/空态形态/focused 输入通道，search_launch_app 路线 verified；lint-packs 12 packs 313 steps）；② record-rewrite.md 新增两条改写规则（后置条件最小化 #8 / 输入通道先验证再采用 #7）；③ **勘误**：「actl 无应用启动命令」是误诊——`launch --wait-window` 一直存在，教训入档（断言能力缺口前先查 `actl commands`）；④ v7 流程零外部前置闭环：launch 唤起 + `type focused:` 打字收进步骤，单条 flow-run 完成 12 步全链（run demo-rewrite-011，26.3s，快于 v6 的 34.4s 且步骤更多；落盘逐字节核验、现场归零）——「≥2 步场景 YAML 流程优先」自此无结构障碍。
- **GUI 录制改写重放轮真机验收通过（[80 改写重放轮增补](docs/80-record-acceptance.md)）**：演示会话草稿的完整 agent 改写（v1→v6）+ 六轮真机重放（005-010），**010 全链 11 步 completed**（uTools 逐键搜索→enter 启动记事本→ctrl+O 打开 marker4→全选替换输入→保存→关标签→关窗；落盘内容逐字节等于输入）。**四个新发现全部收口**：#7 uTools 搜索框不吃粘贴（ctrl+v 只建关键词标签不驱动筛选，识图对比实证——弃剪贴板通道改逐键）；#8「enter 后标题=marker4」是标签会话巧合（改 ctrl+O 显式打开，流程对会话状态幂等）；#9 输入前残留未清（用户指出——全选删除幂等化并入流程首步）；#10 文件对话框文件名控件同名父子 ComboBox/Edit（AMBIGUOUS→严格匹配）。附带引擎语义澄清（resume 无 `--retry-step` 对 uncertain 步是 no-op——两轮"字母没落地"误诊教训）与验收脚本纪律再犯纠正（一站式驱动脚本违反 YAML 流程优先，v6 把前置收敛进流程本体）；键入速度实测（每键 ~1s 前台/焦点守卫成本，收口见 focused 目标语法条目）。
- **GUI 录制人工演示轨真机验收通过（[80 演示轨增补](docs/80-record-acceptance.md)）**：用户右键菜单启动/停止（R4 链路随行验收——菜单点击被 pid 过滤排除，`skipped_own: 5`）→ uTools 唤起记事本 → 中英文输入 → ctrl+s → 关闭标签。**首份真人草稿：47 事件（43 真人输入）/10 步 + 6 TODO**——"录制窗口内用户输入必被捕获"裁定为肯定；**IME 旗舰断言**：输入法打出的「测试一下小过 」经 burst 末值读回编译为 `type --paste`（提交文本覆盖 VK 推导——真实 IME 不产生 VK_PROCESSKEY，信 VK 会编译出拼音乱码，提交值优先的设计恰成立）；wait 步骤（7 秒换窗间隔）、三窗口 `{window}` 泛化全过。**发现 #5**：`--timeout 8022` 裸数字被 YAML 解析为整数、flow schema 拒绝（yaml_scalar 不引数字/布尔/null 形态——离线测试的 wait 轮没断言该参数故全漏）；修复+单测+roundtrip 断言；R3 首次真实改写（最小机械修正→`rewritten.yml`→flow-validate 通过）。发现 #4 沉降路径本轮未被触发（复验仍挂）；重复 ctrl+s 等噪音如实记录（清除属改写轮）。
- **GUI 录制注入轨真机验收通过（[80](docs/80-record-acceptance.md)）**：actl CLI 注入驱动的真机四轮——钩子真实安装/事件捕获/点击反查/焦点身份/值读回/注入标记与编译排除/会话协议全部在真实混合 DPI 双屏 + 并发用户环境验证。**四个真机发现全部修复**：①DPI 坐标口径（钩子物理坐标 vs 调用线程 DPI 感知，混合屏下反查命中隔壁屏元素——会话线程设 PER_MONITOR_AWARE_V2）；②LLKHF_INJECTED=0x10（首版误用 0x1=EXTENDED 位，注入键全部漏标）；③值读回 Value→Text 回退链（TextPattern 型应用无 ValuePattern）；④读回与输入提交竞态（停止距最后按键 231ms 读到空——有界沉降重试 ≤3×150ms；**其真机复验被第四轮锁屏阻断，挂起如实**）。文本保真度对照证实（注入 13 字符→verify 精确匹配）。人工演示轨（非注入编译路径/IME/真人键鼠质量）待用户在场；第二轮观察到录制窗口外的并发用户输入进入文档（trace 零非注入事件、逐事件对账，判定为窗口外输入——"窗口内用户输入必被捕获"留给演示轨裁定）。
- **探索引擎 L2 真机验收（[77 L2 增补](docs/77-exploration-s2-acceptance.md)）**：P1 goal 折叠随全程生效（观察即核验）；`texts` 输入原语真机通过——texts 键激活 Document 评分后与菜单栏「编辑」形成真实三方平局卡（应答裁决后 type 单步执行、ValuePattern 读回核验通过）；发现菜单 flyout 在锚窗快照内（探索可观察菜单项）。跨窗链路被真机发现 #3 阻断（菜单开合过渡期 UIA 观察瞬态→失败闭合），机制有 MockHost 全链路单测、收口方向为观察有界重试（受阻如实记录，ROADMAP）。附带实证：find 歧义死候选剪枝与队列回退、窗口标题变化下稳定锚设计、中文 IME 键盘通道限制确认（ASCII 验收,--paste 选项留 spec 决策）。
- **探索模式 S3 真机闭环验收通过（[77 S3 增补](docs/77-exploration-s2-acceptance.md)）**：回写（S1/S2 完成运行→pack 片段，失败运行如实拒绝）→ agent 侧重放（replay-explore-*.yml 全步 verified）→ **用户人工复核**迁移 2 条 unverified→verified（pack 更新+lint-packs 通过）→ **先验飞轮实证**：失败运行的 tried 数据导出 explore.priors 后，同一不可达 goal 从 S3/S3b 的 `goal_check_failed`（窗口重构陷阱）变为 S3d 的 `budget_exhausted:actions`（actions=4/errors=0）——docs/77 发现 1/2 机制化收口。诚实记账：§7 的 5 条 verified 目标本轮 2 条（现有完成运行全部），余 3 条待路线积累。
- **探索模式 S2 真机验收通过（[77](docs/77-exploration-s2-acceptance.md)）**：计算器/记事本 4 场景全过——熵门自动路径（0 决策外抛）、平局决策卡+`--decision` 单选应答+排序回退（一次往返完成两次尝试）、`budget_exhausted:actions` 与 `goal_check_failed` 两类诚实终态实证。token 审计实测：关键词帧 301–1700B/次（与 docs/42 §3 推算量级吻合）、决策往返 0–1 次/任务、对比同窗全量快照循环 5–17× 体积缩减。真机发现两条入 docs/42 §10.3（窗口形态变化使显示元素不可解析→失败闭合；候选排序缺效果风险先验）。验收 spec 沉淀为 fixtures/flows/explore-*.yml（含 calculator-reset 辅助）。

### Added

- **探索模式 S3（spec）：路线回写 + 效果风险先验 + goal 有界重试（[42 §11](docs/42-exploration-mode.md)，协议见 [06](docs/06-design-blueprint.md) S3 增补）**。`explore-writeback --id <run> [--out <file>]`（只读）从 goal_met 的完成运行抽取 pack 格式路线片段——status **恒 unverified**（verified 迁移必须人工复核，走 pack 编辑+git review，自动化只做证据收集；actl 不写 pack 文件，分层铁律：核心 CLI 永不读 pack）；步骤 = acted 点击序列 + 终态 goal 断言，`--app` 精确值替换为 `{window}`；死候选生成 quirk 片段、tried 效果输出 suggested_priors（dead=-4/no_effect=-2）。action_log 截断或 goal 未达成即拒绝——不完整路线不导出。`explore.priors`（-4..=4,≤32 项）由调用方从 pack quirks/上轮 suggested_priors 注入，评分叠加、下限 0——docs/77 发现的「候选排序缺效果风险先验」由此闭环。goal 断言瞬态错误（NOT_FOUND/TIMEOUT）有界重试 2 次（goal_retries 入状态、重试不计 errors），耗尽维持失败闭合、错误码原样传播——docs/77 发现 1 的机制化收口。测试：core 先验 2 + flow route 抽取/渲染（saphyr 往返）5 + goal 重试/先验改道 3 + 契约 golden 等 3 项。
- **探索模式 S2（spec）：有界探索引擎 + 决策卡协议（[42](docs/42-exploration-mode.md) §9.2 delta 全兑现，协议见 [06](docs/06-design-blueprint.md) 探索节）**。Plan 新增可选 `explore` 段（task/app/keywords/goal/budgets，steps 必须为空），`flow-run` 直接执行探索计划，零新命令。引擎循环：关键词投影观察（零命中放宽一次记 widened）→ S1 差分 → 前沿评分（词表命中×身份稳定×角色先验×新鲜加成，整数分稳定排序）→ 熵门（头部领先 ≥2 自动执行，平局外抛）→ probe-before-act（`find` 只读探测，死候选记 tried=dead 不消耗预算）→ 点击 → 帧差分归因 reveals/no_effect → 本地谓词核验 goal（LLM 永不被问"这步成功了吗"）。词法闸（删除/发送/支付等中英模式）命中的候选永不自动执行。决策外抛：run 置 needs_review/decision_pending（复用状态机不加顶点），决策卡自包含、4 KiB/Top-K≤8 构建器硬保证（压缩阶梯，仍超即 PROTOCOL），经 `flow-status` 的 data.decision_card 暴露。应答 `flow-resume --decision <choice_id>`（未知候选 PROTOCOL，--confidence 仅入报告）；应答后引擎侧排序回退（首选 + 卡内非危险候选按序），一次往返摊平淡区间。终态语义：goal 满足 completed；预算耗尽/无候选/观察失败 needs_review 按原因串区分，错误码如实传播。全部状态持久化 `flows/<id>/explore.json`（256 KiB 原子写），token 审计（observation_bytes/card_bytes/计数）入 flow-status，为 42 §9.4"推算以实测替换"提供数据。测试：core 9 项 + 引擎 MockHost 全链路 13 项 + 契约 6 项离线；真机验收列入 ROADMAP（未做不记通过）。

## [0.2.0] - 2026-10-03

本版是浏览器 CDP 通道 + 指针扩展 + 探索模式地基的版本：从零到一建成了 actl 自有浏览器自动化通道（9 个 spec 提交），三项指针扩展全部真机通过，探索模式 S1 落地；同时补齐本地应用注册表、帮助精简、AGENTS 工程纪律、显示端动画与 UIA 诊断增强。

### Added

- **浏览器 CDP 通道 v1（`browser-*` 命令族，spec）**：actl 自有浏览器会话的最小同步 CDP 客户端（新 crate `actl-browser`，tungstenite 为唯一新增直接依赖）。**8 命令**：`browser-launch`（默认有头、`--headless` 后台选项、`--proxy` 显式代理、`--profile` 持久登录复用、`--browser-executable` 接受 app-registry 别名）、`browser-navigate`（网络级失败显式报错不静默）、`browser-read`（`--property` 白名单属性读 + `--iframe` 跨源子 frame）、`browser-type`（守卫输入写，native setter + 读回核验，内容不回显）、`browser-click`（守卫写 + `--expect-text` 断言）、`browser-close`（CDP 优雅关闭 + 幂等注销）、`browser-list`（只读枚举）、`browser-verify`（断言族 + 登录态谓词 `--css/--xpath [--contains]`）。**持久 profile 四件套**：登录暂停点（`pause_before`+`resume_check`）、`resources.browser_profiles` 声明治理（未声明即拒）、同名再启=复用+陈旧接管、扩展策略（ephemeral 恒禁/持久默认保留）。**YAML 流程接入**：治理三件套（outputs 只读生产者/副作用必带 verify/持久 profile 声明）全部就位，探针六步+登录暂停+复用真机全通。安全边界：Target/Runtime/Page/Browser 白名单域——cookie 构造性不可达，永不附着用户默认 profile。设计 [72](docs/72-browser-cdp-integration.md)；x.com 八轮真机 [73](docs/73-browser-twitter-scenario.md)。
- **本地应用注册表（`app-register`/`app-list`/`app-remove`）**：`%LOCALAPPDATA%/actl/apps.json` 存储名字→路径映射，`launch <别名>` 和 `browser-launch --browser-executable <别名>` 走注册表解析；Store 应用支持 `--pattern` 通配（版本号变化自动重解析 glob）。skills 文档含查找指南 [app-locations.md](skills/actl/references/app-locations.md)。
- **探索模式 S1（spec）**：`crates/actl-core/src/diff.rs` 多级身份配对差分 API（automation_id+role 优先、动态 id 回退 name+role、大小写不敏感、重复键容忍——虚拟化列表场景全覆盖，6 单测全绿）；`Projection::Keyword` 任务关键词投影（命中交互元素+祖先链，零命中=空不回退）；`snapshot --keywords K1,K2` CLI 面；devtools `snapshot-diff` 升级为 core 配对。
- **指针扩展（M3 P3，spec）**：①轨迹拖拽 `drag --path X,Y`（≤64 路径点 8px 插值）；②坐标框选 `drag xy:x1,y1 xy:x2,y2 --app`（两端窗口边界+遮挡守卫）；③长按 `key-down --duration Ms`（急停即释放并 ABORTED）。Paint 真机三项全通，见 [74](docs/74-pointer-extensions.md)。
- **动作开始中心闪现（`action_flash`）**：agent 动作启动物理/CDP 写时，屏幕居中偏上出现单行胶囊（`ACTL | 动作名`），从左滑入→停留 1.6s→向右滑出，穿透+置顶+不激活；与信号条同色系。
- **中央任务起止提示**：只在任务开始、完成及异常/中断时提示，步骤过程沿用原状态条；B 开放光带样式，横向从工作区三分之一处进入、中央停留、向三分之二处退出。见 [75](docs/75-action-start-notification.md)。
- **stdio 继承守卫（`actl-uia::process_io`）**：浏览器进程树会继承调用方管道写端致 `$()`/`ReadToEnd` 永不 EOF——长命子进程派生前清自身 stdio 的 INHERIT 标志。兜底一切分离式子进程派生。
- **探索模式 spec 评审通过（[42 §9](docs/42-exploration-mode.md)）**：PRD 非目标逐条核对通过；协议 delta 定稿（NeedsReview 扩展、决策卡 4KiB/Top-K≤8、`flow-resume --decision`）；S1 先行。
- **GUI 操作录制设计提案（[76](docs/76-gui-recording.md)）**：用户手动操作→自动生成 YAML（像 RPA recorder）；~800-1200 行，0.2.0 后优先立项。

### Changed

- **帮助输出精简**：18 个命令的 selector_help 重复块改为 `"see actl find --help"` 引用；`--no-activate` 从 ~200→~60 字符；`--physical` 从 ~130→~40 字符；snapshot 六选项单行化。总体削减 ~8000 字符。
- **skills 文档全面更新**：SKILL.md 新增浏览器三通道路由（playwright/actl CDP/actl UIA）+ 命令速查新增浏览器/app-registry 分类；flows.md 新增浏览器流程规则五条（outputs 限制/verify 断言族/登录暂停/值断言/profile 声明）；版本号从 0.1.6→≥0.1.9。
- **显示端 UIA 响应**：修复检测和只读探测移出窗口消息线程；屏幕边缘提示复用 HWND 减少重建。见 [65](docs/65-observation-blocking-diagnosis.md)。
- **历史日志保留策略（spec）**：检查间隔 1h→15d，容量 64MiB→3GiB；跨进程节流。
- **UIA 查询阶段诊断（spec）**：children 查询细分 FindAllBuildCache/Length/转换，零结果与未知区分。
- **UIA 窗口观察优化**：单次观察内复用客户端和缓存请求，减少重复 COM 调用。
- **任务时间线查询**：按有效首条筛选，有界任务头缓存跳过未变化 journal。
- **UIA 观察诊断（spec）**：`UIA_E_TIMEOUT` 映射为 `TIMEOUT`；观察阶段首尾/耗时/错误码/前台变化/输入活动摘要入时间线。
- **历史清理扫描性能**：仅在超预算时读取排序；按需 trace 增加耗时汇总。
- **桌面根诊断**：有界 UIA 桌面根身份探针，缓存模式须显式指定。

### Known Limitations

- 非开发机 US3 复现搁置（用户裁定 2026-10-02）；机器到位后补验。
- Store 应用 app-registry pattern 受 WindowsApps 权限限制：glob 无法枚举该目录，需注册完整路径（更新后重新注册）。
- 浏览器写入的 uncertain 状态不跨进程持久化（崩溃后由调用方 browser-read 裁决）；flow 级 uncertain 持久化归 M3。
- 显示端常驻会话卡（区别于动作流的浏览器会话 UI）待后续。

## [0.1.9] - 2026-10-02

本版是可靠性、验收与安全加固的版本：金任务跨日双百达标，全部原生夹具和四场景真机复验通过，输入护栏与显示协议健壮化（前台沉降/租约重读/换行折叠/悬浮卡不抢前台），前台弹层优先体系落地（front-report + 判定树 + 清理白名单），授权通道定案（agent 不可自授、急停压过预授权）。

### Added

- **始终信任此流程（指纹持久信任）**：授权三部曲收官——全局托盘开关 → 会话级 standing → **指纹持久信任**（跨会话免点击）。声明 `approval: standing` 的流程运行时，执行器计算冻结计划的 FNV-1a 双通道 128 位指纹并附在交接请求中；显示端查 `trusted-flows.json` 信任集——匹配即刻 Standing 授予，不匹配正常出卡。**模板变更 → 指纹变 → 信任自动失效**。设计见 [32](docs/32-desktop-handoff.md)。
- **`front-report` 前台弹层报告（只读）**：应用用最前方的弹窗引导用户——自动化动手前必须先听它说。汇聚原生 owned 可见弹窗与树内子窗表面（文本全文+按钮清单），附朴素分类提示（error/save/confirm/update/unknown），`front_clear:true` 表示无引导层。观察三件套齐备：title-watch / focus-report / front-report。
- **`focus-report` 只读诊断命令**：UIA 焦点元素、物理焦点根窗（GetGUIThreadInfo）、物理前台窗三视图并列——WinUI 上 UIA focused element 实测会谎报，物理层权威。
- **consent_decision 一等字段 `input_source`**：决策时刻最近输入来源分类（device/foreign_injected/own_injection + age_ms）——人工物理输入与外来程序注入一眼可辨。
- **verify-owner 进程树归属**：属主进程命令行不含令牌时沿父链（有界 4 级）找携带令牌的祖先，载荷带 `via_parent` 如实说明。
- **verify-file IO 失败可区分报错**：`io_kind`（sharing_violation/permission_denied/other）+ `os_code` 入 evidence。
- **flow-status 陈旧计划提示**：冻结计划旁车记录源模板字节指纹，重读比对不一致时提示"模板已改，重试仍跑旧路线"。
- **`flow-clean` 批量归档**：归档历史非完结流程（默认 60 分钟阈值），证据全保留。
- **诊断动词家族（devtools）**：`cleanup-tabs`（按名关标签+弹层白名单）、`snapshot-diff`（快照结构化差异）、`title-watch`（标题瞬态监视）、`close-gone`（关闭→消失计时）、`app-lab`（隔离应用实验台）、`focus-report`、`front-report`——ad-hoc 脚本全部沉淀入库。
- **`composite-locate` 夹具**：UIA 组合定位真机验收五点（复合唯一化/同名消歧/作用域/重建后重定位/暂停恢复后重定位）。

### Changed

- **excel-table-exit 进程断言按已知限制收口**：两个 Excel 退出步 verify 改为 `verify-window absent`（VSTO+输入法加载项钉住进程使进程断言结构性不满足；窗口关闭是可达的最强断言）。
- **preview 框语义澄清**：黄框 = 自动化操作矩形（`--physical` 点击落点为其中心），与视觉边框的偏差是风险信号而非渲染缺陷。

### Fixed

- **前台护栏激活过渡误拒**：type/press/paste 物理前台闸门改为有界沉降轮询（新旋钮 `foreground_settle_ms` 600ms）——激活过渡期空标题窗不再误判为抢焦。
- **显示租约有界重读**：check_active 改为 1500ms 预算内重读——租约新鲜度窗与渲染 tick 贴近导致截图捕获/DWM 打嗝时误报"显示不可用"。
- **`type --verify-value` 换行表示误判**：回读比较前 `\n`/`\r`/`\r\n` 统一折叠为 `\n`——Store 记事本 ValuePattern 实测报孤立 `\r`，严格字节比较必误报。
- **`type --verify-value` 失败证据补齐**：新增 `expected`/`actual`/`comparison`——差异本体首份信封可见。
- **悬浮卡不再抢前台**：横条悬浮信息卡改 `SW_SHOWNOACTIVATE` + `WS_EX_NOACTIVATE`——此前 `SetForegroundWindow` 会偷走自动化目标前台。
- **UWP 标题路径类配对**：同标题多窗时 `focus-window`/`restore-window`/`close-window` 限定 `ApplicationFrameWindow` 框架窗。
- **`${run.dir}` 路径展开**：input default 的路径展开不再依赖 resources 声明——无资源模板中原样传给命令致 screenshot 路径不存在。

### Fixed（夹具，devtools）

- **金任务 2 全链修通**（Store 记事本连串根因）：启动走执行别名、收尾按标签清理+弹层白名单、另存为文件名改键盘通道+阶段化有界重试、UWP 判定收窄、多标签纪律。
- **金任务 1 修通**（UWP 计算器归属唯一性）：owner() 新增 class 过滤限定 ApplicationFrameWindow。
- **夹具收尾双路线分流**：有多任务/同族标签走标签清理，无标签应用走整窗优雅关闭。

### Documentation and validation

- **金任务跨日双百**：GT1 10/10 + GT2 10/10（2026-10-01 晚间）。
- **NFR 数据**：冷启动 P50=67ms/P95=94ms、snapshot P50=433ms、输出体积 <2KB、skeleton 削减 45%。
- **发布产物核对**：gate 全绿、5.3MB 零依赖、79 契约测试 94 全过、golden 22 文件。
- **全部原生夹具复验通过**（11 项）：金 1/金 2/input-target/msinfo32/Explorer/显示端×2/Edge×2/Chrome/flow-evaluation。
- **场景轨四场景真机复验通过**：app-launch-observe、cloudmusic-playback、quark-search（含两处版本漂移修正）、tencent-meeting-draft。
- **修复检测/auto_resume 全链真机验收**：两判定+预算+急停阻断。
- **once_per_run / 组内暂停 / 任务隔离真机验收**（用户协作轮）。
- **stale_read/resume_check 真机矩阵补全**（四格收齐）。
- **有界只读恢复六判据真机验收**。
- **条件模板写入侧三模板真机验收**。
- **agent 代操作通道定案**（docs/32）：四条合成通道全部无法授予——授权必须真人。
- **预授权下急停仍生效（显式验证）**：standing 开启 → 热停 → 写被拒 → recover → 恢复正常。
- **无干预四软件整跑达成（第八轮补记）**：70/71 completed、零人工救场。

## [0.1.8] - 2026-09-30

本版是显示端与人机协作体验的大版本：显示协议演进到 v11（预授权 Standing + 有界等待让行），横条信息架构重做（两行展开/悬浮卡/幽灵按钮/步骤链），暂停-修复-继续链路全程"说真话"（上下文按钮语义、障碍提示、修复探测、opt-in 自动续跑），并修复一个输入安全缺陷。

### Changed

- **[BC] 显示协议 v10–v11，`actl` 与 `actl-signal` 需成对升级**：新增 `Approval::Standing` 常驻信任——托盘开关「预授权」开启后桌面写入免出卡（横条青色描边标记，关闭即撤销）；流程声明 `approval: standing` 首次确认后会话内常驻。v11 起 CHECK 区分两种让行：授权仍在、仅输入活跃 → `YIELD_BUSY`，客户端有界等待安静（3 秒/150ms 轮询，急停即时），超时才失败——误触鼠标/键盘不再打断任务；授权真被撤销维持 YIELDED 立即失败。审计只记终态。[32](docs/32-desktop-handoff.md)
- **横条信息架构重做（四轮用户反馈迭代）**：点击空白展开为 **420×62 两行态**（宽度 1.5 倍、右缘锚定、无箭头指示、再点收回不自动收回），第二行显示当前工作流**步骤链**（✓ 完成/▸ 进行/✕ 失败/○ 待执行/◇ 跳过）；边框成为独立通道（预授权=青色常驻、进行中=状态色慢呼吸、需决策=快脉冲、静止无动画）；**悬浮信息卡**（dwell 500ms）：状态点=图例、空白区=一站式状态卡、步骤链=完整未截断值，与菜单/详情弹窗互斥且带归属。任务详情窗口：标题与状态同行、底部操作单行、幽灵描边按钮、TypeScript/WebView2 内容层（运行时不可用回退原生）。[18](docs/18-display-layer-implementation.md)
- **暂停-修复-继续链路（批次 1–4）**：继续按钮按上下文差异化（失败暂停=「已修复，重试继续」、待核对=「核验并继续」）；横条失败态带步骤 id；详情给**障碍提示**（错误码→可核实的障碍）、尝试计数、留证目录直达；暂停请求可见化（「正在暂停 · {步骤}」+ 超时急停教育）；type 分段间隙观察暂停（长输入亚秒级停下、已送达段数入证据）；action_failed 暂停期间显示端每 5 秒**静默探测** before/verify 并点亮判定（障碍已清除/结果已存在-防代做双重生效）；流程 YAML 新增 `auto_resume: action_failed`（默认关）：Retriable 判定自动续跑，预算 2 次/运行持久计数，急停/路线缺陷永不自动。[32](docs/32-desktop-handoff.md)

### Fixed

- **`--physical --with` 修饰键静默误配 Win 键（安全）**：白名单收紧为 ctrl/shift/alt/win，未知键名 PROTOCOL 拒绝（三条编译路径），"win"改为显式映射——此前 `--with ctrlx` 会注入 Win+点击。
- **needs_review 不再永久钉住横条**：人工意图态（paused/stopped）持久保留继续入口；needs_review 30 分钟超时回空闲（状态文件与核对入口不变）。
- **修复检测真机验收修正**：auto_resume 派发传真实停止代次（曾传标记串致子进程即败）、预算持久计数防目标切换重置、探测证据落 `flows/<id>/probe.jsonl`、孤儿化暂停流程回落到给予窗口内最新 action_failed 目标。

### Documentation and validation

- 新增调试手册（docs/70）与三份横切参考文档（FALLBACK 兜底清单 / STABILITY 护栏地图 / PARAMETERS 时序参数总览）；timing 画像/扫描夹具原生落地（`cargo xtask timing profile|sweep`，三个默认参数本地再证真）；零点击扫描真机完成。批次 1–4 与显示端重做经 computer-use 驱动的真机验收（三组场景 + 三个缺陷当场修复）；预授权两条通道、glyph 渲染与呼吸观感的人工确认仍待用户。

## [0.1.7] - 2026-09-30

本版把四软件应用验证整体迁移为原生 YAML 流程并整批跑通，扩展流程协议（简单条件、有界恢复预算、失败隔离、声明式输入与资源），显示协议演进到 v9（停止代次、单次交接、前台资格转交），工具链全原生化（移除全部 PowerShell），并交付 timing 画像/扫描夹具与三份横切参考文档。

### Changed

- **[BC] 显示协议 v7–v9，`actl` 与 `actl-signal` 需成对升级**：急停废止已运行调用的代次，修复归档原始状态、不授予权限、不自动续跑（删除 v1/时间戳兼容解析与直接解除握手）；任务配置支持 `handoff.approval: once_per_run`，首次确认后同任务不因空闲/总时长反复交接；动作开始向前台资格进程转交、结束归还，覆盖连续 CLI、最小化与失败路径（身份核验接受同文件路径别名）。输入监听增加 Raw Input 冗余与消息泵心跳，健康未知即撤销授权。新增 `execution-status` / `execution-recover`；显示端定时核对停止代次并主动收回旧提示。[停止与恢复](docs/47-execution-recovery.md) · [交接策略](docs/52-task-handoff-policy.md) · [前台资格](docs/62-foreground-handoff.md)
- **[BC] 观察失败契约收紧**：窗口清单、身份基线、新窗口等待及 PID 定位迁移到可失败的 RawView 枚举，必要属性失败不再伪装为空值、缺席或唯一结果；`wait --idle` 系统输入查询失败返回带证据的 INTERNAL，并在轮询中响应流程暂停/停止。
- **工具链全原生化**：移除剩余 41 个 PowerShell 文件——业务动作迁入 workflow，诊断、回归调度、夹具与发布打包迁入 `actl-devtools`（经 `cargo xtask` 调用），门禁改跑 Rust 测试；verify-file/owner/closed 改用原生 ZIP/XML/Win32/WMI 检查器，删除运行时解释器，补 DTD/膨胀限制与进程身份测试；新增 `verify --url-query` 精确解码断言。[迁移记录](docs/61-native-tooling-migration.md)
- **Excel 关闭路线定案（模板）**：close 步改后台文档关闭序列（文件菜单 → ListItem「关闭」→ 等后台消失），断言保持工作簿窗口消失（`wait --gone`，60 秒拆窗耐心）；close-window 新增 `--system-close`（投递 SC_CLOSE，点标题栏 X 的同一命令）——Excel 裸框架对 WM_CLOSE 只隐藏不退程，模板在后台关闭后以 `close-window --hwnd <归属> --system-close` + `verify-closed`（窗口+进程双断言）收尾。完整流程后 VSTO/输入法加载项钉住进程为已知限制，壳归环境治理。[docs/68](docs/68-combined-batch-first-round.md)
- **观察耐心家族收口**：verify-window 断言失败改 15 秒内重读（带加载项的 Excel 拆窗实测 3–15 秒）；`verify --value` 与 verify-clipboard 补 5 秒沉降；verify-rows 行树缺失改报 NOT_FOUND 入沉降（尚未渲染，可重试）；`wait --gone` 把垂死窗口的观察错误视为"尚未消失"在超时内继续轮询；有界恢复的 retryable 集纳入 INTERNAL（冷启动 UIA 抖动）。观察假阴性不再停整轮。[docs/68](docs/68-combined-batch-first-round.md) · [docs/70](docs/70-debug-playbook.md)
- **模板验证收敛化与就绪门**：易瞬态验证步配置 `recovery`（3 次/15 秒）只读自动重查；应用模板必填 `ready_target`，就绪阶段等待业务控件出现并稳定；组合模板首步 `wait --idle` 桌面空闲门。启动成功不再直接假定页面可输入。
- **UIA 语义动作执行前复核**：预览后按 RuntimeId、PID、角色、名称、AutomationId 与原生宿主身份复核目标，布局移动不再误报不可操作；控件禁用或身份变化仍拒绝，键盘焦点与物理命中护栏保留。
- **标题定位原生优先**：先从可见原生窗口筛选候选、仅读取候选的 UIA provider 并复核 PID 与标题，不存在原生候选时保留虚拟窗口发现路径——已知窗口的定位不再被无关应用的整桌面枚举故障阻断。
- **窗口激活去重**：已在前台不重复请求激活；后台使用单次原生前台请求并有界核验；启动与前台步骤拆分，避免已启动窗口因激活失败被重复启动。
- **内部时序一致性**：desktop-prepare/restore 与 minimize/restore 的输入锁等待、挂起唤醒再探针、verify 族沉降轮询全部改跟 `Timing` 旋钮（`ACTL_LOCK_WAIT_MS`/`ACTL_PROBE_MS`/`ACTL_POLL_MS`），默认值不变，仅 env 联动生效。[PARAMETERS](docs/PARAMETERS.md)
- **维护性**：历史清理间隔 60 秒→60 分钟并移入显示端后台线程；旧契约用例合并去重、并发测试写者循环缩减；gate 各步耗时随 stderr 输出；陈旧 PowerShell 描述与注释清理。

### Added

- **流程协议扩展**：简单条件 `when/unless/otherwise`（条件观察失败不走否定分支，附参数化模板）；步骤级 `recovery` 有界观察恢复预算（持久化、继续不补满、暂停摘要含失败原因与重查日志）；`section` + `on_failure: skip_section` 失败隔离——一段失败保留证据、同段其余步骤跳过、后续段继续，整轮以 needs_review 收束，显式重试成功后还原；声明式 `inputs`/`--inputs` 与运行上下文资源（预检不创建、失败保留不覆盖、恢复不重初始化）；可恢复操作组（1–16 调用、以 verify 收尾）与文件/行集合/剪贴板/进程归属/关闭断言族；`${run.dir_native}` 反斜杠占位符（Windows 原生对话框拒收正斜杠路径）。[20](docs/20-yaml-workflow-protocol.md) · [63](docs/63-simple-workflow-conditions.md) · [64](docs/64-bounded-observation-recovery.md) · [55](docs/55-resumable-desktop-tasks.md) · [60](docs/60-workflow-resources.md)
- **失败分级与三路留证**：步骤记录 `attempt_errors`（最近 8 次错误码），同写步骤同错三连标记 `route_defect_suspected` 提示勿盲目重试；步骤失败瞬间向 flow-resources 落三件套——整屏 PNG、原生窗口清单 JSON、定向 UIA 快照 JSON（吞错不碍流程、同名覆盖不堆积）；无源失败合成带错误原文（截断 200 字）的最小结构化证据。
- **误触自动恢复（pardon）**：授权被纯鼠标移动类外部输入撤销时，静默窗（默认 4 秒）内无新输入且前台未变，显示端按每任务预算（默认 2 次）自动重新授权，满足同样条件的暂停流程自动发起续跑；任何键盘事件、点击、滚轮永不赦免，用户暂停、检查点、待核对与停止状态永不自动继续；YAML `handoff.pardon_input` 逐流程覆盖，`ACTL_HANDOFF_PARDON=0` 全局停用。真机模拟误触验收待做。[32](docs/32-desktop-handoff.md)
- **局部观察与目标预览**：`snapshot --within <严格选择器>` 容器局部快照（depth/limit、真实截断原因，局部 ref 绑定容器身份重放，不回退全窗口序号）；严格选择器含精确 ID/名称时先原生条件查询完整候选再核验唯一性（深树容器不被无关分支预算阻断）；`preview` 独立目标预览与 `--candidate` 同快照候选显式切换（不点击、不授予桌面权限、不默认选择），附预览失效交互回归用例。[48](docs/48-local-snapshot.md) · [50](docs/50-deep-container-query.md) · [46](docs/46-target-preview.md)
- **应用 workflow 全家**：VS Code/Word/Excel/Zotero 验证直接 flow-run，整批入口 verify-applications.yml（声明式隔离目录、初始文本与随包 Office 模板）；新增网易云播放、夸克搜索、腾讯会议草稿/日期、通用启动观察 workflow（once_per_run 交接，恢复先核验结果）；VS Code 改状态栏按钮与菜单控件路线，动态输入等稳定后再核验；首条业务流程 release-review-draft（Chrome→Word 全静态锚点，写段联测待授权）。[57](docs/57-application-workflow-migration.md) · [61](docs/61-native-tooling-migration.md) · [67](docs/67-vscode-navigation-controls.md) · [43](docs/43-release-review-draft.md)
- **诊断与回归入口**：`history --timeline` 把任务全部 journal 事件合并为单一时间序列；显示端触发的执行器经 `ACTL_TRACE_FILE` 落阶段 trace；`cargo xtask regress --unattended` 只跑登记的无输入用例；`cargo xtask preview-tests` 串行运行目标预览交互用例；`cargo xtask timing profile|sweep` 时序画像/参数扫描夹具——`ACTL_*` 旋钮的固定消费者，优雅关闭+verify-closed、标记文件隔离、急停/显示端未就绪/锁屏预检让行报阻塞（77）。[41](docs/41-logging-metadata.md) · [timing](docs/timing-2026-09.md)

### Fixed

- **整屏留证 COM 线程污染**：失败留证的整屏 PNG 路径把执行器线程 CoInitializeEx(STA) 后从不释放，同线程后续一切 UIA 调用以 RPC_E_CHANGED_MODE 失败——combined-batch 恢复核验连续 INTERNAL 的根因；改 COINIT_MULTITHREADED + 配对释放 + CHANGED_MODE 容忍。修复后整批 69/69。
- **桌面根枚举瞬态毒化**：标题定位的桌面根整批枚举被无关顶层窗口创建/销毁竞态毒化，所有应用报 STALE_REF、无匹配时误报；改仅对桌面根做有界瞬态重试（StaleRef、3 次/200ms），`uia:` 作用域枚举失败统一映射。净环境 8/8、扰动下直查 36/36。[38](docs/38-flow-output-bindings.md)
- **四软件路线缺陷（docs/68 定案）**：Word 打开期间正文核验改 UIA `role:Document` text（排他句柄期不做磁盘读，重开后兼查）；Excel 复制改 `type --verify-value` 提交跳转 + 窗口级 ctrl+c（网格单元格无元素级焦点上报）；Zotero 导入前置集合树就绪检查、快速搜索改 click 聚焦+全选+替换（type 为追加语义、tab 会清空过滤）、导出对话框移除 verify-owner（子进程无任务令牌）、导出文件名用 `run.dir_native`。Zotero 9/9、Excel 13/13 真机通过。
- **重复交接误判**：Raw Input 读取并校验事件来源，排除无设备且携带注入标记的自身事件；真实设备、未知来源与读取失败仍触发让行；交接历史记录弹窗、确认与失效原因，不记录键值/坐标。[54](docs/54-repeated-handoff.md)
- **显示与恢复细节**：悬停暂停/继续按钮不再覆盖主状态文字；pending 步骤恢复复用紧邻且完全相同的现场/前置检查；授权撤销保留首个原因，交接诊断关联失败调用与监听状态；display-status 过滤已退出进程的陈旧 popup 记录；状态条持久保留 flow 绑定，快照到期/显示端重启后仍可继续。
- **VS Code Quick Input 导航**：直接写 ValuePattern 并在 quickInput_list 内激活唯一结果；关闭核验只读进程实例身份。[59](docs/59-vscode-quick-input.md)
- **发布包技能补齐**：ZIP 包含可独立安装的 `skills/actl/`（应用档案、流程索引、references 本地相对链接），打包后核对必需文件与内部链接；不修改已发布的 0.1.6 附件。

### Documentation and validation

- 新增调试手册（调试阶梯、观察耐心对照、正斜杠原生表面规则、症状速查）与三份横切参考文档——FALLBACK（全量兜底/降级清单）、STABILITY（按步骤生命周期的护栏地图）、PARAMETERS（协议常量/内部旋钮/硬编码时序总览）；docs 索引同步。[70](docs/70-debug-playbook.md) · [FALLBACK](docs/FALLBACK.md) · [STABILITY](docs/STABILITY.md) · [PARAMETERS](docs/PARAMETERS.md)
- 十份应用 pack 同步输入身份规范（ref 快照绑定、焦点断言），修改过的已验证路线改记 manual 待复验；ZCode 超深会话与局部观察只读档案、无人值守验收工具留证。[45](docs/45-app-pack-refresh.md) · [49](docs/49-local-observation-acceptance.md) · [44](docs/44-phase2-acceptance.md)
- 真机状态如实记录：四软件整批 combined-batch1 第七轮 69/69 completed——含三次人工救场（user_paused 让行、Zotero-Word 集成模态、重开 STALE_REF）与一次 COM 线程修复，非无干预跑通；单链 VS Code 26/26、Zotero 9/9、Excel 13/13。timing sweep 整轮当前受阻于显示端表面被盖（covered_or_offscreen），预检阻塞路径已真机验证。无干预整跑、误触自动恢复真机验收与 60 秒观察阻塞定位仍待，见 [ROADMAP](docs/ROADMAP.md)。

## [0.1.6] - 2026-09-28

本版新增可恢复的 YAML 流程、任务历史与桌面人工交接，强化观察完整性、输入目标核验和窗口归属，并整理桌面提示与控制界面。

### Changed

- **[BC] UIA 观察结果更严格**：`get` 保留空字符串并增加 `unsupported`，仅在 Pattern 明确不支持时回退；`snapshot` 标明节点/深度截断，属性和子树读取失败不再伪装为空值或空树。`find`、旧定位/ref 重放和表格提取拒绝不完整观察，元素等待仅把 `NOT_FOUND` 当作缺席；表格仅在明确不支持 GridPattern 时回退。[观察说明](docs/37-uia-observation.md)
- **[BC] 桌面交接与显示协议 v6**：副作用动作等待匹配显示端就绪；桌面授权须由用户点击开始/继续或按 `Ctrl+Alt+Y`，外部键鼠输入撤销授权并请求让行。暂停和拖动状态条不写粘性急停，关闭显示端撤销授权并使租约失效；显式急停保留来源、时间和进程信息，重启不清除，解除后任务仍需另行继续。请配套更新 `actl` 与 `actl-signal`。[交接说明](docs/32-desktop-handoff.md) · [急停生命周期](docs/35-stop-lifecycle.md)
- **桌面界面与文案**：交接卡和任务详情使用深色圆角外框、分级字号与原生可访问控件；默认展示任务、状态、目标或进度和下一步提示，编号及原始诊断收进可复制、可滚动的详细信息。暂停/继续按状态显示，急停按钮与快捷键同行；托盘重复入口合并为“任务详情”，退出项改为“关闭”。长标题省略、布局随 DPI 调整，支持悬停、按下、禁用与键盘焦点反馈。
- **显示绘制**：状态条为 280×34 DIP，状态点 12 DIP，使用 Direct2D/DirectWrite 和逐像素透明圆角。屏幕边缘按目标显示器及 DPI 呈现运行、等待和暂停状态；减少动态设置下静止，截图时隐藏，不拦截点击。拉丁和数字使用内嵌 Source Code Pro Medium，中文优先使用思源黑体并提供系统字体回退。[显示实现](docs/18-display-layer-implementation.md)
- **构建与维护**：YAML 解析器及标准文件锁所需的声明 MSRV 提高至 1.89；实际工具链仍由 `rust-toolchain.toml` 锁定。协议默认超时集中到 `actl-core::timing`，内部轮询保留环境变量调优，命令默认值不变。显示端按职责拆分，gate/regress 在 `target/validation/release` 构建配套程序。

### Added

- **YAML 流程 v1**：新增 `actl-flow` 与 `flow-validate/run/status/pause/resume`，提供 Schema、字段级错误、全步骤预检、参数数组、不可覆盖计划、写前记录、命令边界暂停和恢复前验证。写操作不自动重试，结果不确定时先核对；托盘支持当前流程暂停/继续。[流程协议](docs/20-yaml-workflow-protocol.md)
- **流程数据与恢复**：步骤输出可通过 JSON Pointer 导出显式类型的数据，并通过 `bind` 绑定后续参数；`max_age_ms` 拒绝过期或时间未知的数据。新增 `retry: stale_read` 和 `resume_check`，支持只读陈旧引用的有界重新定位与恢复上下文检查；写入后的观察失败即使指定 `--retry-step` 也不重放写步骤。[输出绑定](docs/38-flow-output-bindings.md) · [恢复说明](docs/39-flow-observation-recovery.md)
- **任务历史与诊断**：新增 `history` 和 `task-archive`，通过 `ACTL_TASK_ID` 及 run/step/call/action 关联跨调用事件、报告和证据，区分执行事实、推测与用户反馈。记录日志健康、完整性和控制请求的 `request_id`，区分调用完成与流程完成，保留安全的送达元数据；默认不记录正文、标题或剪贴板，不清理桌面恢复记录。[日志元数据](docs/41-logging-metadata.md)
- **UIA 组合定位**：支持 `uia:<JSON>`，将角色、AutomationId、精确名称或名称包含作 AND 匹配，并限定唯一容器的子级/后代；歧义、观察失败和遍历超限明确拒绝，不静默降级。`find --target` 提供同语义诊断，流程在执行前校验目标结构。[定位说明](docs/33-uia-composite-selectors.md)
- **输入与结果断言**：`press --target <selector> [--snapshot <id>]` 核验现有输入焦点；`type --verify-value <expected>` 在同一控件读回完整值，失败保留送达信息。click/press 支持 `--expect element:<target>`，有界等待目标出现，失败返回 `ASSERTION_FAILED` 并附原因。流程中的 verify 区分 Name/Value/Text，wait 支持 Text。
- **空闲等待与显式物理点击**：新增 `wait --idle <ms>`，等待系统键鼠空闲；指针点击族与 hover 支持 `--no-activate`，须显式 `--physical`，结束后恢复窗口标志和光标，异常残留按进程实例核对恢复。drag 和键盘命令不支持该选项，仍受目标、焦点和桌面交接护栏约束。
- **窗口与显示管理**：新增 `list-windows --native` 及 `--native --hwnd`，可观察隐藏/无标题窗口与指定子句柄；新增 `display-start/status`、`minimize/restore-window`、`desktop-prepare/restore`。窗口整理保存恢复记录并报告跳过项，`launch --wait-window` 提供就绪等待。
- **场景与回归入口**：regress 支持清单、suite/case 筛选、二进制哈希与分离日志。增加 Office 编辑保存重开、VS Code 文件编辑、Zotero 搜索、夸克与浏览器隔离场景；统一任务标识、原生窗口归属核验、人工让行保留现场及离线清理测试。应用知识归入 pack，业务流程统一放在 `skills/actl/workflows/`。[场景记录](docs/25-deep-edit-search-tests.md)

### Fixed

- **交接与控制反馈**：授权后的 750ms 宽限窗吸收本次点击/按键余韵，之后恢复严格接管检测；快捷键不可用时提示点击按钮。控制窗口固定任务绑定，暂停/继续在后台执行并保留失败反馈；继续入口检查急停、可恢复状态及 800ms 暂停稳定时间，区分“已请求暂停”和“已暂停”，状态读取失败明确显示未知。
- **目标身份与输入焦点**：带 `--snapshot` 的 ref 在实际命中时核验窗口与控件身份，缺失 RuntimeId 不放行；输入前及注入边界重验具体焦点。HWND 键盘绑定按句柄判断，不再把句柄误当标题匹配；`focus-window` 核验原生前台，只读观察不隐式激活窗口。
- **执行与暂停**：流程预检提前拒绝无条件 wait/verify、错误选择器与等待谓词，避免前序动作生效后才发现配置错误。流程 wait 在轮询时响应暂停/停止，`wait --window` 使用原生窗口轮询，减少 UIA 阻塞；输入锁观测正确释放所获锁，等待后复查停止，key-up 与修饰点击如实记录影响。
- **窗口观察与清理**：batch 使用原生 HWND 基线，枚举失败保留已执行步骤并报告 `window_observation_error`，不推断无新增窗口。fixture 使用独立文件/profile、HWND/PID/进程实例与任务归属核验；不关闭归属不明的窗口或用户恢复的标签。CLI 退出后读取日志有界，避免 GUI 继承管道导致无限等待。

### Documentation and validation

- 增加 winapp 只读对照诊断、浏览器连接原型、自动测试评估和探索模式设计记录。浏览器原型独立于正式 CLI/YAML；探索模式、MCP 适配和游戏方向尚未实现。MCP 规划为在被控 Windows 设备本地启动，Linux/macOS 执行后端不在当前范围。[浏览器原型](docs/34-browser-connection-prototype.md) · [winapp 对照](docs/36-winapp-uia-diagnostics.md) · [探索提案](docs/42-exploration-mode.md)
- 代码门禁与真实桌面验收分别记录。部分系统 fixture、浏览器原型、最新界面交互、跨实体显示器和屏幕阅读器验收仍待完成；局部通过不代表全量真机回归通过，具体证据和限制见对应文档及 [ROADMAP](docs/ROADMAP.md)。

## [0.1.5] - 2026-09-27

浏览器真实会话的内容通道、视觉兜底闭环、输入与寻址面规范化。场景实测与
踩坑细节见 docs/16 与 git 日志,此处只记变更本体。

### Added
- **浏览器内容通道**:`snapshot --view content` 内容域投影(应用工具栏等
  chrome 整枝丢弃,与 `--skeleton` 组合对浏览器页面的 token 削减达 70-80%);
  `wait --stable` settle 谓词(观测样本连续两次不变即就绪,滚动/导航后的
  加载判定,替代固定 sleep)。
- **坐标寻址 `xy:<x>,<y>`(视觉兜底的"手")**:click 家族与 hover 接受屏幕
  坐标,面向 UIA 失明区(自绘菜单、无名按钮、Canvas)——screenshot 出图、
  多模态读坐标、`click xy:` 落点,闭环补全。守卫:强制 `--app` + 显式
  `--physical` + 窗口边界检查(坐标须落在目标窗口当前 bounds 内,窗口
  移动即拒)。
- **寻址与窗口锚补强**:`id:<Id>#N` 序数(多文档页同名 AutomationId);
  `--app hwnd:<n>` 句柄锚(SPA 站点导航改标题后锚点不失效);
  `snapshot --hwnd`(任务栏 XAML 岛等无标题特殊窗可达)。
- **输入面补全**:middle-click(新标签页/新实例)、triple-click(段落选)、
  `--with ctrl,shift`(修饰键与点击原子组合,多选/范围选);`press win`/
  `press shift` 等裸修饰键可用。
- **浏览器应用档案 pack**(quirks + 路线,Edge/Chrome 双实测背书)与
  SKILL 的 actl×playwright 分工协议;pack 契约校验入 gate。
- **文档**:docs/15 系统 UI 区域普查、docs/16 浏览器场景战记。

### Fixed
- **wait 族对 AMBIGUOUS 快速失败**:确定性多命中原先会等满超时并报误导性
  TIMEOUT,现在立即失败并附候选。
- **[BC] `list-windows` 的 rect 统一为物理像素**:此前为逻辑坐标,与截图/
  坐标寻址在高 DPI 多屏下跨缩放混层;依赖旧值的消费方需按 DPI 换算迁移。

## [0.1.4] - 2026-09-27

M2 收官批:命令面 27/31 + 窗口治理与可观测性 + 信号条重设计 + Office/微信场景实测
+ 应用档案 pack 体系。

### Added
- **命令**:`wait --property` 谓词等待(value/name/checked,--contains 子串);
  `select`/`toggle`(SelectionItem/Toggle 直调);`paste`(ctrl+v 到当前选中——
  无元素落点的粘贴原语,刻意不做元素聚焦)。
- **窗口(FR3)**:`close-window`/`focus-window` 支持 `--pid`/`--hwnd`(多窗 AMBIGUOUS
  附候选);`list-windows` 每窗口补 `hwnd` 与叠放事实(foreground/topmost/minimized/
  visible/z_order/rect/owner_title)。
- **launch 通用修复**:裸程序名经 App Paths 注册表解析(`launch WINWORD` 不再
  NOT_FOUND);子进程 stdio 分离(GUI 程序继承管道句柄会挂死调用方)。
- **可观测性**:batch `steps[]` 每步结果与耗时(失败部分结果随 evidence 带出)、
  `windows_created` 窗口差集、步间 `blocked_by` 弹窗闸门(前台劫持检测,warning
  不阻断);`ACTL_TRACE=1` 引擎阶段打点(stderr,慢环境归因)。
- **skeleton 单链折叠**:非交互单保留子祖先折叠(ref 编号不受影响);token NFR
  修订为分形态口径(深单链 ≥70%;浏览器/小窗口结构受限,实测值入档)。
- **help 四要素**:全部命令的 -h 统一 Usage/参数/Example/Errors(表驱动 after_help,
  `build_cli()` 单点注入)。
- **actl-signal 重设计**:状态灯波纹动画(执行中)/呼吸(空闲)、条宽自适应、
  执行中实时耗时、Per-Monitor DPI(WM_DPICHANGED + 字体缓存失效)。
- **回归治理**:`cargo xtask regress` 一键全量(显式 Git Bash 探测,避开 WSL bash
  假败);全部 fixture trap 清理 + 前置断言;**GUI 应用全面弃用 taskkill**
  (会话复活根因);batch 窗口差集 + `close-window --pid/--hwnd` 构成清理闭环。
- **场景实测轨(docs/14,不进 CI)**:Office 三闭环双应用全通——Excel(GridPattern
  表格直读首次真实触发、剪贴板直灌+名称框导航读写、原地保存+磁盘终验)、Word
  (TextPattern 中英文正文、paste 写入);环境特性入档:受保护视图静默拒绝编辑、
  锁屏分层(UIA 后台可用/注入护栏拒绝)。
- **微信核心通道全通**:发送(读回门+三重验证)、会话搜索、文件发送(#32770 对话框
  Alt+O 确认——真"打开"是 SplitButton 无 ref)、联系人(列表/详情)、朋友圈(浏览/
  滚动/恢复)、@人(选择器 UIA 可见;U+FFFC 内嵌对象)、群信息;mmui 特性入档:
  Invoke 不触发导航须 --physical、右键菜单 UIA 不可见、rect-center 兜底(命中检查
  保留,实战拦截遮挡误点)。
- **应用档案 pack v1**(`skills/actl/apps/{excel,word,wechat}.yml`):路线知识固化
  为可校验数据,核心 CLI 不读(agent/skill 层消费);`cargo xtask lint-packs` 零依赖
  校验(步骤命令对活的 `actl commands` schema);SKILL.md 增弹窗协议、应用路线、
  actl×wechat-ui-collector 分工协议("实时操作用 actl,存量检索用 collector")。

### Fixed
- `ensure_element_focus` 前置免扰:焦点已在目标窗口时不再强行 SetFocus 首个
  Edit(Excel 多 Edit 场景旧启发式会吞掉后续按键)。

### Changed
- [spec] batch 输出与错误 evidence 结构(steps/windows_created/blocked_by)、
  `paste` 新命令、`wait --property` 新参数、list-windows 加法字段、help 注入
  机制——docs/06 同步;契约测试扩至 15 项。

### 搁置
- `list-notifications`:WinRT 依赖超出纯 Win32 边界,移 M3(蓝图 FR4 留位)。

### 回归
- 金任务 1 3/3、金任务 2 10/10、US1 16/16、验收 A/B/C 全绿;regress 跑后窗口与
  临时目录零残留。

## [0.1.3] - 2026-09-27

M2 第三批:观察层强化 + 定位性能结构优化 + 可观测性 + 信号条交互 + 版本口径修订。

### Added
- **L4 虚拟化容器定位(spec)**:定位链第四级——已物化树空手时走
  ItemContainerPattern 精确查询(Explorer 文件列表实证:树内 0 命中 → 命中未物化
  条目;US1 条目级确认收口);`find` 自动走 L4 并上报 resolved_by。
- P1 滚入视野(offscreen 元素物理动作前自动 ScrollIntoView);`get --property text`
  (TextPattern 正文专用通道,与 value 分工);`extract --table` GridPattern 直读
  优先 + source 上报;`list-windows` 物理叠放事实;`batch steps[]`;`ACTL_TRACE`。
- **定位性能结构优化**:`id:` 走 provider 侧属性条件(单次往返)、collect_matches
  惰性属性读(绝对收益待正常日复测)。
- actl-signal 交互:右键菜单/品牌图标/拖动+位置记忆/字重打磨/热键优雅降级;
  时序基建(Timing 九参数单源 + profile/sweep fixtures)。

### Changed
- **版本口径修订**:里程碑版本 = 0.<里程碑号>.0(M2=0.2.0、M3=0.3.0 协议冻结);
  token vs 截图对比指标搁置(不再作 M2 门槛)。
- CI:job name 引号修复(YAML plain scalar 含 ": " 致 0s 假败)。

## [0.1.2] - 2026-09-26

M2 收尾:分发三渠道 + crates.io 自动化发布 + 信号条交互打磨 + 时序评估闭环。

### Added
- README 示例化(安装三渠道/快速上手/信号条/安全设计/命令面);
- scoop manifest(scoop/actl.json,哈希与官方 checksums 交叉验证,autoupdate);
- Agent Skill(skills/actl:SKILL.md + references/error-codes.md 恢复手册,自包含无仓库链接);
- crates.io 发布准备:publish 元数据 + 版本化 path 依赖 + PUBLISHING.md 手册
  (actl 名被 2018 旧包占用,crate 名用 actl-cli、bin 名仍 actl;core dry-run 通过,
  实际发布需维护者 token 按序三条命令);
- **US1 十次重跑 10/10(100%,验收门 ≥90%)**;
- NFR 实测:冷启动 P50 54.9ms / P95 73.3ms(P95<80 达标;P50 微超 50——信号层两次
  状态写+锁探测的有意权衡,后续可优化为仅长命令写 begin 态);snapshot 全量 3.9KB /
  skeleton 2.4KB(削减 38%,记事本 23 个交互引用);
- 外部机器 US3 复现留待非开发机验证(README 安装三渠道已具备);
- crates.io 已发布:actl-core/actl-uia/actl-cli @ 0.1.1(cargo install actl-cli → bin actl,全链路验收通过);
- actl-signal 交互三件套:右键菜单(窗口/托盘共用:清除停止/显示隐藏/退出;修复托盘回调不区分鼠标消息——悬停即清停止的 bug);自绘品牌图标(运行时 32×32 DIB 绘制:深底圆角方+绿点,与状态灯同母题,单测验证 HICON 有效);HTCAPTION 无焦点拖动+位置持久化(signal-pos.txt,虚拟桌面钳制防拔屏丢失);
- **时序基建**:actl-core `Timing` 单源(9 参数:轮询/唤醒上界/探针/焦点链/分批/修饰键/锁等待),默认值=生产现值,环境变量覆盖(`ACTL_POLL_MS` 等)供 fixtures 免重编译扫描——未来"按应用自适应时序"的种子;`timing-profile.sh`(各应用延迟画像,仪器=envelope 自带 duration_ms)+ `timing-sweep.sh`(Q2 轮询检出延迟/Q3 分批吞吐/Q5 焦点链成功率);首次运行实测发现并记录:锁屏下注入被护栏正确拒绝(物理前台=锁屏界面)、UWP 僵尸框架后实例内容树不物化(计算器 buttons=3,需重装恢复);
- **时序扫描结论(docs/timing-2026-09.md)**:Q2 轮询 50/100/250ms 检出无差异(瓶颈是被等事件);Q3 分批 128 比 32 快 17% 但停止粒度×4,不值;Q5 焦点链全部 10/10 有裕量——**三默认值全部维持**;画像发现本日环境整体慢 ~3×(绝对值不可跨日比较,fixture 可随时重测);
- **信号条 UI 打磨**:主行 SEMIBOLD 字重(细字发虚反馈);目标分隔符 `→`→`·`(箭头字形落到中文字体链渲染粗笨);热键优雅降级修复(某次大改中丢失,被占时不再 panic,提示行自适应点击停止);
- **release×crates.io 同步发布**:推送 tag 同时驱动 GitHub Release 与 crates.io 三 crate(crates job:版本守卫 tag==Cargo.toml,按依赖序 publish;token 经 gh secret 配置)。

## [0.1.1] - 2026-09-26

M2 前两批:输入可靠性硬化 + 命令面 24 + fallback 定位链 + US1 跨 3 应用 +
显示信号层(actl-signal 状态条与紧急停止)。

### Added
- **M2 第一批·输入可靠性硬化**:
  1. **输入占用协调**(doc 09 §6.2):多 actl 实例互斥的会话级命名互斥体 `actl-input-lock`(RAII 释放,ABANDONED 接管,忙 → TIMEOUT 附恢复建议),press/type 注入动作段全程持锁;type 键盘通道**分批注入**(32 字符/批)批间复核物理前台,焦点被抢即停在批边界并如实上报(`delivered`/`stopped_early`);envelope 新增 `input_effects`(keyboard/clipboard/pointer,部分送达 = ok:true + keyboard:"partial")。单测 4 项:跨线程锁互斥、分批发完、边界停止、注入错误传播。
  2. **激活感知目标解析**:WM_NULL 探针(SendMessageTimeoutW,60ms,不阻塞调用方)测目标是否在泵消息;挂起 → ShowWindowAsync 唤醒 + 有界等待(≤1.6s),把挂起 UWP 应用每个跨进程 UIA 调用 ~2s 的隐形 stall 变成一次有界、可上报的等待;snapshot/find 上报 `wake_ms`。
  3. **`--physical` 规范裁定(spec)+ 物理鼠标链核心**:06 §2 规则 2 修订——键盘授权 = 强制 `--app` + 护栏 + 输入锁;指针物理注入必须显式 `--physical`(独占用户鼠标)。`click --physical`:UIA clickable point(无则拒绝,不猜中心点)→ 命中检查(WindowFromPoint 根窗口**标题**比对;WinUI 三件套使严格 HWND 比对不可行)→ 输入锁 → move+down+up 一批注入;虚拟桌面归一化(VIRTUALDESK,多屏/负坐标)。真机闭环验证:显示 7 → 物理点"清除" → 显示 0。
  4. **修饰键治理修订**(doc 09 §3.4):废弃"两端无条件清场 11 键"(会踩掉用户物理按住的修饰键);改为注入前检测修饰键物理状态,有按住则等待 ≤1.5s 后 NOT_ACTIONABLE 拒绝(按住 ctrl 时注入 's' 会被劫持成 ctrl+s;用户按键与历史残留不可区分,fail-closed);注入序列自拥有自释放(同批原子);`click --physical` 同样检查(ctrl+click 语义劫持)。部分 SendInput 失败的报错改为"状态未知,勿盲目重试"。
  5. **click 语义分发补齐**:Invoke → SelectionItem → Toggle(复选框/开关)→ ExpandCollapse(折叠面板/树节点),实际 pattern 随结果上报(此前无 pattern 的控件直接 NOT_ACTIONABLE)。
- **M2 第二批·fallback 定位链**:
  - **L1 精确 + AMBIGUOUS 结构化候选**:语义选择器多命中不再静默取首个——AMBIGUOUS 附 `evidence.candidates`(ref/role/name/automation_id,ref 与 snapshot 同源编号可直接重放,docs/12 §4 协议评审项落地;`CtlError` 增可选 `evidence` 字段进 envelope)。
  - **L2 名称模糊**:`name:` 精确落空后自动降级大小写/空白不敏感匹配(`fuzzy_contains`,`resolved_by:"fuzzy-name"`);`@eN` 不参与模糊化(ref 漂移 = STALE_REF 保持严格)。
  - **L3 序数直选**:新目标语法 `role:Type#N`(1 起,`resolved_by:"role-ordinal"`)。
  - **`--near <锚点>` 距离消歧**:多命中时取与锚点矩形中心距最近者(锚点接受完整选择器语法,须唯一匹配;`resolved_by:"anchor"`)。
  - `resolved_by` 真实上报("primary"|"fuzzy-name"|"role-ordinal"|"anchor"|"ref-replay"),click/get/set-value/type 支持 `--near`。
  - 真机验证(计算器):primary/AMBIGUOUS(34 候选)/role#2/anchor(--near id:clearButton)全通。
- **M2 第二批·命令面补全(24 命令 + 契约测试)**:
  - **指针族**:`right-click`/`double-click`/`hover`(clickable point + 命中检查 + 锁,命名即物理授权)、`drag --from/--to`(双定位 + 失败补发 LEFTUP)、`scroll --down/--up N`(UIA Scroll pattern 优先,pattern 宣称支持但调用失败时物理滚轮兜底——记事本实测);mouse 原语泛化(虚拟桌面归一化统管)。
  - **键盘持有**:`key-down`/`key-up` 半事件(必须成对;进程异常退出留下的按住状态由修饰键空置检查兜底拒绝)。
  - **窗口**:`resize-window --width/--height`(保位置 SetWindowPos)。
  - **verify 族(立即断言,与 wait 分工)**:`--exists`/`--value`(精确或 `--contains`,失败附 evidence.actual)/`--checked`(Toggle On);`click --expect`(RuntimeId 新窗口断言,与 press 同语义)。
  - **`wait --gone`**(等元素消失)。
  - **`extract --table`**:容器 → DataItem/ListItem 行 → Text/Edit 单元格的通用树表扁平化。
  - **envelope 契约测试**(actl-cli 集成,CI 安全):成功/PROTOCOL/usage 三态退出码、commands 自发现 schema 抽查 24 命令。
  - 真机抽样:verify(exact 失败/contains 通过)、right-click(菜单弹出→esc)、key-down/up、resize、hover、scroll(滚轮兜底)全通;命中检查 HWND 兜底修复(记事本窗口元素不暴露 NativeWindowHandle,按标题重解析)。
- **M2 第二批收官**:
  - **金任务 1/2 回归**:命令面大改(17→24)后双双 10/10。
  - **snapshot_id 版本化-lite**:快照记录落盘(LOCALAPPDATA,保留 20 份),`--snapshot <id>` 回放校验窗口 RuntimeId(同标题重开的新实例 → STALE_REF);元素级树前缀校验留待会话设计。
  - **US1 跨 3 应用首版 16/16**(fixtures/us1-cross-app.sh):计算器→记事本→资源管理器,磁盘+内容双验;资源管理器条目级确认因 UIA 虚拟化列为开放项(M2 第三批:ItemContainerPattern)。
- **显示信号协议 + actl-signal 伴随程序**(docs/11 显示层 Phase A/B、docs/13 拉取哲学):
  - **状态源四原语**:`%LOCALAPPDATA%ctl\state.json`(命令/目标/阶段/时间戳心跳,开始/结束/batch 步间落盘)、`stop-requested` 粘性停止标志(显式动作清除)、输入占用锁(已有,"正在操控"权威信号)、失联判定(>10s 无心跳如实显示失联,不显示成功)。核心为 actl-core `state` 模块(路径可注入,3 项单测)。
  - **执行侧闸门**:输入锁获取点优先检查停止标志 → 新错误码 **`ABORTED`**(spec:用户停止与业务错误、权限错误语义分离,清除标志后可重试);batch 步间双闸门(停止 + 步级状态)。
  - **`actl-signal`**(actl-uia 内第二 bin,保持唯一 unsafe 豁免点;纯 Win32 零新依赖):置顶半透明状态条(WS_EX_NOACTIVATE 不抢焦点,工作区右上角)+ 托盘锚点 + **Ctrl+Alt+F12 紧急停止热键**;250ms 全拉取轮询(锁+状态文件,零事件订阅——docs/13 事故价目表);五态渲染:空闲深灰/占用琥珀(命令+目标)/执行浅灰/停止红/失联暗;点击状态条=清除停止。
  - 真机验证(截图核对):空闲/占用两态渲染正确;停止标志 → 注入拒绝(ABORTED、零字符落盘)→ 清除恢复。热键待物理按键验证。
- MIT LICENSE(对齐 zotero_cli/article-mcp);workspace license/repository 元数据修正。

## [0.1.0] - 2026-09-26

M1 激进 MVP:纵向跑通"snapshot → 定位 → 操作 → 反馈"全链路;17 命令,
双金任务 10/10,冷启动 P95 53.9ms(实测)。首个可安装版本。

### Added

**命令面(17,按认知模型分组)**
- 感知:`snapshot`(UIA 树 + `@eN` ref,含 `--skeleton` 祖先链投影)、`find`(条件查元素,snapshot 同源 ref 编号)、`get`(属性白名单,含 ValuePattern)、`list-windows`(含 owned dialog)、`status`、`commands`(自发现 schema,clap 同源生成)。
- 执行:`click`(Invoke 优先 / SelectionItem 回退,实际 pattern 随结果上报)、`type`(元素置焦 + 键盘逐字,含 `--paste` 剪贴板通道)、`press`(组合键,`--expect window:<substr>` 后置断言)、`set-value`(UIValuePattern 直写,不经键盘)、`launch`(spawn + trailing args)。
- 窗口/流程:`focus-window`、`close-window`(WM_CLOSE 优雅关闭)、`wait --element/--window`(TIMEOUT 语义的耐心等待)、`batch --json`(单进程顺序执行,首错即停,禁嵌套)。
- 系统:`get-clipboard` / `set-clipboard`(CF_UNICODETEXT,ClipGuard 保证任何路径释放)。

**协议**
- envelope v1:统一输出契约、9 错误码(含 `ASSERTION_FAILED`/`INTERNAL`)、退出码三态 0/1/2(PROTOCOL→2)、可选 `snapshot_id`。
- 目标语法四形态:`@eN`(须 `--app`)、`name:<substr>`、`id:<AutomationId>`、`role:<Type>`。

**输入引擎(actl-uia::kbd,取代 uiautomation crate 的 Keyboard)**
- 双通道规则:组合键成员经 VkKeyScanW 映射走 VK 事件(修饰键语义成立,IME 不拦 ctrl+键);纯文本走 KEYEVENTF_UNICODE(绕 IME)。
- 通用修饰键 VK_CONTROL/VK_MENU/VK_SHIFT(AHK/pywinauto 同款),开场/结尾全场清场 11 键。
- 编译层纯函数 `compile_key_spec`(报错时零事件发出)+ 编译层单测 7 项与解析层回归(f1-f12 全段、双通道断言、shift 包裹、多键序列、不可映射组合拒绝)。

**护栏(fail-closed 族)**
- verify-then-inject:注入前物理前台(GetForegroundWindow)核对,焦点归属用物理层判据(GetGUIThreadInfo 上溯根窗口);press/type 强制 `--app`。

**验收与工程**
- 金任务 1(计算器 `123×456+789` → `56,877`):10/10,120 次点击零失败,`@eN` ref 重放 100% 稳定;实测 P50 40.2ms / P95 53.9ms / snapshot 275ms / 二进制 0.78MB。
- 金任务 2(记事本输入 → ctrl+s → 另存为 → 写值 → 保存):10/10,含内容读回断言与磁盘逐字终验;覆盖 owned dialog、系统对话框 ValuePattern、IME 对策。
- `cargo xtask setup/gate/fmt` 单点门禁(pre-push 与 CI 同源调用);Makefile 惯例转发层;git hooks(Conventional Commits 校验 + merge-conflict 检查);tag 驱动 release 流水线(verify→build→publish)。
- workspace 四 crate(actl-core / actl-uia / actl-physical / actl-cli);CI(windows-latest);文档体系 PRD / ROADMAP / TECH_STACK / 调研 01-11;AGENTS.md 协作规范。

**设计决策(入档,详见 docs/06)**
- 项目定名 **actl**(agent control);命令面设计(扁平动词式、命名四规则、L1-L3 分层与解耦/整合原则、近似命令对裁定);帮助系统三级层级与 stdout 协议例外条款。
- [spec] PRD 冷启动 P95 50ms → 80ms(附 P50 < 50ms):Windows 进程 spawn 固定开销 ~30ms 为不可压缩项。

### Fixed
- **[BC] `press`/`type` 强制 `--app`;[BC] `@eN` 未指定 `--app` 直接 PROTOCOL 拒绝**:无锚定的键盘注入与 ref 回放会打进"当下恰好前台"的窗口(事故 #1 的真实路径,曾实际偏射)。
- **组合键退化裸键**:`KEYEVENTF_UNICODE` 不携带修饰键语义,`ctrl+s` 曾被打成孤立 's' → 组合键成员改走 VK 通道;**纯文本被 IME 改写**(空格吞成上屏确认、半角标点全角化)→ 纯文本固定 UNICODE 通道,中文模式下唯一可靠文本通道为 `type --paste`;组合键成员不可映射 VK 时 PROTOCOL 报错,不静默降级。
- **owned dialog 不可见**:"另存为"等 IFileSaveDialog 挂靠宿主 depth-1,窗口枚举与 `--expect` 此前全部漏扫 → 两阶段扫描(root 快路径,0 命中才深扫;深扫进热路径曾致每命令 ~2s);`--expect` 升级为 **RuntimeId 基线的新窗口断言**(标题基线会被脏标记"*"改名骗过)。
- **窗口解析**:多匹配 → **AMBIGUOUS 附候选标题**(旧病:首个命中静默胜出);WinUI 三件套(ApplicationFrameWindow/TitleBarWindow/CoreWindow)折叠为单一可见窗口且保留 CoreWindow(内容元素只在其子树)。
- **焦点归属判据改物理层**:UIA focused element 在 WinUI 上会反向谎报(物理层已聚焦仍报别窗口编辑框),曾致成批误拒绝。
- **f10/f11/f12 解析边界**:旧条件 `p.len()==2` 把 f10-f12 挡在解析层而后端支持 1..=12。
- **窗口标题假信号**:效果断言改 UIA Value 内容读回 + 磁盘终验(新记事本设置页打开时标题仍是文档名、tab 名滞后只取首行)。

### Changed
- **[BC]** 全部 crate 由 `ctl-*` 更名为 `actl-*`,bin 名 `actl`。
