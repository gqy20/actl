//! action —— 语义动作执行:click(pattern 分发)、type/press(键盘注入 +
//! verify-then-inject 护栏)、set-value(Value pattern 直写)。
//! 物理指针动作见 `pointer`;读取与断言见 `read`。

use actl_core::keys::KeySpec;
use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UISelectionItemPattern, UITogglePattern,
    UIValuePattern,
};
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use crate::locate::{Located, locate};
use crate::read::set_clipboard_text;
use crate::window::{find_window, focus_window};
use crate::{input, internal, kbd, timing};

/// 键盘命令家族说明:type/press 走 SendInput 键盘事件(键盘语义命令的执行本体,
/// 非物理"兜底",ADR-007 的 --physical 约束针对 click/scroll 等 UIA 等价物的兜底路径)。
pub struct InputResult {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
}

impl From<&Located> for InputResult {
    fn from(loc: &Located) -> Self {
        Self {
            window_title: loc.window_title.clone(),
            role: loc.role.clone(),
            name: loc.name.clone(),
        }
    }
}

/// click:语义执行分发(06 M1 范围:Invoke/Value/Selection)。
/// Invoke 优先;无 Invoke 但支持 SelectionItem(列表项/树项/选项卡)→ select(),
/// 语义同为"点击选中"。实际使用的 pattern 随结果上报,agent 可归因。
pub fn click(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<(Located, &'static str), CtlError> {
    let loc = locate(app, target, near)?;
    let _t = actl_core::trace::scope("click.pattern");
    // pattern 派发区段进 UIA 看门狗(真机实证 2026-10-05:菜单项 invoke
    // 打开 flyout 时跨进程 COM 无界阻塞——locate/identity/observation 已
    // 覆盖,动作模式调用是最后一个裸区段;20s 击杀留 stderr 标记)。
    let _watchdog = crate::watchdog::enter();
    if loc.element.get_pattern::<UIInvokePattern>().is_ok() {
        crate::feedback::semantic("click", &loc.element, &loc.window_title, || {
            loc.element
                .get_pattern::<UIInvokePattern>()
                .map_err(internal)?
                .invoke()
                .map_err(internal)
        })?;
        return Ok((loc, "invoke"));
    }
    if loc.element.get_pattern::<UISelectionItemPattern>().is_ok() {
        crate::feedback::semantic("select", &loc.element, &loc.window_title, || {
            loc.element
                .get_pattern::<UISelectionItemPattern>()
                .map_err(internal)?
                .select()
                .map_err(internal)
        })?;
        return Ok((loc, "selection-item"));
    }
    // 复选框/开关:点击语义 = Toggle(状态翻转)
    if loc.element.get_pattern::<UITogglePattern>().is_ok() {
        crate::feedback::semantic("toggle", &loc.element, &loc.window_title, || {
            loc.element
                .get_pattern::<UITogglePattern>()
                .map_err(internal)?
                .toggle()
                .map_err(internal)
        })?;
        return Ok((loc, "toggle"));
    }
    // 折叠面板/树节点:点击语义 = ExpandCollapse
    if loc.element.get_pattern::<UIExpandCollapsePattern>().is_ok() {
        crate::feedback::semantic("expand", &loc.element, &loc.window_title, || {
            loc.element
                .get_pattern::<UIExpandCollapsePattern>()
                .map_err(internal)?
                .expand()
                .map_err(internal)
        })?;
        return Ok((loc, "expand-collapse"));
    }
    Err(CtlError::new(
        ErrorCode::NotActionable,
        format!(
            "{} ({}) exposes none of Invoke/SelectionItem/Toggle/ExpandCollapse; \
             scroll/focus first, try `set-value`, or `--physical` for a real click",
            target.describe(),
            loc.role
        ),
    ))
}

/// type:定位 → SetFocus →(护栏)→ 逐字键盘输入或剪贴板粘贴。
/// `--paste`:文本经剪贴板 + ctrl+v 送达——**中文模式 IME 下唯一可靠的文本通道**
/// (UNICODE 包在 WinUI/TSF 仍会被 IME 组合改写:空格吞成上屏、标点全角化,
/// 战记实测);代价是占用共享剪贴板,输出透明上报 via 字段,不静默。
pub fn type_text(
    app: Option<&str>,
    target: &Target,
    text: &str,
    paste: bool,
    near: Option<&str>,
    verify_value: Option<&str>,
) -> Result<TypeOutcome, CtlError> {
    let loc = locate(app, target, near)?;
    let identity = crate::identity::identity(&loc.element);
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    let mut action = crate::feedback::prepare(
        "type",
        crate::feedback::keyboard(paste, paste),
        crate::feedback::target(&loc.element, &loc.window_title),
    )?;
    action.executing();
    loc.element.set_focus().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) cannot take keyboard focus",
                target.describe(),
                loc.role
            ),
        )
    })?;
    // 护栏与 press 相同:元素置焦后仍要物理前台核对,文本才允许注入。
    // 沉降轮询容忍激活过渡瞬态(空标题过渡窗/标题未就位),真抢焦仍拒。
    if app.is_some() {
        std::thread::sleep(std::time::Duration::from_millis(timing().type_focus_ms));
        let fg = kbd::foreground_settles_to(&loc.window_title);
        if fg != loc.window_title {
            return Err(actl_core::handoff::foreground_error(&loc.window_title, &fg));
        }
    }
    let _focus = crate::identity::FocusGuard::enter(&loc.element)?;
    // 输入占用锁(doc 09 §6.2):多 actl 实例互斥;持锁覆盖整个注入动作段
    let _anchor = crate::feedback::KeyboardAnchor::capture()?;
    crate::feedback::check_foreground(&loc.window_title)?;
    crate::feedback::check_keyboard_anchor()?;
    action.executing();
    let total = text.chars().count();
    let outcome = |delivered: usize, stopped_early: bool| TypeOutcome {
        located: (&loc).into(),
        delivered,
        total,
        stopped_early,
    };
    if paste {
        set_clipboard_text(text)?;
        // ctrl+v 走 VK 组合通道(IME 不拦截带修饰键组合);单次原子动作不分批
        kbd::send_key_spec(&KeySpec {
            modifiers: vec!["ctrl".into()],
            keys: vec![actl_core::keys::Key::Char('v')],
        })?;
        action.delivered();
        crate::input_verify::verify_value(&loc.element, &identity, verify_value, total)?;
        Ok(outcome(total, false))
    } else {
        // 键盘逐字通道:分批注入,批间复核物理前台——焦点被抢(真人点走/弹窗
        // 夺焦)即停在批边界并如实上报已送达量(并发输入损坏注入的四次活体
        // 实证结论,docs/spike-findings)
        let chars: Vec<char> = text.chars().collect();
        let pattern = Some(loc.window_title.clone());
        let delivered = input::send_in_segments(
            &chars,
            timing().segment_chars,
            |seg| {
                kbd::send_key_spec(&KeySpec {
                    modifiers: Vec::new(),
                    keys: seg.iter().map(|c| actl_core::keys::Key::Char(*c)).collect(),
                })
            },
            || {
                crate::identity::check_focus_anchor().is_ok()
                    && pattern
                        .as_ref()
                        .map(|p| {
                            kbd::foreground_title()
                                .map(|t| t.contains(p.as_str()))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
            },
        )?;
        if delivered < total {
            action.partial();
        } else {
            action.delivered();
        }
        crate::input_verify::verify_value(&loc.element, &identity, verify_value, delivered)?;
        Ok(outcome(delivered, delivered < total))
    }
}

/// type 的结果:注入了多少、是否中途被焦点抢占截断(部分送达是如实上报,
/// 不是失败——ok:true + input_effects.keyboard="partial")。
pub struct TypeOutcome {
    pub located: InputResult,
    pub delivered: usize,
    pub total: usize,
    pub stopped_early: bool,
}

/// press:可选先置前台(--app)。实测链路(docs/spike-findings.md):窗口级 SetFocus
/// 只激活窗口、不建立键盘焦点(键会被丢);必须再对窗口内内容元素做**元素级** SetFocus。
/// 启发式:窗口内首个 Document/Edit;找不到(如纯按钮面板)退化为仅窗口前置。
///
/// **verify-then-inject 护栏(事故教训,docs/spike-findings.md #6)**:注入前用 Win32
/// 物理前台(GetForegroundWindow)验证前台确为目标窗口——UIA 焦点视图可能与物理前台
/// 不一致,"看起来聚焦了"不等于键会进目标。不匹配则拒绝注入,宁可失败不可错发。
pub fn press_keys(spec: &KeySpec, app: Option<&str>) -> Result<Option<String>, CtlError> {
    press_keys_targeted(spec, app, None)
}

/// With a target, assert existing focus without focusing another field/window.
pub fn press_keys_targeted(
    spec: &KeySpec,
    app: Option<&str>,
    target: Option<&Target>,
) -> Result<Option<String>, CtlError> {
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    let loc = target.map(|t| locate(app, t, None)).transpose()?;
    let _focus = loc
        .as_ref()
        .map(|l| crate::identity::FocusGuard::enter(&l.element))
        .transpose()?;
    let focused = match &loc {
        Some(l) => Some(l.window_title.clone()),
        None => prepare_keyboard(app)?,
    };
    // 输入占用锁(doc 09 §6.2):护栏之后、注入之前取得,覆盖整个按键序列
    let _anchor = crate::feedback::KeyboardAnchor::capture()?;
    let mut action = crate::feedback::prepare(
        "press",
        crate::feedback::keyboard(false, false),
        actl_core::activity::ActionTarget {
            window: focused.clone(),
            ..Default::default()
        },
    )?;
    if let Some(title) = &focused {
        crate::feedback::check_foreground(title)?;
    }
    crate::feedback::check_keyboard_anchor()?;
    action.executing();
    kbd::send_key_spec(spec)?;
    action.delivered();
    Ok(focused)
}

/// key-down/key-up:与 press 相同的护栏链,但只发半边事件(见 kbd::send_key_partial)。
pub fn press_half(spec: &KeySpec, app: Option<&str>) -> Result<Option<String>, CtlError> {
    press_half_with_direction(spec, app, true)
}

pub fn press_half_with_direction(
    spec: &KeySpec,
    app: Option<&str>,
    down: bool,
) -> Result<Option<String>, CtlError> {
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    let focused = prepare_keyboard(app)?;
    let _anchor = crate::feedback::KeyboardAnchor::capture()?;
    // 半事件方向由调用方决定;此函数不持锁跨进程(key-up 是另一进程,锁不横跨)
    let mut action = crate::feedback::prepare(
        if down { "key-down" } else { "key-up" },
        crate::feedback::keyboard(false, false),
        actl_core::activity::ActionTarget {
            window: focused.clone(),
            ..Default::default()
        },
    )?;
    if let Some(title) = &focused {
        crate::feedback::check_foreground(title)?;
    }
    crate::feedback::check_keyboard_anchor()?;
    action.executing();
    kbd::send_key_partial(spec, down)?;
    action.delivered();
    Ok(focused)
}

/// 键盘注入的前置链:置前台 → 元素级聚焦 → 物理前台护栏(verify-then-inject)。
fn prepare_keyboard(app: Option<&str>) -> Result<Option<String>, CtlError> {
    Ok(match app {
        Some(pattern) => {
            let title = crate::window::focus_window(pattern)?;
            std::thread::sleep(std::time::Duration::from_millis(timing().press_focus_ms));
            // 元素聚焦是异步请求且存在竞态(实测固定延时忽好忽坏):
            // 反复 SetFocus 并验证 UIA 焦点元素真的变为可编辑角色,最多 3 轮
            ensure_element_focus(pattern)?;
            // 护栏:物理前台必须是目标窗口,否则注入会打进别处(已发生过真实事故);
            // 沉降轮询容忍激活过渡瞬态,真抢焦仍拒
            let fg = kbd::foreground_settles_to(&title);
            if fg != title {
                return Err(actl_core::handoff::foreground_error(&title, &fg));
            }
            Some(title)
        }
        None => None,
    })
}

/// 反复对窗口内可编辑元素 SetFocus,直到**物理**键盘焦点的根窗口是目标。
/// 判据用 GetGUIThreadInfo 物理层而非 UIA focused element:后者在 WinUI 上
/// 实测会谎报(物理层 RichEditD2DPT 已聚焦时仍报别窗口编辑框,导致成批误拒绝
/// ——战记"信源必须用可靠层"的又一次实例)。重试耗尽未就绪 → 放行,由
/// verify-then-inject 物理前台护栏做最终裁决(它才是注入前的权威闸门)。
fn keyboard_focus_matches(pattern: &str, hwnd: isize, title: &str) -> bool {
    if let Some(raw) = pattern.strip_prefix("hwnd:") {
        crate::window::parse_hwnd(raw).is_ok_and(|expected| expected == hwnd)
    } else {
        title.contains(pattern)
    }
}

fn ensure_element_focus(pattern: &str) -> Result<(), CtlError> {
    // 前置免扰:物理键盘焦点已落在目标窗口时直接放行——多 Edit 应用
    // (Excel:首页搜索框/名称框/编辑栏)里"找首个 Edit 强行 SetFocus"会把
    // 已正确的焦点搅到搜索框,后续按键全被吞(Office 实测:ctrl+s/f12 失效)
    if kbd::focused_root_window()
        .map(|(hwnd, title)| keyboard_focus_matches(pattern, hwnd, &title))
        .unwrap_or(false)
    {
        return Ok(());
    }
    for _ in 0..timing().focus_retries {
        crate::feedback::check_stop()?;
        if let Some(edit) = find_editable_element(pattern) {
            crate::feedback::check_stop()?;
            let _ = edit.set_focus();
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
        if kbd::focused_root_window()
            .map(|(hwnd, title)| keyboard_focus_matches(pattern, hwnd, &title))
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    Ok(())
}

/// 在标题匹配的窗口内找首个 Document/Edit(元素级键盘焦点的落点)。
fn find_editable_element(pattern: &str) -> Option<UIElement> {
    let auto = UIAutomation::new().ok()?;
    let walker = auto.create_tree_walker().ok()?;
    let win = find_window(&auto, &walker, pattern).ok()?;
    fn dfs(walker: &UITreeWalker, elem: &UIElement, depth: u32) -> Option<UIElement> {
        if depth > 12 {
            return None;
        }
        let role = format!("{:?}", elem.get_control_type().ok()?);
        if role == "Document" || role == "Edit" {
            return Some(elem.clone());
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            if let Some(hit) = dfs(walker, &c, depth + 1) {
                return Some(hit);
            }
            child = walker.get_next_sibling(&c).ok();
        }
        None
    }
    dfs(&walker, &win, 0)
}

/// set-value:UIValue pattern 直写,不经键盘(与 type 的分工见 06 §3.6)。
pub fn set_value(
    app: Option<&str>,
    target: &Target,
    value: &str,
    near: Option<&str>,
) -> Result<InputResult, CtlError> {
    let loc = locate(app, target, near)?;
    // Value pattern 调用同进 UIA 看门狗(click pattern 派发同理由)。
    let _watchdog = crate::watchdog::enter();
    loc.element.get_pattern::<UIValuePattern>().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no Value pattern; try `type` (keyboard path) instead",
                target.describe(),
                loc.role
            ),
        )
    })?;
    crate::feedback::semantic("set-value", &loc.element, &loc.window_title, || {
        loc.element
            .get_pattern::<UIValuePattern>()
            .map_err(internal)?
            .set_value(value)
            .map_err(internal)
    })?;
    Ok((&loc).into())
}

/// select:在容器内按选项名选中一项(SelectionItem pattern;06 §3.6 与
/// set-value 的分工——离散选项 vs 文本值)。选项匹配:精确 Name,落空后
/// 子串兜底(与定位链 L1/L2 同哲学);多命中 → AMBIGUOUS(如实上报)。
pub fn select_option(
    app: Option<&str>,
    container: &Target,
    option: &str,
    near: Option<&str>,
) -> Result<(Located, String, &'static str), CtlError> {
    let loc = locate(app, container, near)?;
    let hits = selection_items_under(&loc.element, option, false);
    let (items, level) = if hits.len() == 1 {
        (hits, "exact-name")
    } else if hits.is_empty() {
        let fuzzy = selection_items_under(&loc.element, option, true);
        if fuzzy.is_empty() {
            return Err(CtlError::new(
                ErrorCode::NotFound,
                format!(
                    "no selectable item named {option:?} under {} (exact and substring)",
                    container.describe()
                ),
            ));
        }
        (fuzzy, "substring-name")
    } else {
        (hits, "exact-name")
    };
    if items.len() > 1 {
        let names: Vec<String> = items
            .iter()
            .filter_map(|e| e.get_name().ok())
            .take(5)
            .collect();
        return Err(CtlError::with_evidence(
            ErrorCode::Ambiguous,
            format!("{option:?} matches {} selectable items", items.len()),
            serde_json::json!({ "candidates": names }),
        ));
    }
    let item = &items[0];
    let name = item.get_name().unwrap_or_default();
    crate::feedback::semantic("select", item, &loc.window_title, || {
        item.get_pattern::<UISelectionItemPattern>()
            .map_err(|_| {
                CtlError::new(
                    ErrorCode::NotActionable,
                    format!("{name:?} exposes no SelectionItem pattern"),
                )
            })?
            .select()
            .map_err(internal)
    })?;
    Ok((loc, name, level))
}

/// 容器子树内收集支持 SelectionItem 且名字匹配的元素。
fn selection_items_under(elem: &UIElement, option: &str, substring: bool) -> Vec<UIElement> {
    let auto = match UIAutomation::new() {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    let walker = match auto.create_tree_walker() {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };
    fn dfs(
        walker: &UITreeWalker,
        elem: &UIElement,
        option: &str,
        substring: bool,
        out: &mut Vec<UIElement>,
    ) {
        use uiautomation::patterns::UISelectionItemPattern;
        if out.len() >= 32 {
            return;
        }
        if let Ok(name) = elem.get_name() {
            let hit = if substring {
                name.contains(option)
            } else {
                name == option
            };
            if hit && elem.get_pattern::<UISelectionItemPattern>().is_ok() {
                out.push(elem.clone());
            }
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            dfs(walker, &c, option, substring, out);
            child = walker.get_next_sibling(&c).ok();
        }
    }
    let mut out = Vec::new();
    dfs(&walker, elem, option, substring, &mut out);
    out
}

/// toggle:翻转 Toggle 状态并上报翻转后状态(与 click 的分工:toggle 无点击
/// 副作用、可读状态;复选框/开关的语义动作)。
pub fn toggle_element(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<(Located, String), CtlError> {
    let loc = locate(app, target, near)?;
    let pattern = loc.element.get_pattern::<UITogglePattern>().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no Toggle pattern; `click` dispatches toggle semantics for checkboxes",
                target.describe(),
                loc.role
            ),
        )
    })?;
    crate::feedback::semantic("toggle", &loc.element, &loc.window_title, || {
        loc.element
            .get_pattern::<UITogglePattern>()
            .map_err(internal)?
            .toggle()
            .map_err(internal)
    })?;
    let state = pattern
        .get_toggle_state()
        .map(|s| format!("{s:?}"))
        .unwrap_or_else(|_| "unknown".into());
    Ok((loc, state))
}

/// paste:ctrl+v 注入到目标窗口的**当前焦点/选中处**(pack 路线的关键原语:
/// Excel 选中格、文件管理器选中项、重命名框等"无元素"落点)。护栏链与 press
/// 相同,但**刻意不做元素级聚焦**——SetFocus 会移动选中,违背本命令语义;
/// 只做窗口前置 + 物理前台护栏 + 输入锁。剪贴板内容只读不写(消费方)。
pub fn paste(app: &str) -> Result<String, CtlError> {
    use actl_core::keys::Key;
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    let title = focus_window(app)?;
    std::thread::sleep(std::time::Duration::from_millis(timing().press_focus_ms));
    let fg = kbd::foreground_settles_to(&title);
    if fg != title {
        return Err(actl_core::handoff::foreground_error(&title, &fg));
    }
    let _anchor = crate::feedback::KeyboardAnchor::capture()?;
    let mut action = crate::feedback::prepare(
        "paste",
        crate::feedback::keyboard(true, false),
        actl_core::activity::ActionTarget {
            window: Some(title.clone()),
            ..Default::default()
        },
    )?;
    crate::feedback::check_foreground(&title)?;
    crate::feedback::check_keyboard_anchor()?;
    action.executing();
    kbd::send_key_spec(&KeySpec {
        modifiers: vec!["ctrl".into()],
        keys: vec![Key::Char('v')],
    })?;
    action.delivered();
    Ok(title)
}

#[cfg(test)]
mod focus_tests {
    use super::keyboard_focus_matches;

    #[test]
    fn hwnd_focus_preserves_the_selected_field() {
        assert!(keyboard_focus_matches("hwnd:123", 123, "Zotero"));
        assert!(keyboard_focus_matches("hwnd:0x7b", 123, "Zotero"));
        assert!(!keyboard_focus_matches("hwnd:123", 456, "Zotero"));
        assert!(!keyboard_focus_matches("hwnd:invalid", 123, "hwnd:invalid"));
        assert!(keyboard_focus_matches("Zotero", 123, "My Library - Zotero"));
        assert!(!keyboard_focus_matches("Other", 123, "Zotero"));
    }
}
