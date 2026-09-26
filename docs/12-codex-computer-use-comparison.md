# 12 · Codex Computer Use 实证对照与生态定位

调研日期:2026-09-26 · 性质:调研分析,不覆盖现行规范 · 对照基线:actl v0.1.0(17 命令,见 [CHANGELOG](../CHANGELOG.md))

关联:[调研全景](01-research-landscape.md) · [差距分析](02-gap-analysis.md) · [设计蓝图](06-design-blueprint.md) · [Jev 集成提案](08-agent-cli-jev-design-proposal.md) · [输入后端评审](09-input-backends-review.md) · [M0 实测](spike-findings.md)

> 本文依据:本机安装的 OpenAI Codex 26.924(MS Store 包 `OpenAI.Codex_26.924.1866.0`)插件文件实证、官方文档与 changelog、社区逆向与评测数据(检索均于 2026-09-26)。协议相关建议仅为登记,不构成变更;OpenAI 侧信息可能随后续版本失效。

## 1. 实现事实(本机实证)

### 1.1 分发与进程链

- 经 MS Store MSIX 包分发(`OpenAI.Codex_26.924.1866.0`,即 ChatGPT 桌面应用,2026-07 起 Codex app 并入其中);Computer Use 插件版本 26.924.20706,双插件并存:`computer-use`(SKILL.md + docs 三篇)与 `unified-computer-use`(cua_repl MCP server + turn hooks)。
- 原生助手 `codex-computer-use-swift.exe`(56.5 MB,签名有效:OpenAI OpCo, LLC)位于 `AppData\Local\OpenAI\Codex\runtimes\cua_node\<hash>\bin\node_modules\@oai\sky\bin\windows\swift\x64\`,由 ChatGPT.exe 拉起并经命名管道 `\\.\pipe\codex-computer-use-<uuid>` 通信;另有一个 1.5 MB 的 `codex-computer-use.exe`,仅充当 config.toml 的 `notify` 钩子(turn-ended 通知)。
- 调用形态:agent 在 node_repl 的 JS 会话中 `await import("@oai/sky")` 获得 `sky` 对象,驱动 13 个方法;SKILL.md 明确**禁止**直接 spawn 助手 exe 或自建协议客户端。这是"模型写代码驱动 SDK"(code execution)形态,与 OpenAI API 侧对 GPT-6 Astra 推荐 code execution 路径的方向一致。
- 配置面:config.toml 支持 per-app 允许列表(`aumids` / `exes` / `always_allowed_app_ids`)与 `default_app_access`;管理员可用 requirements.toml 的 `[features].computer_use = false` 整体禁用。

### 1.2 技术栈自述

SKILL.md 原文:**"It uses SendInput, UI Automation, and Windows.Graphics.Capture screenshots that work even when windows are occluded."**

三个要点:

1. UIA 优先 + 物理注入(SendInput)与按窗口捕获(Windows.Graphics.Capture)兜底——与 actl 的"UIA 优先、`--physical` 兜底"(ADR-007)同构;
2. "遮挡可用"与官方文档"On Windows, Computer Use runs on the active desktop(仅前台)"存在出入;26.924 为 2026-09 社区 issue 之后的版本,可能已改进,存疑记录;
3. sky 的 `get_window_state` 默认 `include_screenshot: true`、`include_text: false`——**截图默认开、accessibility 文本默认关**,guidance.md 给出的理由是"桌面应用 accessibility 树普遍较弱时截图是最佳默认"。这与 actl(文本树默认、不碰截图)互为镜像,也再次印证 M0 spike 的 UIA 长尾结论(Electron 深而稀疏、Canvas 不可达)。

### 1.3 时间线与官方定位

| 时间 | 事件 |
|---|---|
| 2026-04-16 | Computer Use 发布(仅 macOS;公告《Codex for almost everything》) |
| 2026-04-23 | 内置浏览器内可用(点击本地 dev server 复现问题) |
| 2026-05-29 | Windows 支持(官方文档口径:仅前台、接管键鼠) |
| 2026-06-15 | Record & Replay(演示一遍工作流 → 生成可复用 skill,依赖 Computer Use) |
| 2026-07-09 | 并入 ChatGPT 桌面应用;GPT-5.6 提速 |
| 2026-08-31 | GPT-6 Astra(官方建议用于依赖截图与视觉判断的困难任务) |

官方给 Computer Use 的**降级顺序**:目标应用有专用 plugin/MCP server 就优先结构化集成;本地 web 应用先用内置浏览器;Computer Use 只留给"只存在于 GUI、无法用文件或命令输出验证"的场景。**这句话本身就是 actl 以 skill 形态接入 Codex 时的定位句:actl 就是那个排在 Computer Use 前面的"结构化集成"。**

### 1.4 质量与成本信号(社区数据)

- Windows 实测一例(issue #43777):完成一个授权应用配置任务耗时 51 分 12 秒、117 次外层调用(其中 50 次 GUI 调用,平均约 26 秒/次);
- 失败后的排障同样消耗订阅额度,有用户反复失败直到额度耗尽;2026-08~09 Windows issue 密集(原生应用枚举为空、"sky trusted RPC 未配置"、UIA WinEvent 订阅引发 DWM 冻结、Win10 22H2 截图接口缺失);
- macOS 体验显著更好:虚拟光标后台执行不抢用户鼠标、Locked use(锁屏续跑)、Record & Replay。

### 1.5 开源边界与可读性分层

**Codex CLI 开源,Computer Use 闭源。** [openai/codex](https://github.com/openai/codex)(Apache-2.0,126k★,2026-09-26 经 GitHub API 核验)公开 codex-rs 全部 Rust 源码;与 Computer Use 相关的公开部分只有外围——配置类型(`codex-rs/config/src/computer_use.rs`)、app-server 协议 v2 schema、Guardian 审查 prompt(`classifier_instructions.md` 的 "Computer and Browser Use" 节)与 TUI 展示层;桌面控制实现本体不在公开仓库。闭源侧证据:插件 plugin.json 标注 `license: Proprietary` 且 repository 指向内部私有 monorepo(github.com/openai/openai);`@oai/sky` 不在公共 npm(registry 查询 404,仅随 app 运行时分发);三个内部包(`@oai/sky` v0.7.4、`@oai/cua-repl` v0.1.0、`@oai/cua` v0.2.5)内均无 LICENSE 文件。

本机可见内容分三层:

| 层 | 内容 | 可读性 |
|---|---|---|
| 文本资产 | 插件四篇 markdown(SKILL/api/guidance/confirmations,即行为规格);`@oai/cua-repl` 的 `instructions/`(**按 OS × surface 拆分的注入指令**:`windows/computer.md`、`browser.md`、`browser-cloud-guidance.md` 等)与 README(运行开关:`CUA_REPL_ENABLED_SURFACES=browser\|computer`、`CUA_REPL_BROWSER_ENV=codex-app/training/cloud/orbit`、`NODE_REPL_JS_BANNER` 可整体替换引导脚本);`@oai/cua` 的 tinysky-alt-* 文档族 | 完全可读,prompt 层即源码 |
| JS 逻辑层 | `@oai/sky` dist 为**未压缩 tsc 产物**:`targets/{windows,mac,linux}/` 按动词拆文件(click/drag/clipboard_*/accessibility_tree/…),`core/` 为管道协议;`service.js` 可见运行时分发协议(setup 返回方法表、execute 按名反射调用、drag 句柄仅限 Linux);`sky/docs/` 另存 window/window2/full-desktop 三套 API 文档;publishConfig 泄露 macOS bundle 全貌(SkyComputerUseService/Client、CUALockScreenGuardian、normal/relaxed 双变体);运行时 node_modules 另捆绑 playwright(浏览器面 CDP 后端)与 sharp/pngjs/pixelmatch/jpeg-js(截图管线的图像处理依赖) | 可读但**无许可** |
| 原生二进制 | `codex-computer-use-swift.exe`(56.5MB,UIA/SendInput/WGC 实现本体)与 `codex-computer-use.exe`(1.5MB,turn-ended 通知) | 编译产物,仅可逆向 |

**对 actl 的合法边界**:可读 ≠ 开源——无 LICENSE 即保留所有权利。协议形状、确认策略、按 OS 组织 targets、恢复语义等**事实与设计思想**可合法借鉴;**代码一行不可复制**(并受 AGENTS.md"定案前不引入 vendored 外部代码"约束,Proprietary 代码永远不应 vendored);UIA 实现的可复制参考仍是 terminator(MIT)与 agent-desktop(Apache-2.0)。附带一条可行性佐证:sky 的 `targets/` 为三平台同构目录(linux 目录含 accessibility_tree/click/drag/clipboard_* 等动作集),对应 [ROADMAP](ROADMAP.md) Phase 3 的 Linux(AT-SPI2)后端候选。

## 2. sky window2 API × actl v0.1.0 对照

sky 共 13 个方法(api.md);actl 侧标注 v0.1.0 已实现命令与 M2 规划(见 [ROADMAP](ROADMAP.md))。

### 2.1 方法级映射

| sky window2 方法 | actl v0.1.0 | M2 规划 | 差异要点 |
|---|---|---|---|
| `list_apps()`(应用目录,含 lastUsedDate/useCount) | —(`launch` 不依赖应用目录) | — | sky 以应用为入口;actl 以窗口为目标,`launch` 直接 spawn + trailing args |
| `list_windows()` | `list-windows`(含 owned dialog) | — | 同层;actl 已处理 WinUI 三件套折叠为单一可见窗口 |
| `get_window()`(按 id 再水化窗口绑定) | —(跨 snapshot 须重取;窗口可按句柄定位) | — | sky 的窗口 id 是 OS 级、可再绑定;actl 的 `@eN` 绑定 snapshot,契约更严(蓝图 §5) |
| `launch_app()` | `launch` | — | 同构;sky 按 app id 或 .exe 路径 |
| `get_window_state()` | `snapshot`(`--skeleton`)/ `get` | — | **默认互为镜像**:sky 截图开/文本关,actl 文本树开/无截图;sky 作用域 per-window,actl 以 `--app` 锚定 |
| `click()`(element_index 或坐标+screenshotId) | `click`(Invoke 优先/SelectionItem 回退) | `right-click` `double-click` `--expect` | actl 无坐标路径,目标走 `@eN`/`name:`/`id:`/`role:` 四形态语义 |
| `press_key()` | `press`(VK/UNICODE 双通道) | `key-down`/`key-up` | sky 用 X11 keysym 命名(`Control_L+a`、`KP_0`,跨平台抽象痕迹);actl 按 Windows 语义双通道实现并有 IME 实证(docs/09) |
| `type_text()` | `type`(置焦+逐字,`--paste` 剪贴板通道) | — | sky 靠 guidance 教模型"先观察焦点再输入";actl verify-then-inject 在工具层强制 |
| `scroll()`(坐标 + delta,输入注入) | — | `scroll`(UIA Scroll pattern,容器语义) | 两种路线:sky 明确不收元素目标("先点坐标聚焦再滚");actl 蓝图走容器 pattern |
| `set_value()` | `set-value`(UIValuePattern 直写) | — | 几乎同构,均绕键盘 |
| `drag()`(纯坐标 stroke) | — | `drag --from @e3 --to @e8` | sky 坐标制(面向 canvas 手势);actl ref 语义 |
| `perform_secondary_action()`(UIA secondary actions:Raise/Expand/Collapse/Scroll…) | — | `toggle`(Toggle pattern,近似) | sky 暴露 secondary action 全谱;actl 对应物是 M3 的 pattern/L3 策略层,不新增动词 |
| `activate_window()` | `focus-window` | `resize-window`/`close-window` | sky 定位为"可选逃生舱"(输入方法自动激活目标窗口);actl 中是一等公民 |

### 2.2 结构性差异(单边能力)

**actl 独有**:

- `find`(定向查找省 token;sky 只能整树输出后由模型自己找);
- `wait` 谓词(M1:--element/--window;M2 补全)——sky 无等待原语,靠 LLM 循环 refresh;
- `press --expect`(RuntimeId 基线的新窗口断言)——sky 无断言原语,执行后验证全靠模型看新截图,这正是"假成功"失效形态的温床(见 §3.3);
- `batch`(单进程共享 snapshot)、`get/set-clipboard`、`status`、`commands` 自发现;
- envelope 错误码 + hint:sky 的失败以 JS throw 返回,恢复流程写在 guidance.md(文档层纪律);actl 把恢复语义放进协议(机器可读);
- verify-then-inject 护栏(注入前物理前台核对,`press`/`type` 强制 `--app`)。

**sky 独有**:

- `get_window` 再水化(窗口绑定丢失后按 id 恢复);
- `list_apps` 应用目录(含使用频率信号);
- 截图捕获(actl 明确非目标,PRD §1.3);
- secondary actions 全谱暴露;
- 与浏览器插件联动(chrome / in-app browser 后端)。

### 2.3 六个设计专题对照

1. **观察默认互为镜像(token 哲学)**。sky:截图默认开、a11y 文本默认关,理由是"弱 accessibility 树下截图是最佳默认";actl:紧凑语义树默认、`--skeleton` 投影、`find` 先窄后深、无截图。同一个"UIA 长尾"问题,OpenAI 的答案是退回视觉,actl 的答案是投影 + `--physical` 兜底——这是本项目差异化的最直白对照物。
2. **新鲜度契约**。sky 原文:"Element indexes, screenshot IDs, and coordinates are valid only for the observation that produced them",坐标动作的 `screenshotId` 必须命中缓存;actl:跨 snapshot 必须重取 ref(蓝图 §5)+ `--expect` 用 RuntimeId 基线防"标题脏标记"骗过断言。两家都把"防 stale"当一等契约,载体不同(sky 用截图缓存 id,actl 用 snapshot_id + UIA RuntimeId)。
3. **焦点安全:文档纪律 vs 工具护栏**。sky 的 guidance.md 用大段流程教模型"typing 前先观察焦点、停下确认、单独一格输入、失败即结果未知";actl v0.1.0 把同一事故类(CHANGELOG 事故 #1 键盘偏射)做成 verify-then-inject 护栏:注入前物理前台核对、焦点归属用物理层判据、`press`/`type` 强制 `--app`。这是 docs/03 §3"把可靠性从 agent 的 prompt 技巧下沉为工具原语"的活例证——同一问题两种解法,actl 选了可被测试的那一种。
4. **结果未知语义**。sky:"Input or refresh outcome is unknown; reobserve before retrying"(JS try/catch + 文档流程);actl:错误码 + hint + (M2)evidence,机器可读。注意 [08 §4.3](08-agent-cli-jev-design-proposal.md) 提出的任务态 `outcome_unknown` 与 sky guidance 的这条语义一致,互为印证。
5. **确认与授权分层**。sky 把确认策略放在 prompt 层:confirmations.md 定义四级分类(Hand-off / Always confirm / Pre-approval / No confirmation)+ 不可协商拒绝清单(禁自动化终端/Run 对话框/认证对话框/密码管理器/安全设置、禁 Win 键组合、内容不可信原则);actl 把授权放在工具层(`--physical` 显式、`--dry-run` 全局),PRD §7 明确"敏感命令白名单留给上层 agent 策略"。两层互补而非互斥:将来写 actl 的 skill 文档时,confirmations.md 的四级分类是"故障排查/授权指南"章节的现成参照系。
6. **输入工程深度**。sky 的按键命名用 X11 keysym(`Control_L+a`、`KP_0`、`Numpad_Enter`),是跨平台抽象层,未提及 IME;actl 的双通道(组合键成员走 VK、纯文本走 KEYEVENTF_UNICODE)+ 中文 IME 实测结论(中文模式下唯一可靠文本通道为 `type --paste`)+ 开场/结尾全场清场 11 键,在 Windows 输入语义上显著更深(docs/09 已系统化)。另注:sky 的 `AccessibilityState` 除 tree 外还携带 `document_text` / `focused_element` / `selected_text` / `selected_elements` 结构化字段——这是对 [08 §6.3](08-agent-cli-jev-design-proposal.md)"只保留可交互控件会丢失完成证据"主张的外部佐证。

## 3. 生态定位(对 01/02 的更新)

### 3.1 巨头收敛验证

2026-04 起,OpenAI 旗舰产品的桌面控制实现从纯截图路线转向 **UIA 优先 + 截图兜底** 的混合路线,API 侧同时向"模型写代码驱动工具"迁移。docs/01 押注的结构化路线已不是社区边缘共识,而是前沿玩家的既成事实。**方向判断更稳,但独立工具的空窗期在缩短**——"结构化 Windows 桌面自动化"正在变成订阅应用的默认基础设施。

### 3.2 三组竞争叙事

| 维度 | Codex Computer Use | actl |
|---|---|---|
| 开放性 | 闭源(license: Proprietary)、MSIX 捆绑、订阅额度计费、56.5MB 助手 + Node 运行时 | 开源单二进制(~0.8MB)、任何能跑 shell 的 agent、无运行时依赖 |
| 表面形态 | JS SDK in REPL(模型写代码,一次调用可复合多步) | 原子 CLI 动词 + envelope(每步可归因、可验证、可重放) |
| 观察默认 | 截图开 / a11y 文本关 | 文本树开 / 无截图 |

第三行的含义:token 经济学上 actl 的默认观察成本比 sky 低一个量级级差(截图每张约 1,000–1,800 token[Anthropic 口径],文本树经 skeleton 投影可削 78–96%)。

### 3.3 失败归因与 actl 命中点

2026 年独立研究对 computer use 失败的归因:任务推理/控制 53.9%、验证/反馈盲区 39.3%、grounding 仅 12–18%(CUADebug n=204;How Benchmarks Mis-Score n=150);90% 的失败以"声称成功"告终(CURA);OSWorld 2.0 长程任务最佳 20.6%,模型自我纠错预算不足 7%。

对 actl 的含义:**`wait`/`verify`/`--expect` 不是锦上添花,是对准当前第一瓶颈(验证缺失与假成功)的原语**;grounding 已不是主要矛盾,进一步佐证"UIA 语义定位"路线的可行性。

### 3.4 skills 分发空缺

Agent Skills 标准(agentskills.io)已被 47 个客户端采纳(含 ChatGPT/Codex、Claude Code、Cursor、Copilot),skill 常驻成本约 50–100 token;但"Windows 桌面自动化"品类缺位:官方 anthropics/skills 无此分类,skills.sh 无此 topic,质量最高的独立 Windows skill 仅 ~50 star。浏览器侧同类(playwright-cli)安装量 16.6 万。**actl 在 M2 对外发布时同步发布 skill(SKILL.md + references/ 错误码恢复指南 + 示例脚本,从 envelope 生成),是成本极低、天花板很高的分发渠道**;官方降级顺序(§1.3)意味着装了 actl skill 的 Codex 会优先走 actl 而非它自家的 Computer Use。

## 4. 行动建议(登记,不动协议)

1. **README 定位句**(M2 发布时):引用官方降级顺序——"有结构化集成就优先于 computer use",actl 即 Windows 桌面上的那个结构化集成;
2. **skill 发布**(M2 同步):按 playwright-cli 范式(SKILL.md 命令速查 + `allowed-tools` 白名单)或 orca 存根范式(`actl commands` 已是现成的机器 schema);
3. **M2 协议评审候选登记**:AMBIGUOUS/STALE_REF 的 evidence 携带结构化候选列表(带 ref/role/name),使外部决策模型可免重取 snapshot 直接裁决(对照 sky 的 element_index 模型与 agent-desktop 的 recovery/disposition 结构化字段);走硬规则 8 流程;
4. **观察点**:Codex CU 的 Windows 质量曲线(26.924 已声称遮挡可用,若后台执行坐实则三组叙事中"前台接管"一项失效)、开源 System-1 决策模型(Kev、cua-s1-forms)对 08 号提案的可供性。

## 5. 资料范围

- 本机实证(2026-09-26):插件 `~/.codex/plugins/cache/openai-bundled/computer-use/26.924.20706/`(SKILL.md、docs/api.md、docs/guidance.md、docs/confirmations.md、plugin.json);unified-computer-use 的 .mcp.json;`~/.codex/config.toml` computer_use 配置;进程与签名(签发:OpenAI OpCo, LLC)。
- 开源状态核验(2026-09-26):[openai/codex 仓库](https://github.com/openai/codex)(Apache-2.0;CU 相关公开路径 `codex-rs/config/src/computer_use.rs`、app-server-protocol v2、guardian `classifier_instructions.md`);registry.npmjs.org/@oai/sky 返回 404(未公开分发);运行时内部包 `@oai/sky` v0.7.4、`@oai/cua-repl` v0.1.0、`@oai/cua` v0.2.5(dist 结构、instructions/、publishConfig 均可本机阅读)。
- 官方:[Computer Use 文档](https://learn.chatgpt.com/docs/computer-use) · [changelog](https://learn.chatgpt.com/docs/changelog) · 公告《Codex for almost everything》(2026-04-16) · API 侧 [computer tool 指南](https://developers.openai.com/api/docs/guides/tools-computer-use)。
- 社区:openai/codex issues #43692/#36833/#43777(Windows 质量与实测);mac-use 项目 NOTES/SPECS(实现逆向,2026-04)。
- 研究:CUADebug(arXiv:2608.02643)、How Benchmarks Mis-Score(arXiv:2607.28367)、CURA(arXiv:2608.27808)、StateAct(arXiv:2607.22798)、OSWorld 2.0(arXiv:2606.29537)。
- 生态:agentskills.io 规范与客户端列表 · anthropics/skills · skills.sh(Vercel)· lahfir/agent-desktop 协议文档。

OpenAI 侧信息以 26.924 版本为准,后续版本可能变化;社区逆向结论未经 OpenAI 证实,引用时已标注。
