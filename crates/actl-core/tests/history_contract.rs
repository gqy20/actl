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
