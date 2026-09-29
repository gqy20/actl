# Changelog

本项目的全部显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0),
版本语义遵循 SemVer;0.x 期间破坏性变更在条目中以 **[BC]** 标注。
条目只记"变了什么与为什么";过程细节、实测战况与归因在 git 提交信息与 docs/(场景实测
记录、timing 报告)里,不在此重复。release notes 由本文件生成:正文中的
`@eN` 等 ref 语法必须置于反引号内——裸 `@` 会被 GitHub 解析为用户提及
(v0.1.3 曾因此把无关账号 en 列进 Contributors)。

## [Unreleased]

- 真机回归新增目标预览入口：`cargo xtask preview-tests` 构建 CLI 与 devtools 后串行运行 preview_window 交互用例（活动急停时整轮让行报阻塞 77；锁屏/遮挡由用例护栏拒绝并如实失败；默认跳过需人工点击的 mouse_selection_acceptance），替代此前仅存于 docs/46 的手工命令片段。flow 契约测试的 Sandbox 补清 ACTL_TASK_CONFIG，消除开发者 shell 残留该变量时的隐性污染。

- 测试清理与门禁计时：旧 cli.rs 契约用例并入 cli_contract.rs（去除 6 项重复、删除恒真断言、合并同调用的两个 batch 断言）；停止代际并发一致性测试写者循环 100→30 轮，去除纯归档 IO 耗时；gate 各步耗时随 stderr 输出，慢环节可头内比较。核查中确认 `--physical --with <键名>` 缺白名单校验、未知键名静默映射为 Win 键，已登记 ROADMAP 待修。

- UIA 语义动作预览后按 RuntimeId、PID、角色、名称、AutomationId 和原生宿主身份复核目标；布局移动不再误报不可操作。动作重新获取所选 Pattern，控件禁用或身份变化仍拒绝；键盘焦点与物理命中护栏保留。workflow 的 metadata 证据保留 target_identity_changed / target_disabled 原因。

- VS Code workflow 使用状态栏按钮跳转行、快速访问按钮切换文件和菜单执行保存/撤销/恢复/替换，减少快捷键注入；动态输入框与结果项等待稳定，跳转后等待目标行列出现再核验。组合模板同步；新版独立 workflow 真机 26/26 通过，主文件与第二文件精确核验及正常关闭通过；实测与限制见 docs/67。

- 标题定位先从可见原生窗口筛选候选，仅读取候选的 UIA provider，并复核 PID 与标题；不存在原生候选时保留虚拟窗口发现路径。避免已知原生窗口的定位被无关应用的整桌面 UIA 枚举故障阻断，目标读取失败仍报错。

- 日志历史清理间隔从 60 秒改为 60 分钟；显示端将清理放到独立后台线程，前台交接和授权审计不再同步扫描历史目录。保留天数、容量上限和跨进程互斥规则不变。

- 应用窗口模板增加必填 ready_target，就绪阶段等待业务控件出现并稳定。VS Code 首次打开与重开拆开窗口、归属、前台和编辑器全文核验，扩展为 26 步；组合模板同步为 68 步，避免启动成功后直接假定页面可输入。

- 补齐组合选择器的窗口枚举、HWND 解析、原生候选查询、属性匹配和读值耗时打点；`ACTL_TRACE=1` 同时输出阶段开始，调用未返回时也能看到停在哪层。60 秒观察阻塞本轮未复现，实测与剩余边界见 docs/65。

- 前台交接的执行器身份核验支持同文件路径别名；流程保留窗口准备/发现的诊断元数据。应用窗口模板拆分启动与前台步骤，避免已启动窗口因激活失败被重复启动；修正 Chrome 夹具 profile 参数。已有窗口 12 次切换/最小化恢复、新窗口热/冷启动及条件模板真机记录见 docs/62、docs/64；单次 UIA 观察约 60 秒的慢点仍未解决。

- **有界观察恢复**：步骤支持 recovery 次数/时限/间隔配置，自动重查仅限可恢复的只读观察；预算持久化且继续不补满。暂停摘要与详情展示失败原因、重查次数、处理建议及观察日志，未知结果不选择否定分支、写入不自动重放。边界与证据见 [64](docs/64-bounded-observation-recovery.md)。

- **简单流程条件**：支持 when/unless/otherwise，窗口存在则前置、不存在则启动，以及可选控件和已达目标状态的跳过。新增只读 `verify-window` 断言；未知观察不走否定分支，不确定动作恢复保持原路线并先核验。附三个参数化模板；协议和验收范围见 [63](docs/63-simple-workflow-conditions.md)。

- 显示协议 v9 补齐前台资格转交与动作结束归还，覆盖连续 CLI、最小化及失败路径；旧显示端需正常替换。不扩大到所有进程，不改变暂停/停止判断。历史记录增加转交/归还结果，窗口失败附原生返回值；慢响应只重查一次授权状态，不重放动作。Chrome/ZCode 六步真机通过，开始一次，见 docs/62。

- 全量移除剩余 41 个 PowerShell 文件：业务动作迁入 workflow，诊断、回归调度、夹具和发布打包迁入 actl-devtools，经 cargo xtask 调用。门禁改跑 Rust 测试；移除 17 个 Bash 包装/过时实验入口，未替代的历史实验保留待办。
- verify-file/verify-owner/verify-closed 改用原生 ZIP/XML/Win32/WMI，删除运行时解释器；补连续 WinRT 调用、DTD/膨胀限制和进程身份测试。新增 verify --url-query，用于夸克搜索的精确解码核验。
- 新增网易云播放、夸克搜索、腾讯会议草稿及日期修改、通用启动观察 workflow；均按 once_per_run 交接，恢复先核验结果。模板预检通过不代表真实软件验收通过。迁移清单及测试证据见 docs/61。


- 修复 VS Code Quick Input 导航：直接写 ValuePattern 并在 quickInput_list 内激活唯一结果，核验行号/文件内容；关闭核验只读取进程实例身份，避免要求退出阶段的命令行元数据，窗口观察放到进程读取之后。

- 修复悬停暂停/继续按钮时主状态文字被操作提示覆盖；pending 步骤恢复时复用紧邻且完全相同的现场/前置检查，减少重复进程探查，不合并不确定步骤的结果核验。

- 四个应用验证可直接使用 flow-run：YAML 声明隔离目录、初始文本与随包 Office 模板，运行上下文生成默认输入；预检不创建资源，恢复不重新初始化。整批入口为 verify-applications.yml，删除 prepare-workflow.ps1、verify-applications.ps1 和 office-package.ps1；新 workflow 真机验收待完成。

- 新增 workflow 连续操作组、文件/行集合/剪贴板/进程归属/关闭断言及规范化全文比较，提供四个完整应用 YAML；操作组中断后核验结果，不自动重放。


- workflow 增加声明式 inputs 与 --inputs 文件，预检后保存具体计划，恢复不重读输入；新增 VS Code、文本核验与 ValuePattern 编辑模板，VS Code 场景只准备资源。
- 窗口激活已在前台时不重复请求，后台使用单次原生前台请求并有界核验，避免窗口根 UIA SetFocus 的额外副作用；误让行整体验收仍待真机复验。

### Changed

- **暂停入口持久保留**：状态条保留已展示的 flow 绑定，调用快照到期及显示端重启后仍从持久化任务恢复继续入口；不从多个旧任务猜选、不自动授权，完成任务不再显示继续。

- **可恢复 VS Code 验证入口**：新增 vscode-resume.ps1，将桌面动作登记为持久化 flow；暂停后可从显示端继续，重新定位目标并核对完整文本，未知或部分完成的写入不自动重放。新增跨执行器恢复测试；该入口不宣称普通 PowerShell 脚本都可恢复。

- **交接诊断补全**：授权撤销保留首个原因；历史关联失败调用、输入解析分支与长度、监听注册状态和心跳年龄。输入来源统计不包含键值、设备句柄或坐标；VS Code 增加最小输入保存验证入口。

- **重复交接修复**：Raw Input 读取并校验事件来源，排除无设备且携带 actl 注入标记的自身事件，避免自动按键撤销同任务授权。真实设备、未知来源和读取失败继续触发让行；交接历史记录实际弹窗、用户确认、授权复用及失效原因，不记录键值或坐标。仅在确实等待授权时显示等待状态。

- **应用失败修复**：关闭脚本对异步退出有界观察且不重发关闭；Office 窗口在 native/UIA 检查之间消失时重新核实。严格选择器支持按已知角色查询原生候选，VS Code/Word 场景改用语义目标，启动归属检查使用原生窗口并显式核验窗口激活。证据与真机限制见 [53](docs/53-application-failure-repair.md)。

- **任务交接策略（显示协议 v8）**：flow YAML 与脚本任务配置支持 `handoff.approval: once_per_run`，首次确认后同任务不因空闲/总时长反复交接。策略变化不能复用旧授权，人工接管、停止与监听故障仍撤销；默认保留 expiring。新增整批应用验证入口，遇到交接中断停止整批。

- **应用验证脚本**：VS Code/浏览器跨调用 ref 绑定来源快照；编辑器、Word 正文和 Excel 单元格键盘操作增加具体焦点断言，避免旧场景只验证窗口。补充静态回归与 [应用复验记录](docs/51-application-revalidation.md)，保留失败轮次和未验收路线。

- **深树容器定位**：严格 UIA 选择器含精确 ID/名称时使用原生条件查询完整候选，再核验组合条件与唯一性，避免局部容器被无关深分支的客户端预算阻断。保留 RawView、子级/后代范围、候选数量限制及 provider 错误；宽泛选择器仍按原预算遍历。复现与验收见 [50](docs/50-deep-container-query.md)。

- **停止恢复显示一致性**：显示端定时核对停止代次，停止、修复或状态损坏后主动收回旧交接提示，无需等待旧 CLI 再次请求；当前代次的新任务提示保留。execution-status 在同一文件锁内读取代次与停止记录，避免并发停止/修复时报告矛盾状态。
- **[BC] 停止代次与恢复（显示协议 v7）**：急停废止已运行调用的代次；之后即使修复或确认新任务，旧调用仍不能续跑。新观察和预览不受历史停止阻断，新写任务在开始/继续时确认当前代次。新增 execution-status / execution-recover；修复归档原始状态、不授予权限、不自动续跑。删除 v1/时间戳兼容解析、直接 clear_stop 接口及 WM_APP+49 解除握手；旧显示端需成对升级。流程详情继续请求经主显示端核验并绑定排队调用的代次。设计和验收见 [47](docs/47-execution-recovery.md)。
- **[BC] Phase 2 观察失败契约**：UIA 窗口清单、身份基线、新窗口等待及 PID 定位迁移到可失败的 RawView 枚举，必要属性失败不再伪装为空值、缺席或唯一结果。`wait --idle` 系统输入查询失败返回带 evidence 的 INTERNAL，并在轮询中响应 flow 暂停/停止；Rust `idle_ms` 改为 Result。
- **输入监听冗余**：显示端增加 Raw Input 键鼠活动计数、注册状态核验和消息泵心跳；健康未知时撤销授权并返回既有监听不可用原因，恢复健康不会自动授权。不记录键值/坐标，不发送探针输入；实际人工接管仍待验收。
- **启动清理开销**：历史日志保留扫描通过跨进程文件锁限制为每分钟一次，启动时的短期调用快照扫描限制为每秒一次；失败不记成功，时钟回退重新扫描，不改变急停或流程恢复记录。

### Added

- **ZCode 超深会话复验**：恢复历史会话后，旧版 sidebar 查询仍返回遍历预算超限，修复版前后局部观察完整、身份与内容一致，3 个绑定引用读回通过。首轮 provider STALE_REF 失败记录保留；完整窗口树仍截断，本轮不计算削减率。证据见 [50](docs/50-deep-container-query.md)。

- **ZCode 只读档案**：新增已验证的 sidebar 局部观察及快照绑定读回路线；保留启动中内容变化的受阻记录。当前稳定页面完整 JSON 字节减少约 13.8%，不等同 token 或耗时改善，也不代替历史超深页面与虚拟化验收，见 [50](docs/50-deep-container-query.md)。

- **局部观察验收入口**：新增只读采样脚本，记录完整/局部快照、绑定引用读回、窗口及容器稳定性和逐调用证据；分别统计元素与完整 envelope 字节，截断、根节点孤立和观察失败不计为有效削减。真实窗口限制见 [49](docs/49-local-observation-acceptance.md)。

- **容器局部观察**：`snapshot --within <严格 UIA 选择器> --app/--hwnd` 支持 depth/limit，返回范围、投影前节点数及真实截断原因。局部 ref 配合 snapshot_id 保存容器身份和预算，重放核对原容器与元素，不回退全窗口序号。减少无关输出；容器唯一性定位仍可能扫描全窗口，验收与限制见 [48](docs/48-local-snapshot.md)。

- **候选目标纠正**：`preview @eN --candidate @eM --snapshot <id> --app <window>` 展示同一快照的 2–20 个候选，支持切换、明确选择和取消。选择时重新定位并核对身份/边界；返回选择结果和快照绑定，始终 executed=false，不授予桌面权限。关闭和超时不默认选择。
- **预览失效回归用例**：增加 release CLI 在自有原生目标移动、改名、隐藏、销毁、重建、遮挡、隔离急停和关闭消息下的交互桌面测试；按 CLI PID 核对窗口回收，保存逐调用输出与清理证据。provider 消失时保留原生观察失败，不强制改写为 STALE_REF；验收范围见 [46](docs/46-target-preview.md)。
- **独立目标预览**：`preview` 为单一 UIA 目标显示边框与动作说明，支持关闭和超时退出；不点击、不输入、不授予桌面权限。ref 要求快照绑定，目标变化、离屏或中心点遮挡时停止，预览窗口不抢焦点并随调用清理。首批不含坐标预览、候选选择或流程编辑器；验收见 [46](docs/46-target-preview.md)。
- **无人值守验收入口**：`cargo xtask regress --unattended` 仅运行明确登记的无输入用例，其余保留 skipped，并通过 complete 区分局部运行与全量验收。新增只读性能/观察采样脚本，保存环境、二进制哈希、逐调用输出、100 次新进程耗时和稳定观察下的字节削减率；不把字节数宣称为 tokenizer token 数。浏览器原型失败报告保留有界异常原因链。验收与遗留项见 [44](docs/44-phase2-acceptance.md)。

- **首条业务流程 YAML**：交付 `skills/actl/workflows/release-review-draft.yml`——读取已打开的 actl Releases 页(Chrome)，在 Word 新文档生成核对草稿并以静态文件名另存为。演练输出绑定（标题跨应用进入草稿）、时效声明、检查点授权边界、恢复后源稳定性核验（resume_check 绑定）与写后业务核验；全静态锚点、零逐次输入。协议预检、读段真机（检查点前零派发）与时效拒绝路径（过期不派发动作）已验证；写段联测待用户授权执行，不宣称业务闭环。浏览器 pack 新增窗口标题读取路线与 GitHub 页主文档定位知识，索引与限制见 [43](docs/43-release-review-draft.md)。

### Fixed

- **应用 YAML 同步输入身份规范**：更新 10 份应用档案，补齐 ref 与同次快照绑定、输入框焦点断言、读取后的结果比较要求，修复微信空点击目标及旧右键参数、浏览器链接消歧和固定正文序号。修改过的已验证路线改记 manual 待复验，保留历史日期与证据；不把静态检查记为新版真机通过。详见 [45](docs/45-app-pack-refresh.md)。
- **桌面根枚举瞬态毒化（原始标题 fixture STALE_REF 根因）**：标题定位（`find_window` 与 `uia:` 作用域标题路径）对桌面根做一次整批 `FindAll(Children)`，任何无关顶层窗口处于创建/销毁中途都会让整批枚举失败——所有应用的标题定位报 STALE_REF，无匹配窗口时把 NOT_FOUND 误报成 STALE_REF（350ms 周期的 `Start-Process notepad` 扰动下 9/10 复现，本机无 notepad 关联时启动退化为系统选应用对话框，顶层窗口竞态同样成立；HWND 路径不做桌面根枚举故不受影响，当日差异由此解释）。修复为仅对桌面根枚举做有界瞬态重试（仅 StaleRef、3 次/200ms，与 stale_read 同参数；持久失败保留错误并附 attempts），`uia:` 作用域此前把枚举失败误映射 Internal 一并统一为 StaleRef；窗口子树枚举与既有控件失效语义不变。净环境 fixture 8/8 完整通过、扰动与显示端重启下直查 36/36、无匹配探针 30/30 NOT_FOUND；显示端创建叠加扰动的元素级瞬态 NOT_FOUND 作为独立观察项登记，见 [38](docs/38-flow-output-bindings.md)。
- **完整发布包补齐技能**：ZIP 包含可独立安装的 `skills/actl/`，保留应用档案和流程索引，流程与归档说明收进 references；技能内只使用本地相对链接，不包含 URL；设计与验收记录留在开发仓库，不打包完整 docs。打包后核对必需文件、内容和技能内部链接，缺失即失败。安装说明明确技能须由 Agent 加载、桌面写操作需要配套显示端。此修复不修改已发布的 0.1.6 附件。

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
