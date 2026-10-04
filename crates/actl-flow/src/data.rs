//! Explicit typed outputs and whole-argument bindings. No expression evaluation.
use crate::{
    Plan, Step,
    engine::{Run, StepStatus},
};
use actl_core::CtlError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub pointer: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}
impl Kind {
    pub fn accepts(self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::Array => value.is_array(),
            Self::Object => value.is_object(),
            Self::Null => value.is_null(),
        }
    }
}
fn error(step: &str, field: &str, message: &str) -> CtlError {
    crate::schema::error(field, Some(step), message)
}
pub fn validate(plan: &Plan) -> Result<(), CtlError> {
    let mut known = BTreeMap::new();
    for (i, handler) in plan.popup_handlers.iter().enumerate() {
        let field = format!("popup_handlers[{i}]");
        if !matches!(
            handler.classification.as_str(),
            "error" | "save" | "confirm" | "update"
        ) {
            return Err(error(
                "",
                &field,
                "classification must be error/save/confirm/update; unknown popups always escalate",
            ));
        }
        if handler.app.is_empty() || handler.click.is_empty() {
            return Err(error("", &field, "app and click must be non-empty"));
        }
    }
    for (i, step) in plan.steps.iter().enumerate() {
        crate::branch::validate(step)?;
        crate::bounded::validate(step)?;
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
            for (index, reference) in &call.bind {
                let field = format!("steps[{i}].{phase}.bind.{index}");
                let index: usize = index
                    .parse()
                    .ok()
                    .filter(|n: &usize| n.to_string() == *index)
                    .ok_or_else(|| {
                        error(
                            &step.id,
                            &field,
                            "binding index must be a canonical zero-based integer",
                        )
                    })?;
                if call.args.get(index).is_none_or(|s| s.starts_with('-')) {
                    return Err(error(
                        &step.id,
                        &field,
                        "binding requires an existing value argument, not an option",
                    ));
                }
                if !matches!(
                    known.get(reference),
                    Some(Kind::String | Kind::Number | Kind::Boolean)
                ) {
                    return Err(error(
                        &step.id,
                        &field,
                        "binding must reference a preceding step's scalar output",
                    ));
                }
            }
        }
        if step.outputs.len() > 32 {
            return Err(error(&step.id, "outputs", "at most 32 outputs per step"));
        }
        for (name, output) in &step.outputs {
            let field = format!("steps[{i}].outputs.{name}");
            if output.max_age_ms == Some(0) {
                return Err(error(&step.id, &field, "max_age_ms must be positive"));
            }
            if !actl_core::history::valid_id(name) {
                return Err(error(&step.id, &field, "invalid output name"));
            }
            let p = &output.pointer;
            let valid_escape = p
                .as_bytes()
                .windows(2)
                .all(|w| w[0] != b'~' || matches!(w[1], b'0' | b'1'))
                && !p.ends_with('~');
            if p.len() > 1024 || (!p.is_empty() && !p.starts_with('/')) || !valid_escape {
                return Err(error(&step.id, &field, "invalid JSON Pointer"));
            }
            known.insert(format!("{}.{}", step.id, name), output.kind);
        }
    }
    Ok(())
}

pub fn extract(step: &Step, result: &Value) -> Result<BTreeMap<String, Value>, CtlError> {
    let mut outputs = BTreeMap::new();
    for (name, specification) in &step.outputs {
        let field = format!("outputs.{name}");
        let value = result
            .pointer(&specification.pointer)
            .ok_or_else(|| error(&step.id, &field, "output field missing"))?;
        if !specification.kind.accepts(value) {
            return Err(error(&step.id, &field, "output type mismatch"));
        }
        if serde_json::to_vec(value)
            .map_err(|e| CtlError::internal(e.to_string()))?
            .len()
            > 16384
        {
            return Err(error(&step.id, &field, "output exceeds 16 KiB"));
        }
        outputs.insert(name.clone(), value.clone());
    }
    Ok(outputs)
}

pub fn resolve(step: &Step, run: &Run) -> Result<Step, CtlError> {
    let mut resolved = step.clone();
    for call in std::iter::once(&mut resolved.run)
        .chain(resolved.when.iter_mut())
        .chain(resolved.unless.iter_mut())
        .chain(resolved.otherwise.iter_mut())
        .chain(resolved.before.iter_mut())
        .chain(resolved.verify.iter_mut())
        .chain(resolved.resume_check.iter_mut())
    {
        for (index, reference) in &call.bind {
            let (producer, name) = reference
                .split_once('.')
                .ok_or_else(|| error(&step.id, "bind", "invalid reference"))?;
            let record = run
                .steps
                .iter()
                .find(|s| s.id == producer && s.status == StepStatus::Completed)
                .ok_or_else(|| error(&step.id, "bind", "producer is not completed"))?;
            let value = record
                .outputs
                .get(name)
                .ok_or_else(|| error(&step.id, "bind", "saved output missing"))?;
            let text = match value {
                Value::String(s) => s.clone(),
                Value::Bool(_) | Value::Number(_) => value.to_string(),
                _ => return Err(error(&step.id, "bind", "argument requires a scalar output")),
            };
            if text.len() > 16384
                || text.contains('\0')
                || text.starts_with('-')
                || text
                    .strip_prefix("@e")
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            {
                return Err(error(
                    &step.id,
                    "bind",
                    "bound value is too large, an option, a transient ref or contains NUL",
                ));
            }
            let index: usize = index
                .parse()
                .map_err(|_| error(&step.id, "bind", "invalid index"))?;
            let arg = call
                .args
                .get_mut(index)
                .ok_or_else(|| error(&step.id, "bind", "index out of bounds"))?;
            *arg = text;
        }
        call.bind.clear();
    }
    Ok(resolved)
}

pub fn check_saved(plan: &Plan, run: &Run) -> Result<(), CtlError> {
    let mut bytes = 0;
    for (step, record) in plan.steps.iter().zip(&run.steps) {
        crate::branch::check_saved(step, record)?;
        crate::bounded::check_saved(step, record)?;
        for (name, value) in &record.outputs {
            let spec = step
                .outputs
                .get(name)
                .ok_or_else(|| error(&step.id, "outputs", "undeclared saved output"))?;
            if !spec.kind.accepts(value) {
                return Err(error(&step.id, "outputs", "saved output type mismatch"));
            }
            let size = serde_json::to_vec(value)
                .map_err(|e| CtlError::internal(e.to_string()))?
                .len();
            if size > 16384 {
                return Err(error(&step.id, "outputs", "saved output exceeds 16 KiB"));
            }
            bytes += size;
        }
        if record.status == StepStatus::Completed && step.outputs.len() != record.outputs.len() {
            return Err(error(
                &step.id,
                "outputs",
                "completed step is missing saved outputs",
            ));
        }
    }
    if bytes > 65536 {
        return Err(CtlError::protocol("flow outputs exceed 64 KiB"));
    }
    Ok(())
}
