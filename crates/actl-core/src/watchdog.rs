//! 进程级看门狗:跨进程挂起族的结构性防护(运行时治理第三轮,docs/06 §7.7)。
//!
//! 动因(2026-10-04 十五僵尸实证,ROADMAP 观察项):UIA 跨进程调用阻塞在
//! 卡死目标上是**无界**的——companion ack 路径已证有界(3s),挂点在
//! 提供程序侧,逐调用点加超时不可行。本模块把界限放在**进程级**:
//! 语义是"无进展"而非"总时长"——任何进展点(步派发/等待轮询/恢复重试/
//! 录制事件)重置期限;期限耗尽 = 进程卡在任何不可恢复的等待里,自我
//! 终止并留证。
//!
//! 2026-10-14 收口:击杀前看门狗线程自己吐兜底信封(INTERNAL,
//! evidence.no_progress_ms)——挂死也可解析;硬退出走宿主注入的
//! TerminateProcess(std::process::exit 的 CRT 清理在主线程持锁卡死时
//! 同样会卡,13112 签名实证)。调用方按既有语义恢复:flow 执行器被杀
//! 则 run 留 Running/中断态,flow-resume 的 uncertain 重放核对;脚本
//! 调用方拿到非零退出码、stderr 标记与兜底信封。
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// 期限的绝对时刻(unix ms);0 = 未武装。
static DEADLINE_MS: AtomicU64 = AtomicU64::new(0);
/// 最近一次进展的绝对时刻(unix ms),供终止诊断报"卡了多久"。
static LAST_PROGRESS_MS: AtomicU64 = AtomicU64::new(0);
/// 武装时配置的期限宽度(unix ms);reset 续命用——进展即续满整窗。
static ARMED_WINDOW_MS: AtomicU64 = AtomicU64::new(0);

const ENV_MS: &str = "ACTL_WATCHDOG_MS";
/// 看门狗默认期限:2 分钟(2026-10-06 由 300s 降档)。合法长路径全部带心跳
/// (树扫描逐节点、wait 逐轮、录制逐批、flow 逐步、探索逐轮)——心跳在,
/// 期限只是上限校验;心跳停(单点卡死)即触顶。降档动机:尾段挂起族的滞尾
/// 自清速度(300s 下每轮真机验收积压多个楔死进程的体感);单命令正常路径
/// 含 wait 都在 60s 内,全树扫描类由逐节点心跳保护(网易云全树 126s 实证)。
pub const DEFAULT_MS: u64 = 120_000;

/// 武装看门狗并启动监视线程;`timeout_ms == 0` 显式关闭(调试/诊断用)。
/// 幂等:重复调用只刷新期限,不叠加线程。
/// 硬退出回调:main 注入 actl_uia::watchdog::hard_exit_after_envelope
/// (TerminateProcess,绕过 CRT 清理——看门狗必须用与 finish 同款的死法,
/// std::process::exit 的 CRT 清理在主线程持锁卡死时同样会卡,13112 签名
/// 实证)。未注入(单测/非 actl.exe 宿主)时回退 std::process::exit。
/// 函数指针 + OnceLock:注入一次,看门狗线程按需读。
static HARD_EXIT: std::sync::OnceLock<fn(i32) -> !> = std::sync::OnceLock::new();

/// 宿主注入硬退出实现(仅 actl.exe main 调用;库消费方保持默认回退)。
pub fn install_hard_exit(f: fn(i32) -> !) {
    let _ = HARD_EXIT.set(f);
}

fn hard_exit(code: i32) -> ! {
    match HARD_EXIT.get() {
        Some(f) => f(code),
        // 回退路径只在无宿主注入时走(单测);std::process::exit 到不了
        // TerminateProcess 但无注入面也无可绕的锁
        None => std::process::exit(code),
    }
}

pub fn arm(timeout_ms: u64) {
    ARMED_WINDOW_MS.store(timeout_ms, Ordering::Relaxed);
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
                    // stderr 诊断行(人类第一现场)
                    eprintln!(
                        "[watchdog] no progress for {stuck_ms}ms; self-terminating — the \
                         process is stuck in an unbounded wait (UIA provider block family, \
                         see docs/06 §7.7); raise ACTL_WATCHDOG_MS if this workload is legit"
                    );
                    // 击杀前吐兜底信封(2026-10-14 剩余逃逸路径收口):挂死也
                    // 可解析——command 取 PANIC_COMMAND(两阶段精化同 panic
                    // hook),INTERNAL + evidence.no_progress_ms。极窄竞态:主
                    // 线程死前恰好也在 println 会双信封交错——工程上可忽略,
                    // 调用方按「stdout 含 ok:false 即失败」消费不受影响。
                    let command = crate::PANIC_COMMAND.get().copied().unwrap_or("unknown");
                    let err = crate::CtlError::with_evidence(
                        crate::ErrorCode::Internal,
                        format!("watchdog: no progress for {stuck_ms}ms; process stuck in an unbounded wait"),
                        serde_json::json!({
                            "no_progress_ms": stuck_ms,
                            "hint": "the process is stuck in an unbounded wait (UIA provider block family); raise ACTL_WATCHDOG_MS if this workload is legit",
                        }),
                    );
                    let elapsed = crate::BOOT
                        .get()
                        .map(|b| b.elapsed().as_millis() as u64)
                        .unwrap_or(0);
                    let env = crate::ErrorEnvelope::new(command, &err, elapsed);
                    if let Ok(text) = serde_json::to_string(&env) {
                        println!("{text}");
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                    }
                    hard_exit(1);
                }
            }
        })
        .ok();
}

/// 进展心跳:重置期限。调用点 = 有界循环的每一轮(步派发/等待轮询/
/// 恢复重试/录制事件)。未武装时为无操作。
pub fn reset() {
    let window = ARMED_WINDOW_MS.load(Ordering::Relaxed);
    if window == 0 {
        return; // 未武装(或显式关闭)
    }
    let now = unix_ms();
    LAST_PROGRESS_MS.store(now, Ordering::Relaxed);
    // 进展即续满整窗(P0 修复 2026-10-06):原实现 span = deadline-now 后
    // deadline = now+span 是恒等式——期限从不移动,看门狗退化为「总时长
    // 硬顶」。300s 默认时代长期潜伏(整轮 <300s 掩盖),降档 120s 后被
    // 010 实证引爆:fill-3 进展正常(杀进程消息「no progress for 110ms」
    // ——进展明明在发生)仍被杀。语义:心跳每跳一次,无进展窗口从头计。
    DEADLINE_MS.store(now + window, Ordering::Relaxed);
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
