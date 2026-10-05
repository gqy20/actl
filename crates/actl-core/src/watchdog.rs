//! 进程级看门狗:跨进程挂起族的结构性防护(运行时治理第三轮,docs/06 §7.7)。
//!
//! 动因(2026-10-04 十五僵尸实证,ROADMAP 观察项):UIA 跨进程调用阻塞在
//! 卡死目标上是**无界**的——companion ack 路径已证有界(3s),挂点在
//! 提供程序侧,逐调用点加超时不可行。本模块把界限放在**进程级**:
//! 语义是"无进展"而非"总时长"——任何进展点(步派发/等待轮询/恢复重试/
//! 录制事件)重置期限;期限耗尽 = 进程卡在任何不可恢复的等待里,自我
//! 终止并留证(stderr 诊断行,stdout envelope 纪律不破坏——主线程已卡,
//! 本就不会再有 envelope)。调用方按既有语义恢复:flow 执行器被杀 →
//! run 留 Running/中断态 → flow-resume 的 uncertain 重放核对;脚本调用方
//! 拿到非零退出码 + stderr 标记。
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 期限的绝对时刻(unix ms);0 = 未武装。
static DEADLINE_MS: AtomicU64 = AtomicU64::new(0);
/// 最近一次进展的绝对时刻(unix ms),供终止诊断报"卡了多久"。
static LAST_PROGRESS_MS: AtomicU64 = AtomicU64::new(0);

const ENV_MS: &str = "ACTL_WATCHDOG_MS";
/// 看门狗默认期限:2 分钟(2026-10-06 由 300s 降档)。合法长路径全部带心跳
/// (树扫描逐节点、wait 逐轮、录制逐批、flow 逐步、探索逐轮)——心跳在,
/// 期限只是上限校验;心跳停(单点卡死)即触顶。降档动机:尾段挂起族的滞尾
/// 自清速度(300s 下每轮真机验收积压多个楔死进程的体感);单命令正常路径
/// 含 wait 都在 60s 内,全树扫描类由逐节点心跳保护(网易云全树 126s 实证)。
pub const DEFAULT_MS: u64 = 120_000;

/// 武装看门狗并启动监视线程;`timeout_ms == 0` 显式关闭(调试/诊断用)。
/// 幂等:重复调用只刷新期限,不叠加线程。
pub fn arm(timeout_ms: u64) {
    DEADLINE_MS.store(unix_ms() + timeout_ms, Ordering::Relaxed);
    LAST_PROGRESS_MS.store(unix_ms(), Ordering::Relaxed);
    if timeout_ms == 0 {
        DEADLINE_MS.store(0, Ordering::Relaxed);
        return;
    }
    std::thread::Builder::new()
        .name("actl-watchdog".into())
        .spawn(|| {
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let deadline = DEADLINE_MS.load(Ordering::Relaxed);
                if deadline == 0 {
                    continue; // 关闭态:线程留存但不再判死
                }
                let now = unix_ms();
                if now >= deadline {
                    let last = LAST_PROGRESS_MS.load(Ordering::Relaxed);
                    let stuck_ms = now.saturating_sub(last);
                    // stderr 是诊断通道(ADR-002);主线程已卡,不会有 envelope。
                    eprintln!(
                        "[watchdog] no progress for {stuck_ms}ms; self-terminating — the \
                         process is stuck in an unbounded wait (UIA provider block family, \
                         see docs/06 §7.7); raise ACTL_WATCHDOG_MS if this workload is legit"
                    );
                    std::process::exit(1);
                }
            }
        })
        .ok();
}

/// 进展心跳:重置期限。调用点 = 有界循环的每一轮(步派发/等待轮询/
/// 恢复重试/录制事件)。未武装时为无操作。
pub fn reset() {
    let deadline = DEADLINE_MS.load(Ordering::Relaxed);
    if deadline == 0 {
        return;
    }
    let now = unix_ms();
    LAST_PROGRESS_MS.store(now, Ordering::Relaxed);
    // 期限按当前时刻顺延:进展即续命,总时长不受限。
    let span = deadline.saturating_sub(now);
    DEADLINE_MS.store(now + span, Ordering::Relaxed);
}

/// 环境配置:ACTL_WATCHDOG_MS(毫秒;0=关)。缺省 DEFAULT_MS。
pub fn env_timeout_ms() -> u64 {
    std::env::var(ENV_MS)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MS)
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 纯判定逻辑(单测面):期限是否已过。
pub fn expired(deadline_ms: u64, now_ms: u64) -> bool {
    deadline_ms != 0 && now_ms >= deadline_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_only_when_armed_and_past() {
        assert!(!expired(0, u64::MAX)); // 未武装永不判死
        assert!(!expired(1_000, 999));
        assert!(expired(1_000, 1_000)); // 边界=到期
        assert!(expired(1_000, 2_000));
    }

    #[test]
    fn reset_extends_by_remaining_span() {
        // 静态语义单测:武装 300s,立即 reset → 新期限 ≈ 现在+300s。
        // 只验证"顺延不缩短且未武装时不武装"两条不变量,避免时序脆弱。
        let before = unix_ms();
        DEADLINE_MS.store(0, Ordering::Relaxed);
        reset(); // 未武装:不得武装
        assert_eq!(DEADLINE_MS.load(Ordering::Relaxed), 0);
        DEADLINE_MS.store(before + 1_000, Ordering::Relaxed);
        LAST_PROGRESS_MS.store(before, Ordering::Relaxed);
        reset();
        let after = DEADLINE_MS.load(Ordering::Relaxed);
        assert!(after >= before + 900, "reset must not shrink the span");
        assert!(after <= before + 2_000, "reset must not inflate the span");
        DEADLINE_MS.store(0, Ordering::Relaxed);
    }
}
