//! Independent control records never publish desktop feedback or grant permission.
use crate::{ErrorCode, history::Journal, state::SignalPaths};
use serde_json::json;
#[derive(Clone)]
pub struct Request {
    pub run_id: String,
    pub request_id: String,
    pub action: String,
    pub source: String,
}
pub fn request_id() -> String {
    std::env::var("ACTL_FLOW_REQUEST_ID")
        .ok()
        .filter(|v| crate::history::valid_id(v))
        .unwrap_or_else(crate::snapshot::new_snapshot_id)
}
pub fn source() -> &'static str {
    match std::env::var("ACTL_FLOW_REQUEST_SOURCE").as_deref() {
        Ok("signal_control") => "signal_control",
        Err(std::env::VarError::NotPresent) => "cli",
        _ => "unknown",
    }
}
impl Request {
    pub fn new(run_id: &str, action: &str) -> Self {
        Self {
            run_id: run_id.into(),
            request_id: request_id(),
            action: action.into(),
            source: source().into(),
        }
    }
    pub fn record(&self, stage: &str, error: Option<ErrorCode>) {
        let root = SignalPaths::default().dir;
        let call = format!("control-{}", crate::snapshot::new_snapshot_id());
        let Ok(mut journal) = Journal::open(&root, &call, Some(self.run_id.clone())) else {
            return;
        };
        journal.record(
            "call_started",
            json!({"command":"flow-control","version":env!("CARGO_PKG_VERSION")}),
        );
        journal.record("control_request", json!({"run_id":self.run_id,"request_id":self.request_id,
            "action":self.action,"source":self.source,"source_verified":false,"stage":stage,"error_code":error}));
        journal.record(
            "call_finished",
            json!({"phase":if error.is_some(){"error"}else{"done"},"error_code":error}),
        );
    }
}
