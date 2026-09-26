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
