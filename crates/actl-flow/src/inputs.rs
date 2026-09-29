//! Resolve explicit whole-argument inputs before validating and persisting a plan.
use actl_core::CtlError;
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    #[serde(default)]
    default: Option<String>,
}

pub fn resolve(text: &str, inputs: Option<&str>) -> Result<super::Plan, CtlError> {
    let mut raw = super::decode_yaml(text)?;
    let declarations: BTreeMap<String, Input> =
        match raw.as_object_mut().and_then(|v| v.remove("inputs")) {
            Some(value) => {
                if value.as_object().is_some_and(|items| {
                    items.values().any(|item| {
                        item.get("default")
                            .is_some_and(|default| !default.is_string())
                    })
                }) {
                    return Err(super::schema::error(
                        "inputs",
                        None,
                        "default must be a string",
                    ));
                }
                serde_json::from_value(value).map_err(|_| {
                    super::schema::error(
                        "inputs",
                        None,
                        "expected input declarations with optional string default",
                    )
                })?
            }
            None => BTreeMap::new(),
        };
    let supplied: BTreeMap<String, String> = match inputs {
        Some(text) => serde_json::from_value(super::decode_yaml(text)?)
            .map_err(|_| super::schema::error("inputs", None, "input file must be a string map"))?,
        None => BTreeMap::new(),
    };
    if declarations.len() > 128 || supplied.keys().any(|key| !declarations.contains_key(key)) {
        return Err(super::schema::error(
            "inputs",
            None,
            "too many inputs or undeclared input",
        ));
    }
    let mut values = BTreeMap::new();
    for (name, declaration) in declarations {
        let field = format!("inputs.{name}");
        if !actl_core::history::valid_id(&name) {
            return Err(super::schema::error(&field, None, "invalid input name"));
        }
        let value = supplied
            .get(&name)
            .cloned()
            .or(declaration.default)
            .ok_or_else(|| super::schema::error(&field, None, "required input missing"))?;
        if value.len() > 16_384 || value.contains('\0') || value.starts_with('-') {
            return Err(super::schema::error(
                &field,
                None,
                "input exceeds 16 KiB, contains NUL or starts with an option prefix",
            ));
        }
        values.insert(name, value);
    }
    if let Some(steps) = raw
        .get_mut("steps")
        .and_then(serde_json::Value::as_array_mut)
    {
        for (i, step) in steps.iter_mut().enumerate() {
            for phase in ["run", "before", "verify", "resume_check"] {
                if let Some(args) = step
                    .get_mut(phase)
                    .and_then(|v| v.get_mut("args"))
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for (j, arg) in args.iter_mut().enumerate() {
                        if let Some(object) = arg.as_object() {
                            let field = format!("steps[{i}].{phase}.args[{j}]");
                            let name = object
                                .get("input")
                                .and_then(serde_json::Value::as_str)
                                .filter(|_| object.len() == 1)
                                .ok_or_else(|| {
                                    super::schema::error(&field, None, "expected {input: name}")
                                })?;
                            let value = values.get(name).ok_or_else(|| {
                                super::schema::error(&field, None, "undeclared input reference")
                            })?;
                            *arg = serde_json::Value::String(value.clone());
                        }
                    }
                }
            }
        }
    }
    super::parse(&serde_json::to_string(&raw).map_err(|e| CtlError::internal(e.to_string()))?)
}
