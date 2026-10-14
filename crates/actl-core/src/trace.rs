//! stderr 阶段打点(`ACTL_TRACE=1` 启用):慢现场归因仪器。
//!
//! stdout 契约不变(ADR-002)——trace 只进 stderr,禁用时零输出。
//! 用途:慢现场把"命令总耗时"分解到引擎阶段(窗口解析/唤醒/树遍历/L4/pattern),
//! 与 envelope 的 duration_ms/wake_ms/waited_ms 构成完整观测面:
//! envelope 字段 = 机器消费的常态数据;trace = 人类诊断的按需旁路。
//! env 只在进程内读一次(OnceLock,同 timing.rs 纪律)。

use std::sync::OnceLock;
use std::time::Instant;

/// 是否启用(ACTL_TRACE 存在且非 "0"/空，或设置了 ACTL_TRACE_FILE)。
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        (std::env::var("ACTL_TRACE")
            .map(|v| !v.is_empty() && v != "0")
            .unwrap_or(false))
            || file_path().is_some()
    })
}

/// 可选落盘路径(`ACTL_TRACE_FILE`)：追加写，供显示端派生的执行器保留阶段证据。
/// stderr 输出不变；文件写失败静默忽略(诊断旁路不得影响命令结果)。
fn file_path() -> Option<&'static std::path::Path> {
    static PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os("ACTL_TRACE_FILE")
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
    })
    .as_deref()
}

fn emit(line: &str) {
    if enabled() {
        eprintln!("{line}");
    }
    if let Some(path) = file_path() {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// 打一个阶段耗时点:`[actl-trace] <stage> <ms>ms`(未启用时 no-op)。
pub fn stage(stage: &str, started: Instant) {
    if enabled() || file_path().is_some() {
        emit(&format!(
            "[actl-trace] {stage} {}ms",
            started.elapsed().as_millis()
        ));
    }
}

/// Content-minimal aggregate diagnostics; shares stderr/file routing with timings.
pub(crate) fn detail(stage: &str, data: &serde_json::Value) {
    if enabled() {
        emit(&format!("[actl-trace] {stage} {data}"));
    }
}

/// 区间计时 guard:`let _t = trace::scope("locate.dfs");` 离开作用域自动打点。
/// 未启用时 Drop 内不输出(构造开销一次 Instant,纳秒级)。
pub struct TraceScope {
    stage: &'static str,
    started: Instant,
}

pub fn scope(stage: &'static str) -> TraceScope {
    if enabled() || file_path().is_some() {
        emit(&format!("[actl-trace] {stage} begin"));
    }
    TraceScope {
        stage,
        started: Instant::now(),
    }
}

/// 动态标签版(scope 的 owned 镜像):调用侧拼接阶段名(browser.<label>
/// 按调用方归位)——非 'static 场景用;语义/Drop 纪律与 scope 完全同款。
pub struct TraceScopeOwned {
    stage: String,
    started: Instant,
}

pub fn scope_owned(stage: String) -> TraceScopeOwned {
    if enabled() || file_path().is_some() {
        emit(&format!("[actl-trace] {stage} begin"));
    }
    TraceScopeOwned {
        stage,
        started: Instant::now(),
    }
}

impl Drop for TraceScopeOwned {
    fn drop(&mut self) {
        stage(&self.stage, self.started);
        slow_phase_notice(&self.stage, self.started);
    }
}

impl Drop for TraceScope {
    fn drop(&mut self) {
        stage(self.stage, self.started);
        slow_phase_notice(self.stage, self.started);
    }
}

/// 慢阶段默认告警(2026-10-06 基建③:trace 常态化——代价与信号的平衡点):
/// 超过阈值的阶段无论 ACTL_TRACE 是否启用都 stderr 一行(快路径零噪音;
/// 慢阶段是「为何这么久」的第一现场——Word open 12s/locate.dfs 3s 级)。
/// 阈值默认 2000ms,ACTL_SLOW_SCOPE_MS 可调(0=关)。
fn slow_phase_notice(stage: &str, started: Instant) {
    let slow_ms: u128 = std::env::var("ACTL_SLOW_SCOPE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    if slow_ms == 0 {
        return;
    }
    let elapsed = started.elapsed().as_millis();
    if elapsed >= slow_ms {
        eprintln!(
            "[actl-slow] {stage} {elapsed}ms exceeds {slow_ms}ms — set ACTL_TRACE=1 for the full phase breakdown"
        );
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
