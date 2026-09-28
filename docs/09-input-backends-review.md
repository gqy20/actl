# 09 · 鼠标定位与键盘注入：实现审查和方案调研

日期：2026-09-26 · 性质：研究与评审记录，不修改现行协议

关联：[设计蓝图](06-design-blueprint.md) · [技术约束](TECH_STACK.md) · [已有实测记录](spike-findings.md) · [Jev 集成提案](08-agent-cli-jev-design-proposal.md)

## 1. 结论与审查范围

建议保持 **UIA 语义定位与操作优先，显式物理输入使用统一 Win32 执行层**。最紧迫的差距是快捷键编码、目标身份、焦点归属、输入状态清理与结果验证，而不是更换技术栈。

没有一个开源库能同时保证所有 Windows 应用的后台执行、准确定位、输入兼容性和完成验证。应按层选择参考：FlaUI/pywinauto 用于 UIA 与控件适配，AutoHotkey 用于键盘语义与兼容策略，enigo 用于 Rust 输入后端实现对照。

本次读取了 actl-uia 的 lib.rs/kbd.rs、actl-core 的 keys.rs/target.rs/snapshot.rs、CLI 入口与测试，以及本机 Cargo 缓存中的 uiautomation 0.25.1 inputs.rs。对外资料优先使用 Windows 官方文档、项目官方仓库和源码。

工作区有另一位 agent 持续开发。本记录基于读取时的工作树，期间看到 HEAD 为 aa7ac7b，以及未提交的焦点、修饰键和 ref 防护修改。描述以函数和行为为锚点，不作为未来版本的永久结论。实现前必须检查发现是否已经被修复。

本次未安装工具、未修改实现、未调用真实鼠标或键盘注入。已有事故和 WinUI 表现来自仓库记录，不是本轮复现实测。

## 2. 当前实际方案

| 能力 | 已观察实现 | 实际边界 |
|---|---|---|
| 窗口定位 | find_window：顶层窗口标题子串，取首个匹配 | 无唯一性保证，尚未形成 HWND/PID/实例身份绑定 |
| 控件定位 | locate_walk：DFS，支持 ref/name/id/role | ref 是重新计数；其余首个命中即返回 |
| click | 定位后调用 UIInvokePattern::invoke | 不移动鼠标，不具备坐标点击、拖动、右键等能力 |
| set-value | UIValuePattern::set_value | 直接写控件值；不能假设等价于完整键盘输入事件链 |
| type | 定位 → UIA SetFocus → uiautomation Keyboard::send_text | 全局键盘输入；当前没有与 press --app 一致的前台检查 |
| press | KeySpec → 自建 SendInput 事件数组 | 命名键用 VK + scan；初读字符全用 Unicode，末次复查已改为优先 VK，见 §3.1 |
| press --app | 窗口 SetFocus、寻找首个 Document/Edit、延时与焦点检查、前台标题校验 | 仅标题校验，元素归属和失败处理仍不充分 |
| 物理鼠标 | actl-physical 返回 physical-scaffold | 仍是骨架，未形成 DPI、多屏、命中检查流程 |
| 结果等待 | press 的 --expect 支持等待标题子串出现 | 检查范围为顶层窗口，不能证明窗口属于本次动作或是新出现 |

近期加入的“ref 必须带 --app”和“press --app 注入前检查前台”是有价值的改进，但仍分别缺少元素身份与窗口实例身份验证。

## 3. 优先差距

### P0：先解决可能操作错误目标或错误动作的问题

#### 3.1 快捷键字符编码：初读发现，开发中已调整

位置：kbd.rs::send_key_spec 的 Key::Char 分支。

初次读取时 ctrl+s 编码为 Ctrl DOWN → Unicode 's' → Ctrl UP，而不是 Ctrl DOWN → VK_S DOWN/UP → Ctrl UP。这是明确的事件语义差异；具体哪些应用接受或忽略该序列，需要靶应用验证。

**末次复查更新：**另一位 agent 已添加 char_vk，字符优先经 VkKeyScanW 转 VK，只有无法映射时回退 Unicode。因此不能再把常见 ctrl+s 的旧编码问题当作最新实现的未修复项。剩余检查包括：映射使用的线程布局、返回修饰位只保留 Shift 而未处理 Ctrl/Alt、带修饰键但无法映射的字符仍回退 Unicode，以及上述路径的回归测试。新实现的 GUI 兼容性本轮未复测。

Windows 把 KEYEVENTF_UNICODE 转为 VK_PACKET，再由应用消息翻译形成字符输入。它不等价于真实 S 键事件，不能据此保证触发快捷键。[KEYBDINPUT 官方说明](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput)

建议把“文本”“逻辑按键/快捷键”“物理扫描码”分成明确路径。快捷键中的字母走 VK；标点和布局相关键按目标线程 HKL 映射；无法表示的快捷键报错，不偷偷降级为文本。文本可以用 Unicode，但换行、Tab、IME 和应用接收行为需要独立验证。

#### 3.2 焦点保护不统一，且标题不是目标身份

位置：lib.rs::type_text、press_keys、ensure_element_focus；kbd.rs::foreground_title。

- type 在 SetFocus 后直接逐事件输入，缺少前台和目标元素核对。
- press 无 --app 时不会执行上述前台检查。
- press --app 用 fg.contains(pattern) 判定。同名窗口可能通过；空 pattern 甚至能匹配空字符串。
- ensure_element_focus 只判断全局 UIA 焦点的 role 是否为 Document/Edit，没有核实它是否属于目标窗口；尝试耗尽仍返回 Ok。
- 每次 press 都重新聚焦首个编辑控件，可能把用户刚导航到的文件名框或第二个字段切走。

建议形成统一的输入前置流程：解析唯一目标实例 → 确定预期焦点元素 → 必要时请求聚焦 → 有界等待 → 核对 HWND/进程实例/控件归属 → 注入。保留目标窗口内已有正确焦点；不要为所有快捷键强制寻找首个 Edit。

GetForegroundWindow 只能反映当前前台，无法让“检查后注入”成为原子事务。长文本分批检查，焦点变化后停止，报告可能已输入的部分；不能承诺零竞态。

Windows 限制抢前台行为，不能通过简单调用或固定 sleep 保证成功。聚焦失败应明确返回。[SetForegroundWindow](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setforegroundwindow)

#### 3.3 ref 与多匹配仍可能导致静默错点

位置：lib.rs::find_window、locate_walk；core::Target。

ref 加 --app 只能缩小窗口范围，不能解决窗口内插入节点后 DFS 序号改变。标题、名称、角色和 AutomationId 的首个匹配也可能选错；AutomationId 不应被假定为全桌面唯一。

建议绑定快照上下文、窗口实例与元素特征；执行前验证身份。多匹配返回 AMBIGUOUS 和候选；弱定位只提供候选，不静默替换目标。

此外 top_level_of_focused 当前一路向父节点攀升直到 walker 返回错误，未在桌面根的直接子窗口处停止，存在把桌面根当目标范围的代码路径。需用 mock 树验证，并区分树尽头与 COM 读取失败。

#### 3.4 无条件释放全部修饰键与部分注入失败

位置：kbd.rs::release_all_modifiers、send_key_spec。

当前每次开始和结束都释放全部修饰键，读取时列表已扩展到通用码与左右变体共 11 个。它能尝试清理残留，也可能干扰用户正在按住的键；开场清理忽略 SendInput 返回值。若主数组只注入一部分，尾部释放事件不一定已送达。

建议记录本次执行拥有的按下键；异常时尽力释放这些键并报告清理结果。若发现用户仍按着冲突修饰键，等待或返回输入冲突，而不是假装能完美恢复物理键盘状态。准备与校验完整事件序列应在首次注入前完成。

SendInput 返回的是插入输入流的事件数量，不是目标应用成功处理数量；它不会重置已有按键状态。UIPI 也可能阻止注入，而且不能仅从返回值或 GetLastError 断言一定是 UIPI。[SendInput 官方说明](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)

### P1：兼容性、规范和测试缺口

#### 3.5 VK、扫描码、扩展键与布局尚未形成完整模型

vk_event 当前用 MapVirtualKeyW，未使用目标线程布局；没有显式扩展键标记，缺少物理扫描码输入的独立语义。方向键、Insert/Delete、右 Ctrl/Alt、数字键盘等应纳入编码测试。

注意：写入 wScan 不代表正在使用 KEYEVENTF_SCANCODE 模式；VK 与 scan 模式均有用途，不应一律改成 scan。扫描码及扩展位应按 API 契约处理。[KEYBDINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-keybdinput)

仓库注释将 wScan=0、左右修饰键与 WinUI 丢键直接建立因果关系，但本轮未独立验证。微软 SendInput 示例本身包含未赋 wScan 的 VK 快捷键，因此 wScan=0 不能被泛化为必然无效。需要控制焦点、VK_PACKET、布局等变量后再决定应用特例。

#### 3.6 F10–F12 存在确定的解析边界问题

位置：keys.rs::parse_key_expr。

is_fkey 同时要求 p.len() == 2 与数值在 1..=12，导致 f10/f11/f12 在解析层被拒绝，即使后端映射已支持。现有测试覆盖 f4/f5 和 f13 无效，没有覆盖这一边界。应先增加回归测试再修复。

#### 3.7 文本路径与按键路径分裂

type 仍调用 uiautomation 0.25.1 Keyboard::send_text。本地上游源码显示：它会尝试把可映射字符转成 VK，而非所有字符固定 Unicode；Keyboard 默认 interval=50ms，并逐事件发送。这使它与自建 press 的编码、节奏、清理和前台保护不一致。

建议统一输入编译与执行层。分别定义纯文本、换行、Tab、快捷键和扫描码；基于目标控件选择合适能力，不能让“默认延时”承担可靠性职责。长文本评测需同时测速度与完整性。

#### 3.8 --physical 的实现与规范解释不一致

AGENTS.md 硬规则 7 和 docs/06 §7 要求物理输入显式 --physical。当前 type/press 实际走全局键盘注入，CLI 没有对应 flag；源码注释将其解释为“键盘命令本体，不是兜底”。这与现行文字约束存在冲突。

需按正式 spec 决定接口：沿用现行规范加显式授权，或明确修订规则及调用者预期。不能只靠源码注释改变授权语义。本次不修改行为。

#### 3.9 UIA 能力和错误信息不足

当前 click 仅支持 Invoke，未覆盖 Toggle、SelectionItem、ExpandCollapse、ScrollItem 等明确语义。读取 pattern 的失败全部映射成“不支持”，walker 的错误以 .ok() 作为尽头，可能掩盖元素失效、权限或 provider 故障。

建议暴露实际支持的动作；按命令语义分发，不能把 Toggle 等同于无副作用的 click。保留 HRESULT 诊断，区分不支持、失效、超时和访问问题。读取使用有范围的 CacheRequest，减少逐属性跨进程往返。[UIA 缓存机制](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients)

## 4. 各类路线的适用性

| 路线 | 适用情况 | 对 actl 的建议 |
|---|---|---|
| UIA pattern | 应用提供语义控件，支持 Invoke/Value 等 | 主路径；可避免坐标依赖，但 provider 行为和完成结果仍需验证 |
| UIA 几何定位 + SendInput | 可识别元素但需要右键、拖动等物理动作 | 显式 --physical 后使用，增加点击点与目标命中校验 |
| HWND 消息 / ControlSend | 特定传统 Win32 控件 | 应用适配层选用并逐应用验证，不作为现代应用通用后台输入 |
| 剪贴板粘贴 | 大段文本且目标接受粘贴 | 可选明确策略；考虑剪贴板并发、粘贴事件和数据保留，不静默使用 |
| 应用 API / COM / 原生 CLI | 应用有稳定业务接口 | 上层 agent 可优先选择；actl 聚焦 GUI 缺口，避免把所有应用 API 塞进核心 |
| 图像模板 / OCR / VLM | UIA 不暴露内容的 Canvas 等 | 当前 PRD 非目标；只作为外部方案比较，不本轮扩展范围 |
| 驱动或虚拟 HID | 特殊测试与设备仿真 | 带来安装、权限和维护成本，本项目没有采用依据，也不用于绕过系统限制 |

ControlSend 并不是普遍可靠的后台键盘接口；AutoHotkey 自己说明了修饰键对活动窗口的影响，以及某些 Edit 控件更适合直接设置文本。[官方 ControlSend 文档源码](https://raw.githubusercontent.com/AutoHotkey/AutoHotkeyDocs/v2/docs/lib/ControlSend.htm)

## 5. 开源方案对比

“优选”是对本项目的匹配判断，不代表完成了同环境跑分。以下均为只读调研，没有安装或引入依赖。

| 项目 | 最值得参考的部分 | 引入代价 / 局限 | 建议 |
|---|---|---|---|
| [FlaUI](https://github.com/FlaUI/FlaUI) | UIA2/UIA3、控件抽象、原生焦点与鼠标操作 | .NET 技术栈，整体替换会增加运行与桥接成本 | UIA 行为和测试设计的主要参考 |
| [pywinauto](https://github.com/pywinauto/pywinauto) | win32/uia 后端、定位、输入兼容策略 | Python 运行时，不符合当前 Rust 单文件主线 | 外部对照与兼容性基准候选 |
| [AutoHotkey](https://github.com/AutoHotkey/AutoHotkey) | 文本与按键区分、VK/scan、修饰键策略 | 脚本运行时，不能直接提供 actl 的身份和证据契约 | 键盘语义参考，按需做外部基准 |
| [enigo](https://github.com/enigo-rs/enigo) | Rust 输入 API、文本/按键/扫描码、已按键追踪、DPI 处理 | 不负责 UIA 目标身份与任务完成；仍需自己的保护层 | 最接近现有栈的输入库候选，先对照评测 |
| [PyAutoGUI](https://github.com/asweigart/pyautogui) | 简单坐标操作和图像定位原型 | 官方 README 明示主要支持主屏；键盘发往当前焦点 | 不作为多屏 Windows 可靠执行核心 |
| [SikuliX](https://github.com/oculix-org/SikuliX1) | 图像驱动的视觉自动化 | 视觉路线及运行时成本，不符合当前 UIA 定位主线 | Canvas 路线参考，暂不引入 |
| [Appium Windows Driver](https://github.com/appium/appium-windows-driver) | WebDriver 测试生态 | 依赖 server；其 README 提示 WinAppDriver 多年未维护 | 不作为本项目新增基础依赖 |

FlaUI 将窗口前置与控件聚焦区分处理，鼠标动作通过 clickable point 定位；这是当前“窗口 SetFocus + 首个 Edit”启发式应重点对照的部分。[AutomationElement 源码](https://raw.githubusercontent.com/FlaUI/FlaUI/master/src/FlaUI.Core/AutomationElements/AutomationElement.cs)

pywinauto 明确提供文本 VK_PACKET 与普通 VK 的兼容选择。AutoHotkey 也区分 SendText 与快捷键发送，说明文本和按键需要不同语义。[pywinauto keyboard](https://pywinauto.readthedocs.io/en/latest/code/pywinauto.keyboard.html) · [AutoHotkey Send 文档源码](https://raw.githubusercontent.com/AutoHotkey/AutoHotkeyDocs/v2/docs/lib/Send.htm)

enigo 的 Windows 源码包含目标线程键盘布局获取、扩展扫描码处理、按下键追踪和线程级 DPI awareness。但阅读主分支不等于验证已发布版本，仍需锁定版本测试多屏和输入异常。[Windows 实现](https://github.com/enigo-rs/enigo/blob/main/src/win/win_impl.rs)

enigo 页面本次显示约 1.8k stars，未作为安装推荐自动执行。项目文档列出它也不等于已经评估通过；若正式引入，需核对适用的安装规则、依赖数量、windows crate 版本与许可。当前继续复用已有 windows-rs 做小而明确的封装是可行选择，不复制 vendored 外部代码。

## 6. 鼠标物理路径的建议设计

现阶段缺少的不是“算一个控件中心点”，而是完整执行链：

1. 定位唯一目标，验证窗口实例和元素状态。
2. 优先 UIA pattern；确需物理动作时检查显式 --physical。
3. 获取 UIA clickable point。若只剩 bounding rectangle，不能直接认定中心点可点击。
4. 确保目标区域可见，检查点击点的窗口/元素命中关系；被遮挡或关系不明时停止。
5. 统一采用物理屏幕像素，明确窗口坐标到屏幕坐标的转换与 DPI awareness。
6. 多屏绝对输入按虚拟桌面原点和尺寸归一化，处理负坐标；使用 ABSOLUTE + VIRTUALDESK 的正确语义。
7. 执行按钮序列，记录部分发送和清理结果；重新观察后置条件。

UIA GetClickablePoint 返回物理屏幕坐标。SendInput 的绝对坐标范围是 0–65535，未设置 VIRTUALDESK 时默认映射主屏；相对鼠标移动还可能受加速度影响。[GetClickablePoint](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationelement-getclickablepoint) · [MOUSEINPUT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-mouseinput)

点击前的可见性和命中检查仍有竞态，不能用它们保证目标之后不会移动。拖动还需要取消、释放按键和过程失败语义，不能直接由几个无保护的 move/down/up 命令拼成。

### 6.1 如何区分真实接管鼠标与后台执行

应分别描述三个维度：是否使用系统真实指针、是否改变前台/键盘焦点、是否需要用户暂时避免并发输入。它们不能合并为一个“后台”布尔值。

| 操作方式 | 真实鼠标 | 焦点影响 | 并发使用预期 |
|---|---|---|---|
| UIA Invoke/Value/Toggle 等语义操作 | 不注入鼠标事件 | 应用仍可能激活窗口或弹出对话框 | 通常干扰较小，不能保证无干扰 |
| UIA 定位后物理点击 | 通常移动指针并按下/释放 | 点击通常改变焦点 | 执行期间需协调物理输入 |
| SendInput 键盘 | 不移动指针 | 依赖前台与控件焦点 | 输入期间需协调键盘与窗口切换 |
| 原地物理点击、滚轮 | 未必移动指针，但仍使用全局输入流 | 依动作和应用而异 | 属于物理执行，不因指针没动而豁免 |
| 物理拖动、悬停 | 使用真实指针位置与按钮状态 | 可能改变交互状态 | 按完整动作段协调输入 |

定位策略与执行策略必须分开记录：UIA 定位 → Invoke 是语义执行；UIA 定位 → clickable point → SendInput 是物理执行。

建议只有两个执行等级：默认允许语义执行；显式 --physical 才允许物理执行。允许物理不等于强制每一步使用物理，仍优先语义能力。不支持时明确返回，不能自动升级权限。

以下是拟议的结果字段，不是现行协议：

```json
{
  "target_resolution": "uia",
  "execution": "send_input",
  "input_effects": {
    "pointer": "move_and_click",
    "keyboard": "none",
    "focus": "may_change"
  }
}
```

执行前能力描述表达预计影响，执行结果记录实际采用路径与可观察影响；无法判断时明确 unknown，不能报告无影响。窗口激活等副作用可能来自应用本身，不能仅由后端名称推断。

### 6.2 输入占用的范围、协调与退出

建议采用短时输入占用：取得 actl 进程间输入执行锁 → 校验目标与焦点 → 执行动作段 → 释放本次按下的键/按钮 → 记录结果并释放锁。锁应在最后一次前置校验之前取得，避免等待锁期间焦点已经变化。

- 锁只协调遵守该协议的 actl 实例，不能阻止用户或其他软件输入，也不能保证检查与执行原子化。
- 模型推理、网络请求和普通等待期间通常释放输入占用；再次操作前重新校验。拖动等需要连续按住按钮的片段不可中途交给模型长时间思考。
- 发现焦点变化或可检测的用户介入时停止后续动作、清理本次持有的键/按钮，返回已执行部分。用户介入检测不是绝对可靠机制，不能用它承诺零冲突。
- 默认不通过屏蔽用户键鼠维持控制；取消、超时、错误路径都必须尝试释放本次输入状态。
- 不无条件恢复鼠标到原位置：用户可能已主动移动，自动恢复会再次抢走指针。
- 剪贴板也是共享状态。若采用粘贴策略，应单独记录影响，避免覆盖用户新写入的数据；鼠标/焦点分类不能替代剪贴板并发处理。

如果用户需要持续使用自己的桌面，同时自动化必须密集使用物理输入，应考虑独立交互测试会话或虚拟机。它们仍需可用的交互桌面；不能承诺锁屏或断开 RDP 后一切照常运行。

**落地记录（2026-09-28）**：

- 锁名为会话级命名互斥体 `actl-input-lock`（CreateMutexExW，无 `Global\` 前缀，见 actl-uia `input.rs`）。**此名自此公开为共享约定**：驱动同一桌面 UI 的配套工具（如 wechat-ui-collector 的公众号采集/历史回填线）在 UI 驱动段前应获取同名锁——两程序并存不打架；锁仍只协调遵守协议者，用户输入不受阻（本节既有语义不变）。
- 两件套产品化（对齐 collector 实证路线）：①`wait --idle <ms>`——GetLastInputInfo 空闲门，"人不在"的判定；②`--no-activate`——WS_EX_NOACTIVATE 后台指针点击（docs/06 §7.8）。采集类长任务 = 空闲门 + 后台点击：人不在时干活、不抢前台不改 Z 序；键盘仍需前台（fail-closed）。
- 光标送回原位是上文"不无条件恢复鼠标"的**受控例外**：仅 `--no-activate` 动作适用——注入把光标移到点击点是本工具自己制造的扰动，送回在持有输入锁的同一动作段内立即执行（drag 落点有语义，不送回）。送回是"撤销自身扰动"，不是"抢占用户指针"，与该原则不冲突。

## 7. 分层实现建议

建议统一为：目标解析 → 目标/焦点校验 → 输入计划编译 → 事件注入 → 结果验证。

- actl-core：纯类型化输入意图、解析与计划规则，可以对 VK/scan/Unicode 的中间表示做无副作用单测。
- actl-uia：UIA 与当前唯一允许 unsafe 的 Win32 封装位置，包含原生身份、焦点和注入边界。
- actl-physical：可承担显式物理策略与公开能力封装；若希望移动 unsafe 实现，先修订现有架构规则，不能直接违反唯一豁免点。
- CLI：统一物理授权、预算和 envelope，报告实际使用的输入策略、目标身份、发送数量和验证状态。

Jev 只选择已经生成的合法动作。不能让模型修补不确定的焦点、猜测扫描码，或凭高置信度跳过后置条件。

## 8. 推荐验收顺序

### 第一批：不触碰真实桌面的回归测试

- 快捷键编译必须产生 VK 字母事件，而不是 VK_PACKET；文字输入与快捷键分开断言。
- f1/f9/f10/f11/f12 的合法性与 f0/f13 的拒绝行为。
- 扩展键、左右修饰键、布局映射失败、emoji 的 UTF-16 代理对。
- 模拟 SendInput 部分成功，验证结果未知与清理路径。
- 模拟同名窗口、空标题条件、元素插入和焦点落在其他窗口。
- 用模拟树确认默认窗口选择停在正确顶层窗口，不上行到整个桌面。

### 第二批：受控 GUI 环境

使用系统自带记事本、计算器和设置窗口，遵守仓库 fixtures 范围。涉及用户正在使用的桌面时，不以真实前台试错；优先专用交互测试会话。

- 保存快捷键以对话框/文件产物为证据，不能只看 SendInput 返回数。
- 两个同名窗口、多编辑框、模态对话框、焦点被切换。
- 中文 IME 开关、英文/非英文布局、大小写锁定、用户持有修饰键。
- 多行长文本、emoji、换行与 Tab；记录实际文本与耗时。
- 普通与 elevated 窗口的权限差异，拒绝路径必须可解释。
- 物理鼠标启用后再测 100%/150%/200% 缩放、左右副屏、负坐标与遮挡。
- RDP 连接/断开、锁屏状态单列兼容矩阵，不承诺所有输入路径均可工作。

### 排期建议

先修输入编码、目标身份和统一前置校验，再补按键状态管理与后置验证；之后实现显式物理鼠标并验证 DPI/多屏。最后再评估引入 enigo 或应用特例是否确实降低维护成本。

## 9. 本轮验证结果

`cargo xtask gate` 在本轮读取工作树时通过：fmt、clippy 和 27 项测试通过。当前测试主要覆盖协议和纯逻辑；actl-uia 没有单元测试，这次通过不代表输入兼容性和焦点安全已通过 GUI 验收。

本轮只新增研究文档，不修复另一位 agent 正在修改的输入实现。以上问题应按对应函数逐项交接，并先核对最新版本后补复现测试。

## 10. 实证闭环更新(2026-09-26 深夜,金任务 2 攻坚后)

本评审 §2/§5 建议的分路径方案已在同夜落地并被 GUI 实测证实,关键证据链(docs/spike-findings.md"键盘注入排查战记"):

- **快捷键 → VK 通道**:`ctrl+s` 旧编码(Ctrl DOWN → Unicode 's' → Ctrl UP)实测被记事本当文本打出孤立 's';字符经 VkKeyScanW 转 VK 后组合语义成立(标题出现 `*ss`→保存成功)。
- **纯文本 → UNICODE 通道**:组合键修复后纯文本若走 VK,中文 IME 会把空格吞成上屏确认、半角冒号全角化(`golden task 2:` → `goldentask2:`);纯文本固定 UNICODE 后逐字一致。
- **通用修饰键**:VK_LCONTROL 改 VK_CONTROL 等(AHK globaldata.cpp / pywinauto keyboard 同款),清场全集 11 键。
- **验收**:金任务 2(fixtures/golden-task-2.sh)7/7——type → ctrl+s(`--expect window:另存为`)→ set-value 文件名 → 磁盘内容逐字一致。§8 第一批清单中的"对话框产物证据"即为此形态。
- 本评审指出的遗留(线程布局 HKL、非 Shift 修饰位、组合键映射失败回退、回归测试)仍未做,交接有效。
