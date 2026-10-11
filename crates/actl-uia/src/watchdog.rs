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

/// 重臂当前区段(不新建、不清除):树扫描等**逐节点推进**的长路径在循环里
/// 每次迭代调用——区段语义从「整段 wall clock」变为「单次跨进程调用」的
/// 有界性。真机实证(2026-10-06):冷 provider 的合法慢扫描在整段口径下
/// 撞 20s 限误杀(网易云全树 find 三度中止),逐节点重臂后慢推进不再触限、
/// 单点卡死的 children 调用照常 20s 击杀。
/// 单槽约束:enter/drop 是全局单槽时间戳——嵌套调用 enter 后 drop 会清零
/// 外层区段(调用方必须保证「嵌套守卫调用之后不再依赖外层区段」,见
/// scroll_element 的两段式先例);rearm 只刷新时间戳,无此约束。
pub fn rearm() {
    IN_UIA_SINCE_MS.store(unix_ms(), Ordering::Release);
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

/// **envelope 后硬终止**(尾段挂起族根杀,2026-10-11):`finish` 打完
/// envelope 并 flush 后调用——析构链上的无界阻塞面(thread_local 定位缓
/// 存、栈上 UIElement COM 代理的跨进程 Release、signal worker join)在
/// `process::exit` 里仍可能卡死(exit 走 C 运行时清理,不保证到达 OS;
/// 13112 签名实证:envelope 已落盘、进程 CPU 恒零赖活数小时)。本函数直
/// 接 TerminateProcess——绕过一切运行时清理,把「永久挂起」变成「立即
/// 消失」;envelope 已 flush、状态已显式落盘(finish(phase)/Journal 每
/// 事件 flush),不依赖析构面。仅限 main 的 finish 调用,库代码禁用。
pub fn hard_exit_after_envelope(code: i32) -> ! {
    // Windows TerminateProcess(当前进程):无清理、无析构、无 atexit。
    // unsafe 理由(AGENTS §5,crate 豁免点):win32 直调,当前进程伪句柄
    // (GetCurrentProcess)按文档恒常;不能经 std::process::exit——它走
    // CRT 清理,恰是挂起面。
    unsafe {
        let handle = windows::Win32::System::Threading::GetCurrentProcess();
        let _ = windows::Win32::System::Threading::TerminateProcess(handle, code as u32);
    }
    // TerminateProcess(自身)不返回;防御性兜底(理论上到不了)
    std::process::abort();
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
