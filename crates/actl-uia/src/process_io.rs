//! 分离式子进程的 stdio 继承守卫(Win32 直调;unsafe 集中于本 crate,AGENTS §5.5)。
//!
//! 背景(docs/72):浏览器等 GUI 进程树会继承本进程 stdio 的管道写端——调用方
//! (agent 的管道捕获/$()/ReadToEnd)在 actl 退出后仍等不到 EOF(实测挂死;
//! `Stdio::null()` 三件套对 Edge 多子进程树不够)。CreateProcess 只传递带
//! INHERIT 标志的句柄,故派生长命子进程前清掉自身三件 stdio 的该标志。

use windows::Win32::Foundation::{HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation};
use windows::Win32::System::Console::{
    GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

/// 清当前进程 stdin/stdout/stderr 的 INHERIT 标志(此后的子进程不再继承它们)。
/// 幂等;非句柄型 stdio(已重定向到文件/管道之外)失败即忽略,保持原状。
/// CLI 进程随后退出,不恢复标志——长命子进程派生是本守卫唯一目标场景。
pub fn clear_stdio_inheritance() {
    for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        let handle = match unsafe { GetStdHandle(which) } {
            Ok(h) => h,
            Err(_) => continue,
        };
        if handle.is_invalid() {
            continue;
        }
        let _ = unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) };
    }
}
