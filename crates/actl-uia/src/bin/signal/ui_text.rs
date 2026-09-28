//! User-facing summaries. Diagnostic IDs and evidence remain in the expanded view.
use actl_core::state::SignalPaths;
use serde_json::Value;

pub struct Summary {
    pub title: String,
    pub status: String,
    pub body: String,
}

pub fn plan(paths: &SignalPaths, id: &str) -> Option<Value> {
    if !actl_core::history::valid_id(id) {
        return None;
    }
    let path = paths.dir.join("flows").join(id).join("plan.json");
    if std::fs::metadata(&path).ok()?.len() > 1_048_576 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
pub fn task_title(plan: Option<&Value>) -> String {
    plan.and_then(|p| p["name"].as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("当前任务")
        .replace(['\r', '\n'], " ")
}
pub fn reason(reason: &str) -> &'static str {
    match reason {
        "checkpoint" | "start_paused" => "等待你确认后继续。",
        "pause_requested" | "user_paused" => "你已请求暂停。",
        "human_takeover_or_grant_expired" => "检测到其他输入或授权已过期，请重新确认。",
        "handoff_pending" => "尚未获得桌面授权；若提示已关闭，请重新发起。",
        "user_deferred" => "本次暂不开始。",
        "stop_requested" => "已启用紧急停止；解除后仍需另行继续。",
        "precondition_failed" => "操作条件未满足，请查看详细信息。",
        "resume_check_failed" => "恢复检查未通过，请核对目标后再继续。",
        "binding_validation_failed" => "步骤输入未通过检查，请查看详细信息。",
        "action_failed" => "操作未完成，请先核对已有结果。",
        "verification_failed" => "结果未通过核对，不会自动重做。",
        _ => "需要处理，请查看详细信息。",
    }
}
pub fn summary(state: Option<&Value>, plan: Option<&Value>, stopped: bool) -> Summary {
    let title = task_title(plan);
    let Some(state) = state else {
        return Summary {
            title,
            status: if stopped {
                "紧急停止已启用"
            } else {
                "状态未确认"
            }
            .into(),
            body: if stopped {
                "任务状态未确认。解除急停不会自动继续。"
            } else {
                "暂时无法读取任务状态，请稍后查看。"
            }
            .into(),
        };
    };
    let status = state["status"].as_str().unwrap_or("");
    let mut lines = Vec::new();
    if let Some(steps) = state["steps"].as_array().filter(|s| !s.is_empty()) {
        let completed = steps.iter().filter(|s| s["status"] == "completed").count();
        lines.push(format!("已完成 {completed} / {} 步", steps.len()));
        let current = steps
            .iter()
            .find(|s| {
                matches!(
                    s["status"].as_str(),
                    Some("running" | "verifying" | "uncertain" | "failed")
                )
            })
            .or_else(|| steps.iter().find(|s| s["status"] == "pending"));
        if let Some(step) = current {
            let title = plan
                .and_then(|p| p["steps"].as_array())
                .and_then(|steps| steps.iter().find(|s| s["id"] == step["id"]))
                .and_then(|s| s["title"].as_str())
                .filter(|s| !s.trim().is_empty());
            if let Some(title) = title {
                lines.push(title.replace(['\r', '\n'], " "));
            }
        }
        if status == "completed" {
            let verified = steps.iter().filter(|s| s["verified"] == true).count();
            lines.push(format!("其中 {verified} 步有核验记录。"));
        }
    }
    if stopped {
        lines.push("解除急停后，任务仍需另行继续。".into());
    } else if let Some(r) = state["reason"].as_str() {
        lines.push(reason(r).into());
    } else if status == "needs_review" {
        lines.push("上一步结果未确认，请先核对。".into());
    } else if status == "running" {
        lines.push("任务执行中；暂停需等待当前操作结束。".into());
    } else if matches!(status, "failed" | "stopped") {
        lines.push("请查看详细信息，由调用方核对后接续。".into());
    }
    Summary {
        title,
        status: if stopped {
            "紧急停止已启用"
        } else {
            super::flow_ui::status_name(status)
        }
        .into(),
        body: lines.join("\r\n"),
    }
}

pub fn feedback(message: &str, status: Option<&str>) -> String {
    match message {
        "" | "流程状态已更新" => String::new(),
        "正在继续流程" if status == Some("running") => String::new(),
        "正在继续流程" => "正在检查，请稍候…".into(),
        "正在请求暂停" | "已请求暂停，等待当前操作结束" if status == Some("paused") => {
            String::new()
        }
        "正在请求暂停" | "已请求暂停，等待当前操作结束" => {
            "正在暂停，等待当前操作结束。".into()
        }
        _ if message.contains('：') || message.contains('\n') => {
            "操作未完成，请展开详细信息。".into()
        }
        _ => message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn summary_uses_titles_and_does_not_claim_unverified_success() {
        let p = json!({"name":"整理会议记录", "steps":[{"id":"technical-id", "title":"写入纪要"}]});
        let s = json!({"status":"paused", "reason":"resume_check_failed", "steps":[{"id":"technical-id", "status":"pending"}]});
        let view = summary(Some(&s), Some(&p), false);
        assert_eq!(view.title, "整理会议记录");
        assert!(view.body.contains("写入纪要"));
        assert!(view.body.contains("恢复检查未通过"));
        assert!(!view.body.contains("technical-id"));
        let done =
            json!({"status":"completed", "steps":[{"status":"completed", "verified":false}]});
        assert!(
            summary(Some(&done), None, false)
                .body
                .contains("0 步有核验记录")
        );
        assert_eq!(summary(None, Some(&p), false).status, "状态未确认");
    }
    #[test]
    fn feedback_does_not_keep_saying_resuming_after_execution_started() {
        assert_eq!(feedback("正在继续流程", Some("running")), "");
        assert!(!feedback("正在继续流程", Some("paused")).is_empty());
        assert_eq!(feedback("已请求暂停，等待当前操作结束", Some("paused")), "");
        assert!(!feedback("INTERNAL：example", Some("paused")).is_empty());
    }
}
