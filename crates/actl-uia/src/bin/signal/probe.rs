//! 修复检测探测(批次4):action_failed 暂停期间,由显示端以 5s 节奏静默运行
//! 失败步骤的 before/verify(只读),把"什么时候可以点继续"点亮。
//! 双判据语义(防代做双重生效):before 过=障碍已清除,可重试;verify 过=结果
//! 已存在(可能被手动完成),继续前需人工核对。auto_resume 仅在
//! before 过且 verify 未过时触发,且仅当流程显式声明 auto_resume: action_failed,
//! 预算 2 次/运行(从 probe.jsonl 的成功派发记录计数)。
//! 红线:探测只跑只读命令;user_paused/checkpoint/needs_review 永不探测续跑;
//! 急停/停止代次由 flow-resume 路径自身核验,探测不绕过任何护栏。
use actl_core::state::{SignalPaths, unix_ms};

/// 探测节流与放弃边界。
const PROBE_INTERVAL_MS: u64 = 5000;
const PROBE_GIVE_UP_MS: u64 = 10 * 60 * 1000;
/// auto_resume 预算(每流程,按持久记录累计)。预授权(全局常驻同意,
/// 2026-10-14 信任对齐)是同意层最高档——预算放宽换吞吐,授予审计照记。
const AUTO_RESUME_BUDGET: u32 = 2;
const AUTO_RESUME_BUDGET_PREAUTH: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// before 通过、verify 未通过:障碍已清除,可安全重试(auto_resume 条件)。
    Retriable,
    /// verify 通过:结果已存在,可能是手动完成——继续重发有双重生效风险。
    AlreadyDone,
    /// 判据未满足/不可判定:无提示(不重复念叨"还没修好")。
    Pending,
}

/// 纯决策(可测):双判据 → 裁决。
pub fn verdict(before_ok: Option<bool>, verify_ok: Option<bool>) -> Verdict {
    match (before_ok, verify_ok) {
        (_, Some(true)) => Verdict::AlreadyDone,
        (Some(true), _) => Verdict::Retriable,
        _ => Verdict::Pending,
    }
}

/// 每 tick(100ms)驱动;返回 (流程 id, 裁决) 供显示层高亮。
/// 探测目标选择(真机验收修正):显示端绑定(visible-flow/最近调用)在最近流程
/// 完成后会孤儿化更早的暂停流程;discover 在多未完结残留(历史验证轮)下恒 None。
/// 探测自己的确定性目标 = 给予窗口内**最新更新**的 paused+action_failed 流程——
/// 与显示端"多候选不猜"的防误绑纪律互不影响(探测只读,不改按钮)。
fn newest_probe_target(paths: &SignalPaths) -> Option<String> {
    let now = unix_ms();
    let dir = std::fs::read_dir(paths.dir.join("flows")).ok()?;
    let mut best: Option<(u64, String)> = None;
    for entry in dir.flatten() {
        let id = entry.file_name().into_string().ok()?;
        let Some(state) = super::flow_ui::read(paths, &id) else {
            continue;
        };
        if state["status"] != "paused" || state["reason"] != "action_failed" {
            continue;
        }
        let updated = state["updated_ms"].as_u64()?;
        if now.saturating_sub(updated) < PROBE_GIVE_UP_MS
            && best.as_ref().is_none_or(|(t, _)| updated > *t)
        {
            best = Some((updated, id));
        }
    }
    best.map(|(_, id)| id)
}

/// paused_since_ms 用于绑定版本与放弃边界；后台选择并探测合格目标。
pub fn tick(
    paths: &SignalPaths,
    bound: Option<&str>,
    status: Option<&str>,
    reason: Option<&str>,
    paused_since_ms: u64,
    plan: Option<&serde_json::Value>,
) -> Option<(String, Verdict)> {
    let _trace = actl_core::trace::scope("signal.repair_probe");
    #[derive(Clone, PartialEq, Eq)]
    struct Key {
        root: std::path::PathBuf,
        bound: Option<String>,
        status: Option<String>,
        reason: Option<String>,
        updated: u64,
        generation: String,
    }
    #[derive(Clone)]
    struct Result {
        id: String,
        verdict: Verdict,
        updated: u64,
    }
    thread_local! {
        static WORKER: std::cell::RefCell<super::probe_worker::Worker<Key, Option<Result>>> = std::cell::RefCell::new(Default::default());
    }
    let generation = paths.execution_generation().ok()?;
    if paths.stop_requested() {
        return None;
    }
    let key = Key {
        root: paths.dir.clone(),
        bound: bound.map(str::to_string),
        status: status.map(str::to_string),
        reason: reason.map(str::to_string),
        updated: paused_since_ms,
        generation,
    };
    let input = key.clone();
    let worker_paths = paths.clone();
    let plan = plan.cloned();
    let result = WORKER.with(|worker| {
        worker.borrow_mut().poll(
            key,
            std::time::Duration::from_millis(PROBE_INTERVAL_MS),
            move || {
                let (id, verdict, updated) = evaluate(
                    &worker_paths,
                    input.bound.as_deref(),
                    input.status.as_deref(),
                    input.reason.as_deref(),
                    input.updated,
                    plan.as_ref(),
                )?;
                Some(Result {
                    id,
                    verdict,
                    updated,
                })
            },
        )
    });
    let result = match result {
        Ok(result) => result??,
        Err(error) => {
            eprintln!("INTERNAL: repair probe unavailable: {error}");
            return None;
        }
    };
    // A completed/stopped/revised task must never inherit a delayed worker verdict.
    let state = super::flow_ui::read(paths, &result.id)?;
    if revision(&state) != Some(result.updated) {
        return None;
    }
    Some((result.id, result.verdict))
}

fn revision(state: &serde_json::Value) -> Option<u64> {
    if state["status"] != "paused" || state["reason"] != "action_failed" {
        return None;
    }
    state["updated_ms"]
        .as_u64()
        .filter(|at| unix_ms().saturating_sub(*at) < PROBE_GIVE_UP_MS)
}

fn evaluate(
    paths: &SignalPaths,
    bound: Option<&str>,
    status: Option<&str>,
    reason: Option<&str>,
    paused_since_ms: u64,
    plan: Option<&serde_json::Value>,
) -> Option<(String, Verdict, u64)> {
    let eligible = bound.is_some()
        && status == Some("paused")
        && reason == Some("action_failed")
        && plan
            .map(|p| p["steps"].as_array().is_some_and(|s| !s.is_empty()))
            .unwrap_or(false)
        && unix_ms().saturating_sub(paused_since_ms) < PROBE_GIVE_UP_MS;
    let id = bound
        .filter(|_| eligible)
        .map(str::to_string)
        .or_else(|| newest_probe_target(paths))?;
    let id = id.as_str();
    // 回落目标(非显示端绑定)时,plan 参数属于原绑定——按探测目标重读。
    let plan_value = match plan {
        Some(p) if bound == Some(id) => p.clone(),
        _ => super::ui_text::plan(paths, id)?,
    };
    // 阻塞步骤 = 首个未完成步骤(与引擎口径一致);取其 before/verify。
    let flow = super::flow_ui::read(paths, id)?;
    let updated = revision(&flow)?;
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
    let current = super::flow_ui::read(paths, id)?;
    if revision(&current) != Some(updated) {
        return None;
    }
    let verdict = verdict(before_ok, verify_ok);
    record(
        paths,
        id,
        "repair_probe",
        &serde_json::json!({
            "step_index": index,
            "before_ok": before_ok,
            "verify_ok": verify_ok,
            "verdict": format!("{verdict:?}").to_lowercase(),
        }),
    );
    Some((id.to_string(), verdict, updated))
}

/// 探测/自动续跑证据落盘:flows/<id>/probe.jsonl(伴侣进程无调用 journal,
/// flow_event 在这里不落——真机验收发现事件丢失,改为独立追加)。
pub fn record(paths: &SignalPaths, id: &str, event: &str, data: &serde_json::Value) {
    use std::io::Write;
    let line = serde_json::json!({
        "event": event, "ts_ms": unix_ms(), "data": data,
    });
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.dir.join("flows").join(id).join("probe.jsonl"))
    else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

/// auto_resume 是否应触发:显式声明 + Retriable + 预算内。预算从 probe.jsonl
/// **持久计数**已派发的 auto_resume 事件(真机验收修正:内存预算在探测目标切换
/// 或伴侣重启时被重置,多暂停流程残留下超发);记账 = 调用方在派发成功后 record。
pub fn consume_auto_resume(paths: &SignalPaths, id: &str, declared: bool, v: Verdict) -> bool {
    if !declared || v != Verdict::Retriable {
        return false;
    }
    // 2026-10-14 信任对齐:预授权(全局常驻同意)开着时预算放宽——信任档
    // 换吞吐,授予审计行照记;其余门(停止代次/急停/needs_review 永不
    // 自动续)不变
    let budget = if paths.preauthorized() {
        AUTO_RESUME_BUDGET_PREAUTH
    } else {
        AUTO_RESUME_BUDGET
    };
    let used = std::fs::read_to_string(paths.dir.join("flows").join(id).join("probe.jsonl"))
        .map(|text| {
            text.lines()
                .filter(|l| l.contains("\"event\":\"auto_resume\""))
                .count()
        })
        .unwrap_or(0);
    (used as u32) < budget
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
    fn auto_resume_budget_scales_with_preauthorization() {
        // 2026-10-14 信任对齐:预授权(同意层最高档)开着时预算 2→5,
        // 关闭时维持 2;显式声明与 Retriable 前置门不因此放宽
        let tmp = std::env::temp_dir().join(format!(
            "actl-probe-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let paths = actl_core::state::SignalPaths::at(&tmp);
        std::fs::create_dir_all(tmp.join("flows").join("f1")).unwrap();
        let probe = tmp.join("flows").join("f1").join("probe.jsonl");
        // 未预授权:3 次已用 → 拒(预算 2)
        std::fs::write(
            &probe,
            "\"event\":\"auto_resume\"
\"event\":\"auto_resume\"
\"event\":\"auto_resume\"
",
        )
        .unwrap();
        assert!(!consume_auto_resume(&paths, "f1", true, Verdict::Retriable));
        // 预授权开:同 3 次已用 → 允(预算 5)
        paths.set_preauth(true).unwrap();
        assert!(consume_auto_resume(&paths, "f1", true, Verdict::Retriable));
        // 5 次已用 → 预授权也拒
        std::fs::write(
            &probe,
            "\"event\":\"auto_resume\":\"n\"
"
            .repeat(6)
            .as_bytes(),
        )
        .unwrap();
        assert!(!consume_auto_resume(&paths, "f1", true, Verdict::Retriable));
        // 前置门不放宽:未声明 / 非 Retriable 仍拒
        assert!(!consume_auto_resume(
            &paths,
            "f1",
            false,
            Verdict::Retriable
        ));
        assert!(!consume_auto_resume(
            &paths,
            "f1",
            true,
            Verdict::AlreadyDone
        ));
        std::fs::remove_dir_all(&tmp).ok();
        // 清理显示端状态的 preauth 文件(测试目录内的,已在 remove_dir_all 内)
    }
    #[test]
    fn only_current_action_failure_can_use_a_worker_result() {
        let now = unix_ms();
        let mut state =
            serde_json::json!({"status":"paused","reason":"action_failed","updated_ms":now});
        assert_eq!(revision(&state), Some(now));
        for status in ["completed", "running", "stopped", "needs_review"] {
            state["status"] = serde_json::json!(status);
            assert_eq!(revision(&state), None);
        }
        state["status"] = serde_json::json!("paused");
        state["reason"] = serde_json::json!("pause_requested");
        assert_eq!(revision(&state), None);
        state["reason"] = serde_json::json!("action_failed");
        state["updated_ms"] = serde_json::json!(now - PROBE_GIVE_UP_MS);
        assert_eq!(revision(&state), None);
    }
    #[test]
    #[ignore = "read-only live discovery timing; set ACTL_PROBE_PROFILE_OUTPUT"]
    fn profile_discovery() -> Result<(), Box<dyn std::error::Error>> {
        let file = std::fs::File::create_new(std::env::var("ACTL_PROBE_PROFILE_OUTPUT")?)?;
        let paths = SignalPaths::default();
        let start = std::time::Instant::now();
        let target = newest_probe_target(&paths);
        serde_json::to_writer_pretty(
            file,
            &serde_json::json!({
                "duration_ms":start.elapsed().as_millis(),"target":target,
                "binary":std::env::current_exe()?,"ts_ms":unix_ms(),
                "flow_count":std::fs::read_dir(paths.dir.join("flows"))?.count(),
                "input_injected":false,"flow_execution":false
            }),
        )?;
        Ok(())
    }
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
