//! Structural diagnostics before typed deserialization. Runtime validation remains authoritative.
use actl_core::CtlError;
use serde_json::{Value, json};
pub fn error(field: &str, id: Option<&str>, message: impl Into<String>) -> CtlError {
    CtlError::with_evidence(
        actl_core::ErrorCode::Protocol,
        message,
        json!({"field":field,"step_id":id}),
    )
}
pub fn decode(raw: Value) -> Result<super::Plan, CtlError> {
    let mut value = raw.clone();
    // 探索计划可省略 steps 键(等价空数组);静态计划必须显式给出。
    let empty_steps: Vec<Value> = Vec::new();
    let steps = match raw.get("steps").and_then(Value::as_array) {
        Some(steps) => steps,
        None if raw.get("explore").is_some() => &empty_steps,
        None => {
            return Err(error("steps", None, "steps must be an array"));
        }
    };
    for (i, step) in steps.iter().enumerate() {
        let base = format!("steps[{i}]");
        let id = step.get("id").and_then(Value::as_str);
        let fields = step
            .as_object()
            .ok_or_else(|| error(&base, id, "step must be an object"))?;
        for key in fields.keys() {
            if ![
                "id",
                "title",
                "run",
                "when",
                "unless",
                "otherwise",
                "before",
                "verify",
                "resume_check",
                "section",
                "pause_before",
                "retry",
                "recovery",
                "on_failure",
                "evidence",
                "outputs",
            ]
            .contains(&key.as_str())
            {
                return Err(error(&format!("{base}.{key}"), id, "unknown step field"));
            }
        }
        for key in ["id", "title"] {
            if !step.get(key).is_some_and(Value::is_string) {
                return Err(error(&format!("{base}.{key}"), id, "required string field"));
            }
        }
        for key in [
            "run",
            "before",
            "verify",
            "resume_check",
            "when",
            "unless",
            "otherwise",
        ] {
            if key != "run" && step.get(key).is_none_or(Value::is_null) {
                continue;
            }
            let call = step
                .get(key)
                .ok_or_else(|| error(&format!("{base}.{key}"), id, "required invocation"))?;
            let fields = call.as_object().ok_or_else(|| {
                error(&format!("{base}.{key}"), id, "invocation must be an object")
            })?;
            for field in fields.keys() {
                if !["command", "args", "bind"].contains(&field.as_str()) {
                    return Err(error(
                        &format!("{base}.{key}.{field}"),
                        id,
                        "unknown invocation field",
                    ));
                }
            }
            if !call.get("command").is_some_and(Value::is_string) {
                return Err(error(
                    &format!("{base}.{key}.command"),
                    id,
                    "required command string",
                ));
            }
            if let Some(args) = call.get("args") {
                let args = args.as_array().ok_or_else(|| {
                    error(
                        &format!("{base}.{key}.args"),
                        id,
                        "args must be a string array",
                    )
                })?;
                for (j, arg) in args.iter().enumerate() {
                    if !arg.is_string() {
                        return Err(error(
                            &format!("{base}.{key}.args[{j}]"),
                            id,
                            "argument must be a string",
                        ));
                    }
                }
            }
        }
        for (key, allowed) in [
            ("retry", &["never", "read_only", "stale_read"][..]),
            ("on_failure", &["pause", "fail", "skip_section"][..]),
            ("evidence", &["metadata", "full"][..]),
        ] {
            if let Some(v) = step.get(key)
                && !v.as_str().is_some_and(|s| allowed.contains(&s))
            {
                return Err(error(&format!("{base}.{key}"), id, "invalid policy value"));
            }
        }
        if step.get("pause_before").is_some_and(|v| !v.is_boolean()) {
            return Err(error(
                &format!("{base}.pause_before"),
                id,
                "expected boolean",
            ));
        }
        let _: super::Step =
            serde_json::from_value(step.clone()).map_err(|e| error(&base, id, e.to_string()))?;
    }
    // Keep serde's top-level strictness after step-specific diagnostics.
    serde_json::from_value(value.take()).map_err(|e| error("$", None, e.to_string()))
}
