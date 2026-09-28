//! The renderer must dismiss stale prompts even when the caller stops polling.
use super::*;

struct Fixture {
    paths: SignalPaths,
    panel: HWND,
}
impl Fixture {
    fn new() -> Self {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let generation = paths.execution_generation().unwrap();
        let request = Request {
            generation: generation.clone(),
            call: "lifecycle-call".into(),
            task: "lifecycle-task".into(),
            target: "owned test window".into(),
            ts_ms: unix_ms(),
        };
        paths.write_display("handoff-42.json", &request).unwrap();
        // Real HWND, kept hidden: no consent, keyboard hook, or input injection.
        let panel = unsafe { build_panel_with(Some(&request), &paths).unwrap() };
        SERVICE.with(|cell| {
            *cell.borrow_mut() = Service {
                generation: Some(generation),
                request: Some((42, request)),
                panel: Some(panel),
                ..Service::default()
            };
        });
        Self { paths, panel }
    }
    fn tick(&self) {
        unsafe {
            tick_with(&self.paths, true);
        }
    }
    fn assert_dismissed(&self) {
        SERVICE.with(|cell| {
            let s = cell.borrow();
            assert!(
                s.request.is_none(),
                "stale request remains without caller polling"
            );
            assert!(s.panel.is_none(), "stale prompt remains visible");
        });
        assert!(!unsafe { IsWindow(Some(self.panel)).as_bool() });
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        SERVICE.with(|cell| {
            let mut s = cell.borrow_mut();
            unsafe {
                close_panel(&mut s);
            }
            *s = Service::default();
        });
        std::fs::remove_dir_all(&self.paths.dir).unwrap();
    }
}

#[test]
fn renderer_dismisses_old_prompt_after_stop_without_caller_polling() {
    let f = Fixture::new();
    f.paths.request_stop().unwrap();
    f.tick();
    f.assert_dismissed();
    assert!(f.paths.stop_requested());
}

#[test]
fn renderer_dismisses_old_prompt_after_repair_without_caller_polling() {
    let f = Fixture::new();
    f.paths.recover_execution().unwrap();
    f.tick();
    f.assert_dismissed();
    assert!(!f.paths.stop_requested());
}

#[test]
fn renderer_dismisses_prompt_when_execution_state_is_corrupt() {
    let f = Fixture::new();
    std::fs::write(f.paths.dir.join("stop-generation"), "broken\n").unwrap();
    f.tick();
    f.assert_dismissed();
    assert_eq!(f.paths.execution_status()["state"], "repair_required");
}

#[test]
fn renderer_keeps_current_prompt_during_historical_stop() {
    let f = Fixture::new();
    f.paths.request_stop().unwrap();
    let generation = f.paths.execution_generation().unwrap();
    SERVICE.with(|cell| {
        let mut s = cell.borrow_mut();
        s.generation = Some(generation.clone());
        let (_, request) = s.request.as_mut().unwrap();
        request.generation = generation;
        f.paths.write_display("handoff-42.json", request).unwrap();
    });
    f.tick();
    SERVICE.with(|cell| assert!(cell.borrow().request.is_some()));
    assert!(unsafe { IsWindow(Some(f.panel)).as_bool() });
    assert!(f.paths.stop_requested());
}
