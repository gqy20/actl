use actl_core::activity::*;
use actl_core::signal_session::{ActionGuard, SignalSession};
use actl_core::state::*;

#[test]
fn display_v3_contract_and_action_ack_identity() {
    use actl_core::display::*;
    let l = DisplayLease {
        protocol: DISPLAY_PROTOCOL,
        build: "0.1.5".into(),
        pid: 42,
        epoch: "display-42".into(),
        state: "ready".into(),
        ts_ms: 100,
    };
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/display_v3.json")).unwrap();
    assert_eq!(serde_json::to_value(l).unwrap(), expected);
    let p = paths();
    let now = unix_ms();
    let ack = DisplayAck {
        epoch: "epoch-a".into(),
        call_id: "call-a".into(),
        action_id: 7,
        ts_ms: now,
    };
    p.write_display("ack-call-a.json", &ack).unwrap();
    assert!(p.acknowledged("call-a", 7, "epoch-a", now));
    assert!(!p.acknowledged("call-a", 8, "epoch-a", now));
    assert!(!p.acknowledged("call-a", 7, "epoch-b", now));
    assert!(!p.acknowledged("call-b", 7, "epoch-a", now));
    assert!(!p.acknowledged("call-a", 7, "epoch-a", now + 1));
    std::fs::remove_dir_all(p.dir).unwrap();
}

fn paths() -> SignalPaths {
    SignalPaths::at(std::env::temp_dir().join(format!(
        "actl-signal-{}-{}",
        std::process::id(),
        actl_core::snapshot::new_snapshot_id()
    )))
}

#[test]
fn v2_signal_golden_and_legacy_read() {
    let legacy =
        r#"{"pid":42,"command":"paste","app":"Notepad","step":null,"phase":"done","ts_ms":1}"#;
    let mut state: SessionState = serde_json::from_str(legacy).unwrap();
    assert_eq!(state.phase, Phase::Done);
    assert!(state.action.is_none());
    state.version = 2;
    state.call_id = "call-42".into();
    state.seq = 3;
    state.started_ms = 1;
    state.action = Some(ActionState {
        id: 1,
        seq: 2,
        kind: "paste".into(),
        phase: ActionPhase::Delivered,
        effects: InputEffects {
            keyboard: true,
            clipboard: ClipboardEffect::Read,
            focus: true,
            ..Default::default()
        },
        target: ActionTarget {
            window: Some("Notepad".into()),
            ..Default::default()
        },
        ts_ms: 2,
        expires_ms: 2002,
    });
    let actual = serde_json::to_value(&state).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("golden/signal_v2.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn heartbeat_keeps_long_call_alive_without_resetting_start_or_action() {
    let p = paths();
    let mut call = SignalSession::start(p.clone(), "wait", None);
    let first = p.read_state().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let next = p.read_state().unwrap();
    assert!(next.ts_ms > first.ts_ms);
    assert_eq!(next.started_ms, first.started_ms);
    assert_eq!(next.call_id, first.call_id);
    call.finish(Phase::Done, None);
    let terminal = p.read_state().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(p.read_state().unwrap().ts_ms, terminal.ts_ms);
    let _ = std::fs::remove_dir_all(p.dir);
}

#[test]
fn simultaneous_publishers_keep_both_calls() {
    let p = paths();
    let mut a = SignalSession::start(p.clone(), "wait", Some("A"));
    let p2 = p.clone();
    std::thread::spawn(move || {
        let mut b = SignalSession::start(p2, "snapshot", Some("B"));
        b.finish(Phase::Done, None);
    })
    .join()
    .unwrap();
    let calls = p.read_calls();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .any(|s| s.app.as_deref() == Some("A") && s.phase == Phase::Running)
    );
    a.finish(Phase::Done, None);
    let _ = std::fs::remove_dir_all(p.dir);
}

#[test]
fn fast_actions_survive_poll_gaps_are_deduplicated_and_not_replayed() {
    let p = paths();
    let mut session = SignalSession::start(p.clone(), "batch", None);
    for _ in 0..40 {
        let mut a = ActionGuard::begin(
            "click",
            InputEffects {
                pointer: true,
                ..Default::default()
            },
            ActionTarget {
                point: Some([-40, 250]),
                ..Default::default()
            },
        );
        a.executing();
        a.delivered();
    }
    session.finish(Phase::Done, None);
    let states = p.read_calls();
    assert_eq!(states[0].recent_actions.len(), 32);
    let mut cursor = ActionCursor::new(0);
    assert_eq!(cursor.take(&states, unix_ms()).len(), 32);
    assert!(cursor.take(&states, unix_ms()).is_empty());
    assert!(
        ActionCursor::new(unix_ms() + 1)
            .take(&states, unix_ms() + 1)
            .is_empty()
    );
    assert!(
        ActionCursor::new(0)
            .take(&states, unix_ms() + 3000)
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(p.dir);
}

#[test]
fn cancelled_and_partial_actions_never_generate_click_feedback() {
    let p = paths();
    let mut s = SignalSession::start(p.clone(), "type", None);
    {
        let _a = ActionGuard::begin("click", Default::default(), Default::default());
        p.request_stop();
    }
    assert_eq!(
        p.read_state().unwrap().action.unwrap().phase,
        ActionPhase::Cancelled
    );
    p.clear_stop();
    let mut a = ActionGuard::begin("type", Default::default(), Default::default());
    a.executing();
    a.partial();
    s.finish(Phase::Done, None);
    assert!(
        ActionCursor::new(0)
            .take(&p.read_calls(), unix_ms())
            .is_empty()
    );
    let _ = std::fs::remove_dir_all(p.dir);
}
