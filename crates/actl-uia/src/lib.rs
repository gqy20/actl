//! actl-uia —— Windows UIA 后端:capture(窗口定位 + DFS 遍历)。
//!
//! 全仓库唯一接触 COM 的 crate(AGENTS.md §5.5)。
//! 已知限制(spike #4,docs/spike-findings.md):walker 在"无子/无兄弟"时返回 Err,
//! 与真错误不可区分——当前以 `.ok()` 视为尽头;HRESULT 分类列入 M1 遗留。
//! 性能注记(spike #3):本版逐属性跨进程读取,release 下金任务实测,超 500ms 再上 cache request。

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
fn find_window(
    auto: &UIAutomation,
    walker: &UITreeWalker,
    pattern: &str,
) -> Result<UIElement, CtlError> {
    let root = auto.get_root_element().map_err(internal)?;
    let mut child = walker.get_first_child(&root).ok();
    while let Some(w) = child {
        if w.get_name().map(|n| n.contains(pattern)).unwrap_or(false) {
            return Ok(w);
        }
        child = walker.get_next_sibling(&w).ok();
    }
    Err(CtlError::new(
        ErrorCode::NotFound,
        format!("no top-level window title contains {pattern:?}"),
    ))
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
        None => top_level_of_focused(&auto, &walker)?,
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
