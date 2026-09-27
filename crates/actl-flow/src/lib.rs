//! Optional workflow layer. No Windows dependencies, shell evaluation or asynchronous runtime.
pub mod engine;
pub mod store;
use actl_core::CtlError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub version: u32,
    pub name: String,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub title: String,
    pub run: Invocation,
    pub before: Option<Invocation>,
    pub verify: Option<Invocation>,
    #[serde(default)]
    pub pause_before: bool,
    #[serde(default)]
    pub retry: Retry,
    #[serde(default)]
    pub on_failure: Failure,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Retry {
    #[default]
    Never,
    ReadOnly,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    #[default]
    Pause,
    Fail,
}

pub fn parse(text: &str) -> Result<Plan, CtlError> {
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
    let plan: Plan = serde_saphyr::from_str_with_options(text, options)
        .map_err(|e| CtlError::protocol(format!("invalid flow YAML: {e}")))?;
    if plan.version != 1
        || plan.name.trim().is_empty()
        || plan.name.len() > 256
        || plan.steps.is_empty()
        || plan.steps.len() > 256
    {
        return Err(CtlError::protocol(
            "flow requires version 1, a name (<=256 bytes) and 1-256 steps",
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for step in &plan.steps {
        if !actl_core::history::valid_id(&step.id)
            || !ids.insert(&step.id)
            || step.title.trim().is_empty()
            || step.title.len() > 256
        {
            return Err(CtlError::protocol(
                "step IDs must be unique and valid; titles require 1-256 bytes",
            ));
        }
        for call in std::iter::once(&step.run)
            .chain(step.before.iter())
            .chain(step.verify.iter())
        {
            if call.command.is_empty()
                || call.command.len() > 64
                || call.args.len() > 128
                || call
                    .args
                    .iter()
                    .any(|a| a.len() > 16_384 || a.contains('\0'))
            {
                return Err(CtlError::protocol("invalid flow invocation size"));
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
    Ok(plan)
}
