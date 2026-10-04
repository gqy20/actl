//! UIA 调用看门狗(docs/81 §8 僵尸事故的根治项)。uiautomation 的 COM 调用
//! 可以无限期不返回(Word 级 provider 楔死实证,2026-10-04 十余个不可杀
//! actl.exe 僵尸的成因):挂起的 actl 进程攥住调用方管道,事后击杀则卡在
//! 内核态 RPC 里永不清退。进入 UIA 调用区段时登记时间戳,守护线程超限即
//! 带专用退出码自毙——把"永久挂起→不可杀僵尸"变成"限内的诚实失败"。
//! 非侵入:区段粒度(定位/树观察/焦点查询),不是逐 COM 调用。

use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};

/// 自毙退出码(125)与 stderr 标记:调用方据此区分看门狗击杀与常规失败。
pub const WATCHDOG_EXIT_CODE: i32 = 125;

static IN_UIA_SINCE_MS: AtomicU64 = AtomicU64::new(0);
static THREAD: Once = Once::new();

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 看门狗判定(纯函数,可测):in_call_since=进入区段的 unix 毫秒(0=不在区段);
/// limit=0 表示禁用。
pub fn should_abort(in_call_since_ms: u64, now_ms: u64, limit_ms: u64) -> bool {
    limit_ms > 0 && in_call_since_ms > 0 && now_ms.saturating_sub(in_call_since_ms) > limit_ms
}

pub struct UiaCallScope {
    _private: (),
}
impl Drop for UiaCallScope {
    fn drop(&mut self) {
        IN_UIA_SINCE_MS.store(0, Ordering::Release);
    }
}

/// 进入一个 UIA 调用区段;drop 时清除登记。首次调用懒启动守护线程。
pub fn enter() -> UiaCallScope {
    IN_UIA_SINCE_MS.store(unix_ms(), Ordering::Release);
    THREAD.call_once(|| {
        std::thread::Builder::new()
            .name("actl-uia-watchdog".into())
            .spawn(|| loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let since = IN_UIA_SINCE_MS.load(Ordering::Acquire);
                let limit = crate::timing().uia_watchdog_ms;
                if should_abort(since, unix_ms(), limit) {
                    // OS 终止进程时回收文件锁/句柄,不留僵尸;退出码与 stderr
                    // 标记让调用方看到"看门狗击杀"而非无限等待。
                    eprintln!(
                        "[actl-watchdog] UIA call exceeded {limit}ms; aborting to avoid a wedged process"
                    );
                    std::process::exit(WATCHDOG_EXIT_CODE);
                }
            })
            .ok();
    });
    UiaCallScope { _private: () }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aborts_only_past_limit_inside_scope() {
        // 区段外(0)/禁用(0)/限内不击杀;限外击杀
        assert!(!should_abort(0, 10_000, 20_000));
        assert!(!should_abort(1_000, 10_000, 0));
        assert!(!should_abort(1_000, 10_000, 20_000));
        assert!(should_abort(1_000, 21_001, 20_000));
        // 时间回绕(时钟跳变)不误杀
        assert!(!should_abort(50_000, 10_000, 20_000));
    }
}
