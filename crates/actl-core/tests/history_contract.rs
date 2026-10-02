use actl_core::{
    history::*,
    signal_session::SignalSession,
    state::{Phase, SignalPaths},
};
use serde_json::{Value, json};
fn root() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "actl-history-{}",
        actl_core::snapshot::new_snapshot_id()
    ))
}

#[test]
fn task_timeline_skips_foreign_call_tails_but_reports_unknown_headers() {
    let root = root();
    let mut own = Journal::open(&root, "own", Some("mine".into())).unwrap();
    own.record("call_started", json!({"command":"status"}));
    own.record("call_finished", json!({}));
    let foreign = json!({"version":1,"call_id":"foreign","task_id":"other","seq":1,"ts_ms":1,
        "event":"call_started","data":{}});
    // A valid non-target identity allows skipping its entire payload. This is
    // task-scoped diagnostics, not a filesystem-wide integrity audit.
    std::fs::write(
        root.join("history/foreign.jsonl"),
        format!("{foreign}\n{{broken foreign tail\n"),
    )
    .unwrap();
    let result = read_timeline(&root, Some("mine"), 500).unwrap();
    assert_eq!(result["events"].as_array().unwrap().len(), 2);
    assert_eq!(result["incomplete_files"], 0);
    let expected: Value =
        serde_json::from_str(include_str!("golden/task_timeline_scope.json")).unwrap();
    assert_eq!(
        json!({"total":result["total"],"truncated":result["truncated"],
        "incomplete_files":result["incomplete_files"]}),
        expected
    );
    std::fs::write(root.join("history/unknown.jsonl"), "{broken header\n").unwrap();
    assert_eq!(
        read_timeline(&root, Some("mine"), 500).unwrap()["incomplete_files"],
        1
    );
    assert_eq!(
        read_timeline(&root, None, 500).unwrap()["incomplete_files"],
        2
    );
    drop(own);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn task_header_cache_invalidates_changed_files_and_falls_back_when_corrupt() {
    let root = root();
    std::fs::create_dir_all(root.join("history")).unwrap();
    let first = json!({"version":1,"call_id":"foreign","task_id":"other","seq":1,"ts_ms":1,
        "event":"call_started","data":{}});
    let path = root.join("history/foreign.jsonl");
    std::fs::write(&path, format!("{first}\n")).unwrap();
    assert_eq!(read_timeline(&root, Some("mine"), 500).unwrap()["total"], 0);
    assert!(root.join("history/cache/task-headers.json").exists());
    assert_eq!(read_timeline(&root, Some("mine"), 500).unwrap()["total"], 0);
    let mut changed = first;
    changed["task_id"] = json!("mine");
    changed["data"] = json!({"command":"status"});
    std::fs::write(&path, format!("{changed}\n")).unwrap();
    assert_eq!(read_timeline(&root, Some("mine"), 500).unwrap()["total"], 1);
    std::fs::write(
        root.join("history/cache/task-headers.json"),
        "{broken index",
    )
    .unwrap();
    assert_eq!(read_timeline(&root, Some("mine"), 500).unwrap()["total"], 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn finished_call_distinguishes_paused_workflow_from_completion() {
    let root = root();
    let mut session = SignalSession::start_with_task(
        SignalPaths::at(&root),
        "flow-resume",
        None,
        Some("run".into()),
    );
    actl_core::signal_session::workflow(serde_json::from_value(json!({"run_id":"run","step_id":"a","title":"private","status":"paused","reason":"resume_check_failed","error_code":"NOT_FOUND"})).unwrap());
    session.finish(Phase::Done, None);
    let result = query(&root, Some("run"), None, 20, false).unwrap();
    let events = result["calls"][0]["events"].as_array().unwrap();
    let data = &events.last().unwrap()["data"];
    assert_eq!(data["workflow_status"], "paused");
    assert_eq!(data["workflow_completed"], false);
    assert_eq!(data["workflow_error_code"], "NOT_FOUND");
    assert!(data["error_code"].is_null());
    let mut normalized = data.clone();
    normalized["duration_ms"] = json!(0);
    assert_eq!(
        normalized,
        serde_json::from_str::<Value>(include_str!("golden/workflow_finished_paused_v1.json"))
            .unwrap()
    );
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workflow_actions_and_report_evidence_can_be_traced() {
    let root = root();
    let mut session = SignalSession::start_with_task(
        SignalPaths::at(&root),
        "flow-run",
        None,
        Some("run-1".into()),
    );
    actl_core::signal_session::workflow(
        serde_json::from_value(
            json!({"run_id":"run-1","step_id":"submit","title":"Private task","status":"running"}),
        )
        .unwrap(),
    );
    let mut action = actl_core::signal_session::ActionGuard::begin(
        "click",
        Default::default(),
        Default::default(),
    );
    action.delivered();
    session.finish(Phase::Done, None);
    let linked = query(&root, Some("run-1"), Some("submit"), 20, false).unwrap();
    let event = linked["calls"][0]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event"] == "action")
        .unwrap();
    assert_eq!(event["run_id"], "run-1");
    assert_eq!(event["step_id"], "submit");
    assert!(event["call_id"].is_string());
    assert!(event["data"]["action_id"].is_number());
    let mut normalized = event.clone();
    normalized["call_id"] = json!("call-1");
    normalized["ts_ms"] = json!(0);
    normalized["seq"] = json!(3);
    normalized["data"]["action_id"] = json!(1);
    assert_eq!(
        normalized,
        serde_json::from_str::<Value>(include_str!("golden/workflow_action_v1.json")).unwrap()
    );

    assert!(!linked.to_string().contains("Private task"));
    let evidence = root.join("result.txt");
    std::fs::write(&evidence, "test evidence").unwrap();
    let file = root.join("report.json");
    let mut report = json!({"task_id":"run-1","outcome":"completed","summary":"done","verification":"observed","call_ids":[],
        "artifacts":[{"id":"result","kind":"local_file","reference":evidence}],
        "feedback":[{"source":"agent","kind":"fact","text":"observed result","verified":true,"evidence_ids":["result"]}]});
    std::fs::write(&file, report.to_string()).unwrap();
    archive(&root, &file).unwrap();
    assert_eq!(
        query(&root, Some("run-1"), None, 20, true).unwrap()["reports"][0]["evidence_status"][0]["availability"],
        "available"
    );
    std::fs::remove_file(evidence).unwrap();
    assert_eq!(
        query(&root, Some("run-1"), None, 20, true).unwrap()["reports"][0]["evidence_status"][0]["availability"],
        "missing"
    );
    report["feedback"][0]["kind"] = json!("inference");
    std::fs::write(&file, report.to_string()).unwrap();
    assert!(archive(&root, &file).is_err());
    drop(action);
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn journal_preserves_task_events_without_private_content_or_heartbeats() {
    let root = root();
    let mut session = SignalSession::start_with_task(
        SignalPaths::at(&root),
        "type",
        Some("PRIVATE TITLE"),
        Some("task-1".into()),
    );
    let mut action = actl_core::signal_session::ActionGuard::begin(
        "type",
        Default::default(),
        actl_core::activity::ActionTarget {
            window: Some("PRIVATE TITLE".into()),
            ..Default::default()
        },
    );
    action.executing();
    action.delivered();
    session.finish(Phase::Done, None);
    let history = read_history(&root, Some("task-1"), 10).unwrap();
    assert_eq!(history["calls"].as_array().unwrap().len(), 1);
    assert_eq!(history["calls"][0]["complete"], true);
    assert!(!history.to_string().contains("PRIVATE"));
    let events = history["calls"][0]["events"].as_array().unwrap();
    assert_eq!(events.len(), 5);
    let mut normalized = events.clone();
    for event in &mut normalized {
        event["call_id"] = json!("call-1");
        event["ts_ms"] = json!(0);
        if event["event"] == "call_started" {
            event["data"]["version"] = json!("test");
        }
        if event["event"] == "call_finished" {
            event["data"]["duration_ms"] = json!(0);
        }
    }
    let expected: Value = serde_json::from_str(include_str!("golden/history_v1.json")).unwrap();
    assert_eq!(json!(normalized), expected);
    assert!(
        read_history(&root, Some("other"), 10).unwrap()["calls"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    drop(action);
    drop(session);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn reports_are_validated_and_revisions_do_not_overwrite() {
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("report.json");
    let mut report = json!({"task_id":"task-1","outcome":"partial","summary":"8 of 10 done","verification":"two results missing","call_ids":["call-1"],"feedback":[{"source":"agent","text":"retry after observation","verified":false}]});
    std::fs::write(&input, report.to_string()).unwrap();
    let a = archive(&root, &input).unwrap();
    let b = archive(&root, &input).unwrap();
    assert_ne!(a, b);
    assert!(a.exists() && b.exists());
    let stored: Value = serde_json::from_slice(&std::fs::read(a).unwrap()).unwrap();
    assert_eq!(stored["source"], "caller");
    report["task_id"] = json!("../escape");
    std::fs::write(&input, report.to_string()).unwrap();
    assert_eq!(
        archive(&root, &input).unwrap_err().code,
        actl_core::ErrorCode::Protocol
    );
    std::fs::write(&input, vec![b' '; 65_537]).unwrap();
    assert!(archive(&root, &input).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn incomplete_calls_and_capacity_are_visible() {
    let root = root();
    let mut journal = Journal::open(&root, "call-1", None).unwrap();
    journal.record("call_started", json!({}));
    assert_eq!(
        read_history(&root, None, 10).unwrap()["incomplete_files"],
        1
    );
    assert!(cleanup(&root.join("history"), u64::MAX, 0).is_err());
    assert!(root.join("history/call-1.jsonl").exists());
    assert!(read_history(&root, None, 101).is_err());
    drop(journal);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn retention_cleans_old_history_but_preserves_live_calls_and_recovery_records() {
    let root = root();
    let dir = root.join("history");
    std::fs::create_dir_all(&dir).unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 86_400);
    for name in ["old", "live"] {
        let path = dir.join(format!("{name}.jsonl"));
        std::fs::write(&path, "record").unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old))
            .unwrap();
    }
    let mut state = actl_core::state::SessionState::now(1, "wait", None, None, Phase::Running);
    state.version = 2;
    state.call_id = "live".into();
    SignalPaths::at(&root).write_state(&state);
    std::fs::create_dir_all(root.join("desktop")).unwrap();
    std::fs::write(root.join("desktop/recovery.json"), "keep").unwrap();
    cleanup(&dir, 7 * 86_400_000, 100).unwrap();
    assert!(!dir.join("old.jsonl").exists());
    assert!(dir.join("live.jsonl").exists());
    assert!(root.join("desktop/recovery.json").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_calls_have_independent_sequences() {
    let root = root();
    let workers: Vec<_> = (0..8)
        .map(|n| {
            let root = root.clone();
            std::thread::spawn(move || {
                let mut j =
                    Journal::open(&root, &format!("call-{n}"), Some("shared".into())).unwrap();
                j.record("call_started", json!({}));
                j.record("call_finished", json!({}));
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let history = read_history(&root, Some("shared"), 100).unwrap();
    assert_eq!(history["calls"].as_array().unwrap().len(), 8);
    assert_eq!(history["incomplete_files"], 0);
    std::fs::remove_dir_all(root).unwrap();
}
