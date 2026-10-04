//! 时序参数单源:默认值 = 生产路径;环境变量覆盖 = 测试/调优路径
//! (timing-profile/timing-sweep fixtures 用,免重编译扫描;也是未来
//! "按应用自适应时序"的种子)。env 只在进程内读一次(OnceLock),
//! 非法值静默回退默认——时序旋钮绝不能因配置错误让命令失败。

use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    /// wait 族轮询间隔(docs/13 §4.2 生态共识:pywinauto 90 / terminator 100)
    pub poll_ms: u64,
    /// 挂起唤醒的有界等待上界(实测挂起 stall ~2s,取略低)
    pub wake_bound_ms: u64,
    /// WM_NULL 探针超时(快路径零误伤)
    pub probe_ms: u32,
    /// 焦点建立重试轮数
    pub focus_retries: u32,
    /// press 链 SetFocus 后的稳定等待
    pub press_focus_ms: u64,
    /// type 链 SetFocus 后的稳定等待
    pub type_focus_ms: u64,
    /// 前台护栏入口的沉降预算:窗口激活过渡期前台可能是空标题过渡窗
    /// (ForegroundStaging)或目标标题未就位——轮询至匹配或预算耗尽。
    /// 真被抢焦(标题稳定地不是目标)不受影响照样拒。
    pub foreground_settle_ms: u64,
    /// type 键盘通道分批大小(停止响应粒度 = 本值)
    pub segment_chars: usize,
    /// 修饰键空置等待上限
    pub modifier_wait_ms: u64,
    /// 输入占用锁等待上限
    pub lock_wait_ms: u32,
    /// wait --stable 的采样间隔(settle 判定:连续两次样本相等即稳定;
    /// 与 poll_ms 分开——内容加载的稳定窗需要比存在性轮询更长的间隔)
    pub settle_ms: u64,
    /// 可见性/门控快速轮询间隔(desktop/display_gate/window_ready 探测拍)
    pub fast_poll_ms: u64,
    /// window_ready 主轮询间隔
    pub ready_poll_ms: u64,
    /// window_ready 首段可见性探测窗(等首帧可见再进主轮询)
    pub vis_probe_ms: u64,
    /// 桌面交接 pending 态轮询间隔
    pub handoff_poll_ms: u64,
    /// UIA 调用区段看门狗上限(docs/81 §8):定位/树观察等区段超过此时长,
    /// 守护线程带专用退出码自毙——无界 COM 挂起会变成不可杀僵尸(实证)。
    /// 0 = 禁用。
    pub uia_watchdog_ms: u64,
}

/// CLI 协议默认值(help 四要素承诺的值)。**刻意不接 env 覆盖**:
/// 它们是协议契约的一部分,覆盖通道是命令行 `--timeout`;环境变量静默
/// 改协议默认值会让 help 文本变成谎言(agent-desktop 的 dead env knobs
/// 前车之鉴)。调内部旋钮用上面的 Timing 字段。
pub const EXPECT_MS: u64 = 2000;
pub const WAIT_MS: u64 = 5000;
pub const WAIT_WINDOW_MS: u64 = 5000;
/// 显示端握手窗口(display-start help 承诺"up to 4s";预告确认 3s)。
pub const DISPLAY_READY_MS: u64 = 4000;
pub const DISPLAY_ACK_MS: u64 = 3000;

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll_ms: 100,
            wake_bound_ms: 1600,
            probe_ms: 60,
            focus_retries: 3,
            press_focus_ms: 120,
            type_focus_ms: 150,
            foreground_settle_ms: 600,
            segment_chars: 32,
            modifier_wait_ms: 1500,
            lock_wait_ms: 3000,
            settle_ms: 250,
            fast_poll_ms: 25,
            ready_poll_ms: 50,
            vis_probe_ms: 1000,
            handoff_poll_ms: 100,
            uia_watchdog_ms: 20_000,
        }
    }
}

fn env_num<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

impl Timing {
    /// 进程内首次调用读 env 并缓存;之后零开销。
    pub fn load() -> &'static Timing {
        static T: OnceLock<Timing> = OnceLock::new();
        T.get_or_init(|| {
            let d = Timing::default();
            Timing {
                poll_ms: env_num("ACTL_POLL_MS", d.poll_ms),
                wake_bound_ms: env_num("ACTL_WAKE_BOUND_MS", d.wake_bound_ms),
                probe_ms: env_num("ACTL_PROBE_MS", d.probe_ms),
                focus_retries: env_num("ACTL_FOCUS_RETRIES", d.focus_retries),
                press_focus_ms: env_num("ACTL_PRESS_FOCUS_MS", d.press_focus_ms),
                type_focus_ms: env_num("ACTL_TYPE_FOCUS_MS", d.type_focus_ms),
                foreground_settle_ms: env_num("ACTL_FOREGROUND_SETTLE_MS", d.foreground_settle_ms),
                segment_chars: env_num("ACTL_SEGMENT_CHARS", d.segment_chars),
                modifier_wait_ms: env_num("ACTL_MODIFIER_WAIT_MS", d.modifier_wait_ms),
                lock_wait_ms: env_num("ACTL_LOCK_WAIT_MS", d.lock_wait_ms),
                settle_ms: env_num("ACTL_SETTLE_MS", d.settle_ms),
                fast_poll_ms: env_num("ACTL_FAST_POLL_MS", d.fast_poll_ms),
                ready_poll_ms: env_num("ACTL_READY_POLL_MS", d.ready_poll_ms),
                vis_probe_ms: env_num("ACTL_VIS_PROBE_MS", d.vis_probe_ms),
                handoff_poll_ms: env_num("ACTL_HANDOFF_POLL_MS", d.handoff_poll_ms),
                uia_watchdog_ms: env_num("ACTL_UIA_WATCHDOG_MS", d.uia_watchdog_ms),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_production_values() {
        let d = Timing::default();
        assert_eq!(d.poll_ms, 100); // 生态共识
        assert_eq!(d.wake_bound_ms, 1600); // 挂起 stall 实测 ~2s
        assert_eq!(d.segment_chars, 32);
        assert_eq!(d.lock_wait_ms, 3000);
        assert_eq!(d.modifier_wait_ms, 1500);
        assert_eq!(d.focus_retries, 3);
        assert_eq!(d.foreground_settle_ms, 600); // 激活过渡瞬态 < 1s
        assert_eq!(d.settle_ms, 250); // 稳定窗 = 2×250ms
        assert_eq!(d.fast_poll_ms, 25);
        assert_eq!(d.ready_poll_ms, 50);
        assert_eq!(d.vis_probe_ms, 1000);
        assert_eq!(d.handoff_poll_ms, 100);
    }

    #[test]
    fn protocol_defaults_are_pinned() {
        // 协议默认值是 help 契约,锁死后随 spec 变更(硬规则 8)
        assert_eq!(EXPECT_MS, 2000);
        assert_eq!(WAIT_MS, 5000);
        assert_eq!(WAIT_WINDOW_MS, 5000);
    }

    #[test]
    fn env_num_parses_and_falls_back() {
        assert_eq!(env_num::<u64>("_ACTL_TEST_ABSENT_", 42), 42);
        assert_eq!(env_num::<u64>("PATH", 42), 42); // 非数字 → 默认
        // 不测 set_var(edition 2024 unsafe):解析正确性由上两行覆盖
    }
}
