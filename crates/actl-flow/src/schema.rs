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
    // P0-2 prologue/epilogue(2026-10-14 YAML 审计轮):流程级准备/收尾样板
    // 声明。展开发生在原始 Value 层——解码后 Plan 只见全量 steps,冻结
    // 计划/指纹/revise/恢复零改动。语义=纯位置展开(prologue 先于业务
    // steps,epilogue 收尾),不引入 try/finally:步骤失败停在失败步,
    // epilogue 不跑(资源留现场,AGENTS 排障纪律);explore 计划不消费
    // 步骤样板,互斥。
    expand_bookends(&mut value)?;
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
                "ensure",
                "section",
                "pause_before",
                "retry",
                "recovery",
                "on_failure",
                "evidence",
                "outputs",
                "background",
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

/// prologue/epilogue 展开:两键消费 → steps 前后拼接。字段校验交由
/// 展开后的既有步骤级校验(样板步骤就是普通步骤);这里只做形态检查
/// (数组)、explore 互斥与三段 id 唯一性。
fn expand_bookends(value: &mut Value) -> Result<(), CtlError> {
    let Some(obj) = value.as_object_mut() else {
        return Ok(());
    };
    let prologue = obj.remove("prologue");
    let epilogue = obj.remove("epilogue");
    if prologue.is_none() && epilogue.is_none() {
        return Ok(());
    }
    if obj.get("explore").is_some() {
        return Err(error(
            "prologue",
            None,
            "explore plans do not consume step bookends (prologue/epilogue)",
        ));
    }
    let bookend_vec = |v: Option<Value>, name: &str| -> Result<Vec<Value>, CtlError> {
        match v {
            None => Ok(Vec::new()),
            Some(Value::Array(a)) => Ok(a),
            Some(_) => Err(error(name, None, "prologue/epilogue must be a step array")),
        }
    };
    let prologue = bookend_vec(prologue, "prologue")?;
    let epilogue = bookend_vec(epilogue, "epilogue")?;
    let steps: Vec<Value> = match obj.remove("steps") {
        Some(Value::Array(a)) => a,
        Some(_) => return Err(error("steps", None, "steps must be an array")),
        // 无 steps 但有样板:业务步可为空(纯准备/收尾计划)
        None => Vec::new(),
    };
    // 三段 id 唯一性(引擎按 id 编址恢复/重试,重复 id 是规格错误)
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for step in prologue.iter().chain(steps.iter()).chain(epilogue.iter()) {
        if let Some(id) = step.get("id").and_then(Value::as_str)
            && !seen.insert(id)
        {
            return Err(error(
                "steps",
                Some(id),
                "duplicate step id across prologue/steps/epilogue",
            ));
        }
    }
    obj.insert(
        "steps".to_string(),
        Value::Array(prologue.into_iter().chain(steps).chain(epilogue).collect()),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn bookends_expand_in_order_and_merge_ids() {
        let raw = json!({
            "version": 1, "name": "t",
            "prologue": [{"id":"launch","title":"L","run":{"command":"status"}}],
            "steps": [{"id":"work","title":"W","run":{"command":"status"}}],
            "epilogue": [{"id":"close","title":"C","run":{"command":"status"}}]
        });
        let plan = super::decode(raw).unwrap();
        let ids: Vec<&str> = plan.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["launch", "work", "close"],
            "prologue -> steps -> epilogue"
        );
    }

    #[test]
    fn no_bookends_is_zero_behavior_change() {
        let raw = json!({
            "version": 1, "name": "t",
            "steps": [{"id":"a","title":"A","run":{"command":"status"}}]
        });
        let plan = super::decode(raw).unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].id, "a");
    }

    #[test]
    fn duplicate_ids_across_sections_rejected() {
        let raw = json!({
            "version": 1, "name": "t",
            "prologue": [{"id":"dup","title":"P","run":{"command":"status"}}],
            "steps": [{"id":"dup","title":"S","run":{"command":"status"}}]
        });
        assert_eq!(
            super::decode(raw).unwrap_err().code,
            actl_core::ErrorCode::Protocol
        );
    }

    #[test]
    fn explore_with_bookends_rejected() {
        let raw = json!({
            "version": 1, "name": "t",
            "explore": {"task":"x","app":"a","goal":{"command":"verify","args":["--exists","name:x"]}},
            "prologue": [{"id":"p","title":"P","run":{"command":"status"}}]
        });
        assert_eq!(
            super::decode(raw).unwrap_err().code,
            actl_core::ErrorCode::Protocol
        );
    }

    #[test]
    fn non_array_bookend_rejected() {
        let raw = json!({
            "version": 1, "name": "t",
            "prologue": "launch",
            "steps": []
        });
        assert_eq!(
            super::decode(raw).unwrap_err().code,
            actl_core::ErrorCode::Protocol
        );
    }
}
