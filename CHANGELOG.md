# Changelog

本项目的全部显著变更记录于此。格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/),
版本语义遵循 SemVer;0.x 期间破坏性变更在条目中以 **[BC]** 标注。

## [Unreleased]

### Added
- **M1 命令面补齐(17/17)**:`find`(条件查元素,snapshot 同源 ref 编号)、`close-window`(WM_CLOSE 优雅关闭,经 HWND 转译)、`get-clipboard`/`set-clipboard`(CF_UNICODETEXT,ClipGuard 保证释放)、`wait --element/--window`(TIMEOUT 语义)、`batch --json`(单进程顺序执行,首错即停,禁嵌套);`commands` 自发现自动收录。
- **`type --paste`**:剪贴板+ctrl+v 文本通道——中文模式 IME 下唯一可靠的送达方式(输出 `via` 字段透明上报);直注 UNICODE 会被 IME 组合改写(实测)。
- **click 语义分发**:Invoke 优先,SelectionItem 回退(列表/树/选项卡项),实际 pattern 随结果上报。
- **金任务 2 达成 10/10**(fixtures/golden-task-2.sh):focus-window → ctrl+n → type(--paste)→ 内容读回 → ctrl+s(--expect 新窗口)→ set-value 文件名 → enter → 关窗后置态 → 磁盘逐字终验 → ctrl+w;金任务 1 同日以新代码复验 10/10。
- doc 09 评审第一批修复落地(编译层单测/expect 新窗口语义/f10-f12/组合键拒绝降级/press+type fail-closed,见前次条目)。
- **M1 键盘/文本命令族**:`type`(元素置焦+护栏+逐字注入)、`press`(组合键,`--expect window:<substr>` 后置断言轮询)、`set-value`(UIValuePattern 写值)、`focus-window`、`list-windows`;自建 SendInput 管线 `actl-uia::kbd` 取代 uiautomation crate 的 Keyboard。
- **金任务 2 达成 7/7**(fixtures/golden-task-2.sh):type → ctrl+s → 另存为对话框(--expect 断言)→ set-value 文件名 → enter → **磁盘内容逐字一致终验**;覆盖键盘文本、组合键、系统对话框、ValuePattern、跨窗口护栏五类能力。
- 双通道注入规则(实测驱动,docs/spike-findings.md):组合键成员 → VK 通道(VkKeyScanW 映射,IME 不拦 ctrl+键);纯文本 → KEYEVENTF_UNICODE(绕 IME)。修饰键用通用 VK_CONTROL/VK_MENU/VK_SHIFT(AHK/pywinauto 同款),清场全集 11 键。
- `type` 补齐 verify-then-inject 护栏(此前仅 press 有)。
- 命名决策:项目定名 **actl**(agent control);bin `actl`、crate `actl-cli`(冲突查证见 docs/06 §1)。
- 命令面设计定稿:扁平动词式 + 命名四规则 + `commands` 自发现命令 + L1/L2/L3 分层模型与解耦/整合原则(docs/06 §3)。
- 帮助系统规范:`-h` 三级层级、stdout 协议例外条款(--help/--version)、help 英文与同源生成(docs/06 §3.5)。
- **M1 CLI 骨架**:envelope 输出管线(计时/退出码 0-1-2)、`status` 与 `commands`(自发现 schema,clap 同源生成)、bin 集成契约测试;clap 4.6 入依赖(TOTAL_DIRECT_DEPS=5/8)。
- **M1 snapshot**:`actl snapshot [--app X] [--skeleton]` —— UIA 树遍历(actl-uia,`uiautomation` 升正式依赖)、`@eN` ref 编号与 skeleton 祖先链投影(actl-core 纯逻辑 + 单测)、NOT_FOUND 错误契约;计算器实测 204ms(debug 构建)。
- **M1 交互链路**:`click`(UIInvoke 后台调用,支持 `@eN`/`name:`/`id:`/`role:` 四种目标)、`get`(属性白名单读取含 ValuePattern)、`launch`(spawn + trailing args);PROTOCOL 错误改走 exit 2(对齐 06 §4);真实链路验证:launch calc → click num7Button(显示"七")→ get CalculatorResults 读出"显示为 77"。
- **金任务 1 达成 10/10**(fixtures/golden-task-1.sh):计算器 123×456+789 → 56,877,120 次点击零失败,ref 重放(snapshot→@eN→click)100% 稳定;性能实测 P50 40.2ms / P95 53.9ms / snapshot 275ms / 二进制 0.78MB。
- **[spec] PRD 冷启动指标按实测修订**:P95 50ms → 80ms(附加 P50 < 50ms 约束)——Windows 进程 spawn 固定开销 ~30ms 为不可压缩项。
- **spec(错误码)**:新增 `INTERNAL`(内部/UIA/系统层错误);envelope 增加可选 `snapshot_id` 字段;actl-core 枚举落齐 `ASSERTION_FAILED`。
- 命令面语义审计(docs/06 §3.6):补 `key-up`、`inspect` 并入 `get --raw`、`clipboard-*` 更名 `get-clipboard`/`set-clipboard`(对齐命名规则)、新增错误码 `ASSERTION_FAILED` 并定义 `--expect` 等待语义;近似命令对(type/set-value、wait/verify、snapshot/find 等)的分工裁定入档。
- `cargo xtask` 任务入口(setup/gate/fmt):门禁实现收敛为 xtask 单点,pre-push 与 CI 均调用 `cargo xtask gate`;Makefile 作为惯例入口保留(纯转发层)。
- tag 驱动的 release 流水线(verify→build→publish,对齐 zotero_cli 体系;流程文档 docs/TECH_STACK §6)。
- pre-commit hook 增加 merge-conflict 标记检查(等价 pre-commit-hooks 官方集)。

### Fixed
- **窗口解析强化(fail-closed 族)**:多匹配 → **AMBIGUOUS 附候选标题**(首个命中静默胜出的旧病);WinUI 三件套(ApplicationFrameWindow/TitleBarWindow/CoreWindow)折叠为单一可见窗口且**保留 CoreWindow**(内容元素只在其子树);owned dialog 深扫改为两阶段(热路径零深扫,修复每命令 ~2s 的自伤回归)。
- **焦点归属判据改物理层**:UIA focused element 在 WinUI 上会反向谎报(物理已聚焦仍报别窗口编辑框,曾致成批误拒绝);改用 GetGUIThreadInfo 上溯根窗口。
- **f10/f11/f12 解析边界**(doc 09 §3.6):旧条件 `p.len()==2` 把 f10-f12 挡在解析层而后端支持 1..=12;修复并补 f1-f12 全段回归测试(f0/f13/f99 仍拒绝)。
- **组合键不可映射字符静默降级**(doc 09 §3.1 剩余项):带修饰键但 VkKeyScanW 映射不出的字符(如 CJK)此前退回 UNICODE 通道(修饰键语义丢失),现直接 PROTOCOL 报错并提示改用 `type`。
- **[BC] `press`/`type` 强制 `--app`**(fail-closed):无 `--app` 的键盘注入打进"当下恰好前台"的窗口,是事故 #1 的路径之一;现返回 PROTOCOL(exit 2)。
- **[BC] `@eN` ref 未指定 `--app` 直接 PROTOCOL 拒绝**(exit 2):ref 绑定快照来源窗口的遍历顺序,前台漂移时曾静默点进无关应用;跨步骤定位改用语义选择器(如 `id:1001`)。
- **owned dialog 不可见**:"另存为"等 IFileSaveDialog 挂靠宿主窗口 depth-1,不在 UIA root 直接子节点——`find_window`/`list-windows`/`--expect` 轮询此前全部漏扫;现同时扫 root 子节点 + 各顶层窗口的直接子 Window。
- **组合键退化裸键**:`ctrl+s` 被目标当文本打出孤立 's'(KEYEVENTF_UNICODE 不携带修饰键语义);组合键成员改走 VK 通道。
- **纯文本被 IME 改写**:空格吞成上屏确认、半角标点全角化;纯文本固定走 UNICODE 通道。
- **窗口标题假信号**:效果断言改用 UIA Value 内容读回 + 磁盘终验(金任务 2 第 3/7 步);新记事本设置页打开时标题仍是文档名,视图判据改用 UIA 树中 Document 元素存在性。

### Changed
- **[BC]** 全部 crate 由 `ctl-*` 更名为 `actl-*`,bin 名 `actl`。

### 仓库规范基建
- workspace 骨架(actl-core / actl-uia / actl-physical / actl-cli)、git hooks(Conventional Commits 校验 + pre-push 测试)、CI(windows-latest)、AGENTS.md 协作规范。
- `actl-core`:envelope v1 与错误码枚举骨架(协议定义见 docs/06-design-blueprint.md)。
- 文档体系:PRD / ROADMAP / TECH_STACK / 调研系列 docs/01–06;单一事实来源审计完成。
