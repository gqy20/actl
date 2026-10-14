//! 物理鼠标注入(doc 09 §6 的执行层;unsafe 豁免点之一,AGENTS.md §5.5)。
//!
//! 规范裁定(06 §2 规则 2 修订):键盘命令的授权 = 强制 `--app` + 护栏 + 输入
//! 占用锁;**指针物理注入必须显式声明**——`click --physical` 用 flag,而
//! right-click/double-click/hover/drag 无 UIA 语义等价物,命名即声明。
//! 坐标语义:虚拟桌面归一化(VIRTUALDESK,0-65535 映射整个虚拟桌面),
//! 正确处理多屏与负坐标;绝不默认主屏。

use actl_core::CtlError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
    MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// WHEEL_DELTA:一格滚轮 = 120;正值向上(远离用户),负值向下。
const WHEEL_DELTA: i32 = 120;

/// 物理屏幕像素 → 虚拟桌面归一化 0-65535。
fn normalize(x: i32, y: i32) -> (i32, i32) {
    unsafe {
        let (vx, vy, vw, vh) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        if vw <= 0 || vh <= 0 {
            return (0, 0);
        }
        let nx = ((x - vx) as f64 * 65535.0 / vw as f64).round() as i32;
        let ny = ((y - vy) as f64 * 65535.0 / vh as f64).round() as i32;
        (nx.clamp(0, 65535), ny.clamp(0, 65535))
    }
}

fn mouse_event(flags: MOUSE_EVENT_FLAGS, nx: i32, ny: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: actl_core::handoff::INPUT_TAG,
            },
        },
    }
}

/// 单批注入:原子性最大化,缩小"检查后窗口移动"的竞态窗。
fn send_all(inputs: &[INPUT]) -> Result<(), CtlError> {
    crate::feedback::check_stop()?;
    send_raw(inputs)
}

// Cleanup releases remain possible after consent was revoked.
fn send_raw(inputs: &[INPUT]) -> Result<(), CtlError> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(CtlError::internal(format!(
            "SendInput(mouse) delivered {sent}/{} events; pointer state is now unknown",
            inputs.len()
        )))
    }
}

fn abs(flags: MOUSE_EVENT_FLAGS) -> MOUSE_EVENT_FLAGS {
    flags | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK
}

/// 物理左键点击:移动 → 按下 → 抬起。
pub fn left_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
    ])
}

/// 物理右键点击(弹出上下文菜单)。
pub fn right_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_RIGHTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_RIGHTUP, nx, ny, 0),
    ])
}

/// 物理双击(同批两次按压,系统按双击时距识别)。
pub fn double_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
    ])
}

/// 仅移动指针(hover:触发悬停效果,不驻留)。
pub fn move_to(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0)])
}

/// 当前光标物理坐标(屏幕系;--no-activate 动作后送回原位用)。
pub fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT::default();
    // SAFETY: 出参写入门,无前置条件。
    unsafe { GetCursorPos(&mut pt) }.ok().map(|_| (pt.x, pt.y))
}

/// 物理拖拽:按住左键从 (x1,y1) 移到 (x2,y2) 再松开。
/// 拖拽期间目标可能移动/中途取消;失败时尽力补发 LEFTUP 释放按键(doc 09 §6)。
pub fn drag_to(x1: i32, y1: i32, x2: i32, y2: i32) -> Result<(), CtlError> {
    let (ax, ay) = normalize(x1, y1);
    let (bx, by) = normalize(x2, y2);
    let result = send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), ax, ay, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), ax, ay, 0),
        mouse_event(abs(MOUSEEVENTF_MOVE), bx, by, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, bx, by, 0),
    ]);
    if result.is_err() {
        // 按钮可能仍被按住:尽力释放,结果仍如实上报失败
        let _ = send_raw(&[mouse_event(MOUSEEVENTF_LEFTUP, bx, by, 0)]);
    }
    result
}

/// 轨迹插值的步长(物理像素):画布/签名类应用按 MOVE 事件采样轨迹,
/// 过粗丢细节、过细撑大批次;8px 是肉眼平滑与批次规模的折中。
const PATH_STEP_PX: i32 = 8;

/// 路径点插值(纯函数,离线可测):相邻点间按 PATH_STEP_PX 线性补齐,端点保持精确。
pub(crate) fn interpolate_path(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let Some(&first) = points.first() else {
        return out;
    };
    out.push(first);
    for i in 1..points.len() {
        let (a, b) = (points[i - 1], points[i]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let dist = (dx as f64).hypot(dy as f64).ceil() as i32;
        let steps = (dist / PATH_STEP_PX).max(1);
        for s in 1..=steps {
            let t = s as f64 / steps as f64;
            out.push((
                a.0 + (dx as f64 * t).round() as i32,
                a.1 + (dy as f64 * t).round() as i32,
            ));
        }
    }
    out
}

/// 轨迹拖拽:按下→沿路径点(绝对物理像素,内部按 8px 步长插值)移动→抬起。
/// 失败时尽力补发 LEFTUP(与 drag_to 同纪律)。
pub fn drag_path(points: &[(i32, i32)]) -> Result<(), CtlError> {
    let Some(&(sx, sy)) = points.first() else {
        return Err(CtlError::protocol("drag path requires at least one point"));
    };
    let Some(&(ex, ey)) = points.last() else {
        return Err(CtlError::protocol("drag path requires at least one point"));
    };
    let interpolated = interpolate_path(points);
    if interpolated.len() > 32_768 {
        return Err(CtlError::protocol(format!(
            "drag path interpolates to {} move events (max 32768); reduce waypoints or span",
            interpolated.len()
        )));
    }
    let (nsx, nsy) = normalize(sx, sy);
    let mut events = vec![
        mouse_event(abs(MOUSEEVENTF_MOVE), nsx, nsy, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nsx, nsy, 0),
    ];
    for (x, y) in &interpolated {
        let (nx, ny) = normalize(*x, *y);
        events.push(mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0));
    }
    let (nex, ney) = normalize(ex, ey);
    events.push(mouse_event(MOUSEEVENTF_LEFTUP, nex, ney, 0));
    let result = send_all(&events);
    if result.is_err() {
        let _ = send_raw(&[mouse_event(MOUSEEVENTF_LEFTUP, nex, ney, 0)]);
    }
    result
}

/// 定时轨迹拖拽(--human):按下→按 `TimedPoint` 时间轴逐事件移动→抬起。
/// 与 drag_path 的整批直发不同,**事件间隔按时间轴睡眠**——速度剖面
/// (行为指纹面)由时间轴携带;睡眠按 ≤50ms 分片,每片前过停止代次核验
/// (key-down --duration 同纪律):急停即抬起已按下的键并报 ABORTED
/// (消息如实说明已释放)。
pub fn drag_timed(points: &[actl_core::trajectory::TimedPoint]) -> Result<(), CtlError> {
    let Some(&first) = points.first() else {
        return Err(CtlError::protocol("timed drag requires at least one point"));
    };
    let Some(&last) = points.last() else {
        return Err(CtlError::protocol("timed drag requires at least one point"));
    };
    let (fx, fy) = normalize(first.x, first.y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), fx, fy, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), fx, fy, 0),
    ])?;
    let release = |x: i32, y: i32| {
        let (nx, ny) = normalize(x, y);
        let _ = send_raw(&[mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0)]);
    };
    let mut last_ms = 0u32;
    for point in points {
        let wait = point.t_ms.saturating_sub(last_ms);
        last_ms = point.t_ms;
        if let Err(error) = sleep_sliced_with_stop(wait) {
            release(last.x, last.y);
            return Err(error);
        }
        let (nx, ny) = normalize(point.x, point.y);
        if let Err(error) = send_all(&[mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0)]) {
            release(last.x, last.y);
            return Err(error);
        }
    }
    release(last.x, last.y);
    Ok(())
}

/// 分片睡眠(≤50ms 片,每片前急停核验):0ms 直接过(key-down --duration
/// 的 100ms 分片在此收紧到 50ms——轨迹事件间隔本身 5-40ms 量级)。
fn sleep_sliced_with_stop(total_ms: u32) -> Result<(), CtlError> {
    let mut remain = total_ms;
    while remain > 0 {
        actl_core::stop::check_current()?;
        let slice = remain.min(50);
        std::thread::sleep(std::time::Duration::from_millis(slice as u64));
        remain -= slice;
    }
    Ok(())
}

/// 滚轮:notches 为正向上滚、负向下滚(每格 120)。
pub fn wheel_at(x: i32, y: i32, notches: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(
            MOUSEEVENTF_WHEEL,
            nx,
            ny,
            notches.saturating_mul(WHEEL_DELTA),
        ),
    ])
}

/// 物理中键点击(浏览器:链接新标签;任务栏:新实例)。
pub fn middle_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_MIDDLEDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_MIDDLEUP, nx, ny, 0),
    ])
}

/// 物理三击(编辑器段落级选择:双击选词、三击选段;系统按三击时距识别)。
pub fn triple_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    let click = || mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0);
    let _ = click;
    send_all(&[
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
        mouse_event(abs(MOUSEEVENTF_LEFTDOWN), nx, ny, 0),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny, 0),
    ])
}

/// 修饰键+点击的混合事件批(PointerSpec::Click 的执行体)。
pub(crate) fn send_mixed(inputs: Vec<INPUT>) -> Result<(), CtlError> {
    send_all(&inputs)
}

/// 按钮点击事件片段(物理像素坐标;每次 = move+按下+抬起,多次由调用方循环)。
pub(crate) fn click_events_for(button: &crate::pointer::MouseButton, x: i32, y: i32) -> Vec<INPUT> {
    let (nx, ny) = normalize(x, y);
    let (down, up) = match button {
        crate::pointer::MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        crate::pointer::MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        crate::pointer::MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };
    vec![
        mouse_event(abs(MOUSEEVENTF_MOVE), nx, ny, 0),
        mouse_event(abs(down), nx, ny, 0),
        mouse_event(up, nx, ny, 0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_path_covers_endpoints_and_respects_step() {
        let pts = interpolate_path(&[(0, 0), (16, 0), (16, 8)]);
        assert_eq!(pts.first(), Some(&(0, 0)));
        assert_eq!(pts.last(), Some(&(16, 8)));
        // 0→16(2 步)→16,8(1 步):含首共 4 点,步长不超过 PATH_STEP_PX
        assert!(pts.len() >= 4 && pts.len() <= 6, "{pts:?}");
        for w in pts.windows(2) {
            let d = ((w[1].0 - w[0].0) as f64).hypot((w[1].1 - w[0].1) as f64);
            assert!(d <= PATH_STEP_PX as f64 + 1.0, "{w:?}");
        }
        // 单点/空路径:原样返回
        assert_eq!(interpolate_path(&[]), Vec::<(i32, i32)>::new());
        assert_eq!(interpolate_path(&[(5, 5)]), vec![(5, 5)]);
    }

    #[test]
    fn normalize_maps_primary_origin_to_zero() {
        unsafe {
            let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let (nx, ny) = normalize(vx, vy);
            assert_eq!((nx, ny), (0, 0));
            let (fx, fy) = normalize(
                vx + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                vy + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            );
            assert!((fx - 65535).abs() <= 1 && (fy - 65535).abs() <= 1);
        }
    }
}
