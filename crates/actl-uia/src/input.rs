//! 输入占用协调(doc 09 §6.2):短时进程间锁 + 动作段内焦点复核。
//!
//! 背景(四次活体实证,docs/spike-findings):多步骤流程反复抢前台时,真人
//! 并发键入与注入交错互相损坏。协议:取得输入锁 → 校验目标与焦点 → 执行
//! 动作段 → 释放。锁只协调遵守协议的 actl 实例,不能阻止用户或其他软件
//! 输入——所以长注入还必须分批,批间复核焦点,被抢即停并上报已送达部分。

use actl_core::{CtlError, ErrorCode};
use windows::Win32::Foundation::{
    CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{CreateMutexExW, ReleaseMutex, WaitForSingleObject};
use windows::core::w;

/// 会话内命名互斥体"actl-input-lock"(不加 Global\ 前缀:只协调同会话的 actl 实例;
/// 名字同步内联在 CreateMutexExW 调用里——w! 宏只吃字面量)。
/// 等锁上限:等太久说明另一个实例在做长动作段,放弃比排队更诚实。
pub const LOCK_TIMEOUT_MS: u32 = 3000;

/// 持锁 RAII 守卫:Drop 时释放互斥体并关句柄,任何返回路径都不泄漏占用。
pub struct InputLock(HANDLE);

impl InputLock {
    /// 等待并取得输入占用锁;忙 → TIMEOUT(附恢复建议)。
    /// 停止标志优先于一切:用户按了紧急热键后,所有物理注入在此被拒
    /// (ABORTED)——显示信号协议的执行侧闸门(docs/11 §10.3)。
    pub fn acquire(timeout_ms: u32) -> Result<Self, CtlError> {
        Self::acquire_at(&actl_core::state::SignalPaths::default(), timeout_ms)
    }

    /// 路径可注入版(测试隔离:真实 %LOCALAPPDATA% 里可能残留用户按下
    /// 紧急热键留下的停止标志,单测不得依赖全局状态)。
    pub fn acquire_at(
        paths: &actl_core::state::SignalPaths,
        timeout_ms: u32,
    ) -> Result<Self, CtlError> {
        if paths.stop_requested() {
            return Err(CtlError::new(
                ErrorCode::Aborted,
                "stop requested via signal channel (emergency hotkey); clear the flag to resume",
            ));
        }
        unsafe {
            // CreateMutexExW(flags=0) 等价 CreateMutexW;access=SYNCHRONIZE(0x0010_0000)
            // (裸值:免为单常量引入 Storage::FileSystem feature)
            let handle = CreateMutexExW(None, w!("actl-input-lock"), 0, 0x0010_0001)
                .map_err(|e| CtlError::internal(format!("CreateMutexExW(actl-input-lock): {e}")))?;
            let waited = WaitForSingleObject(handle, timeout_ms);
            if waited == WAIT_OBJECT_0 || waited == WAIT_ABANDONED {
                // ABANDONED:上一持有者异常退出,OS 已把所有权交给我们——
                // 正确姿势是接管并继续(残留修饰键清场由 kbd 层负责)
                let lock = Self(handle);
                if paths.stop_requested() {
                    return Err(CtlError::new(
                        ErrorCode::Aborted,
                        "stop requested while waiting for input lock",
                    ));
                }
                Ok(lock)
            } else if waited == WAIT_TIMEOUT {
                let _ = CloseHandle(handle);
                Err(CtlError::new(
                    ErrorCode::Timeout,
                    format!(
                        "another actl instance holds the input lock (waited {timeout_ms}ms); \
                         retry after it finishes or raise --timeout"
                    ),
                ))
            } else {
                let _ = CloseHandle(handle);
                Err(CtlError::internal(format!(
                    "WaitForSingleObject returned {waited:?}"
                )))
            }
        }
    }
}

impl Drop for InputLock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}

/// 分批发送的可测核心:按批切片逐段发送,批间复核焦点;
/// 复核失败即停,返回已完成的段数。纯逻辑,注入动作由调用方注入闭包。
pub fn send_in_segments<T, F, V>(
    items: &[T],
    segment_len: usize,
    mut send: F,
    mut focus_ok: V,
) -> Result<usize, CtlError>
where
    F: FnMut(&[T]) -> Result<(), CtlError>,
    V: FnMut() -> bool,
{
    let segment_len = segment_len.max(1);
    let mut done = 0usize;
    for seg in items.chunks(segment_len) {
        // 首批之前调用方已完成护栏校验;批间必须复核——焦点被抢(真人点走、
        // 弹窗夺焦)继续注入 = 键打进别处(事故 #1 形态)
        if done > 0 && !focus_ok() {
            return Ok(done);
        }
        if let Err(mut error) = send(seg) {
            let mut evidence = error
                .evidence
                .take()
                .unwrap_or_else(|| serde_json::json!({}));
            if !evidence.is_object() {
                evidence = serde_json::json!({"cause":evidence});
            }
            evidence["delivered_before_error"] = done.into();
            evidence["failed_segment_size"] = seg.len().into();
            error.evidence = Some(evidence);
            return Err(error);
        }
        done += seg.len();
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_is_mutual_across_threads() {
        // 隔离:临时目录,避免读到用户真实停止标志
        let paths = actl_core::state::SignalPaths::at(
            std::env::temp_dir().join(format!("actl-lock-test-{}", std::process::id())),
        );
        let a = InputLock::acquire_at(&paths, 0).expect("first acquire");
        // 互斥体所有权在线程:同线程重入会成功,必须跨线程验证互斥;
        // 锁本身留在原线程,跨线程只把错误码传回来
        let isolated_paths = paths.clone();
        let other = std::thread::spawn(move || match InputLock::acquire_at(&isolated_paths, 0) {
            Err(e) => Some(e.code),
            Ok(_) => None,
        });
        match other.join().expect("thread ok") {
            Some(code) => assert_eq!(code, ErrorCode::Timeout),
            None => panic!("second acquire must time out"),
        }
        drop(a);
        // 释放后可再取
        assert!(InputLock::acquire_at(&paths, 0).is_ok());
    }

    #[test]
    fn segments_send_all_when_focus_holds() {
        let items: Vec<usize> = (0..70).collect();
        let mut sent: Vec<usize> = Vec::new();
        let done = send_in_segments(
            &items,
            32,
            |seg| {
                sent.extend_from_slice(seg);
                Ok(())
            },
            || true,
        )
        .unwrap();
        assert_eq!(done, 70);
        assert_eq!(sent, items);
    }

    #[test]
    fn segments_stop_at_boundary_when_focus_lost() {
        let items: Vec<usize> = (0..100).collect();
        let mut calls = 0;
        // 第 2 次复核(即 1 批完成后)报焦点丢失 → 停在第 2 批前
        let done = send_in_segments(
            &items,
            10,
            |_seg| Ok(()),
            || {
                calls += 1;
                calls < 2
            },
        )
        .unwrap();
        assert_eq!(done, 20, "停在批边界,报告已送达量");
    }

    #[test]
    fn segment_error_preserves_delivered_prefix() {
        let items: Vec<usize> = vec![1, 2, 3, 4];
        let err = send_in_segments(
            &items,
            2,
            |seg| {
                if seg[0] == 3 {
                    Err(CtlError::internal("inject failed"))
                } else {
                    Ok(())
                }
            },
            || true,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
        assert_eq!(err.evidence.as_ref().unwrap()["delivered_before_error"], 2);
    }
}
