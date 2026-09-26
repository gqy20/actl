//! 物理鼠标注入(doc 09 §6 的执行层;unsafe 豁免点之一,AGENTS.md §5.5)。
//!
//! 规范裁定(06 §2 规则 2 修订):键盘命令的授权 = 强制 `--app` + 护栏 + 输入
//! 占用锁;**指针物理注入必须显式 `--physical`**——它独占用户的物理鼠标,
//! 与键盘不同量级。坐标语义:虚拟桌面归一化(VIRTUALDESK,0-65535 映射整个
//! 虚拟桌面),正确处理多屏与负坐标;绝不默认主屏。

use actl_core::CtlError;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// 物理屏幕像素 → 虚拟桌面归一化 0-65535(双精度→整型的最后一次舍入)。
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

fn mouse_event(flags: MOUSE_EVENT_FLAGS, nx: i32, ny: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: nx,
                dy: ny,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 物理左键点击:移动到 (x, y)(物理屏幕像素)→ 按下 → 抬起。
/// 三事件一批:原子性最大化,缩小"检查后窗口移动"的竞态窗。
pub fn left_click_at(x: i32, y: i32) -> Result<(), CtlError> {
    let (nx, ny) = normalize(x, y);
    let inputs = [
        mouse_event(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            nx,
            ny,
        ),
        mouse_event(
            MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            nx,
            ny,
        ),
        mouse_event(MOUSEEVENTF_LEFTUP, nx, ny),
    ];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(CtlError::internal(format!(
            "SendInput(mouse) delivered {sent}/{} events",
            inputs.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_maps_primary_origin_to_zero() {
        // 虚拟桌面原点(常为负,主屏左侧有副屏时)映射到 0
        unsafe {
            let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let (nx, ny) = normalize(vx, vy);
            assert_eq!((nx, ny), (0, 0));
            // 右下角 ≈ 65535
            let (fx, fy) = normalize(
                vx + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                vy + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            );
            assert!((fx - 65535).abs() <= 1 && (fy - 65535).abs() <= 1);
        }
    }
}
