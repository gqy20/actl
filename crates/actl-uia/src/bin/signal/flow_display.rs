//! Keep the explicitly displayed task available after transient calls expire.
use actl_core::state::{Phase, SessionState, SignalPaths, WorkflowProgress};

pub(super) fn retain(paths: &SignalPaths, states: &mut Vec<SessionState>) {
    let pointer = paths.dir.join("visible-flow.json");
    let mut selected = std::fs::read(&pointer)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
    if let Some(call) = states
        .iter()
        .filter(|s| s.workflow.is_some())
        .max_by_key(|s| s.ts_ms)
        && let Some(flow) = &call.workflow
        && actl_core::history::valid_id(&flow.run_id)
        && selected
            .as_ref()
            .and_then(|s| s["observed_ms"].as_u64())
            .is_none_or(|ts| call.ts_ms > ts)
    {
        let value = serde_json::json!({"id":flow.run_id,"observed_ms":call.ts_ms});
        if let Err(error) = paths.write_display("visible-flow.json", &value) {
            eprintln!("INTERNAL: cannot retain visible workflow: {error}");
        }
        selected = Some(value);
    }
    let Some(id) = selected.as_ref().and_then(|s| s["id"].as_str()) else {
        return;
    };
    if states
        .iter()
        .any(|s| s.workflow.as_ref().is_some_and(|f| f.run_id == id))
    {
        return;
    }
    let Some(state) = super::flow_ui::read(paths, id) else {
        return;
    };
    let Some(status @ ("paused" | "needs_review" | "stopped")) = state["status"].as_str() else {
        return;
    };
    // 按钮状态时效:paused/stopped 是人工意图态,持久保留(暂停入口持久保留的本意);
    // needs_review 是"紧接着的核对动作",超时无人核对说明结果已被人工消化——
    // 不再注入,横条回空闲(状态文件与 flow-status 核对入口不受影响)。
    // 没有这条,历史待核对流程会永久钉在横条上出「检查并继续」按钮。
    if status == "needs_review"
        && let Some(updated) = state["updated_ms"].as_u64()
        && actl_core::state::unix_ms().saturating_sub(updated) > 30 * 60 * 1000
    {
        return;
    };
    let Some(updated) = state["updated_ms"].as_u64() else {
        return;
    };
    let plan = super::ui_text::plan(paths, id);
    let mut call = SessionState::now(0, "flow-status", None, None, Phase::Done);
    call.ts_ms = updated;
    call.workflow = Some(WorkflowProgress {
        run_id: id.into(),
        title: super::ui_text::task_title(plan.as_ref()),
        status: status.into(),
        reason: state["reason"].as_str().map(str::to_owned),
        step_id: None,
        error_code: None,
        actions: vec![
            if status == "needs_review" {
                "recheck"
            } else {
                "resume"
            }
            .into(),
        ],
    });
    states.push(call);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_needs_review_stops_pinning_the_bar_but_paused_stays() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let now = actl_core::state::unix_ms();
        for (id, status, updated) in [
            ("review-now", "needs_review", now),
            ("review-old", "needs_review", now - 31 * 60 * 1000),
            ("paused-old", "paused", now - 24 * 60 * 60 * 1000),
        ] {
            let dir = paths.dir.join("flows").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("state.json"),
                format!(r#"{{"status":"{status}","updated_ms":{updated},"steps":[]}}"#),
            )
            .unwrap();
            // 每个流程各自成为"最近展示"的指针,独立断言注入结果。
            std::fs::write(
                paths.dir.join("visible-flow.json"),
                format!(r#"{{"id":"{id}","observed_ms":{updated}}}"#),
            )
            .unwrap();
            let mut states = vec![];
            retain(&paths, &mut states);
            match id {
                "review-old" => assert!(
                    states.is_empty(),
                    "needs_review past 30 minutes must stop pinning the bar"
                ),
                _ => assert!(
                    states.len() == 1 && states[0].workflow.is_some(),
                    "{id} should stay visible"
                ),
            }
        }
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
    #[test]
    fn paused_task_survives_call_expiry_and_other_unfinished_tasks() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        for id in ["selected", "other"] {
            let dir = paths.dir.join("flows").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("state.json"),
                br#"{"status":"paused","reason":"pause_requested","updated_ms":100,"steps":[]}"#,
            )
            .unwrap();
        }
        let mut state = SessionState::now(1, "flow-run", None, None, Phase::Done);
        state.workflow = Some(WorkflowProgress {
            run_id: "selected".into(),
            title: "Task".into(),
            status: "paused".into(),
            reason: Some("pause_requested".into()),
            step_id: None,
            error_code: None,
            actions: vec!["resume".into()],
        });
        retain(&paths, &mut vec![state]);
        let mut states = vec![SessionState::now(2, "flow-status", None, None, Phase::Done)];
        retain(&paths, &mut states);
        let view =
            super::super::model::view(&states, actl_core::state::unix_ms(), None, Some(false));
        assert_eq!(
            view.control,
            super::super::model::Control::Continue {
                flow: "selected".into()
            }
        );
        std::fs::write(
            paths.dir.join("flows/selected/state.json"),
            br#"{"status":"completed","updated_ms":101,"steps":[]}"#,
        )
        .unwrap();
        let mut states = vec![];
        retain(&paths, &mut states);
        assert!(states.is_empty(), "completed task must not offer continue");
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
}
