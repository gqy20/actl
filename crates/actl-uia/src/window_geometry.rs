//! window_geometry —— 窗口几何写面(2026-10-13 窗口几何轮,自 window.rs 拆出:
//! 后者顶线 file-lines DENY 1500)。读面在 read.rs(`get --property rect`),
//! 本模块是对称写面:rect 部分字段语义(Selenium setRect 同款),读写零单位
//! 换算(全物理像素,Fix A 进程级 PMv2 兑现)。

use actl_core::{CtlError, ErrorCode};

use crate::window::{WindowRef, native_hwnd, resolve_window};

/// resize-window 的结果面(窗口标题;坐标参数由 CLI 回显,不重复携带)。
pub struct ResizeOutcome {
    pub window_title: String,
}

/// resize-window:窗口几何写面(x/y/w/h 部分字段语义,Selenium setRect 同款)。
/// 锚=WindowRef(标题/PID/HWND 三形态,标题漂移后 --hwnd 仍稳);--x/--y
/// 缺省=保左上角(向后兼容),全给=几何读写闭环——`get --property rect`
/// 输出改一改就是本命令输入,零单位换算。最小化窗口先还原(Selenium
/// 同款语义:SetWindowPos 对 iconic 窗口行为未定义);尺寸下限 120 防零尺寸不可逆。
pub fn resize_window_rect(
    r: WindowRef<'_>,
    width: i32,
    height: i32,
    x: Option<i32>,
    y: Option<i32>,
) -> Result<ResizeOutcome, CtlError> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, IsIconic, SW_RESTORE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        SetWindowPos, ShowWindow,
    };

    let win = resolve_window(r)?;
    let mut action = crate::feedback::window_action("resize-window", &win, false)?;
    let title = win.get_name().unwrap_or_default();
    let Some(hwnd) = native_hwnd(&win) else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "window has no native handle",
        ));
    };
    // 最小化先还原:iconic 窗口的 SetWindowPos 行为未定义(Selenium set size
    // "Restores the window and sets the window size" 同款语义);失败如实传播
    if unsafe { IsIconic(hwnd) }.as_bool() {
        unsafe { ShowWindow(hwnd, SW_RESTORE) }
            .ok()
            .map_err(crate::internal)?;
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|e| CtlError::internal(format!("GetWindowRect: {e}")))?;
    // 部分字段:x/y 给了用新的,缺省保持现值(SWP_NOMOVE 全给时不带)
    let nx = x.unwrap_or(rect.left);
    let ny = y.unwrap_or(rect.top);
    let keep_move = x.is_none() && y.is_none();
    let w = width.max(120);
    let h = height.max(120);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND(std::ptr::null_mut())),
            nx,
            ny,
            w,
            h,
            if keep_move {
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
            } else {
                SWP_NOZORDER | SWP_NOACTIVATE
            },
        )
    }
    .map_err(|e| CtlError::internal(format!("SetWindowPos: {e}")))?;
    action.delivered();
    Ok(ResizeOutcome {
        window_title: title,
    })
}
