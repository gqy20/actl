use actl_core::{
    history::{Journal, query},
    log_health, log_integrity,
    state::{Phase, SessionState, SignalPaths},
};
use serde_json::{Value, json};
fn root() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "log-meta-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn open_failure_has_health_even_when_history_cannot_be_read() {
    let root = root();
    std::fs::write(root.join("history"), "block").unwrap();
    assert!(Journal::open(&root, "call", Some("task".into())).is_err());
    let h = log_health::read(&root, Some("task"));
    assert_eq!(h["status"], "degraded");
    assert_eq!(h["failures"][0]["stage"], "journal_open");
    assert_eq!(
        query(&root, None, None, 20, false)
            .unwrap_err()
            .evidence
            .unwrap()["logging_health"]["status"],
        "degraded"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn oversized_event_disables_journal_and_counts_dropped_events() {
    let root = root();
    let mut j = Journal::open(&root, "call", Some("task".into())).unwrap();
    j.record("call_started", json!({}));
    j.record("private", json!({"content":"X".repeat(1024*1024)}));
    j.record("call_finished", json!({}));
    let result = query(&root, Some("task"), None, 20, false).unwrap();
    assert_eq!(result["calls"][0]["integrity"], "unfinished");
    assert_eq!(result["logging_health"]["failures"][0]["dropped_events"], 2);
    assert_eq!(
        result["logging_health"]["failures"][0]["stage"],
        "journal_write"
    );
    assert!(!result.to_string().contains("content"));
    drop(j);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn state_write_failure_is_visible_without_changing_business_state() {
    let root = root();
    std::fs::write(root.join("calls"), "block").unwrap();
    let mut state = SessionState::now(123, "status", None, None, Phase::Done);
    state.version = 2;
    state.call_id = "call".into();
    SignalPaths::at(&root).write_state(&state);
    assert_eq!(
        log_health::read(&root, None)["failures"][0]["stage"],
        "state_write"
    );
    assert_eq!(state.phase, Phase::Done);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn missing_or_corrupt_health_never_guarantees_complete_logging() {
    let root = root();
    let h = log_health::read(&root, None);
    assert_eq!(h["status"], "no_recorded_failure");
    assert_eq!(h["complete_logging_guaranteed"], false);
    std::fs::create_dir(root.join("log-health")).unwrap();
    std::fs::write(root.join("log-health/broken.json"), "{").unwrap();
    assert_eq!(log_health::read(&root, None)["status"], "unknown");
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn integrity_rejects_gaps_identity_changes_and_extra_boundaries() {
    let events = vec![
        json!({"version":1,"call_id":"c","task_id":"t","seq":1,"event":"call_started"}),
        json!({"version":1,"call_id":"c","task_id":"t","seq":2,"event":"call_finished"}),
    ];
    assert_eq!(log_integrity::check(&events, true), "complete");
    for (key, value, expected) in [
        ("seq", json!(1), "sequence_gap_or_duplicate"),
        ("call_id", json!("other"), "identity_or_version_mismatch"),
        ("task_id", json!("other"), "identity_or_version_mismatch"),
        ("event", json!("call_started"), "invalid_boundary"),
    ] {
        let mut broken = events.clone();
        broken[1][key] = value;
        assert_eq!(log_integrity::check(&broken, true), expected);
    }
    assert_eq!(log_integrity::check(&events, false), "malformed_json");
    let mut extra = events.clone();
    let mut last = events[1].clone();
    last["seq"] = json!(3);
    extra.push(last);
    assert_eq!(log_integrity::check(&extra, true), "invalid_boundary");
    let expected: Value =
        serde_json::from_str(include_str!("golden/log_integrity_v1.json")).unwrap();
    assert_eq!(
        json!({"complete":false,"integrity":log_integrity::check(&extra,true)}),
        expected
    );
}
