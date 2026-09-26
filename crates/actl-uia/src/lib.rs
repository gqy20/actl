//! actl-uia —— Windows UIA 后端:capture(窗口定位 + DFS 遍历)。
//!
//! 全仓库唯一接触 COM 的 crate(AGENTS.md §5.5)。
//! 已知限制(spike #4,docs/spike-findings.md):walker 在"无子/无兄弟"时返回 Err,
//! 与真错误不可区分——当前以 `.ok()` 视为尽头;HRESULT 分类列入 M1 遗留。
//! 性能注记(spike #3):本版逐属性跨进程读取,release 下金任务实测,超 500ms 再上 cache request。

#![allow(unsafe_code)] // 豁免点(AGENTS.md §5.5):kbd 模块的 SendInput/MapVirtualKeyW 注入

pub mod kbd;

use actl_core::keys::KeySpec;
use actl_core::{CtlError, ErrorCode, UiNode};
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

/// 收集上限与深度上限(防失控 UI;截断在输出中标记 truncated)。
pub const MAX_ELEMENTS: usize = 5000;
pub const MAX_DEPTH: u32 = 40;

pub struct WindowInfo {
    pub title: String,
    pub class: String,
    pub pid: u32,
}

pub struct CaptureResult {
    pub window: WindowInfo,
    /// DFS 序节点流,`parent` 指向同流索引(供 core 层 skeleton 投影)
    pub nodes: Vec<UiNode>,
    pub truncated: bool,
}

/// 捕获一个顶层窗口的 UIA 子树。
/// `app`:窗口标题子串;None = 焦点元素所在顶层窗口(spike #2:焦点链可能落在后台 UI)。
pub fn capture(app: Option<&str>) -> Result<CaptureResult, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;

    let target = match app {
        Some(pattern) => find_window(&auto, &walker, pattern)?,
        None => top_level_of_focused(&auto, &walker)?,
    };

    let window = WindowInfo {
        title: target.get_name().unwrap_or_default(),
        class: target.get_classname().unwrap_or_default(),
        pid: target.get_process_id().unwrap_or_default(),
    };

    let mut nodes = Vec::new();
    walk(&walker, &target, 0, None, &mut nodes);
    let truncated = nodes.len() >= MAX_ELEMENTS;
    Ok(CaptureResult {
        window,
        nodes,
        truncated,
    })
}

/// 在顶层窗口中按标题子串定位首个匹配;无匹配 → NOT_FOUND(06 §4)。
/// 顶层窗口 + 挂靠宿主的 owned dialog(IFileSaveDialog 等不是 root 的直接子节点,
/// 实测新记事本的"另存为"在宿主窗口的 depth-1,以 role=Window 挂靠)。
/// 顶层窗口 + 挂靠宿主的 owned dialog(IFileSaveDialog 等不是 root 的直接子节点,
/// 实测新记事本的"另存为"在宿主窗口的 depth-1,以 role=Window 挂靠)。
fn top_windows(auto: &UIAutomation, walker: &UITreeWalker) -> Vec<UIElement> {
    let mut out = Vec::new();
    let Ok(root) = auto.get_root_element() else {
        return out;
    };
    let mut child = walker.get_first_child(&root).ok();
    while let Some(w) = child {
        out.push(w.clone());
        let mut sub = walker.get_first_child(&w).ok();
        while let Some(d) = sub {
            if d.get_control_type()
                .map(|t| t == ControlType::Window)
                .unwrap_or(false)
            {
                out.push(d.clone());
            }
            sub = walker.get_next_sibling(&d).ok();
        }
        child = walker.get_next_sibling(&w).ok();
    }
    out
}

fn scan_top_window(auto: &UIAutomation, walker: &UITreeWalker, pattern: &str) -> Option<UIElement> {
    top_windows(auto, walker)
        .into_iter()
        .find(|w| name_matches(w, pattern))
}

fn name_matches(elem: &UIElement, pattern: &str) -> bool {
    elem.get_name()
        .map(|n| n.contains(pattern))
        .unwrap_or(false)
}

fn find_window(
    auto: &UIAutomation,
    walker: &UITreeWalker,
    pattern: &str,
) -> Result<UIElement, CtlError> {
    scan_top_window(auto, walker, pattern).ok_or_else(|| {
        CtlError::new(
            ErrorCode::NotFound,
            format!("no top-level window title contains {pattern:?}"),
        )
    })
}

/// 焦点元素沿父链上行到顶层窗口。
fn top_level_of_focused(auto: &UIAutomation, walker: &UITreeWalker) -> Result<UIElement, CtlError> {
    let mut cur = auto.get_focused_element().map_err(internal)?;
    loop {
        match walker.get_parent(&cur) {
            Ok(p) => cur = p,
            Err(_) => return Ok(cur), // 到达根(尽头 Err 语义,见模块注释)
        }
    }
}

fn walk(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    parent: Option<usize>,
    nodes: &mut Vec<UiNode>,
) {
    if nodes.len() >= MAX_ELEMENTS || depth > MAX_DEPTH {
        return;
    }
    nodes.push(to_node(elem, depth, parent));
    let idx = nodes.len() - 1;
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        walk(walker, &c, depth + 1, Some(idx), nodes);
        child = walker.get_next_sibling(&c).ok();
    }
}

fn to_node(elem: &UIElement, depth: u32, parent: Option<usize>) -> UiNode {
    UiNode {
        depth,
        role: format!(
            "{:?}",
            elem.get_control_type().unwrap_or(ControlType::Custom)
        ),
        name: elem.get_name().ok().filter(|n| !n.is_empty()),
        automation_id: elem.get_automation_id().ok().filter(|a| !a.is_empty()),
        parent,
    }
}

fn internal(e: impl std::fmt::Debug) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("{e:?}"))
}

// ---------- 定位与交互(locate / click / read_property) ----------

use actl_core::target::Target;
use uiautomation::patterns::{UIInvokePattern, UIValuePattern};

/// 定位结果:命中元素 + 所在窗口。
pub struct Located {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub(crate) element: UIElement,
}

/// 在目标窗口内按 Target 定位元素(遍历可提前退出,优于全量 capture)。
/// `Ref(n)` 数不到 = 树已变化 → STALE_REF;语义选择器无命中 → NOT_FOUND(06 §4)。
pub fn locate(app: Option<&str>, target: &Target) -> Result<Located, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let root = match app {
        Some(pattern) => find_window(&auto, &walker, pattern)?,
        None => {
            // fail-closed:@eN 的编号只对快照来源窗口的遍历顺序有意义;对着
            // "当前恰好聚焦的窗口"重放,前台一变就会静默点进别的应用(已实际发生)。
            if matches!(target, Target::Ref(_)) {
                return Err(CtlError::protocol(format!(
                    "{} requires --app: refs are scoped to the window the snapshot came from; \
                     pass --app <title-substr> or re-snapshot",
                    target.describe()
                )));
            }
            top_level_of_focused(&auto, &walker)?
        }
    };
    let window_title = root.get_name().unwrap_or_default();

    let mut counter: u32 = 0;
    if let Some(hit) = locate_walk(&walker, &root, 0, target, &mut counter)? {
        Ok(Located {
            window_title,
            role: format!(
                "{:?}",
                hit.get_control_type().unwrap_or(ControlType::Custom)
            ),
            name: hit.get_name().ok().filter(|n| !n.is_empty()),
            automation_id: hit.get_automation_id().ok().filter(|a| !a.is_empty()),
            element: hit,
        })
    } else if matches!(target, Target::Ref(_)) {
        Err(CtlError::new(
            ErrorCode::StaleRef,
            format!(
                "{} does not resolve — the tree changed since the snapshot",
                target.describe()
            ),
        ))
    } else {
        Err(CtlError::new(
            ErrorCode::NotFound,
            format!("no element matches {}", target.describe()),
        ))
    }
}

fn locate_walk(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    target: &Target,
    counter: &mut u32,
) -> Result<Option<UIElement>, CtlError> {
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    let role = format!(
        "{:?}",
        elem.get_control_type().unwrap_or(ControlType::Custom)
    );
    let hit = match target {
        Target::Ref(n) => {
            if actl_core::is_interactive_role(&role) {
                *counter += 1;
                *counter == *n
            } else {
                false
            }
        }
        Target::Name(s) => elem
            .get_name()
            .map(|n| n.contains(s.as_str()))
            .unwrap_or(false),
        Target::Id(s) => elem.get_automation_id().map(|a| a == *s).unwrap_or(false),
        Target::Role(s) => role == *s,
    };
    if hit {
        return Ok(Some(elem.clone()));
    }
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        if let Some(found) = locate_walk(walker, &c, depth + 1, target, counter)? {
            return Ok(Some(found));
        }
        child = walker.get_next_sibling(&c).ok();
    }
    Ok(None)
}

/// click:UIA Invoke pattern 后台调用(不抢物理鼠标,06 §2.2)。
pub fn click(app: Option<&str>, target: &Target) -> Result<Located, CtlError> {
    let loc = locate(app, target)?;
    let pattern = loc.element.get_pattern::<UIInvokePattern>().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no Invoke pattern; scroll/focus first, or try `set-value` — physical fallback arrives with --physical (M2)",
                target.describe(),
                loc.role
            ),
        )
    })?;
    pattern.invoke().map_err(internal)?;
    Ok(loc)
}

/// get:读取元素属性;`property` 白名单,缺省返回基本全集。
pub const PROPS: &[&str] = &["name", "role", "automation_id", "offscreen", "value"];

pub struct PropertyInfo {
    pub name: Option<String>,
    pub role: String,
    pub automation_id: Option<String>,
    pub offscreen: Option<bool>,
    pub value: Option<String>,
}

pub fn read_property(
    app: Option<&str>,
    target: &Target,
    property: Option<&str>,
) -> Result<PropertyInfo, CtlError> {
    if let Some(p) = property {
        if !PROPS.contains(&p) {
            return Err(CtlError::protocol(format!(
                "unknown property {p:?}: expected one of {PROPS:?}"
            )));
        }
    }
    let loc = locate(app, target)?;
    let want = |p: &str| property.map(|sel| sel == p).unwrap_or(true);
    let value = want("value")
        .then(|| {
            loc.element
                .get_pattern::<UIValuePattern>()
                .ok()
                .and_then(|pat| pat.get_value().ok())
                .filter(|v| !v.is_empty())
        })
        .flatten();
    Ok(PropertyInfo {
        name: want("name").then_some(loc.name.clone()).flatten(),
        role: if want("role") {
            loc.role.clone()
        } else {
            String::new()
        },
        automation_id: want("automation_id")
            .then_some(loc.automation_id.clone())
            .flatten(),
        offscreen: want("offscreen")
            .then(|| loc.element.is_offscreen().ok())
            .flatten(),
        value,
    })
}

// ---------- 输入组(type / press / set-value)与窗口等待 ----------

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

/// type:定位 → SetFocus → 逐字键盘输入(触发输入事件;绕过键盘请用 set-value)。
pub fn type_text(app: Option<&str>, target: &Target, text: &str) -> Result<InputResult, CtlError> {
    let loc = locate(app, target)?;
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
    // 护栏与 press 相同:元素置焦后仍要物理前台核对,文本才允许注入
    if let Some(pattern) = app {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let fg = kbd::foreground_title().unwrap_or_default();
        if !fg.contains(pattern) {
            return Err(CtlError::new(
                ErrorCode::PermDenied,
                format!(
                    "text injection ABORTED by verify-then-inject guard: \
                     physical foreground is {fg:?}, expected a window matching {pattern:?}; \
                     no text was sent"
                ),
            ));
        }
    }
    // 自建管线(uiautomation crate 的 Keyboard 有 wScan=0 缺陷,见 kbd.rs 模块注释)
    let spec = KeySpec {
        modifiers: Vec::new(),
        keys: text.chars().map(actl_core::keys::Key::Char).collect(),
    };
    kbd::send_key_spec(&spec)?;
    Ok((&loc).into())
}

/// press:可选先置前台(--app)。实测链路(docs/spike-findings.md):窗口级 SetFocus
/// 只激活窗口、不建立键盘焦点(键会被丢);必须再对窗口内内容元素做**元素级** SetFocus。
/// 启发式:窗口内首个 Document/Edit;找不到(如纯按钮面板)退化为仅窗口前置。
///
/// **verify-then-inject 护栏(事故教训,docs/spike-findings.md #6)**:注入前用 Win32
/// 物理前台(GetForegroundWindow)验证前台确为目标窗口——UIA 焦点视图可能与物理前台
/// 不一致,"看起来聚焦了"不等于键会进目标。不匹配则拒绝注入,宁可失败不可错发。
pub fn press_keys(spec: &KeySpec, app: Option<&str>) -> Result<Option<String>, CtlError> {
    let focused = match app {
        Some(pattern) => {
            let title = focus_window(pattern)?;
            std::thread::sleep(std::time::Duration::from_millis(120));
            // 元素聚焦是异步请求且存在竞态(实测固定延时忽好忽坏):
            // 反复 SetFocus 并验证 UIA 焦点元素真的变为可编辑角色,最多 3 轮
            ensure_element_focus(pattern)?;
            // 护栏:物理前台必须是目标窗口,否则注入会打进别处(已发生过真实事故)
            let fg = kbd::foreground_title().unwrap_or_default();
            if !fg.contains(pattern) {
                return Err(CtlError::new(
                    ErrorCode::PermDenied,
                    format!(
                        "keyboard injection ABORTED by verify-then-inject guard: \
                         physical foreground is {fg:?}, expected a window matching {pattern:?}; \
                         no keys were sent",
                    ),
                ));
            }
            Some(title)
        }
        None => None,
    };
    kbd::send_key_spec(spec)?;
    Ok(focused)
}

/// 反复对窗口内可编辑元素 SetFocus,直到 UIA 焦点元素确为 Document/Edit(SetFocus
/// 是异步请求,单次调用+固定延时存在竞态)。窗口无可编辑元素时(纯按钮面板)直接通过。
fn ensure_element_focus(pattern: &str) -> Result<(), CtlError> {
    for _ in 0..3 {
        if let Some(edit) = find_editable_element(pattern) {
            let _ = edit.set_focus();
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        if let Ok(auto) = UIAutomation::new() {
            if let Ok(f) = auto.get_focused_element() {
                if let Ok(ct) = f.get_control_type() {
                    let role = format!("{ct:?}");
                    if role == "Document" || role == "Edit" {
                        return Ok(());
                    }
                }
            }
        }
    }
    // 无可编辑元素的窗口(计算器等)不算失败:窗口前置 + 护栏已足够
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
pub fn set_value(app: Option<&str>, target: &Target, value: &str) -> Result<InputResult, CtlError> {
    let loc = locate(app, target)?;
    let pattern = loc.element.get_pattern::<UIValuePattern>().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no Value pattern; try `type` (keyboard path) instead",
                target.describe(),
                loc.role
            ),
        )
    })?;
    pattern.set_value(value).map_err(internal)?;
    Ok((&loc).into())
}

/// 等待标题包含 `substr` 的顶层窗口出现(--expect 的执行体,06 §4:默认 2s 轮询)。
pub fn wait_window(substr: &str, timeout_ms: u64) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Some(title) = scan_top_windows(&auto, &walker, substr) {
            return Ok(title);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::AssertionFailed,
                format!(
                    "expected window containing {substr:?} did not appear within {timeout_ms}ms"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// 当前全部顶层窗口(含 owned dialog)的 UIA RuntimeId——--expect 的"新窗口"基线。
/// 用 RuntimeId 而非标题:标题是可变身份(如记事本脏标记"*无标题"、保存后改名),
/// 按标题比对会把标题变化的既有窗口误判为"新窗口"(实测踩坑)。
pub fn window_identities() -> Result<Vec<Vec<i32>>, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    Ok(top_windows(&auto, &walker)
        .into_iter()
        .filter_map(|w| w.get_runtime_id().ok())
        .collect())
}

/// 等待一个**基线中不存在**、标题包含 `substr` 的新窗口出现。
/// --expect 的后置断言语义:证明"本动作导致了新窗口",而不是"存在同名窗口"
/// (后者会被预先打开的同名窗口恒真欺骗,doc 09 §2)。
/// 基线按 RuntimeId 比对:同名新窗口能通过,标题变化的既有窗口不会误判。
pub fn wait_new_window(
    substr: &str,
    timeout_ms: u64,
    baseline: &[Vec<i32>],
) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Some(title) = top_windows(&auto, &walker)
            .into_iter()
            .filter(|w| name_matches(w, substr))
            .find(|w| {
                w.get_runtime_id()
                    .map(|id| !baseline.contains(&id))
                    .unwrap_or(false) // 读不到身份 → 无法证明"新",不当作命中
            })
            .and_then(|w| w.get_name().ok())
        {
            return Ok(title);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::AssertionFailed,
                format!(
                    "no NEW window containing {substr:?} appeared within {timeout_ms}ms \
                     (a pre-existing window with that title does not satisfy --expect)"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn scan_top_windows(auto: &UIAutomation, walker: &UITreeWalker, substr: &str) -> Option<String> {
    scan_top_window(auto, walker, substr).and_then(|w| w.get_name().ok())
}

/// focus-window:把标题匹配的顶层窗口带到前台(UIA SetFocus,即等效前台)。
/// press/type 等键盘命令作用于全局焦点——多窗口场景必须先 focus-window(06 §3.6 裁定表)。
pub fn focus_window(pattern: &str) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    win.set_focus().map_err(|e| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!("cannot focus window {pattern:?}: {e:?}"),
        )
    })?;
    Ok(win.get_name().unwrap_or_default())
}

/// list-windows:枚举顶层窗口 + owned dialog(title/class/pid),供 agent 选择目标。
pub fn list_windows() -> Result<Vec<(String, String, u32)>, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    Ok(top_windows(&auto, &walker)
        .into_iter()
        .filter_map(|w| {
            let title = w.get_name().ok()?;
            if title.is_empty() {
                return None;
            }
            Some((
                title,
                w.get_classname().unwrap_or_default(),
                w.get_process_id().unwrap_or_default(),
            ))
        })
        .collect())
}
