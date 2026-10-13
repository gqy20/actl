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
    resolve_anchor_inputs(&mut raw, &values)?;
    // 真机发现 #6b(用户目击):引号包裹的 `{input: name}` 是纯字符串,替换不发生;
    // 若 run 与 verify 拿同一字面量会**自洽假通过**(set-clipboard 假通过实证)。
    // 替换后仍残留绑定形态的 args 字符串 = 几乎必然是引号/转义错误 → 拒绝。
    reject_unresolved_bindings(&raw)?;
    crate::sequence::expand(&mut raw)?;
    crate::for_each::expand(&mut raw)?;
    super::parse(&serde_json::to_string(&raw).map_err(|e| CtlError::internal(e.to_string()))?)
}

/// 弹层声明的锚引用解析(工具链缺口②收口):popup_handlers[].app 与
/// popup_ignores[].app 支持 `{input: name}`——模板锚为运行输入(如
/// `${run.id}` 前缀形态)时可静态声明 handler/ignore。在输入解析期展开为
/// 具体字符串,引擎的 `app.contains(&handler.app)` 匹配语义不变;
/// 引号包裹的字符串形式与其他位置同样由 reject_unresolved_bindings
/// 家族的语义兜底(锚字段不在 args 内,这里按"对象才解析、字符串不认"
/// 的既有口径处理——写错为字符串会作为字面锚匹配不到,模板验证即可见)。
fn resolve_anchor_inputs(
    raw: &mut serde_json::Value,
    values: &BTreeMap<String, String>,
) -> Result<(), CtlError> {
    for section in ["popup_handlers", "popup_ignores"] {
        let Some(items) = raw
            .get_mut(section)
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for item in items {
            let Some(app) = item.get_mut("app") else {
                continue;
            };
            let Some(binding) = app.as_object() else {
                continue; // 字面锚:原样保留
            };
            let name = binding
                .get("input")
                .and_then(serde_json::Value::as_str)
                .filter(|_| binding.len() == 1)
                .ok_or_else(|| {
                    super::schema::error(section, None, "expected app: {input: name}")
                })?;
            let value = values
                .get(name)
                .ok_or_else(|| super::schema::error(section, None, "undeclared input reference"))?;
            *app = serde_json::Value::String(value.clone());
        }
    }
    Ok(())
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

/// `--set key=value` 内联覆盖(2026-10-13,可重复):与 --inputs 值文件合并,
/// **set 优先**——换词重跑的零文件通道(临时值不落盘)。返回合并后的
/// string map 文本(JSON,resolve 同一解析器消费)。缺等号/空键/重复键拒绝;
/// 未声明键与值约束(16 KiB/NUL/`-` 前缀)由 resolve 统一裁决(与文件值同口径)。
pub fn overlay(supplied: Option<&str>, set: &[String]) -> Result<Option<String>, CtlError> {
    if set.is_empty() {
        return Ok(supplied.map(str::to_owned));
    }
    let mut values: BTreeMap<String, String> = match supplied {
        Some(text) => serde_json::from_value(super::decode_yaml(text)?)
            .map_err(|_| super::schema::error("inputs", None, "input file must be a string map"))?,
        None => BTreeMap::new(),
    };
    let mut seen_keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for item in set {
        let (key, value) = item.split_once('=').ok_or_else(|| {
            super::schema::error(
                "set",
                None,
                "--set expects key=value (split at the first '=')",
            )
        })?;
        if key.is_empty() {
            return Err(super::schema::error(
                "set",
                None,
                "--set key must not be empty",
            ));
        }
        // 重复键只针对 --set 自身(同键给两次);值文件里的同名键是合法
        // 覆盖对象——set 优先于 --inputs 是本通道的语义,不是错误。
        let duplicate = seen_keys.contains(key);
        seen_keys.insert(key.to_owned());
        if duplicate {
            return Err(super::schema::error("set", None, "--set key given twice"));
        }
        values.insert(key.to_owned(), value.to_owned());
    }
    let merged = serde_json::to_string(&values).map_err(|e| CtlError::internal(e.to_string()))?;
    Ok(Some(merged))
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

    #[test]
    fn popup_anchor_input_reference_resolves() {
        // 工具链缺口②:锚为运行输入时 handler/ignore 可声明 {input: name}。
        let yaml = "version: 1\nname: t\ninputs:\n  window: {default: 'task-1 - VS Code'}\n\
popup_handlers:\n  - app: {input: window}\n    classification: error\n    \
name_contains: 找不到文件\n    click: id:2\npopup_ignores:\n  - app: {input: window}\n    \
name_contains: 通知\nsteps:\n  - id: s1\n    title: t1\n    run:\n      command: wait\n      \
args: [--window, w1, --timeout, '1000']\n";
        let plan = resolve(yaml, None).expect("resolves");
        assert_eq!(plan.popup_handlers[0].app, "task-1 - VS Code");
        assert_eq!(plan.popup_ignores[0].name_contains, "通知");
    }

    #[test]
    fn undeclared_popup_anchor_reference_is_rejected() {
        let yaml = "version: 1\nname: t\ninputs:\n  window: {default: w1}\n\
popup_handlers:\n  - app: {input: missing}\n    classification: error\n    \
name_contains: x\n    click: id:2\nsteps:\n  - id: s1\n    title: t1\n    run:\n      \
command: wait\n      args: [--window, w1, --timeout, '1000']\n";
        let error = resolve(yaml, None).unwrap_err();
        assert_eq!(error.code, actl_core::ErrorCode::Protocol);
        assert!(error.message.contains("undeclared input reference"));
    }
}
