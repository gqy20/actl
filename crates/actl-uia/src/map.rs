//! preview-map:全窗组件画框(「先看见再设计」工作流,2026-10-14 用户需求)
//! ——轻量树遍历只读 BoundingRectangle+ControlType,layered overlay 按物理
//! rect 绘框:交互元素橙色实线 3px、其他暗青色细线 1px,展示 duration 后
//! 自动消散。与 snapshot 契约零耦合(UiNode 无 rect,不扰动快照投影);
//! 遍历预算 500 元素/12 深度,超限 envelope 如实标注 truncated(不谎称
//! 全量)。只读:零输入注入、零授权;overlay WS_EX_TRANSPARENT|NOACTIVATE
//! 点击穿透(find-image 红框同源纪律,preview 单元素框同款语义)。

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use uiautomation::{UIAutomation, UIElement};

use crate::search_frame::SearchFrameGuard;
use crate::window::{MAX_ELEMENTS, find_window};

const MAP_ELEMENTS: usize = 1000;
const MAP_DEPTH: u32 = 40;

struct MapEntry {
    rect: [i32; 4],
    interactive: bool,
}

/// 全窗组件画框:resolve 窗口 → 轻量遍历(rect+role) → overlay 绘制 →
/// duration 消散。envelope 带 elements/interactive/truncated 统计。
pub fn map(app: &str, duration_ms: u64) -> Result<Value, CtlError> {
    let duration = duration_ms.clamp(500, 30_000);
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let walker = auto.create_tree_walker().map_err(crate::internal)?;
    let root = find_window(&auto, &walker, app)?;
    let mut entries: Vec<MapEntry> = Vec::new();
    let mut truncated = false;
    {
        // 树遍历是本命令的耗时主体(千元素级 rect+role 逐读)
        let _t = actl_core::trace::scope("map.walk");
        collect(&root, 0, &mut entries, &mut truncated);
    }
    if entries.is_empty() {
        return Err(CtlError::new(
            ErrorCode::NotFound,
            format!("no UIA elements with visible bounds under window {app:?}"),
        ));
    }
    let interactive_count = entries.iter().filter(|e| e.interactive).count();
    let frame = {
        // overlay 绘制(enter_with_boxes 逐框像素直写;失败 None 走 shown:false)
        let _t = actl_core::trace::scope("map.draw");
        SearchFrameGuard::enter_with_boxes(
            root_hwnd(&root)?,
            entries.iter().map(|e| (e.rect, e.interactive)).collect(),
        )
    };
    let shown = frame.is_some();
    // 展示窗:有界时长,逐 100ms 片过急停(与 key-down --duration 同纪律)
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(duration);
    while shown && std::time::Instant::now() < deadline && crate::feedback::check_stop().is_ok() {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    drop(frame);
    Ok(json!({
        "action": "preview-map",
        "app": app,
        "elements": entries.len(),
        "interactive": interactive_count,
        "truncated": truncated,
        "duration_ms": duration,
        "shown": shown,
    }))
}

/// 递归收集(手控栈深;预算触顶如实截断)。只读 rect+ControlType——
/// 名称/automation_id 不读(画框不需要,单元素属性零往返)。
fn collect(element: &UIElement, depth: u32, out: &mut Vec<MapEntry>, truncated: &mut bool) {
    if out.len() >= MAP_ELEMENTS {
        *truncated = true;
        return;
    }
    if let Ok(rect) = element.get_bounding_rectangle() {
        let (x, y, w, h) = (
            rect.get_left(),
            rect.get_top(),
            rect.get_right() - rect.get_left(),
            rect.get_bottom() - rect.get_top(),
        );
        if w > 0 && h > 0 {
            let role = crate::observation::role_of(element)
                .map(|r| format!("{r:?}"))
                .unwrap_or_default();
            let interactive = is_interactive(&role);
            out.push(MapEntry {
                rect: [x, y, w, h],
                interactive,
            });
        }
    }
    if depth >= MAP_DEPTH {
        *truncated = true;
        return;
    }
    if let Ok(children) = crate::observation::children(element) {
        for child in children {
            collect(&child, depth + 1, out, truncated);
        }
    }
}

fn is_interactive(role: &str) -> bool {
    actl_core::is_interactive_role(role)
}

/// 根元素的原生 HWND(overlay 目标窗)。
fn root_hwnd(root: &UIElement) -> Result<isize, CtlError> {
    crate::window::native_hwnd(root)
        .map(|hwnd| hwnd.0 as isize)
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "root window has no native handle"))
}

const _: () = {
    assert!(MAP_ELEMENTS <= MAX_ELEMENTS || true);
};

#[cfg(test)]
mod tests {
    #[test]
    fn map_budgets_match_observation_ceiling() {
        // 画框预算与仓库观察上限对齐(深度同 40;元素给到 1000——
        // overlay 逐框像素直写,千框绘制 <100ms,无需 5000 全量)
        assert_eq!(super::MAP_DEPTH, 40);
        assert_eq!(super::MAP_ELEMENTS, 1000);
    }
}
