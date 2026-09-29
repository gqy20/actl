use actl_core::{state::SignalPaths, stop::Invocation};

fn paths() -> SignalPaths {
    SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()))
}

#[test]
fn stopped_invocation_never_revives_after_recovery() {
    let p = paths();
    let old = Invocation::capture(&p);
    p.request_stop().unwrap();
    assert!(old.check(&p).is_err());
    p.recover_execution().unwrap();
    assert!(old.check(&p).is_err());
    assert!(Invocation::capture(&p).check(&p).is_ok());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn new_observation_can_run_during_stop_but_permission_stays_revoked() {
    let p = paths();
    p.request_stop().unwrap();
    let new = Invocation::capture(&p);
    assert!(new.check(&p).is_ok());
    assert!(p.stop_requested());
    let generation = new.generation().unwrap();
    p.acknowledge_stop(&generation).unwrap();
    assert!(!p.stop_requested());
    assert!(new.check(&p).is_ok());
    p.request_stop().unwrap();
    assert!(p.acknowledge_stop(&generation).is_err());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn damaged_state_requires_explicit_repair_and_keeps_evidence() {
    let p = paths();
    std::fs::create_dir_all(&p.dir).unwrap();
    std::fs::write(p.stop_file(), "legacy timestamp").unwrap();
    let old = Invocation::capture(&p);
    assert!(old.generation().is_err());
    let report = p.recover_execution().unwrap();
    assert!(report["archived"].as_array().unwrap().len() == 1);
    assert!(!p.stop_requested());
    assert!(old.check(&p).is_err());
    assert!(Invocation::capture(&p).check(&p).is_ok());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn stop_record_contract_and_repeated_stop_cancel_new_observations() {
    let p = paths();
    p.request_stop_from("hotkey", Some("session-a")).unwrap();
    let invocation = Invocation::capture(&p);
    let mut v = serde_json::to_value(p.stop_record().unwrap()).unwrap();
    v["requested_ms"] = 123.into();
    v["pid"] = 42.into();
    v["generation"] = "generation-a".into();
    assert_eq!(
        v,
        serde_json::from_str::<serde_json::Value>(include_str!("golden/stop_record.json")).unwrap()
    );
    p.request_stop().unwrap();
    assert!(invocation.check(&p).is_err());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn a_stale_start_cannot_clear_a_newer_stop() {
    let p = paths();
    p.request_stop().unwrap();
    let g = p.execution_generation().unwrap();
    p.request_stop_from("hotkey", None).unwrap();
    let before = std::fs::read(p.stop_file()).unwrap();
    assert!(p.acknowledge_stop(&g).is_err());
    assert_eq!(before, std::fs::read(p.stop_file()).unwrap());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn repair_refuses_directory_state_and_does_not_remove_it() {
    let p = paths();
    std::fs::create_dir_all(p.stop_file()).unwrap();
    assert!(p.recover_execution().is_err());
    assert!(p.stop_file().is_dir());
    std::fs::remove_dir_all(p.dir).unwrap();
}

#[test]
fn status_is_one_consistent_snapshot_during_concurrent_stop_and_recovery() {
    let p = paths();
    p.execution_generation().unwrap();
    let writer = SignalPaths::at(p.dir.clone());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let start = barrier.clone();
    let thread = std::thread::spawn(move || {
        start.wait();
        // 30 轮 stop/recover 已足以与 1000 次采样竞争出混合快照;更多轮只
        // 增加 Windows 下归档重命名的文件 IO 耗时(100 轮曾把本文件拖到 ~5s)
        for _ in 0..30 {
            writer.request_stop().unwrap();
            writer.recover_execution().unwrap();
        }
    });
    barrier.wait();
    let mut inconsistent = None;
    for _ in 0..1000 {
        let status = p.execution_status();
        let valid = match status["state"].as_str() {
            Some("ready") => status["stop"].is_null(),
            Some("stopped") => {
                status["stop"].is_object() && status["stop"]["generation"] == status["generation"]
            }
            _ => false,
        };
        if !valid {
            inconsistent = Some(status);
            break;
        }
    }
    thread.join().unwrap();
    std::fs::remove_dir_all(p.dir).unwrap();
    assert!(
        inconsistent.is_none(),
        "mixed execution snapshot: {inconsistent:?}"
    );
}
