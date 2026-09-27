//! pointer —— 物理指针动作(--physical 语义,doc 09 §6 七步链):
//! click/right-click/double-click/hover/drag/wheel + scroll 的物理兜底。
//! UIA 语义等价物见 `action`(click)与本模块 scroll_element 的 Scroll pattern 分支。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIAutomation;

use crate::locate::{Located, locate};
use crate::window::{find_window, native_hwnd, window_title_of};
use crate::{input, internal, kbd, mouse, timing};

/// 指针物理动作的参数化模型(PointerSpec,键盘侧 KeySpec 的镜像):
/// 按钮 × 点击数 × 修饰键组合的有限空间,新增操作 = 新数据而非新实现分支。
/// 修饰键与点击原子组合(按下修饰 → 点击 → 释放,自拥有自释放,同 type 纪律)。
pub enum PointerSpec {
    Click {
        button: MouseButton,
        clicks: u8,
        /// 原子持有的修饰键(ctrl+click 多选 / shift+click 范围选)
        modifiers: Vec<String>,
    },
    Hover,
    Drag {
        to: (i32, i32),
    },
    Wheel {
        notches: i32,
    },
}

pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// 兼容层:旧扁平变体的构造便捷(PointerSpec 的数据糖)。
pub enum PointerAction {
    LeftClick,
    RightClick,
    DoubleClick,
    TripleClick,
    MiddleClick,
    ClickWith { modifiers: Vec<String> },
    Hover,
    Drag { to: (i32, i32) },
    Wheel { notches: i32 },
}

impl PointerAction {
    pub fn to_spec(self) -> PointerSpec {
        match self {
            PointerAction::LeftClick => PointerSpec::Click {
                button: MouseButton::Left,
                clicks: 1,
                modifiers: vec![],
            },
            PointerAction::RightClick => PointerSpec::Click {
                button: MouseButton::Right,
                clicks: 1,
                modifiers: vec![],
            },
            PointerAction::DoubleClick => PointerSpec::Click {
                button: MouseButton::Left,
                clicks: 2,
                modifiers: vec![],
            },
            PointerAction::TripleClick => PointerSpec::Click {
                button: MouseButton::Left,
                clicks: 3,
                modifiers: vec![],
            },
            PointerAction::MiddleClick => PointerSpec::Click {
                button: MouseButton::Middle,
                clicks: 1,
                modifiers: vec![],
            },
            PointerAction::ClickWith { modifiers } => PointerSpec::Click {
                button: MouseButton::Left,
                clicks: 1,
                modifiers,
            },
            PointerAction::Hover => PointerSpec::Hover,
            PointerAction::Drag { to } => PointerSpec::Drag { to },
            PointerAction::Wheel { notches } => PointerSpec::Wheel { notches },
        }
    }
}

pub fn pointer_physical(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    action: PointerAction,
) -> Result<(Located, (i32, i32), bool), CtlError> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint};

    let loc = locate(app, target, near)?;
    let scrolled = ensure_visible(&loc.element);
    // clickable point 缺失时的受控退化:BoundingRectangle 中心 + 命中检查把关
    // (mmui/Qt 导航图标等不暴露 clickable point;命中检查不过仍拒,fail-closed 保留)
    let point = match loc.element.get_clickable_point().ok().flatten() {
        Some(p) => p,
        None => {
            let r = loc.element.get_bounding_rectangle().map_err(|_| {
                CtlError::new(
                    ErrorCode::NotActionable,
                    format!(
                        "{} ({}) exposes neither clickable point nor bounding rectangle",
                        target.describe(),
                        loc.role
                    ),
                )
            })?;
            uiautomation::types::Point::new(
                (r.get_left() + r.get_right()) / 2,
                (r.get_top() + r.get_bottom()) / 2,
            )
        }
    };
    let (px, py) = (point.get_x(), point.get_y());

    // 命中检查:坐标处最顶层窗口的根标题 == 目标窗口根标题。
    // ①目标侧从元素沿父链上溯到窗口根(WinUI 控件本身没有 HWND);
    // ②两侧取根标题比对而非严格 HWND——WinUI 三件套(框架/CoreWindow
    //   分属不同 HWND)会让同窗口的严格比对必然失败(实测)。
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    // 目标窗口 HWND:①从元素沿父链爬到第一个带句柄的元素;②部分 provider
    // 不暴露 NativeWindowHandle(实测记事本),按窗口标题兜底重解析
    let target_title = {
        let mut root = loc.element.clone();
        while native_hwnd(&root).is_none() {
            match walker.get_parent(&root) {
                Ok(p) => root = p,
                Err(_) => break,
            }
        }
        let title = native_hwnd(&root).and_then(window_title_of);
        match title {
            Some(t) => t,
            None => find_window(&auto, &walker, &loc.window_title)
                .ok()
                .and_then(|w| native_hwnd(&w))
                .and_then(window_title_of)
                .ok_or_else(|| {
                    CtlError::new(
                        ErrorCode::NotActionable,
                        "target window has no native handle for hit-checking",
                    )
                })?,
        }
    };
    let root_of = |hwnd: windows::Win32::Foundation::HWND| {
        let r = unsafe { GetAncestor(hwnd, GA_ROOT) };
        (!r.0.is_null()).then_some(r)
    };
    let hit = unsafe { WindowFromPoint(POINT { x: px, y: py }) };
    let hit_title = (!hit.0.is_null())
        .then_some(hit)
        .and_then(root_of)
        .and_then(window_title_of);
    if hit_title.as_deref() != Some(target_title.as_str()) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "clickable point ({px},{py}) is occupied by another window \
                 (occluded or minimized); refusing physical pointer action"
            ),
        ));
    }

    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    // 修饰键空置检查:ctrl 按住时的物理点击 = ctrl+click(语义劫持)
    kbd::wait_modifiers_clear()?;
    match action.to_spec() {
        PointerSpec::Click {
            button,
            clicks,
            modifiers,
        } => {
            // 修饰键与点击原子组合:按下修饰 → N 次点击 → 释放修饰,一批注入
            let mut events = kbd::modifier_hold_events(&modifiers, true)?;
            for _ in 0..clicks {
                events.extend(mouse::click_events_for(&button, px, py));
            }
            events.extend(kbd::modifier_hold_events(&modifiers, false)?);
            mouse::send_mixed(events)?;
        }
        PointerSpec::Hover => mouse::move_to(px, py)?,
        PointerSpec::Drag { to: (tx, ty) } => mouse::drag_to(px, py, tx, ty)?,
        PointerSpec::Wheel { notches } => mouse::wheel_at(px, py, notches)?,
    }
    Ok((loc, (px, py), scrolled))
}

/// offscreen 元素尝试 ScrollItemPattern::ScrollIntoView(P1,虚拟化定位配套)。
/// 返回是否执行过滚动;滚动失败静默跳过——由后续 clickable point / 命中检查
/// 如实失败,不做"看不见也硬打"的猜测。滚动后等一拍让容器重绘再取坐标。
fn ensure_visible(elem: &uiautomation::UIElement) -> bool {
    use uiautomation::patterns::UIScrollItemPattern;

    if !elem.is_offscreen().unwrap_or(false) {
        return false;
    }
    let scrolled = elem
        .get_pattern::<UIScrollItemPattern>()
        .and_then(|p| p.scroll_into_view())
        .is_ok();
    if scrolled {
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
    scrolled
}

/// click --physical 的便捷入口(物理左键)。
pub fn click_physical(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<(Located, (i32, i32), bool), CtlError> {
    pointer_physical(app, target, near, PointerAction::LeftClick)
}

/// 拖拽:from/to 各自定位取 clickable point,from 侧命中检查后一批注入。
pub fn drag_physical(
    app: Option<&str>,
    from: &Target,
    from_near: Option<&str>,
    to: &Target,
    to_near: Option<&str>,
) -> Result<(Located, (i32, i32), bool), CtlError> {
    let (_, (fx, fy), from_scrolled) =
        pointer_physical(app, from, from_near, PointerAction::Hover)?;
    let to_loc = locate(app, to, to_near)?;
    let to_scrolled = ensure_visible(&to_loc.element);
    let Some(dest) = to_loc.element.get_clickable_point().ok().flatten() else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} exposes no clickable point for drag destination",
                to.describe()
            ),
        ));
    };
    let point = (dest.get_x(), dest.get_y());
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    mouse::drag_to(fx, fy, point.0, point.1)?;
    Ok((
        locate(app, from, from_near)?,
        (fx, fy),
        from_scrolled || to_scrolled,
    ))
}

/// scroll:UIA Scroll pattern 优先(容器语义,后台);无 pattern → 物理滚轮。
/// 返回 (via, 行数) 供上报。
pub fn scroll_element(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    notches: i32,
) -> Result<(Located, &'static str), CtlError> {
    use uiautomation::patterns::UIScrollPattern;
    use uiautomation::types::ScrollAmount;

    let loc = locate(app, target, near)?;
    if let Ok(scroll) = loc.element.get_pattern::<UIScrollPattern>() {
        let amount = if notches >= 0 {
            ScrollAmount::SmallIncrement
        } else {
            ScrollAmount::SmallDecrement
        };
        // pattern 宣称支持但调用失败(实测记事本 Document)→ 物理滚轮兜底
        let mut ok = true;
        for _ in 0..notches.abs() {
            if scroll
                .scroll(uiautomation::types::ScrollAmount::NoAmount, amount)
                .is_err()
            {
                ok = false;
                break;
            }
        }
        if ok {
            return Ok((loc, "uia-scroll"));
        }
    }
    // 物理兜底:滚轮打在元素 clickable point 上
    let _ = pointer_physical(app, target, near, PointerAction::Wheel { notches })?;
    Ok((loc, "physical-wheel"))
}
