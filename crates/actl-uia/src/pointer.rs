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
    no_activate: bool,
) -> Result<(Located, (i32, i32), bool), CtlError> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint};

    crate::feedback::check_stop()?;
    let _dpi = crate::capture::Pmv2Guard::enter();
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
    // 目标窗口标题 + 根 HWND:①从元素沿父链爬到第一个带句柄的元素;②部分
    // provider 不暴露 NativeWindowHandle(实测记事本),按窗口标题兜底重解析。
    // hwnd 供 --no-activate 的样式守护使用(取 GA_ROOT 根)。
    let (target_title, target_hwnd) = {
        let mut root = loc.element.clone();
        while native_hwnd(&root).is_none() {
            match walker.get_parent(&root) {
                Ok(p) => root = p,
                Err(_) => break,
            }
        }
        let title = native_hwnd(&root).and_then(window_title_of);
        match title {
            Some(t) => (t, native_hwnd(&root)),
            None => {
                let w = find_window(&auto, &walker, &loc.window_title).ok();
                let hwnd = w.as_ref().and_then(native_hwnd);
                let title = hwnd.and_then(window_title_of).ok_or_else(|| {
                    CtlError::new(
                        ErrorCode::NotActionable,
                        "target window has no native handle for hit-checking",
                    )
                })?;
                (title, hwnd)
            }
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

    let bounds = crate::feedback::target(&loc.element, &loc.window_title).bounds;
    // --no-activate 的样式目标:命中检查用的窗口根(样式必须挂在根上才拦得住激活)
    let bg_hwnd = no_activate
        .then(|| {
            target_hwnd
                .map(|h| unsafe { GetAncestor(h, GA_ROOT) })
                .filter(|r| !r.0.is_null())
                .map(|r| r.0 as isize)
        })
        .flatten();
    inject_pointer_at(px, py, action.to_spec(), &loc.window_title, bg_hwnd, || {
        crate::feedback::check_stop()?;
        let hit = unsafe { WindowFromPoint(POINT { x: px, y: py }) };
        if root_of(hit).and_then(window_title_of).as_deref() != Some(target_title.as_str())
            || crate::feedback::target(&loc.element, &loc.window_title).bounds != bounds
        {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "pointer target changed during preparation",
            ));
        }
        Ok(())
    })?;
    Ok((loc, (px, py), scrolled))
}

/// 物理注入公共尾段:输入占用锁 → 修饰键空置检查 → 一批 SendInput。
/// 元素路径(pointer_physical)与坐标路径(pointer_at_physical)共用,
/// 锁与护栏语义不分叉。`bg_hwnd = Some(根 HWND)` 时启用 --no-activate:
/// 置 `WS_EX_NOACTIVATE` 后注入,结束恢复(bg_style journal 兜底崩溃残留)。
fn inject_pointer_at(
    px: i32,
    py: i32,
    spec: PointerSpec,
    window: &str,
    bg_hwnd: Option<isize>,
    validate: impl Fn() -> Result<(), CtlError>,
) -> Result<(), CtlError> {
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    // 修饰键空置检查:ctrl 按住时的物理点击 = ctrl+click(语义劫持)
    kbd::wait_modifiers_clear()?;
    let (kind, keyboard, destination) = match &spec {
        PointerSpec::Click { modifiers, .. } => ("click", !modifiers.is_empty(), None),
        PointerSpec::Hover => ("hover", false, None),
        PointerSpec::Drag { to } => ("drag", false, Some([to.0, to.1])),
        PointerSpec::Wheel { .. } => ("scroll", false, None),
    };
    let mut action = crate::feedback::prepare(
        kind,
        actl_core::activity::InputEffects {
            pointer: true,
            keyboard,
            ..Default::default()
        },
        actl_core::activity::ActionTarget {
            window: Some(window.into()),
            point: Some([px, py]),
            destination,
            ..Default::default()
        },
    )?;
    validate()?;
    kbd::wait_modifiers_clear()?;
    validate()?;
    // --no-activate:两道复核全过后才置位(最小化暴露窗口)。守护声明于
    // InputLock 之后 → Drop 先于锁释放,恢复发生在持锁段内。注入会把光标
    // 移到点击点,非 drag 动作完成后送回原位(用户桌面整洁;恢复失败
    // 不影响已完成的动作)。
    let _bg = bg_hwnd
        .map(|h| {
            crate::bg_style::NoActivateGuard::enter(h, &actl_core::state::SignalPaths::default())
        })
        .transpose()?;
    let saved_cursor = bg_hwnd.is_some().then(mouse::cursor_pos).flatten();
    let is_drag = matches!(spec, PointerSpec::Drag { .. });
    action.executing();
    match spec {
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
    action.delivered();
    // --no-activate 的收尾:把光标送回用户原位(drag 的落点有语义,不送回;
    // 送回失败不影响已完成的动作,静默)。
    if let (Some((sx, sy)), false) = (saved_cursor, is_drag) {
        let _ = mouse::move_to(sx, sy);
    }
    Ok(())
}

/// 坐标点击(xy: 目标)的结果上报。
pub struct CoordOutcome {
    pub window_title: String,
    pub point: (i32, i32),
}

/// 坐标物理动作(xy: 目标,视觉兜底的"手"):无元素定位,窗口边界守卫 +
/// 命中检查后注入。坐标来自调用方(screenshot+VLM/人),core 不做视觉。
///
/// 守卫(比元素路径多一重边界检查——无语义目标可校验时的防漂移手段):
/// ① 全程 PMv2 线程上下文(与 screenshot 同坐标系,坐标即截图物理像素);
/// ② 坐标必须落在 --app 窗口**当前** bounds 内(DWM 可见边界优先)——
///    窗口移动/最小化即拒,不点窗外的东西;
/// ③ WindowFromPoint 命中窗口须属于目标(遮挡检查,与元素路径同款)。
pub fn pointer_at_physical(
    app: &str,
    x: i32,
    y: i32,
    action: PointerAction,
    no_activate: bool,
) -> Result<CoordOutcome, CtlError> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::DWMWA_EXTENDED_FRAME_BOUNDS;
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetWindowRect, IsIconic, WindowFromPoint,
    };

    crate::feedback::check_stop()?;
    let _dpi = crate::capture::Pmv2Guard::enter();
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, app)?;
    let title = win.get_name().unwrap_or_default();
    let hwnd = native_hwnd(&win).ok_or_else(|| {
        CtlError::new(
            ErrorCode::NotActionable,
            "target window has no native handle for the coordinate guard",
        )
    })?;
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "target window is minimized - restore it before coordinate clicks",
        ));
    }
    // bounds:DWM 可见边界优先(最大化窗口的 GetWindowRect 含屏外阴影),
    // 与 screenshot --app 的裁剪口径一致
    let mut r = RECT::default();
    let dwm_ok = unsafe {
        windows::Win32::Graphics::Dwm::DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut r as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
    }
    .is_ok()
        && r.right > r.left;
    if !dwm_ok {
        unsafe { GetWindowRect(hwnd, &mut r) }
            .map_err(|e| CtlError::internal(format!("GetWindowRect: {e}")))?;
    }
    if x < r.left || x >= r.right || y < r.top || y >= r.bottom {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "point ({x},{y}) lies outside target window bounds [{},{}) x [{},{}) - the window may have moved; re-screenshot and recompute the coordinate",
                r.left, r.right, r.top, r.bottom
            ),
        ));
    }
    // 命中检查:坐标处最顶层窗口的根标题 == 目标窗口标题(遮挡拒绝)
    let hit = unsafe { WindowFromPoint(POINT { x, y }) };
    let hit_title = (!hit.0.is_null()).then_some(hit).and_then(|h| {
        let root = unsafe { GetAncestor(h, GA_ROOT) };
        (!root.0.is_null())
            .then_some(root)
            .and_then(window_title_of)
    });
    if hit_title.as_deref() != Some(title.as_str()) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "point ({x},{y}) is occupied by another window (occluded); refusing                  physical pointer action",
            ),
        ));
    }
    let mut before = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut before) }.map_err(internal)?;
    // --no-activate 的样式目标:坐标守卫的窗口根
    let bg_hwnd = no_activate
        .then(|| {
            let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
            (!root.0.is_null()).then_some(root.0 as isize)
        })
        .flatten();
    inject_pointer_at(x, y, action.to_spec(), &title, bg_hwnd, || {
        crate::feedback::check_stop()?;
        let mut current = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut current) }.map_err(internal)?;
        let hit = unsafe { WindowFromPoint(POINT { x, y }) };
        let hit_title = window_title_of(unsafe { GetAncestor(hit, GA_ROOT) });
        if unsafe { IsIconic(hwnd) }.as_bool()
            || current != before
            || x < current.left
            || x >= current.right
            || y < current.top
            || y >= current.bottom
            || hit_title.as_deref() != Some(title.as_str())
        {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "coordinate target changed during preparation",
            ));
        }
        Ok(())
    })?;
    Ok(CoordOutcome {
        window_title: title,
        point: (x, y),
    })
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
    pointer_physical(app, target, near, PointerAction::LeftClick, false)
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
        pointer_physical(app, from, from_near, PointerAction::Hover, false)?;
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
    let source = locate(app, from, from_near)?;
    let before = crate::feedback::target(&source.element, &source.window_title).bounds;
    let dest_before = crate::feedback::target(&to_loc.element, &to_loc.window_title).bounds;
    inject_pointer_at(
        fx,
        fy,
        PointerSpec::Drag { to: point },
        &source.window_title,
        None,
        || {
            use windows::Win32::Foundation::POINT;
            use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint};
            let hit = unsafe { WindowFromPoint(POINT { x: fx, y: fy }) };
            let end = unsafe {
                WindowFromPoint(POINT {
                    x: point.0,
                    y: point.1,
                })
            };
            if window_title_of(unsafe { GetAncestor(hit, GA_ROOT) }).as_deref()
                != Some(source.window_title.as_str())
                || window_title_of(unsafe { GetAncestor(end, GA_ROOT) }).as_deref()
                    != Some(to_loc.window_title.as_str())
                || crate::feedback::target(&source.element, &source.window_title).bounds != before
                || crate::feedback::target(&to_loc.element, &to_loc.window_title).bounds
                    != dest_before
            {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "drag target changed during preparation",
                ));
            }
            crate::feedback::check_stop()
        },
    )?;
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
        let mut feedback = actl_core::signal_session::ActionGuard::begin(
            "scroll",
            Default::default(),
            crate::feedback::target(&loc.element, &loc.window_title),
        );
        crate::feedback::check_stop()?;
        feedback.executing();
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
            feedback.delivered();
            return Ok((loc, "uia-scroll"));
        }
    }
    // 物理兜底:滚轮打在元素 clickable point 上
    let _ = pointer_physical(app, target, near, PointerAction::Wheel { notches }, false)?;
    Ok((loc, "physical-wheel"))
}
