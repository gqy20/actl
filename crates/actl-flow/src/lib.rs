//! Optional workflow layer. No Windows dependencies, shell evaluation or asynchronous runtime.
pub mod branch;
pub mod data;
pub mod engine;
pub mod freshness;
pub mod inputs;
pub mod recovery;
pub mod resources;
mod schema;
pub mod sequence;
pub mod store;
pub mod task_config;
use actl_core::CtlError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<actl_core::handoff::Policy>,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub run: Invocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unless: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<Invocation>,
    pub before: Option<Invocation>,
    pub verify: Option<Invocation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_check: Option<Invocation>,
    #[serde(default)]
    pub pause_before: bool,
    #[serde(default)]
    pub retry: Retry,
    #[serde(default)]
    pub on_failure: Failure,
    #[serde(default)]
    pub evidence: EvidencePolicy,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub outputs: std::collections::BTreeMap<String, data::Output>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePolicy {
    #[default]
    Metadata,
    Full,
}

pub fn failure_evidence(policy: EvidencePolicy, error: &CtlError) -> Option<serde_json::Value> {
    let source = error.evidence.as_ref()?;
    if policy == EvidencePolicy::Full {
        return Some(
            if serde_json::to_vec(source).is_ok_and(|v| v.len() <= 8192) {
                source.clone()
            } else {
                serde_json::json!({"omitted":"evidence_exceeds_8_kib"})
            },
        );
    }
    let mut value = serde_json::json!({"redacted":true});
    for key in ["property", "channel", "comparison"] {
        if let Some(v) = source.get(key) {
            value[key] = v.clone();
        }
    }
    for key in [
        "native_code",
        "delivered_before_error",
        "failed_segment_size",
        "sequence_index",
    ] {
        if let Some(v) = source.get(key).filter(|v| v.is_i64() || v.is_u64()) {
            value[key] = v.clone();
        }
    }
    if let Some(v) = source.get("action_started").filter(|v| v.is_boolean()) {
        value["action_started"] = v.clone();
    }
    if source["stage"] == "handoff" {
        value["stage"] = source["stage"].clone();
    }
    if source["reason"].as_str().is_some_and(|r| {
        matches!(
            r,
            "observation_failed"
                | "human_takeover_or_grant_expired"
                | "handoff_pending"
                | "user_deferred"
                | "user_paused"
                | "pattern_unsupported"
        )
    }) {
        value["reason"] = source["reason"].clone();
    }
    Some(value)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub bind: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Retry {
    #[default]
    Never,
    ReadOnly,
    StaleRead,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    #[default]
    Pause,
    Fail,
}

pub fn parse(text: &str) -> Result<Plan, CtlError> {
    let raw = decode_yaml(text)?;
    let plan = schema::decode(raw)?;
    for (field, invalid, message) in [
        ("version", plan.version != 1, "expected version 1"),
        (
            "name",
            plan.name.trim().is_empty() || plan.name.len() > 256,
            "name requires 1-256 UTF-8 bytes",
        ),
        (
            "steps",
            plan.steps.is_empty() || plan.steps.len() > 256,
            "expected 1-256 steps",
        ),
    ] {
        if invalid {
            return Err(schema::error(field, None, message));
        }
    }
    let mut ids = std::collections::HashSet::new();
    for (index, step) in plan.steps.iter().enumerate() {
        let base = format!("steps[{index}]");
        if !actl_core::history::valid_id(&step.id) || !ids.insert(&step.id) {
            return Err(schema::error(
                &format!("{base}.id"),
                Some(&step.id),
                "step ID must be unique and valid",
            ));
        }
        if step.title.trim().is_empty() || step.title.len() > 256 {
            return Err(schema::error(
                &format!("{base}.title"),
                Some(&step.id),
                "title requires 1-256 UTF-8 bytes",
            ));
        }
        for (phase, call) in [
            ("run", Some(&step.run)),
            ("when", step.when.as_ref()),
            ("unless", step.unless.as_ref()),
            ("otherwise", step.otherwise.as_ref()),
            ("before", step.before.as_ref()),
            ("verify", step.verify.as_ref()),
            ("resume_check", step.resume_check.as_ref()),
        ] {
            let Some(call) = call else { continue };
            if call.command.is_empty() || call.command.len() > 64 {
                return Err(schema::error(
                    &format!("{base}.{phase}.command"),
                    Some(&step.id),
                    "command requires 1-64 bytes",
                ));
            }
            if call.args.len() > 128 {
                return Err(schema::error(
                    &format!("{base}.{phase}.args"),
                    Some(&step.id),
                    "at most 128 arguments",
                ));
            }
            for (j, arg) in call.args.iter().enumerate() {
                if arg.len() > 16_384 || arg.contains('\0') {
                    return Err(schema::error(
                        &format!("{base}.{phase}.args[{j}]"),
                        Some(&step.id),
                        "argument exceeds 16 KiB or contains NUL",
                    ));
                }
            }
        }
    }
    if serde_json::to_vec(&plan)
        .map_err(|e| CtlError::internal(e.to_string()))?
        .len()
        > 262_144
    {
        return Err(CtlError::protocol("normalized flow exceeds 256 KiB"));
    }
    data::validate(&plan)?;
    Ok(plan)
}

fn decode_yaml(text: &str) -> Result<serde_json::Value, CtlError> {
    if text.len() > 262_144 {
        return Err(CtlError::protocol("flow exceeds 256 KiB"));
    }
    let mut options = serde_saphyr::Options::default();
    options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
    options.merge_keys = serde_saphyr::MergeKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    options.strict_booleans = true;
    options.with_snippet = false;
    let mut budget = serde_saphyr::Budget::default();
    budget.max_depth = 16;
    budget.max_aliases = 0;
    budget.max_anchors = 0;
    budget.max_documents = 1;
    budget.max_events = 100_000;
    budget.max_nodes = 50_000;
    budget.max_total_scalar_bytes = 262_144;
    options.budget = Some(budget);
    let raw: serde_json::Value = serde_saphyr::from_str_with_options(text, options)
        .map_err(|e| CtlError::protocol(format!("invalid flow YAML: {e}")))?;
    Ok(raw)
}
