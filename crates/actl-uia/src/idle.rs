//! idle —— 系统输入空闲检测(`wait --idle` 的平台侧)。
//!
//! GetLastInputInfo 只给出"最近一次物理输入的 tick",不含输入内容——零隐私面。
//! 它感知不到 UIA 语义注入,所以"空闲"指"人不在",不是"没有自动化在跑";
//! 自动化实例间的并发由 input.rs 执行锁协调,职责分离。

use actl_core::{CtlError, ErrorCode};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

/// 纯计算:now/last 均为 GetTickCount 域(u32,约 49.7 天回绕)。
/// 独立成函数使回绕语义可单测(wrapping_sub 是唯一正确写法)。
fn idle_from_ticks(now: u32, last: u32) -> u32 {
    now.wrapping_sub(last)
}

fn observed_idle(success: bool, now: u32, last: u32) -> Result<u32, CtlError> {
    if !success {
        return Err(CtlError::with_evidence(
            ErrorCode::Internal,
            "cannot observe session input activity",
            serde_json::json!({"reason":"input_observation_failed","api":"GetLastInputInfo"}),
        ));
    }
    Ok(idle_from_ticks(now, last))
}

/// 当前系统空闲毫秒数(全系统键鼠;粒度受系统 tick ~15.6ms 限制)。
pub fn idle_ms() -> Result<u32, CtlError> {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: cbSize matches the output structure. Failure is unknown activity,
    // never evidence that the user is idle.
    let success = unsafe { GetLastInputInfo(&mut info) }.as_bool();
    // SAFETY: GetTickCount 无前置条件,纯读取系统 tick。
    let now = unsafe { GetTickCount() };
    observed_idle(success, now, info.dwTime)
}

/// `wait --idle`:轮询直到系统空闲 ≥ 阈值(TIMEOUT 语义)。
/// 轮询间隔沿用全仓 wait 纪律(timing().poll_ms);人在键入时每次轮询
/// 都会看到新的 lastInput,自然让路到 deadline。
pub fn wait_idle(threshold_ms: u32, timeout_ms: u64) -> Result<u32, CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        actl_core::wait_control::check()?;
        let idle = idle_ms()?;
        if idle >= threshold_ms {
            return Ok(idle);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::Timeout,
                format!(
                    "system input activity within the last {idle}ms (idle threshold {threshold_ms}ms) \
                     — user is present; retry with a larger --timeout or when the console is idle"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(crate::timing().poll_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::{idle_from_ticks, observed_idle};

    #[test]
    fn failed_input_observation_is_not_idle() {
        let result = observed_idle(false, 3_600_000, 0);
        assert!(
            result.is_err(),
            "failed observation must not report one hour idle"
        );
        let error = result.unwrap_err();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/idle_observation_failed.json"))
                .unwrap();
        assert_eq!(
            serde_json::to_value(actl_core::ErrorEnvelope::new("wait", &error, 0)).unwrap(),
            expected
        );
    }

    #[test]
    fn paused_workflow_does_not_wait_for_idle_timeout() {
        let path = std::env::temp_dir().join(actl_core::new_snapshot_id());
        std::fs::write(&path, b"pause").unwrap();
        let _scope = actl_core::wait_control::Scope::enter(path.clone());
        let error = super::wait_idle(u32::MAX, 30_000).unwrap_err();
        std::fs::remove_file(path).unwrap();
        assert_eq!(error.code, actl_core::ErrorCode::Aborted);
    }

    #[test]
    fn idle_is_zero_right_after_input() {
        assert_eq!(idle_from_ticks(1000, 1000), 0);
    }

    #[test]
    fn idle_accumulates_normally() {
        assert_eq!(idle_from_ticks(10_000, 2_000), 8_000);
    }

    #[test]
    fn tick_wraparound_stays_correct() {
        // last 在回绕前(u32 末端),now 已回绕到小值:真实间隔 201ms。
        let last = u32::MAX - 99; // 距回绕还差 100ms 的时刻
        let now = 101; // 回绕后 101ms
        assert_eq!(idle_from_ticks(now, last), 201);
    }

    #[test]
    fn never_input_reports_uptime_as_idle() {
        // dwTime=0(开机至今无输入):空闲 = 开机时长,只多不少。
        assert_eq!(idle_from_ticks(3_600_000, 0), 3_600_000);
    }
}
