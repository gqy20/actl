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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::windows::io::FromRawHandle;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::Win32::System::Console::SetStdHandle;
    use windows::Win32::System::Pipes::CreatePipe;

    /// 机制回归(launch/record-worker/browser 三调用位共享):可继承管道写端
    /// 挂为本进程 stdout → clear_stdio_inheritance → 派生长命子进程(5s)
    /// → 关自身写端。继承已清:读端立即 EOF;未清(回归态):EOF 要等满
    /// 子进程 5s 生命周期——handoff-02 实证(引擎 4.4s 结束,管道吊 9 分钟)
    /// 的最小复现。真机端到端(经 launch 的管道 EOF)由场景轮人工核验。
    #[test]
    fn cleared_inherit_releases_pipe_eof_at_parent_exit() {
        unsafe {
            let mut read = HANDLE::default();
            let mut write = HANDLE::default();
            CreatePipe(
                &mut read,
                &mut write,
                Some(&SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: std::ptr::null_mut(),
                    bInheritHandle: windows::core::BOOL::from(true),
                }),
                0,
            )
            .expect("CreatePipe");
            // 写端可继承 + 设为本进程 stdout(模拟 shell/调用方的管道捕获)
            let original = GetStdHandle(STD_OUTPUT_HANDLE).unwrap_or_default();
            SetStdHandle(STD_OUTPUT_HANDLE, write).expect("SetStdHandle");
            clear_stdio_inheritance();
            // 长命子进程 5s(等价 handoff-02 的 notepad);stdio 三件套 null 同 launch
            let mut child = std::process::Command::new("cmd.exe")
                .args(["/c", "ping -n 5 127.0.0.1 > nul"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawn long-lived child");
            // 恢复原 stdout,随后关掉自己的写端——EOF 只能来自子进程的继承副本(若有)
            SetStdHandle(STD_OUTPUT_HANDLE, original).expect("restore stdout");
            let _ = CloseHandle(write);
            // 读端交给 std(File::from_raw_handle 接管关闭)
            let mut reader = std::fs::File::from_raw_handle(read.0 as _);
            let started = Instant::now();

            loop {
                let mut sink = [0u8; 64];
                match reader.read(&mut sink) {
                    Ok(0) | Err(_) => break, // EOF:写端全部关闭(Err=broken pipe)
                    Ok(_n) => {}
                }
            }
            let eof_after = started.elapsed();
            drop(reader);
            // 并行测试线程可能在句柄交换窗口期写入探针管道——数据量是旁枝,
            // 不断言;回归本体是 EOF 时序(下方)
            // 子进程必然还活着(未满 5s):EOF 若晚于 3s,说明子进程攥住了写端
            assert!(
                eof_after < Duration::from_secs(3),
                "pipe EOF waited {eof_after:?} — child inherited the write end (regression)"
            );
            // 本测试自身的 cmd/ping 子进程直接收尸(自派生直系子进程,非 GUI 会话应用)
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
