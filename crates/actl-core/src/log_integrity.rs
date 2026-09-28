use serde_json::Value;
/// Integrity of retained bytes, not proof that all calls were recorded.
pub fn check(events: &[Value], valid_json: bool) -> &'static str {
    if !valid_json {
        return "malformed_json";
    }
    let Some(first) = events.first() else {
        return "empty";
    };
    if first["event"] != "call_started" {
        return "missing_start";
    }
    if first["call_id"]
        .as_str()
        .is_none_or(|s| !crate::history::valid_id(s))
    {
        return "invalid_identity";
    }
    for (i, event) in events.iter().enumerate() {
        if event["version"] != 1
            || event["call_id"] != first["call_id"]
            || event["task_id"] != first["task_id"]
        {
            return "identity_or_version_mismatch";
        }
        if event["seq"].as_u64() != Some(i as u64 + 1) {
            return "sequence_gap_or_duplicate";
        }
        if (i > 0 && event["event"] == "call_started")
            || (i + 1 < events.len() && event["event"] == "call_finished")
        {
            return "invalid_boundary";
        }
    }
    if events.last().is_some_and(|e| e["event"] == "call_finished") {
        "complete"
    } else {
        "unfinished"
    }
}
