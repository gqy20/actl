//! stderr 阶段打点(`ACTL_TRACE=1` 启用):慢环境归因仪器。
//!
//! stdout 契约不变(ADR-002)——trace 只进 stderr,禁用时零输出。
//! 用途:慢日把"命令总耗时"分解到引擎阶段(窗口解析/唤醒/树遍历/L4/pattern),
//! 与 envelope 的 duration_ms/wake_ms/waited_ms 构成完整观测面:
//! envelope 字段 = 机器消费的常态数据;trace = 人类诊断的按需旁路。
//! env 只在进程内读一次(OnceLock,同 timing.rs 纪律)。

use std::sync::OnceLock;
use std::time::Instant;

/// 是否启用(ACTL_TRACE 存在且非 "0"/空)。
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("ACTL_TRACE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false)
    })
}

/// 打一个阶段耗时点:`[actl-trace] <stage> <ms>ms`(未启用时 no-op)。
pub fn stage(stage: &str, started: Instant) {
    if enabled() {
        eprintln!("[actl-trace] {stage} {}ms", started.elapsed().as_millis());
    }
}

/// 区间计时 guard:`let _t = trace::scope("locate.dfs");` 离开作用域自动打点。
/// 未启用时 Drop 内不输出(构造开销一次 Instant,纳秒级)。
pub struct TraceScope {
    stage: &'static str,
    started: Instant,
}

pub fn scope(stage: &'static str) -> TraceScope {
    if enabled() {
        eprintln!("[actl-trace] {stage} begin");
    }
    TraceScope {
        stage,
        started: Instant::now(),
    }
}

impl Drop for TraceScope {
    fn drop(&mut self) {
        stage(self.stage, self.started);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default_and_stage_is_silent_noop() {
        // 不设 env(测试进程默认未设)→ enabled() 为 false;stage 不 panic 即可
        let ok = std::panic::catch_unwind(|| {
            stage("test.point", Instant::now());
            let _t = scope("test.scope");
        });
        assert!(ok.is_ok());
    }

    #[test]
    fn scope_drop_invokes_stage_without_panic() {
        let _t = scope("test.lifetime");
        drop(_t);
    }
}
