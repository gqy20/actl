//! Consent decisions contain no input payload, key values, or cursor coordinates.
pub(super) fn record(
    request: &actl_core::handoff::Request,
    stage: &str,
    reason: &str,
    hook: u64,
    raw: u64,
) {
    let paths = actl_core::state::SignalPaths::default();
    let call = format!("consent-{}", actl_core::snapshot::new_snapshot_id());
    if let Ok(mut journal) = actl_core::history::Journal::open_without_retention(
        &paths.dir,
        &call,
        Some(request.task.clone()),
    ) {
        journal.record(
            "consent_decision",
            serde_json::json!({
                "request_call":request.call,"approval":request.approval,"stage":stage,
                "reason":reason,"hook_sequence":hook,"raw_sequence":raw,
                "input_monitor":super::input_watch::diagnostics()
            }),
        );
    }
}
