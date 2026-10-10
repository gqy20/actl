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
    resolve_arguments(&mut raw, &values)?;
    // 真机发现 #6b(用户目击):引号包裹的 `{input: name}` 是纯字符串,替换不发生;
    // 若 run 与 verify 拿同一字面量会**自洽假通过**(set-clipboard 假通过实证)。
    // 替换后仍残留绑定形态的 args 字符串 = 几乎必然是引号/转义错误 → 拒绝。
    reject_unresolved_bindings(&raw)?;
    crate::sequence::expand(&mut raw)?;
    crate::for_each::expand(&mut raw)?;
    super::parse(&serde_json::to_string(&raw).map_err(|e| CtlError::internal(e.to_string()))?)
}

fn reject_unresolved_bindings(raw: &serde_json::Value) -> Result<(), CtlError> {
    fn walk(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(object) => {
                if object.get("args").is_some_and(|args| {
                    args.as_array().is_some_and(|items| {
                        items
                            .iter()
                            .any(|arg| arg.as_str().is_some_and(crate::route::is_input_binding))
                    })
                }) {
                    return true;
                }
                object.values().any(walk)
            }
            serde_json::Value::Array(items) => items.iter().any(walk),
            _ => false,
        }
    }
    if walk(raw) {
        return Err(super::schema::error(
            "args",
            None,
            "unresolved input binding: a quote-wrapped {input: name} is a plain string — emit bindings as YAML flow mappings",
        ));
    }
    Ok(())
}

fn resolve_arguments(
    raw: &mut serde_json::Value,
    values: &BTreeMap<String, String>,
) -> Result<(), CtlError> {
    match raw {
        serde_json::Value::Object(object) => {
            if let Some(args) = object
                .get_mut("args")
                .and_then(serde_json::Value::as_array_mut)
            {
                for arg in args {
                    if let Some(binding) = arg.as_object() {
                        let name = binding
                            .get("input")
                            .and_then(serde_json::Value::as_str)
                            .filter(|_| binding.len() == 1)
                            .ok_or_else(|| {
                                super::schema::error("args", None, "expected {input: name}")
                            })?;
                        let value = values.get(name).ok_or_else(|| {
                            super::schema::error("args", None, "undeclared input reference")
                        })?;
                        *arg = serde_json::Value::String(value.clone());
                    }
                }
            }
            for child in object.values_mut() {
                resolve_arguments(child, values)?;
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                resolve_arguments(child, values)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn quoted_binding_string_is_rejected() {
        // 发现 #6b:引号包裹的绑定是字符串,替换不发生且可能自洽假通过。
        let yaml = "version: 1\nname: t\ninputs:\n  window: {default: w1}\nsteps:\n  \
- id: s1\n    run: {command: press, args: [enter, --app, '{input: window}']}\n";
        let error = resolve(yaml, None).unwrap_err();
        assert_eq!(error.code, actl_core::ErrorCode::Protocol);
        assert!(error.message.contains("unresolved input binding"));
    }

    #[test]
    fn raw_flow_mapping_binding_resolves_with_default() {
        let yaml = "version: 1\nname: t\ninputs:\n  window: {default: w1}\nsteps:\n  \
- id: s1\n    title: t1\n    run:\n      command: press\n      args: [enter, --app, {input: window}]\n";
        let plan = resolve(yaml, None).expect("resolves");
        assert_eq!(plan.steps[0].run.args, vec!["enter", "--app", "w1"]);
    }
}
