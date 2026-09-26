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
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
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
                dwExtraInfo: 0,
            },
        },
    }
}

/// 单批注入:原子性最大化,缩小"检查后窗口移动"的竞态窗。
fn send_all(inputs: &[INPUT]) -> Result<(), CtlError> {
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
        let _ = send_all(&[mouse_event(MOUSEEVENTF_LEFTUP, bx, by, 0)]);
    }
    result
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

#[cfg(test)]
mod tests {
    use super::*;

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
