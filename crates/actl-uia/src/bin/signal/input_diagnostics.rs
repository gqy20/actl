//! Bounded metadata only: never retain key values, device handles or coordinates.
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

static DEVICE_HOOKS: AtomicU64 = AtomicU64::new(0);
static FOREIGN_HOOKS: AtomicU64 = AtomicU64::new(0);
static OWN_HOOKS: AtomicU64 = AtomicU64::new(0);

pub(super) fn hook(injected: bool, tag: usize) {
    let counter = if !injected {
        &DEVICE_HOOKS
    } else if tag == actl_core::handoff::INPUT_TAG {
        &OWN_HOOKS
    } else {
        &FOREIGN_HOOKS
    };
    counter.fetch_add(1, Ordering::SeqCst);
}

#[derive(Default)]
struct State {
    own: u64,
    external: u64,
    last_external: Option<serde_json::Value>,
}
static STATE: Mutex<State> = Mutex::new(State {
    own: 0,
    external: 0,
    last_external: None,
});

pub(super) fn record(reason: &str, copied: u32, capacity: u32, header: u32, kind: u32) {
    if let Ok(mut state) = STATE.lock() {
        if reason == "own_injection" {
            state.own = state.own.saturating_add(1);
        } else {
            state.external = state.external.saturating_add(1);
            state.last_external = Some(serde_json::json!({
                "reason":reason,"copied_bytes":copied,"capacity_bytes":capacity,
                "header_bytes":header,"kind":kind,
                "ts_ms":actl_core::state::unix_ms()
            }));
        }
    }
}

pub(super) fn snapshot() -> serde_json::Value {
    match STATE.lock() {
        Ok(state) => serde_json::json!({"own_events":state.own,
            "external_events":state.external,"last_external":state.last_external,
            "hook_device_events":DEVICE_HOOKS.load(Ordering::SeqCst),
            "hook_foreign_injected_events":FOREIGN_HOOKS.load(Ordering::SeqCst),
            "hook_own_injected_events":OWN_HOOKS.load(Ordering::SeqCst)}),
        Err(_) => serde_json::json!({"reason":"diagnostic_lock_unavailable"}),
    }
}
