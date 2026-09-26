# 13 · 桌面应用的实时状态维护:机制、生态实践与 actl 映射

调研日期:2026-09-26 · 性质:调研分析,不覆盖现行规范 · 对照基线:actl v0.1.0(17 命令)

关联:[设计蓝图](06-design-blueprint.md) · [Jev 集成提案](08-agent-cli-jev-design-proposal.md) · [输入后端评审](09-input-backends-review.md) · [显示层研究](11-desktop-interaction-and-display-research.md) · [Codex CU 对照](12-codex-computer-use-comparison.md) · [M0 实测](spike-findings.md)

> 本文回答:如何实时知道一个窗口/应用的状态——是否前台、是否焦点、是否最小化/可见、内容是否变化。分四部分:Windows 三层机制、生态两种哲学与事故清单、Codex sky 本机实证、对 actl 的映射(M2 wait 默认值、状态判定 checklist、M3 事件订阅前置条件)。所有建议仅为登记,协议变更须走 spec 流程(硬规则 8)。

## TL;DR

1. **"是否在前台、什么状态"的权威答案是一组快照查询 API,不是事件**:GetForegroundWindow + GetGUIThreadInfo(0) + IsIconic/IsZoomed/GetWindowPlacement + IsWindowVisible∧¬DWMWA_CLOAKED——单次进程内调用、无消息泵、天然适合无状态 CLI。actl 的 verify-then-inject 已在用前两个,恰好是官方姿势。
2. **执行域生态没有任何框架维护"常驻实时桌面模型"**:pywinauto/AHK/uiautomation-rs/terminator/agent-desktop/UFO/Playwright/browser-use 全部是"每步重观察 + 显式 wait(轮询)";事件驱动只出现在屏幕阅读器(NVDA)和录制器。共同折中律:**事件当触发器,拉取当真相源**。
3. **事件路线有完整事故价目表**:事件风暴可打死整个 UIA 连接(NVDA #11002)、慢 provider 单次跨进程调用阻塞 5s(#20654)、全量 WinEvent 订阅鼠标移动即烧 3–5% CPU(PowerToys #1264)、Codex 助手空闲订阅引发 DWM 冻结(#36833)。
4. **M2 wait 谓词应采用轮询**,默认值直接取生态共识(间隔 ~100ms、窗口级超时 3–5s、text 类指数退避、动作后 settle ~500ms)。
5. **事件订阅登记为 Phase 3 `serve` 候选**:uiautomation 0.25.1 已有全套事件 API(`event` feature,零新增依赖),但必须配齐防御工程(§4.4 前置条件清单)。

## 1. Windows 三层机制

### 1.1 L1 窗口层·快照查询(无状态 CLI 的默认层)

| 状态 | API | 注意点(均为官方文档口径) |
|---|---|---|
| 前台窗口 | `GetForegroundWindow` | 窗口失活瞬间**可能返回 NULL**,调用方必须处理 |
| 焦点/捕获/激活 | `GetGUIThreadInfo(0)` | idThread=0 即取前台线程状态;GUITHREADINFO 含 hwndActive/hwndFocus/hwndCapture/hwndCaret + flags(如 GUI_INMOVESIZE);官方明说这是"跨进程查其他线程 GUI 状态"的权威入口 |
| 最小化/最大化 | `IsIconic` / `IsZoomed` / `GetWindowPlacement` | GetWindowPlacement 一次拿 showCmd + 恢复态矩形,比 GetWindowRect 更适合判断最小化布局 |
| 真实可见性 | `IsWindowVisible` ∧ **¬`DWMWA_CLOAKED`** | IsWindowVisible 只看样式位链,**完全遮挡时仍返回 TRUE**;虚拟桌面切换后的窗口、部分 UWP/WinUI 窗口被 DWM cloak(样式位仍在、仍收 WM_PAINT),必须配合 DwmGetWindowAttribute(DWMWA_CLOAKED)——值分 APP/SHELL/INHERITED 三种成因。Raymond Chen 的标准组合即 `IsWindowVisible(hwnd) && !IsWindowCloaked(hwnd)` |
| 窗口标题变化 | 无快照 API(轮询 GetWindowTextW 或走 L2 事件) | — |

三条边界事实:WM_ACTIVATE/WM_ACTIVATEAPP 是**发给窗口自身**的消息,外部 CLI 进程收不到,外部观察者只能走上述 API 或 WinEvent;actl CHANGELOG 已实证"UIA focused element 在 WinUI 上反向谎报",改用 GetGUIThreadInfo 物理层判据——与官方"UIA 视图可能与物理状态不一致"的告诫互为印证;actl lib.rs 已登记的"硬过滤 IsWindowVisible=false 会让快照只剩空壳框架"问题与 cloak 机制同源,值得在 M2 状态判定里一并处理。

### 1.2 L2 窗口层·事件(SetWinEventHook)

**机制**:out-of-context hook(回调在调用线程、需专职消息泵,异步但**保序**);作用域按 pid/thread 窄化;`WINEVENT_SKIPOWNPROCESS` 等旗标。关键事件常量:

| 常量 | 语义要点 |
|---|---|
| EVENT_SYSTEM_FOREGROUND (0x0003) | 前台窗口切换(同线程内切换也发) |
| EVENT_OBJECT_FOCUS (0x8005) | 键盘焦点变化 |
| EVENT_OBJECT_SHOW / DESTROY (0x8002/0x8001) | 窗口出现/销毁;**DESTROY 之后不得再查询该对象** |
| EVENT_OBJECT_LOCATIONCHANGE (0x800B) | 位置/尺寸变化——**系统对 caret 与 window 对象都会发**(光标闪烁也是事件源,风暴高发区) |
| EVENT_OBJECT_NAMECHANGE (0x800C) | 标题/名称变化(窗口、按钮、状态栏等系统对象) |
| EVENT_OBJECT_STATECHANGE (0x800A) | enabled/disabled、按下/释放等状态翻转 |
| EVENT_SYSTEM_MINIMIZESTART/END | 最小化/恢复(系统发,server 不发) |
| EVENT_OBJECT_CLOAKED/UNCLOAKED (0x8017/18) | cloak 状态变化——与 DWMWA_CLOAKED 查询配套 |

**坑**:回调处理慢会耗尽 USER 事件内存("eventually resulting in a fault or extremely slow response times",官方原文);各应用事件覆盖不均匀(PowerToys #2634:有应用拖动窗口不发 MOVESIZE 事件——**事件不可依赖为唯一信号**)。**UIPI 好消息**:out-of-context WinEvent 可以听到高完整性(elevated)进程的窗口事件,与注入式 hook 不同(Raymond Chen 2013-03-25);但查询其内容仍受 UIPI 限制。

### 1.3 L3 UIA 层·事件(Add*Handler 族)

四个注册方法:`AddAutomationEventHandler`(按事件 ID + TreeScope)、`AddPropertyChangedEventHandler`(按属性数组)、`AddStructureChangedEventHandler`(树增删)、`AddFocusChangedEventHandler`(**系统级,无法窄化 scope**)。要点:

- **官方可靠性原话**:"Do not assume that all possible events are raised by a UI Automation provider"——provider 不保证发事件;**事件 ≠ 状态真变**(收到后仍要核对真实状态);退订后仍可能收到 late delivery。
- **线程模型**:官方要求事件处理线程用 COM **MTA**;STA 会导致无法移除 handler 等问题。uiautomation crate 的 UIAutomation 类型因此 `!Send/!Sync`。
- **官方防御旋钮**(IUIAutomation6,Win10 1607/1809+):`CoalesceEvents`(重复事件合并)、`ConnectionRecoveryBehavior`(provider 无响应时调整请求超时——官方在基础设施层承认 provider 会挂)。
- **跨进程成本**:逐属性查询 = 逐次跨进程调用(官方:"slow and inefficient");CacheRequest 批量预取是正解(terminator 实测 245 元素 6.5s→200ms)。订阅时指定的 CacheRequest 在订阅后不可变更。

### 1.4 Chromium/Electron 惰性激活

Chromium 官方文档:"**Accessibility features in Chrome are off by default and enabled automatically on-demand**",且官方明说开启有性能代价(整树缓存复制内存;历史上同步代理调用曾让中型网页 10s+ 才可访问)。注册 UIA 客户端/查询会触发目标应用构建 a11y 树。这解释了 M0 spike 场景①(Electron 树"深而稀疏")的深层机制,也意味着 actl 对 Chromium 系目标的首查会触发对方一次性开销——snapshot 性能预算要把这一点计入。

## 2. 生态实践:两种哲学

### 2.1 哲学 A:每步重观察 + 显式 wait(执行域全体,actl 同阵营)

| 工具 | 机制 | 默认值(可直接抄) |
|---|---|---|
| pywinauto | 纯轮询(wait_until + sleep);**全仓库零 WinEvent 代码** | 窗口查找 5s/90ms;exists 0.5s/0.3s |
| AutoHotkey | 轮询循环(WM_TIMER+GetMessage 泵驱动,~10ms/轮),无 hook | SetWinDelay 100ms 是动作后的"可靠性 sleep" |
| uiautomation-rs(actl 依赖) | UIMatcher 轮询查找;事件 API 完整但在 feature 后 | timeout 3s / interval 100ms / depth 7 |
| terminator | 核心=纯拉取(cache request 一次预取);**录制器才用** UIA focus 事件 | locator 默认 timeout 0(一次性);wait_for 100ms 轮询 |
| agent-desktop | snapshot 每次全新遍历 + fail-closed 重识别;wait 族**全轮询**——连 wait event 都是 ≤200ms 的基线差分轮询,不是订阅 | --wait-timeout 30s;window 100ms;text 200ms→1s 指数退避 |
| UFO | 每步全量重观察(截图 + UIA 控件列表,上限 500) | **WaitCommand 硬编码 `time.sleep(3)`(反面教材)**;wait_enabled 0.5s/10s |
| Playwright(浏览器域基准) | actionability 预检:可见/**稳定**(rAF 连续两帧 bbox 不变)/可接收事件/可用 | MCP:action 5s / settle 500ms / navigation 60s |
| browser-use | 底层 CDP 事件常驻连接,但页面语义状态**每步拉取、焦点切换即清缓存** | 导航等待 0.05s 轮询 lifecycle 缓冲 |

### 2.2 哲学 B:常驻事件模型(只有阅读器与录制器)

- **NVDA**(最苛刻的实时 UIA 客户端):UIA 事件(root Subtree 全局组 + 按需局部组,MTA 专用线程)+ 进程内注入的 WinEvent hook + ≤10ms 主 pump。它的防御工程清单就是"实时维护状态"的真实价目表:textChange 事件白名单(#11002 事故后只对控制台/Word 处理)、**禁用 MSAA→UIA 事件映射**(#7345:对方慢消息泵会拖死整个 UIA 客户端库)、CoalesceEvents/ConnectionRecoveryBehavior、挂起窗口丢事件、事件去重、限速 handler、watchdog 线程(且 #20654 显示 watchdog 也取消不了个别 5s 级阻塞)。
- **录制器**(OpenAdapt、terminator-recorder):输入用全局低级钩子(WH_KEYBOARD_LL/MOUSE_LL)做连续流,**UI 结构仍是事件触发的单次拉取**——连"持续监听"的代表也不维护常驻 UIA 树。
- **共同折中律:事件当触发器,拉取当真相源。**

### 2.3 Codex sky 实证(本机 26.924,细节见 [12](12-codex-computer-use-comparison.md))

JS 层是纯 RPC 薄壳(13 个方法名直发原生二进制,坐标点击与元素点击分派为 `click`/`click_element` 两个原生方法),所有状态逻辑在闭二进制里。新鲜度契约:"element_index/screenshotId/坐标仅在产生它的观察内有效"+ 坐标动作的 screenshotId 缓存校验。**巨头选择了 B 路线(原生助手常驻 + 事件监听),并为此付出了真实代价**:空闲时挂着 UIA/WinEvent 订阅,曾引发 DWM 冻结(issue #36833)——这是 A/B 两条路线成本对比的一手数据。

## 3. 事故清单(事件路线价目表)

| 事故 | 机理 | 出处 |
|---|---|---|
| PowerToys 全量 WinEvent 订阅 | 移动鼠标即烧 3–5% CPU;修复结论"30 个窄 hook 优于 1 个全量" | microsoft/PowerToys #1264 |
| 有应用不发事件 | 拖动窗口不触发 MOVESIZE,FancyZones 失效;事件覆盖不均匀必须兜底 | PowerToys #2634/#7010 |
| NVDA 控制台事件风暴 | 10 万行输出后**所有 UIA 应用失联直至重启 NVDA** | nvaccess/nvda #11002 |
| WinEvent 风暴 × 慢消息泵 | MSAA→UIA 事件映射把 UIA 客户端库整个拖死 | NVDA #7345(源码注释) |
| 慢 provider 阻塞 | 单次 buildUpdatedCache(15 属性)阻塞 5.035s,watchdog 无法取消 | NVDA #20654 |
| Codex 助手空闲订阅 | UIA/WinEvent 订阅常驻,Edge 高频 AX 更新引发 DWM 冻结数秒 | openai/codex #36833 |
| UFO 硬编码 sleep(3) | "等待=睡三秒"——轮询派的反面教材,语义与状态脱钩 | microsoft/UFO 源码 |

## 4. 对 actl 的映射

### 4.1 架构判定

M1 的无状态 + verify-then-inject 与生态主流完全一致;"UIA 焦点视图在 WinUI 反向谎报 → 改物理层判据"这一战例证明 actl 已为"UIA 视图不可信"付过学费并选对了答案。**不需要为"实时状态"改变无状态架构**;实时性需求由 wait 谓词(轮询)承接,常驻订阅留给 Phase 3 serve 模式按需引入。

### 4.2 M2 wait/verify 实现约定(默认值取生态共识)

| 参数 | 默认值 | 依据 |
|---|---|---|
| 轮询间隔 | ~100ms | pywinauto 90ms / uiautomation-rs 100ms / terminator 100ms / agent-desktop 100ms |
| 窗口/元素级超时 | 3–5s | pywinauto 5s / uiautomation-rs 3s / Playwright MCP action 5s |
| text/内容类等待 | 指数退避 200ms→1s | agent-desktop text wait |
| 动作后 settle(--expect 轮询起点) | ~500ms | Playwright MCP timeout-settle 500ms |

`wait --gone`(等消失)注意销毁边界:轮询版天然规避"EVENT_OBJECT_DESTROY 之后不得再查询对象"的坑,是轮询优于订阅的又一个具体例证。

### 4.3 状态判定 checklist(供 `get`/`verify` 属性集与 M2 状态断言参考)

- **前台** = GetForegroundWindow(处理 NULL 瞬时);**焦点** = GetGUIThreadInfo(0).hwndFocus(已在用);
- **可见** = IsWindowVisible ∧ ¬DWMWA_CLOAKED(虚拟桌面/WinUI 三件套的坑,与 lib.rs 空壳框架问题同源);
- **最小化/最大化** = IsIconic / IsZoomed / GetWindowPlacement;
- 一律物理层快照优先,UIA 属性作为语义补充(名称/值/pattern 支持),两者冲突时以物理层为准(CHANGELOG 已有先例)。

### 4.4 事件订阅前置条件(Phase 3 `serve` 候选,登记)

技术可行性已确认:uiautomation 0.25.1 提供 add_focus_changed_event_handler 等全套 API(`event` feature),**零新增依赖**(硬规则 6 无忧)。若 serve 模式引入事件加速,必须同时满足:

1. MTA 专职线程 + 消息泵(UIA 事件回调线程要求);
2. 按 pid/事件域**窄化注册**(PowerToys 教训:绝不 EVENT_MIN~MAX 全量);
3. 开启 CoalesceEvents + ConnectionRecoveryBehavior(IUIAutomation6);
4. **所有事件判定带快照核对兜底**(官方:事件不保证发、事件≠状态真变)——事件只做 wait 的触发加速,判定仍走快照;
5. 不空闲全局订阅(Codex #36833 前车之鉴):订阅随命令生命周期创建/销毁,或 serve 空闲时降级为纯轮询;
6. 不维护常驻 UIA 树(生态零先例 + Chromium 惰性激活的性能代价)。

### 4.5 与既有文档的衔接

- [08 §6](08-agent-cli-jev-design-proposal.md) 任务循环的"观察→检查完成条件"用轮询 wait 承接,不需要事件;
- [11](11-desktop-interaction-and-display-research.md) 显示层问题 2("此刻是否占用鼠标/键盘焦点/剪贴板")由 §4.3 的物理层快照组合回答;若未来状态条要事件流,NVDA 防御清单(§2.2)是前置阅读;
- [09 §6.2](09-input-backends-review.md) 输入占用协调的"焦点被抢即停"检测同样依赖 GetGUIThreadInfo 快照(已在 M1 实现路径上);
- [spike 场景①](spike-findings.md) 的 Electron"深而稀疏"由 §1.4 Chromium 惰性激活解释,snapshot 性能预算计入目标应用首查激活成本。

## 5. 资料范围

- 微软 Learn:SetWinEventHook / out-of-context hook functions / event constants / uiauto-eventsforclients / uiauto-eventsoverview / uiauto-threading / uiauto-cachingforclients / GetGUIThreadInfo / GUITHREADINFO / DWMWINDOWATTRIBUTE / Security Considerations for Assistive Technologies(均 2026-09-26 抓取)。
- Raymond Chen(The Old New Thing):2021-01-04 监视窗口尺寸变化、2020-03-02 cloaking 与 IsWindowVisibleOnScreen、2013-03-25 用 accessibility 监视窗口、2026-01-07 caret 与 GetGUIThreadInfo(0)。
- 事故与源码:microsoft/PowerToys #1264/#2634、nvaccess/nvda #11002/#7345/#20654 及 UIAHandler 源码、openai/codex #36833、microsoft/UFO 源码(controller.py 的 sleep(3))。
- 生态源码核验:pywinauto(timings.py)、AutoHotkey(script2.cpp/globaldata.h)、FlaUI(Retry.cs)、uiautomation-rs 0.25.1(docs.rs + 本机 cargo registry 源码,事件 API 位于 `event` feature)、mediar-ai/terminator(locator.rs/tree_builder.rs/recorder)、lahfir/agent-desktop(CONCEPTS.md/wait.rs)、OpenAdapt(openadapt-capture)、Playwright(actionability/injectedScript.ts)、playwright-mcp、browser-use(session.py)。
- Chromium 官方文档:accessibility overview / how_a11y_works(_2) / perf。
- 本机实证:Codex sky 26.924 的 targets/windows JS 层(见 [12 §1.5](12-codex-computer-use-comparison.md));actl v0.1.0 的 kbd.rs/lib.rs 物理层判据。

OpenAI/NVDA 等实现细节以 2026-09-26 版本为准;生态默认值在 M2 实现时应按当时版本复核。
