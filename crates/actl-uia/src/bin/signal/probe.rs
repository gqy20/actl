//! 修复检测探测(批次4):action_failed 暂停期间,由显示端以 5s 节奏静默运行
//! 失败步骤的 before/verify(只读),把"什么时候可以点继续"点亮。
//! 双判据语义(防代做双重生效):before 过=障碍已清除,可重试;verify 过=结果
//! 已存在(可能被手动完成),继续前需人工核对。auto_resume 仅在
//! before 过且 verify 未过时触发,且仅当流程显式声明 auto_resume: action_failed,
//! 预算 2 次/运行(显示端内存计,伴侣重启即重置——保守方向)。
//! 红线:探测只跑只读命令;user_paused/checkpoint/needs_review 永不探测续跑;
//! 急停/停止代次由 flow-resume 路径自身核验,探测不绕过任何护栏。
use actl_core::state::{SignalPaths, unix_ms};

/// 探测节流与放弃边界。
const PROBE_INTERVAL_MS: u64 = 5000;
const PROBE_GIVE_UP_MS: u64 = 10 * 60 * 1000;
/// auto_resume 预算(每流程,伴侣运行期内)。
const AUTO_RESUME_BUDGET: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// before 通过、verify 未通过:障碍已清除,可安全重试(auto_resume 条件)。
    Retriable,
    /// verify 通过:结果已存在,可能是手动完成——继续重发有双重生效风险。
    AlreadyDone,
    /// 判据未满足/不可判定:无提示(不重复念叨"还没修好")。
    Pending,
}

struct ProbeState {
    last_ms: u64,
    verdict: Verdict,
    auto_used: u32,
}
static PROBE: std::sync::Mutex<Option<(String, ProbeState)>> = std::sync::Mutex::new(None);

/// 纯决策(可测):双判据 → 裁决。
pub fn verdict(before_ok: Option<bool>, verify_ok: Option<bool>) -> Verdict {
    match (before_ok, verify_ok) {
        (_, Some(true)) => Verdict::AlreadyDone,
        (Some(true), _) => Verdict::Retriable,
        _ => Verdict::Pending,
    }
}

/// 每 tick(100ms)驱动;返回 (流程 id, 裁决) 供显示层高亮。
/// paused_since_ms 用于放弃边界;无绑定/非 action_failed/无判据时清理并返回 None。
pub fn tick(
    paths: &SignalPaths,
    bound: Option<&str>,
    status: Option<&str>,
    reason: Option<&str>,
    paused_since_ms: u64,
    plan: Option<&serde_json::Value>,
) -> Option<(String, Verdict)> {
    let eligible = bound.is_some()
        && status == Some("paused")
        && reason == Some("action_failed")
        && plan
            .map(|p| p["steps"].as_array().is_some_and(|s| !s.is_empty()))
            .unwrap_or(false)
        && unix_ms().saturating_sub(paused_since_ms) < PROBE_GIVE_UP_MS;
    let Some(id) = bound.filter(|_| eligible) else {
        if let Ok(mut guard) = PROBE.lock() {
            *guard = None;
        }
        return None;
    };
    let now = unix_ms();
    let mut guard = PROBE.lock().ok()?;
    let entry = guard.get_or_insert_with(|| {
        (
            id.to_string(),
            ProbeState {
                last_ms: 0,
                verdict: Verdict::Pending,
                auto_used: 0,
            },
        )
    });
    if entry.0 != id {
        *entry = (
            id.to_string(),
            ProbeState {
                last_ms: 0,
                verdict: Verdict::Pending,
                auto_used: 0,
            },
        );
    }
    let state = &mut entry.1;
    if now.saturating_sub(state.last_ms) < PROBE_INTERVAL_MS {
        return Some((id.to_string(), state.verdict));
    }
    state.last_ms = now;
    let plan_value = plan?;
    // 阻塞步骤 = 首个未完成步骤(与引擎口径一致);取其 before/verify。
    let flow = super::flow_ui::read(paths, id)?;
    let steps = flow["steps"].as_array()?;
    let plan_steps = plan_value["steps"].as_array()?;
    let index = steps
        .iter()
        .position(|s| s["status"] != "completed")
        .or_else(|| steps.len().checked_sub(1))?;
    let run_probe = |invocation: Option<&serde_json::Value>| -> Option<bool> {
        let invocation = invocation?;
        let mut args = vec![invocation["command"].as_str()?.to_string()];
        if let Some(list) = invocation["args"].as_array() {
            args.extend(list.iter().filter_map(|a| a.as_str().map(str::to_string)));
        }
        run_actl(args).ok()
    };
    let before_ok = run_probe(plan_steps.get(index).and_then(|s| s.get("before")));
    let verify_ok = run_probe(plan_steps.get(index).and_then(|s| s.get("verify")));
    state.verdict = verdict(before_ok, verify_ok);
    actl_core::signal_session::flow_event(
        "repair_probe",
        serde_json::json!({
            "step_index": index,
            "before_ok": before_ok,
            "verify_ok": verify_ok,
            "verdict": format!("{:?}", state.verdict).to_lowercase(),
        }),
    );
    Some((id.to_string(), state.verdict))
}

/// auto_resume 是否应触发:显式声明 + Retriable + 预算内;扣减预算并返回真。
pub fn consume_auto_resume(id: &str, declared: bool, v: Verdict) -> bool {
    if !declared || v != Verdict::Retriable {
        return false;
    }
    let Ok(mut guard) = PROBE.lock() else {
        return false;
    };
    let Some(state) = guard
        .as_mut()
        .filter(|(pid, _)| pid == id)
        .map(|e| &mut e.1)
    else {
        return false;
    };
    if state.auto_used < AUTO_RESUME_BUDGET {
        state.auto_used += 1;
        true
    } else {
        false
    }
}

fn run_actl(args: Vec<String>) -> Result<bool, ()> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()
        .map_err(|_| ())?
        .with_file_name("actl.exe");
    let output = std::process::Command::new(exe)
        .args(&args)
        .env("ACTL_SILENT_PROBE", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x08000000)
        .output()
        .map_err(|_| ())?;
    let body: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|_| ())?;
    Ok(output.status.success() && body["ok"] == true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verdict_prioritizes_already_done_over_retriable() {
        // 双判据核心:verify 过(结果已存在)优先于 before 过——防代做双重生效。
        assert_eq!(verdict(Some(true), Some(true)), Verdict::AlreadyDone);
        assert_eq!(verdict(Some(false), Some(true)), Verdict::AlreadyDone);
        assert_eq!(verdict(Some(true), None), Verdict::Retriable);
        assert_eq!(verdict(Some(true), Some(false)), Verdict::Retriable);
        assert_eq!(verdict(None, None), Verdict::Pending);
        assert_eq!(verdict(Some(false), Some(false)), Verdict::Pending);
    }
}
