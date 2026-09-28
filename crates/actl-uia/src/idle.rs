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

/// 当前系统空闲毫秒数(全系统键鼠;粒度受系统 tick ~15.6ms 限制)。
pub fn idle_ms() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: 按 API 契约传入 cbSize 正确的出参指针。返回值不区分成败语义
    // (极老系统才会失败),失败时 dwTime 保持 0——退化为"开机以来无输入"
    // 的保守读数,空闲判定只多不少,不构成误报输入的路径。
    let _ = unsafe { GetLastInputInfo(&mut info) };
    // SAFETY: GetTickCount 无前置条件,纯读取系统 tick。
    let now = unsafe { GetTickCount() };
    idle_from_ticks(now, info.dwTime)
}

/// `wait --idle`:轮询直到系统空闲 ≥ 阈值(TIMEOUT 语义)。
/// 轮询间隔沿用全仓 wait 纪律(timing().poll_ms);人在键入时每次轮询
/// 都会看到新的 lastInput,自然让路到 deadline。
pub fn wait_idle(threshold_ms: u32, timeout_ms: u64) -> Result<u32, CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        let idle = idle_ms();
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
    use super::idle_from_ticks;

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
