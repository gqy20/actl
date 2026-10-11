# Changelog

本项目的全部显著变更记录于此，release notes 由本文件生成。格式遵循
[Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0)，版本语义遵循 SemVer；
0.x 期间破坏性变更在条目中以 **[BC]** 标注。条目只记"变了什么与为什么"；
过程细节与归因在 git 提交信息与 docs/ 里，不在此重复。

## [Unreleased]

### Fixed

- **step 失败证据保留错误原文（[68 待办](docs/68-combined-batch-first-round.md) 收口）**：`failure_evidence` 的 metadata 分支统一附带有界 `message`（200 字符截断）——修复前只有**无** evidence 的 synthetic 兜底带原文，**带** evidence 的失败（如 verify-file NOT_ACTIONABLE 的共享冲突详情）反而只剩 code 丢原文，恢复路径必须重放才能看到原因。message 是错误契约自身字段（envelope 已公开），不属于载荷 redaction 保护面。golden×4 同步（evidence_metadata_v1/observation_metadata/observation_query_metadata/semantic_rejection_v1）+ docs/68 待办勾销 + 新增回归用例（原文随真实证据共存/超长截断）。

### Added

- **`browser-verify --no-popups` 页内弹层断言原语（spec，[06 浏览器节](docs/06-design-blueprint.md)——浏览器页内弹层空档的 v1 检测面；引擎级自动门禁留 ROADMAP 候选）**：原生弹层门禁结构性不覆盖页内 DOM 弹层（browser-* 走 `--session` 非 `--app` 锚；front-report 只收 role:Window 表面）——本谓词补检测面：枚举**可见**弹层表面（默认选择器 `dialog[open],[role=dialog],[role=alertdialog],[aria-modal=true]`，零布局盒=隐藏模板不计；站点自绘浮层走 `--popup-selector` 显式声明），0=通过、>0=`ASSERTION_FAILED` 带 total+样本证据（≤5 条、text≤200 字符折叠空白）。**flow 组合语义**：ASSERTION_FAILED→condition false 是既有映射——when/unless 弹层分支（检测→browser-click 处置）零新协议即可组合。如实边界：无标注自绘横幅不在默认面；在流内静态 role=dialog 段落会过检（过度检测由 samples 证据暴露，收窄靠调用方）；选择器真语法错误 PROTOCOL（浏览器 tokenizer 对未闭合 `[` 按 EOF 闭合是如实语义，`dialog[open`≡`dialog[open]` 实证）。测试：browser 契约 2（help 四要素/--popup-selector 从属 clap 拒绝/未知会话 NOT_FOUND）+ **headless 真机验证六假设全落定**（可见 dialog 命中带样本/隐藏模板不计/自定义横幅默认不中显式中/垃圾选择器 `>>>` PROTOCOL/干净页通过/会话清零——不占交互桌面）。

- **popup_handlers `notice` 分类（spec，[06 运行时治理③](docs/06-design-blueprint.md)——INFO 级弹层无法声明的裁定落地，最窄形态）**：naive 词表（error/save/confirm/update）覆盖不到流程自产的 INFO 级表面（VS Code 1.141「扩展已禁用」通知实证：滞留挡后续步、落 unknown 无法声明处置）。`notice` 处理器认领 unknown 面板：`name_contains` **必填**（按名认领，schema 层拒绝空/缺失——不存在「任意 unknown 一律处置」的宽形态）；已分类面板不被 notice 吞并（分类等值匹配语义不变）；未声明的 unknown 仍永远上抛。审计面 popup_event 双字段：`classification`（面板本体，如实 unknown）+ `handler`（认领声明）。词表演进纪律入册：更宽分类等真实滞留事故再扩。测试：popup_gate +3（schema 必填拒绝/按名认领处置放行 audit 双字段/名字不符与已分类面板均不认领）。**真机验收通过（2026-10-11 探针轮 vscode-notice-probe-01，debug 二进制）**：门禁匹配 notice（unknown 面板+name_contains 命中）→自动点击 Clear Notification (Del)→复查清层→写步执行 completed+verified 全链实证；notice 每次 launch 必现（reopen 第二次实证）。**模板级声明受静态锚局限**（popup_handlers.app 静态子串 vs 模板锚 @run.id——探针靠 run id 前缀约定，正解 handler 支持输入引用，ROADMAP 在案）。

- **`focus` 焦点建立原语（spec，[06 控件身份节](docs/06-design-blueprint.md)——「焦点断言/建立空档」基建项落地；2026-10-11 VS Code Find 面板实证暴露：Close 点击后焦点滞留，`press --target` 只断言不建立）**：UIA `SetFocus` 建立键盘焦点 + GUITHREADINFO 物理焦点根读回核验（3×120ms 退避，RuntimeId 比对），失配 `NOT_ACTIONABLE` 附 expected/actual 证据（焦点去了别处先读证据，不盲目重放）。**零键注入**（input_effects.keyboard:"none"）——只建立焦点，后续注入由 press/type 既有护栏接管（verify-then-inject/前台复核），注入语义严格分离。目标歧义遵循 `AMBIGUOUS`；`--snapshot` 走 ref 身份校验链。flow 中定位为写步（verify 伴随与 click 同规）。测试：cli 契约 2（help 四要素——SetFocus/readback/No keys injected/Errors；坏目标 PROTOCOL）。**真机验收通过（2026-10-11 探针轮）**：ctrl+f→esc 面板绕行后 focus 建立编辑器焦点→press ctrl+end --target 断言式通过→verify focused:Edit 三重实证——建立与断言的分工链正是 Find 面板滞留场景的补面（flow-validate 写步语义同步实证：无 verify 拒绝/verify 伴随过）。

- **`browser-read --deep` shadow DOM 穿透（spec，[06 浏览器节](docs/06-design-blueprint.md)——B 站 shadow 阻断新协议项同日收口）**：在 document 与每个**开放** shadowRoot（open 模式）内递归执行选择器，收集全部命中——Web Component（`<bili-comments>` 类）内部内容从结构性不可见变可读；closed shadow 对页面 JS 结构性不可见，是**如实边界**（不伪装穿透）。与唯一读和 `--all` 均可组合（唯一模式取 elements[0]）；遍历预算有界（节点 ≤50000/收集 ≤5000），触顶 `traversal_capped=true` 如实上报（matches 变下界，不谎称全量）；payload 附 `deep:true`。测试：browser 契约（help 四要素——OPEN shadow roots/closed 边界/traversal_capped；--deep 单独合法不要求 --all）。**真机实证（2026-10-11，B 站）**：评论区 `#commentapp` 普通 CSS 0 命中（非懒加载，滚动后仍 0），`--deep` 命中 shadow 内 `bili-comment-renderer`——协议项登记当日落地。

- **`browser-read --all` 列表枚举形态（spec，[06 浏览器节](docs/06-design-blueprint.md) / [73 round7 收口](docs/73-browser-twitter-scenario.md)——ROADMAP 协议项落地）**：多匹配读动词——返回**全部**匹配：列表语义零命中 = 诚实空集 `total:0`（非 NOT_FOUND，与 find 枚举口径一致）；`--limit`（1..=500，默认 50，requires --all）只裁返回面，`total` 保真 + `truncated_by_limit` 如实标记；逐元素文本截断（64KiB+truncated）与唯一读同规。**位置谓词绕行废止**：`(//a/@href)[k]` 类手工 xpath 不再需要。唯一匹配硬判定只作用于非 `--all` 调用，两形态共存。测试：browser 契约 +2（help 四要素/--limit 从属参数 clap 拒绝）。**真机复验通过（2026-10-11，证据二进制 fnv128=6c3d4e03，本地 fixture 页）**：7 元素全枚举 / `--limit 3` 诚实截断（total:7, returned:3, truncated_by_limit:true）/ 空集 `ok:true + total:0`（非 NOT_FOUND）/ 同选择器唯一模式仍 AMBIGUOUS（两形态共存）/ `--property value` 枚举 input 值；会话优雅关闭、ephemeral profile 干净删除。**首个真实场景消费（B 站，[16 §5.2 增补](docs/16-browser-scenarios.md)）**：搜索页 `--all` 枚举 42 结果卡 + `--property href --all` 取链接后 navigate 直达视频页——选卡/锚点层痛点（活动卡陷阱、整页跳转锚漂移）整体绕开；评论读取被 shadow DOM（`<bili-comments>`）阻断，登记新协议项（穿透读取原语）。

- **for_each 数据驱动迭代（spec，[06 流程协议](docs/06-design-blueprint.md) / [63 收口](docs/63-simple-workflow-conditions.md)——ROADMAP「流程扩展：循环」落地；嵌套分支裁定缓办入册）**：`run: {for_each: {from: "stepid.output" [, max] [, field]}, body: [调用…]}`——列表驱动的**有界**循环：迭代对象必须是**前置步骤 outputs 保存的 array**（典型产源 find/collect；前向引用 PROTOCOL），每项执行一轮 body，`@each` 子串占位派发时注入项标量（`name:@each` → `name:<item>`；field 给定从对象项取成员）。有界纪律：次数=列表长度，max（1..=256 默认 32）兜底，超限 PROTOCOL **不静默截断**；项间停止/暂停检查+看门狗心跳；占位值守卫拒绝 `@eN`（每轮重新观察，不携带跨轮 ref）。约束：for_each 步禁 outputs/read_only-retry、verify 必填；body 禁嵌套 for_each/sequence、禁 bind。失败：第 i 项失败 → 步 Uncertain/Failed + `for_each_item/for_each_child` 位置证据；已完成项重放幂等靠 body 内 ensure/幂等设计（诚实边界入册，不静默跳项）。**通用 while/计数循环不做、嵌套分支缓办**（均无真实场景，出现再立项）。冻结计划四不变式全保留（for_each 是静态结构，列表是运行时数据）。测试：纯函数 7 用例（expand 冻结 round-trip/decode 校验（嵌套/占位/前向引用/max 越界）/列表读取超限/注入守卫/项标量/失败位置）+ flow-validate 契约（合法计划过/坏形态离线拒）。

- **verify-owner 返回 `hwnd_anchor`（[06 verify-owner](docs/06-design-blueprint.md)；ROADMAP「flow 锚点标题插值」登记项降格收口）**：归属核验成功载荷附 `hwnd:<n>` 就绪字符串——outputs/bind 直填后续任意 `--app` 参数槽。**裁定入册：不做标题插值 spec**——HWND 不随标题变（SPA/脏标记改标题正是引入 hwnd 锚的原因），"归属步产锚 → 后续步绑锚"的惯用法用既有机制（outputs/bind/`hwnd:` 锚）即可锚定动态标题窗口，静态计划无须新协议面。真机（Explorer fixture）：目录改名使窗口标题失配后，经 hwnd 锚 snapshot 仍寻址同一窗口；完整 type 链的干净轮受 Store 版记事本（System32 无 notepad.exe、AppContainer 命令行使 verify-owner 不认领——**新发现入 pack 待办**）与幽灵进程污染（docs/41 已知族）阻碍如实记录，留待桌面空闲轮。

- **`get-selection` 选区读取原语（spec，[06 §3.4 系统组](docs/06-design-blueprint.md)；ROADMAP 2026-10-07 登记 Quicker 差距项落地——[10 §6](docs/10-quicker-demand-research.md) 点名「对 AI 接口最有价值」的上下文通道）**：读当前用户选区，纯读取零注入。通道序：**TextPattern 范围**（kind=text，逐范围文本）→ **SelectionPattern 容器**（kind=items，逐项 role/name/automation_id）→ **如实 unavailable**（no_selection/no_selection_support，不猜测不回退剪贴板注入——物理 Ctrl+C 通道须显式物理授权，留给调用方组合，本命令不伪装成能读到）。目标缺省 `focused:`（选区随键盘焦点所在控件，与 `focused:` 寻址族合成显式上下文面）。**边界诚实语义（真机实证入实现）**：UIA 折叠光标（caret）的 GetSelection 返回退化空范围——过滤后全空 = `no_selection`，不把"只有插入点"报成有选区（记事本真机：ctrl+a 读回 ranges=["hello selection world"]、折叠光标如实 no_selection）。测试：cli 契约（help 四要素/PROTOCOL 坏目标）+ 真机验收（记事本输入→选中→读回全链、caret 边界、保存提示白名单清理——close 存活复核如实拦、点不保存后窗口归零）。

- **AMBIGUOUS 目标串联（spec，[06 流程协议](docs/06-design-blueprint.md) / [63 收口](docs/63-simple-workflow-conditions.md)——ROADMAP「flow 歧义候选串联」落地）**：run 相定位**多命中**（目标在 args[0]、有 `--app` 锚、证据含候选）时不再走通用失败语义——引擎立即锚定**骨架快照**（`Host::execute_snapshot`，候选 ref 的身份来源），候选证据**全量保留**（不走默认 redaction），run 进 needs_review（`ambiguous_target:<step>`）、步保持 Pending（locate 拒绝=零派发，无 uncertain 语义）。应答走 **`flow-resume --id <id> --pick @eN`**（校验 ref ∈ 候选集 + 唯一待应答暂停；与 --decision/--retry-step 互斥，探索 run 拒绝）：续跑时 args[0] 重写为选中 ref 并追加 `--snapshot`——身份核验交给既有 ReplayGuard（窗口重建/元素漂移即 STALE_REF 拒绝派发）；陈旧 pick（修订后目标已变）PROTOCOL 拒绝。flow-status 附 **ambiguous_hint**（候选清单+可复制应答命令，同 popup_hint/决策卡先例）。pick 是运行时纠正（同 --retry-step 先例）不改 plan_version，冻结计划四不变式全保留；修订重置步骤时 AMBIGUOUS 记录不迁移。**自动缩小查询裁定不做**（入册）：引擎侧静默改写选择器违背冻结计划显式变更纪律，缩小 = 调用方 pick 或 flow-revise。测试：MockHost 引擎级 4 用例（暂停锚定/pick 重写带 snapshot 绑定/非候选与无暂停 PROTOCOL/陈旧 pick 拒绝）+ 全量既有 flow 测试回归绿。真机验收待首个真实应用轮复用（机制层验收=引擎测试）。

- **`collect` 虚拟化容器有界全量收集（spec，[06 §3.4](docs/06-design-blueprint.md)；ROADMAP 2026-10-07 登记项落地，形态裁定=引擎有界动词非 flow 循环）**：**先语义复位到顶**（收集语义=整表，不受用户遗留滚动位置影响）再滚动（每轮 1 LargeIncrement 页级，语义 ScrollPattern，**零输入注入**——物理轮组合留给调用方显式 `scroll --physical`，ADR-007）× 沉降 × 每轮新鲜容器观察 × 差分去重累积 × 收敛。**完成判据 = 纯经验（空连两轮零新增，`converged_by="empty-streak"`）**；容器 RuntimeId 终检防重建误报 complete。items 不带 `@eN`（虚拟化条目跨轮无 ref 稳定性，重定向走 find）；预算 1..=200 轮/1..=5000 条，截断如实 `complete=false + truncation_reason`（max_items/max_rounds/scroll_reset_failed/scroll_pattern_missing/scroll_call_failed/container_rebuilt）；`--role` 过滤只计过滤后条目。**设计过程中的三处真机否证（Explorer 600 文件 fixture，如实入档）**：① ItemContainerPattern 枚举模式只返当前物化窗（17/600）后假报耗尽——假完整，不采用；② 起始位置依赖（从底部起步误判"起始即末尾"）——复位到顶消除；③ 滚动位置信号（percent+view≥100）提前一页误报（触发后小步仍可推进、漏尾 14 条）——位置信号降为启发，完成权威只有空连两轮。测试：core 状态机单测 6 + golden envelope 契约 + cli 契约（help 四要素/PROTOCOL 预算校验/schema 自发现同步）。真机（资源管理器 fixture）：600 文件目录 **600/600 全收**（42 轮空连收敛、底部起步位验证复位）；5 文件短列表 3 轮即 complete；`--max-items 500` 截断如实；容器双 List 用序数直选；两窗口按 hwnd 优雅关闭。微信/网易云 Electron 列表的兜底路径真机覆盖待后续应用轮（本验收靶为注册的资源管理器）。

- **`cargo xtask doctor --clean`（构建工具，残件清理灰区收编）**：一键诊断顺带删除 ghost 残件映像；残件判定从只认自动 `.ghostN` 扩展为 actl 家族的二级后缀改名产物（手工 `live-`/`locked-` 先例同族——原盘点对手工残件漏报），仍被进程持有的如实保留不硬删。配套纪律入册（[AGENTS bash 条目](AGENTS.md)/[70 §4](docs/70-debug-playbook.md)）：脚本文件纪律只约束执行形态，不豁免工具/yml 优先级——进程普查、锁探针、残件清理这类管辖面直达工具，不再反射性写一次性脚本（2026-10-08 cleanup-check.ps1 重复 doctor 管辖面的教训）。

- **AMBIGUOUS 候选证据附 `context` 上下文行（spec，[06 §4 错误码表](docs/06-design-blueprint.md)；third-hand 调研对照见 [08 增补](docs/08-agent-cli-jev-design-proposal.md)，"flow 歧义候选串联" ROADMAP 项的前置零件）**：多命中候选证据在既有 `{ref,role,name,automation_id}` 之外，**重复标签**（trim 后同名，含空名）附 `context` = 最近具名祖先文本（所在行/容器）——同标签兄弟控件从"调用方读整棵树自行分辨"变"证据自带区分度"。派生走索引父链上溯（walk 缓存读**零额外 COM 往返**，8 跳有界、120 字符截断；遍历根豁免，无具名祖先为 null 如实不造）；唯一标签不带 context 控制证据体积。测试：上下文派生纯函数 5 用例 + 条件发射/空名 null/golden envelope 契约 3 用例（`ambiguous_candidates.json`）。

- **`find --capable click|type` 动作兼容过滤（spec，[06 §3.4 find](docs/06-design-blueprint.md)）**：role 表派生的**建议性**候选池过滤——type = Edit/Document 文本载体（Word/浏览器正文即 Document，word-edit 流程实证），click = 可交互角色减文本载体；零额外属性读。把 docs/31 输入护栏从"执行时 NOT_ACTIONABLE 拒绝"前移到"选择阶段规避"，省 agent 侧无效候选的试错往返。诚实边界：实际可操作性仍以动作时 pattern 分派判定为准，分类不构成承诺；滤空即如实空结果，provider 快路径/全树/L4 虚拟化命中**同口径过滤**（快路径滤空回落全树重判，非静默分叉）。未知取值 PROTOCOL（UIA 触碰前拒绝）；与 `--target` 互斥。测试：core 分类单测 + help 四要素 + cli 契约（schema 列出 capable / 未知值 PROTOCOL / 帮助文本）。

- **探索引擎输入通道内容判据（[06 §L2](docs/06-design-blueprint.md)；探索待办 ② 收口）**：`explore.texts` 原语的 type invocation 按文本内容自动选通道——**非 ASCII 文本注入 `--paste` 走剪贴板通道，ASCII 保持键盘**（与录制编译器 record.rs 同口径的内容子句；引擎是发起方观察不到 IME 键，故只取内容判据），ref 绑定与选择器两形态同覆盖；**writeback 路线导出同口径注入**——中文路线重放不退回键盘退路，导出即真机可用。flow YAML 人工/编译器 authored 的 type 步骤本就透传 `--paste`（Adapter 走完整 CLI 解析），无需变更。测试:MockHost 三用例（非 ASCII selector 形/ref 形注入 + ASCII 不注入）+ route 导出双用例。真机验证（2026-10-05,证据二进制 81ee14e8）：记事本中文 texts 探索轮 goal_met + 三重物理证据（值断言/标题送达 `*探索验收中文 - Notepad`/剪贴板残留）,writeback 导出 `--paste` 注入同口径实证——详见 [77 探索路线 #3](docs/77-exploration-s2-acceptance.md)；同轮积累路线 #3（`apps/notepad.yml` 首建 + 重放全步 verified,待人工复核迁移）。

- **MCP 适配服务 actl-mcp（spec 轮 1-2，[docs/82](docs/82-mcp-adapter-research.md) / [06 §7.8](docs/06-design-blueprint.md)；PRD 2026-10-05 授权）**：远程访问被控 Windows 设备的独立服务——**独立 crate（不依赖任何 actl workspace crate，纯薄壳；async/tokio 仅限该 crate，ADR-001 定界修订由编译器强制）+ 官方 rmcp SDK（协议层不手写，用户裁定）**。① **传输模式 A**：`actl-mcp --listen [--token]`——默认 127.0.0.1 防意外暴露，LAN 显式 0.0.0.0；token 缺省 256-bit 生成 + stderr 配对横幅（人工抄 token = 设备主人知情授权）；axum 壳 Bearer 门禁（401）+ Origin 校验（403，规范 MUST）；明文 HTTP 仅限可信局域网；SSH/stdio 与 tasks 扩展后续可选。② **子进程派发模型（安全穿越）**：每次 tools/call spawn 真实 actl 子进程（ACTL_TASK_ID 审计、kill_on_drop 断连即杀→interrupted 恢复语义）——预告/放行/急停/看门狗全部本地权威，远端结构性无法绕过；急停后子进程绑停止代次自动 fail-closed；envelope 唯一权威，非法输出合成诚实错误不伪造 ok。③ **能力面三层懒加载**：通用 `actl` 工具（速查 ~700 tokens 一次性）+ `actl://help/{command}` 资源模板（按需 ~300-500）+ 全量 schema 不进资源面；支持面 = 全部命令 − 排除表（execution-recover/display-start/desktop-restore/record-worker——操作本地监督机制的动词出局，排除不静默）；`ok:false`→`isError`，code/hint/evidence 全保留（远端失败信息与本地同构）。测试：库单测 9 + HTTP 契约 7 + **rmcp 客户端互操作 1**（SDK 自带客户端栈↔服务端全循环：生命周期协商、tools/list 速查嵌入、tools/call 真实派发、资源读取、排除动词错误语义——裸 HTTP 契约覆盖不到的客户端实现互操作面）；rmcp 强制 2026-07-28 无状态契约实证（_meta/Mcp-Name 头；bearer_auth 自带 Bearer 前缀——auth_header 传纯 token）；js-sys 0.3.85 锁对齐（wasm-bindgen-futures 精确钉死 vs chrono 冲突）。真机跨设备验收待用户场景。

### Fixed

- **测试线程栈上限（clap 命令树深度，[.cargo/config.toml](.cargo/config.toml)）**：debug 构建 clap derive 构建整棵命令树的调用链随子命令数增长——第 50 个子命令（collect）使 selector_input 全量解析测试越过 libtest 线程默认 2MB 栈（STATUS_STACK_OVERFLOW；真机二进制主线程栈充裕不受影响，契约测试 spawn 真二进制一直通过）。`[env] RUST_MIN_STACK=8388608` 仓库级统一（dev 内环/gate/CI 全覆盖），对齐主线程水位、覆盖命令面继续增长。
- **scroll UIA 分支方向反转（真机核验通过）**：`scroll_element` 的语义分支把 notches≥0（`--up`）映射到 `SmallIncrement`（UIA 语义=**向下**滚），与物理滚轮分支（正=向上）反向——`--up` 经 UIA 通道实际向下滚。修为 `--up`→SmallDecrement/`--down`→SmallIncrement。真机实证：资源管理器列表顶部 `scroll --down 5`（via uia-scroll）后物化窗口下移 4-5 条（旧实现会因向下无空间而原地不动）。物理分支方向本就正确，不受影响。
- **handoff golden 契约脱环境依赖（主分支既有红项）**：`handoff::error` 的 execution 态读改**路径注入化**（新增 `error_at(paths, reason)`，生产路径 `error` 不变）；`handoff_yielded.json` 补 [72b2671](docs/47-execution-recovery.md)（基建②）给 handoff 证据加 `execution` 字段时欠下的同步——原测试直读 `%LOCALAPPDATA%` 真实代次，任何有执行状态的机器必红（golden 含确定性 `state:"unavailable"`），隔离目录读后与本机状态解耦。
- **子树物化（2026-10-07 方案2——观察面 N→1 往返收口）**：树观察缓存请求 `TreeScope=Subtree`——`build_updated_cache` **单次调用**物化整棵子树（属性+父子链接入本地缓存），遍历走 `get_cached_children()`（纯本地零往返）——N 次 children RPC 变 1 次。collect 循环不动（上限/顺序/refs 编号**逐字节保留**，真机 diff 实证两模式输出一致）；物化是单次长调用，UIA 区段 20s 限即其界；`ACTL_WALK_LEGACY=1` 回落逐节点遍历（provider 兼容逃生舱 + 重树 A/B 对照组）。真机：记事本 find --name **615ms**（逐节点 776ms / 预取版 2.4s / 原始 5.7s——对原始 **~10×**）；计算器 XAML 同过；**子串搜索 0 命中探测同趟免费**（方案3 被吸收）。重树（网易云级）受控 A/B 用 `ACTL_WALK_LEGACY` 同窗交错，记 ROADMAP。
- **lint-packs 选择器形态建议（知识层杠杆,advisory 不计失败）**：⑤ 号检查对 `name:` 步骤给提示——元素有稳定 AutomationId 优先 `id:`（find --id 走 provider 单调用快路径）,`name:` 子串语义保留给无 id 元素（Electron/CEF 系普遍无 automation id）。现状点名 7 pack 共 20 处（微信 7/腾讯会议 4/Zotero 4 等——多为无 id 应用,属合理保留）;选择器优先级约定入 [PACKS.md](skills/actl/PACKS.md)。**不做无证据的 name:→id: 批量改写**（真实观察确认 id 存在才改,先探测再写死纪律）。
- **树扫描属性预取（2026-10-06 性能轮）**：`tree_bounded` 的缓存请求带上 Name/ControlType/AutomationId/ProcessId——walk 后的逐元素三连读（collect_matches/append_observed/observed_identity 各自 get_*）从 N×3 次跨进程往返变为**零往返缓存读**（cached-first 助手,非 walk 来源元素回退实时读）。真机 A/B（记事本,同窗热态）：`find --name` 2.4-2.6s（低方差）vs 无预取 5.7-7.0s——**~2.5×**;纯 walk 路径中性;重树（往返主导）理论收益更大,受控 A/B 待后续轮。
- **Located 出域审计收口 + 树扫描区段口径（2026-10-06,[06 析构纪律](docs/06-design-blueprint.md) 覆盖面扩展）**：select_option/toggle_element/pointer 家族(physical/drag×2/drag_path/scroll)/type_text/press_keys_targeted 全部收编 into_info 显式 drop 模式——**COM 元素不再跨函数出 actl-uia 域**(返回值全部元数据化;type_text 经 wrapper+inner 保 RAII 守卫生命周期,drag_physical 行尾第三次冗余重定位一并消除);树扫描 collect 前沿循环补 `rearm()` 逐节点重臂——区段口径从「整段 wall clock」变「单次跨进程调用」(冷 provider 合法慢扫描曾被 20s 误杀,网易云全树三度中止实证),单槽嵌套约束入契约文档。
- **find --id provider 侧快路径（[06](docs/06-design-blueprint.md)）**：单一精确条件走 Subtree FindAll + AutomationId 等值——单次跨进程调用替代全树遍历(真机:记事本 258ms vs 1146ms,重树分钟级→秒级);命中走 L4 同形态(`resolved_by:"provider-exact"`,无快照绑定),无命中回落全树路径保契约。**--name/--role 子串语义不走此路径(UIA 属性条件仅等值——诚实边界)**。
- **xtask doctor 一键诊断 + 构建锁自动规避（[xtask](crates/xtask/src/main.rs)）**：`cargo xtask doctor`——进程普查(活/幽灵分组)+ 四产物锁探针(write-open 被拒 = 有进程映射,语义实证)+ ghost 残留映像盘点 + 显示端状态;`dev/evidence/gate` 构建前置自动 ghost-rename 被锁产物(改名不影响持锁进程,cargo 写新文件)——docs/81 §8 僵尸锁构建的日常摩擦从手工规避变自动。
- **进程看门狗默认降档 300s→120s + 心跳修真（[06 §7.7](docs/06-design-blueprint.md)）**：降档动机 = 尾段挂起族滞尾自清速度（300s 下每轮真机验收积压楔死进程的体感）。前置把心跳修成真语义——原 wait×4/录制的心跳只在**循环前**重置一次（轮询/空转超期限即误杀的潜伏缺陷,录制用户停顿 5 分钟也会被杀）,现改循环内逐轮/逐批;树扫描（`collect` 前沿循环）补**逐节点心跳**——合法长路径（全树扫描分钟级,网易云 126s 实证）全部带心跳不受默认降档影响,心跳停止的单点卡死才触顶。契约测试显式设 800ms 不受默认值影响。
- **尾段挂起族根治（两轮,2026-10-06;真机实证:工作完成后进程不退出,内核态不可杀,300s 看门狗后仍滞留）**：根因 = UIElement 的 Drop（跨进程 `Release`）与读取调用同族无界,而析构路径无看门狗覆盖——thread_local 定位缓存、栈内局部、反序析构让 guard 先于 COM 资源退出三个暴露点。修复:① **成功路径硬退出**——envelope 落盘 flush 后 `process::exit(0)`（与既有错误路径对称,析构链整个跳过;状态落盘不依赖 Drop——`finish(phase)` 显式调用、Journal 每事件即 flush）;② **COM 元素不出 actl-uia 域**——`ResolvedInfo` 元数据面 + `Located::into_info()` 消费式转换（调用点在看门狗区内,显式 drop 与声明顺序无关）,read_property/verify 三分支/click/set_value/cache_store 全收编,VerifyOutcome 与 click 返回签名不再携带 COM。真机验证:原卡死命令序列（snapshot/get/find）在修复后 808ms/429ms 干净退出、零僵尸积压。残余暴露面（select/toggle/pointer/type 的 Located 返回路径）记 ROADMAP 待审计;析构纪律入 [06](docs/06-design-blueprint.md)。
- **读路径 UIA 看门狗缺口（真机实证 2026-10-05:`get --property value` 挂起超 300s）**：`read_property` 与 `verify` 的 Value/Checked 分支补 `watchdog::enter()` 区段（20s 击杀留 stderr 标记,`ACTL_UIA_WATCHDOG_MS` 可调）——与 click pattern 派发（493d163）同族,定位覆盖止于 locate,属性/模式读取是最后的裸区段;探索引擎 goal 断言走同路径,裸区段即整轮探索的挂点。
- **launch 选项后置静默失效**：trailing_var_arg 语义下程序名后的 token 原样转发子进程——`launch <prog> --wait-window X` 中选项从未生效且子进程把选项当参数（记事本弹「找不到 --wait-window….txt」实证,`window_ready:false` 假象）;现 fail-fast PROTOCOL 带选项前置指引（只拦本命令独有 `--wait-window`,`--timeout` 是常见子进程参数不拦）。契约测试 + help 同步。
- **launch App Paths 解析面补 HKCU**：Store 版应用（notepad.exe）只注册 HKCU,此前解析面仅 HKLM → `launch notepad.exe` NotFound;现 HKLM 先查 HKCU 补面。真机验证裸名解析并启动成功;单测 volatile 键自注册自清理（不污染机器注册表）。

### Documentation and validation

- **note-typo-012 干净轮收口（Word 样式流程 evidence 二进制指纹级全链，[81 §10](docs/81-word-style-flow.md)）**：010 攻坚链五层修复合力验证通过——环境清零（进程/产物锁/显示端 0.2.1 匹配，旧伴生端 WM_CLOSE 优雅换新）后 **10 步全链 completed，证据二进制 fnv128=19f4b6a8**（009 欠的 evidence 指纹至此补齐）；fill-2/fill-3 干净通过坐实 010/011 卡点=环境污染而非代码缺陷；**默认行走模式**（物化观察+live 动作链，无 legacy 逃生舱）focus 无 NOT_ACTIONABLE——「Word 轮必走 ACTL_WALK_LEGACY」更新为默认模式已过全链。冷启动 UIA 区段杀当日复现一次（exit 125，既有 knob 60s resume 收口）；产物 `flow-resources/note-typo-012/note-typo-012.docx` 独立 meta 复验通过、Word 优雅关闭。**新暴露面如实入档**：flow-resume 成功路径 envelope 落盘后楔死（尾段挂起族新签名，ROADMAP 挂起族条目已补）。011（10-07 污染时代失败记录）留档未覆盖。

- **L0 路线命中重放的 skill 层流程化（[77](docs/77-exploration-s2-acceptance.md) 遗留收口;探索待办 ④a）**：SKILL.md「应用路线」节落地任务到达双分岔指引——①命中 `status: verified` 路线直接重放（≥3 步转 flow 交 flow-run/短路线 batch、语义选择器+`{window}` 绑定、refs 当轮重观察、verify 即断言不静默重试）;②unverified/无路线走探索引擎（`explore:` 段 flow-run 启动、决策卡 `flow-resume --decision` 应答、`explore-writeback` 抽片段合并 pack 人工复核转 verified）——知识飞轮读侧闭环,下次同类任务零探索。

- **遗留验收三项收口（2026-10-05）**：① **健壮性基建第一轮真机验收**——[81 §9](docs/81-word-style-flow.md)：009 轮 10 步全链 completed 总 155s，七件基建逐项生效（UIA 区段看门狗、close 存活复核、定位缓存 `resolved_by:"cache"`、save-disk meta 断言、StepRecord 逐步耗时画像）；evidence 二进制指纹级补跑（note-typo-010）入 ROADMAP 待桌面空闲。② **录制值读回 #4 双轨收口**——[80](docs/80-record-acceptance.md)：dev+探针三轮（rec-ma-7/8/9）+ 证据二进制正式轮（rec-ma-12，fnv128=519b005f，burst 末值 `UNIQROUND12v` 精确命中，13 键全捕获），挂起项清零。③ **网易云队列「收起」方向复验**——识图证实收起成功，推翻 2026-10-04「Table 树内残留=未收起」误判：系 Electron UIA 树滞留，收起核验走截图不走树；同轮关闭攻坚查明主窗为 **frameless（WS_SYSMENU=False）**：进程外 WM_CLOSE/SC_CLOSE 与 UIA WindowPattern.Close 全部无效而 UI 线程健在（WM_NULL 即应、SC_MINIMIZE 正常），可行路径为**像素定位自绘 X + `click xy --physical`**（实测成功→托盘隐藏），close-window 报告诚实非缺陷；配方与 DPI 坐标口径教训入 pack 两条 quirk（`collapse_tree_lag`/`programmatic_close_refused`）。


## [0.2.1] - 2026-10-05

本版是里程碑间的能力补丁：0.2.0 建成的执行底座上，补齐**「人演示→YAML」的录制入口**（GUI 操作录制四件）、**焦点元素寻址**（树不可见应用的单次注入打字）与**运行时治理**（失败采集、弹层门禁、修订续跑、幂等步骤、进程看门狗）；探索模式补完 S2/S3/L2 成闭环，Word 样式流程沉淀首条真实业务编排。四条真机验收轨全部通过，详见各 docs。

### Added

- **GUI 操作录制：人演示一遍，得到可复放的 YAML（spec，[76](docs/76-gui-recording.md) 设计 / [79](docs/79-quicker-recording-research.md) 调研 / [06 录制节](docs/06-design-blueprint.md) 协议）**：`record-start/record-worker/record-stop/record-status` 四命令——低级键鼠钩子捕获（回调极简入队 + 工作线程点击瞬间 UIA 反查 + 焦点身份），trace JSONL 证据文件（密码框键 VK 永不落盘、注入事件如实标注不编译、自家窗口过滤）。**信号条/托盘右键「开始/停止录制」**（动态菜单项，label 前缀「录制中」满足反后台录制提示）。机械编译器：换窗延迟分析→`wait` 步骤；键入 burst 末**值读回**（提交文本覆盖 VK 推导——IME 候选/自动纠正的真值；中文自动 `--paste`）；`--app` 字面标题泛化为 flow inputs。agent 改写轮指引落 skills（[record-rewrite.md](skills/actl/references/record-rewrite.md)：TODO 分类修复/选择器稳健化/噪音清除/参数化/**修订优先迭代**）。真机验收：注入轨四发现全修（DPI 口径/LLKHF 位/Value→Text 回退/读回竞态），人工演示轨 47 事件 43 真人输入全捕获 + IME 旗舰断言成立，改写重放 v1→v7 六轮收敛至单命令零前置全链通过——[80](docs/80-record-acceptance.md) 全记录。
- **焦点元素寻址 `focused:`（spec，[06 §3 命名规则](docs/06-design-blueprint.md)）**：`focused:`（裸）/ `focused:<Type>`（角色校验）解析到 OS 键盘焦点元素——树对语义选择器不可见但焦点可达的应用（Electron 懒树，uTools/网易云实证）的唯一元素寻址通道。属主判据用物理焦点根（GUITHREADINFO，UIA focused 视图在 WinUI 会谎报），角色/属主不符 fail-closed NOT_FOUND 带证据。**与 type 组合即单次注入打字**：`type focused: <词> --app <win>` 走既有逐字键盘通道一次注入全词——uTools 实测 5 字符 916ms（逐键 press 五调用 ≈8.5s），识图实证搜索筛选被驱动；网易云中文 3 字符同验证。
- **运行时治理：失败可采集、弹窗先判读、修改可续跑（spec，[06 §7.7](docs/06-design-blueprint.md) 三轮）**：① **`flow-evidence` 失败采集包**——uncertain/门禁阻断瞬间自动归集截图 + 窗口清单 + 定向 UIA 快照 + 前台弹层快照（含分类），新只读命令索引；② **写步步前弹层门禁 + `popup_handlers` 声明**——写步动手前先听前台引导层，未知面板（unknown 且可交互）→ `popup_pending` 零副作用拦截；声明过分类的面板（error/save/confirm/update）经既有 click 全护栏自动处置留痕；Win11 对话框宿主空壳降噪（无按钮 unknown 不阻断）；③ **`flow-revise` 版本化修订续跑**——步骤级 diff，完成步规格未变保留结果（`plan_version` 溯源），写步变更须 `--accept-rerun` 点名，uncertain 步变更即重试授权；旧计划字节归档永不覆盖、版本谱系 flow-status 可见；④ **`ensure:` 幂等步骤**——只读 check 通过直接完成不派发（重跑/续跑环境前缀自动短路）；⑤ **进程看门狗**——「无进展」判定（步派发/等待轮询/恢复重试/录制事件/探索轮次为心跳），默认 300s（`ACTL_WATCHDOG_MS`），耗尽自我终止留 stderr 标记，UIA 无界挂起族（15 僵尸实证）从不可杀变为限内诚实失败。真机 E2E 全链：冲突框 popup_pending 拦截→四工件采集→修订 v2→自动处置→续跑 completed。附带 `retry_hint`（needs_review 信封直接给可复制的 `--retry-step` 命令）。
- **探索模式 S2/S3/L2 成闭环（spec，[42](docs/42-exploration-mode.md) / [06 探索节](docs/06-design-blueprint.md) / 真机 [77](docs/77-exploration-s2-acceptance.md)）**：**S2 有界探索引擎**——关键词投影观察→差分→前沿评分→熵门（头部领先自动执行、平局外抛决策卡 4KiB/Top-K≤8）→probe-before-act→点击归因→本地 goal 核验；`explore` 计划段直接 flow-run，`flow-resume --decision` 应答。**S3 路线回写**——`explore-writeback` 从完成运行抽取 pack 片段（status 恒 unverified，人工复核迁移），死候选生成 quirk、tried 数据导出 `explore.priors` 效果风险先验（先验飞轮实证：同 goal 从 `goal_check_failed` 变预算耗尽）。**L2 原语**——`explore.texts` 输入原语（解锁搜索框/文件名类任务）+ 跨窗 goal 探测（对话框内断言链）。真机：计算器/记事本 4 场景 + L2 + S3 闭环全过，token 审计实测关键词帧 0.3-1.7KiB/次。
- **Word 样式流程与文档断言扩展（[81](docs/81-word-style-flow.md)）**：`verify-file --kind docx-styles` 逐段样式断言（OOXML styleId + 加粗，机器可验不依赖视觉）；`--kind meta` 落盘元数据断言（`{size,mtime_unix_ms}` + `size_min/mtime_min` 关系键，**元数据读取不受 Office 排他锁影响**——补上「ctrl+s 实际未落盘但只读活文档验证照样通过」的缺口）；`--expected-file` 期望文件读（shell 引号/编码坑消灭）。`word-note-typography.yml` 首条真实业务编排：模板预置样式 + 光标式应用 + 填充式输入（模板 v2 十二空段，步骤 21→12、输入调用 40→25），启动错误框经 popup_handlers 引擎自动处置（治理首轮真机首战）；Word pack 增 style_choreography 路线与四条 quirk。
- **健壮性基建（spec，[06 增补](docs/06-design-blueprint.md)；[81 §8](docs/81-word-style-flow.md) 事故链收口）**：UIA 调用区段看门狗（`ACTL_UIA_WATCHDOG_MS` 默认 20s，超时 exit 125 + stderr 标记——无界 COM 挂起限内失败）；close-window 存活复核（投递后有界等待销毁，仍存活 NOT_ACTIONABLE 带同 pid 弹窗清单）；list-windows 前台兜底（provider 楔死漏窗时 Win32 直读回填）；wait --element 瞬态重试（观察瞬态按未出现继续轮询）；**进程内定位缓存**（同 (app,target) 命中以 RuntimeId+根窗口双复核替代全树 DFS——Word 级树实测 DFS 2-3s/次的大头消除，`resolved_by:"cache"` 如实标注，仅静态语义选择器可缓存）；StepRecord 逐步耗时（started_ms/finished_ms 性能画像）。
- **双循环二进制纪律工具化（[70 §8](docs/70-debug-playbook.md)）**：`cargo xtask dev`（诊断循环，增量 DEBUG 实测 ~4s）/ `cargo xtask evidence`（证据轮，RELEASE + FNV-128 指纹落 binary-log.txt）结构性分离；Makefile 精简为高频别名层。

### Fixed

- **录制值读回护栏按 HWND 比对（[80 发现 #11](docs/80-record-acceptance.md)）**：护栏曾按焦点窗**标题**精确比对——编辑改标题（脏星号）即必然失配，值事件系统性缺失；改用焦点根 HWND（稳定身份）。#4 原始形态（即时停止）正式轮复验通过。
- **input 绑定渲染与残留守卫（[80 发现 #6/#6b](docs/80-record-acceptance.md)）**：渲染层把 `{input: name}` 绑定加引号成字符串致替换不发生，run/verify 拿同一字面量会**自洽假通过**；双层修复（`arg_scalar` 原样发射 + 引擎 PROTOCOL 拒绝残留绑定）。
- **YAML 数字/布尔/null 标量加引号（[80 发现 #5](docs/80-record-acceptance.md)）**：`--timeout 8022` 裸渲染被解析为整数而 flow schema 拒绝；数字形态纯量强制引号。
- **投影快照 ref 编号口径（[06 增补](docs/06-design-blueprint.md)）**：@eN 改为全树 DFS 序数——修复前投影快照重编后与 `click @eN --snapshot` 回放计数错位，投影 ref 从未正确回放过；三口径（输出=身份=回放）对齐。
- **同进程并行 UIA 读回退串行（真机证伪）**：两个并行 find 必有一个 `E_FAIL`——CLI 移除并行分发，批量通道保持串行。

### Documentation and validation

- **录制四条真机验收轨全记录（[80](docs/80-record-acceptance.md)）**：注入轨（四发现修复+文本保真对照）、人工演示轨（IME 提交值编译旗舰断言）、改写重放轮（v1→v7，#7-#11 发现全收口，最终单命令零前置 12 步全链）、#4 沉降正式轮；事故教训同步入档（弹层判读纪律/误点纠正/复活循环破除）。
- **探索模式 S2/S3/L2 真机验收（[77](docs/77-exploration-s2-acceptance.md)）**：熵门自动路径、决策卡往返、诚实终态、token 审计、回写+先验飞轮、texts 输入原语、跨窗 goal。
- **Word 样式流程真机验收（[81](docs/81-word-style-flow.md)）**：note-typo 多轮（样式断言/提速 30%/popup_handlers 首战/健壮性轮事故链）。
- **应用档案扩充**：新增 uTools pack（粘贴死路/树不可见焦点可达/focused 输入通道，search_launch_app verified）；网易云 search/open_queue 复验（focused 单调用中文输入，manual→verified）；Word 增样式路线；lint-packs 12 packs 323 steps。

### Known Limitations

- 非开发机 US3 复现搁置（0.2.0 遗留，用户裁定）；机器到位后补验。
- 浏览器写入 uncertain 状态不跨进程持久化（0.2.0 遗留，归 M3）。
- 网易云队列「收起」方向未完全证实（展开已验；二次点击 ok 但 Table 树内残留，待复查）。
- 健壮性基建第一轮的真机验收部分挂起（[81 §8](docs/81-word-style-flow.md)：009 轮待环境恢复）；看门狗/弹层门禁/修订续跑/ensure 已有契约或 E2E 验证。
- `record-pause` 未实现（菜单不设项）；录制会话时长上限内单会话。

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
