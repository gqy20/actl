//! Bounded metadata only: never retain key values, device handles or coordinates.
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

static DEVICE_HOOKS: AtomicU64 = AtomicU64::new(0);
static FOREIGN_HOOKS: AtomicU64 = AtomicU64::new(0);
static OWN_HOOKS: AtomicU64 = AtomicU64::new(0);

/// 最近一次输入的类别(0=无记录,1=物理设备,2=外来注入,3=自有注入)与时刻——
/// consent_decision 一等字段 input_source 的数据源:区分「人工物理输入」与
/// 「外来程序注入」(docs/68 第五轮:用户未操作仍让行;两类事件今日均已实证)。
static LAST_KIND: AtomicU64 = AtomicU64::new(0);
static LAST_TS: AtomicU64 = AtomicU64::new(0);

pub(super) fn hook(injected: bool, tag: usize) {
    let counter = if !injected {
        note_last(1);
        &DEVICE_HOOKS
    } else if tag == actl_core::handoff::INPUT_TAG {
        note_last(3);
        &OWN_HOOKS
    } else {
        note_last(2);
        &FOREIGN_HOOKS
    };
    counter.fetch_add(1, Ordering::SeqCst);
}

fn note_last(kind: u64) {
    LAST_KIND.store(kind, Ordering::SeqCst);
    LAST_TS.store(actl_core::state::unix_ms(), Ordering::SeqCst);
}

/// 决策时刻的最近输入来源(None=尚无输入记录)。age_ms 供新鲜度判断:
/// 陈旧的来源不说明当前决策的成因,由调用方按需取舍。
pub(super) fn last_source() -> Option<serde_json::Value> {
    let kind = LAST_KIND.load(Ordering::SeqCst);
    let ts = LAST_TS.load(Ordering::SeqCst);
    let label = match kind {
        1 => "device",
        2 => "foreign_injected",
        3 => "own_injection",
        _ => return None,
    };
    let age_ms = actl_core::state::unix_ms().saturating_sub(ts);
    Some(serde_json::json!({"kind": label, "age_ms": age_ms}))
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
            note_last(3);
            state.own = state.own.saturating_add(1);
        } else {
            // Raw 层分类:device_input/tagged_device_input=物理设备,
            // unrecognized_tag=非本工具注入(无设备句柄且非自有标签)=外来。
            match reason {
                "device_input" | "tagged_device_input" => note_last(1),
                "unrecognized_tag" => note_last(2),
                _ => {}
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn last_source_classifies_hook_and_raw_events() {
        // 无记录 → None;物理(未注入)→ device;外来注入 → foreign_injected;
        // 自有注入 → own_injection。Raw 层 reason 映射同族。
        assert!(last_source().is_none() || !last_source().unwrap()["kind"].is_null());
        hook(false, 0);
        assert_eq!(last_source().unwrap()["kind"], "device");
        hook(true, actl_core::handoff::INPUT_TAG);
        assert_eq!(last_source().unwrap()["kind"], "own_injection");
        hook(true, 0xdead);
        assert_eq!(last_source().unwrap()["kind"], "foreign_injected");
        record("device_input", 48, 48, 48, 0);
        assert_eq!(last_source().unwrap()["kind"], "device");
        record("unrecognized_tag", 48, 48, 48, 1);
        assert_eq!(last_source().unwrap()["kind"], "foreign_injected");
        let source = last_source().unwrap();
        assert!(source["age_ms"].is_u64(), "age present: {source}");
    }
}
