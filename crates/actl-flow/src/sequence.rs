//! A bounded action group has one durable uncertain result; it is never auto-replayed.
use crate::Invocation;
use actl_core::CtlError;
pub const COMMAND: &str = "workflow-sequence";

pub fn failure(mut error: CtlError, started: bool, index: usize) -> CtlError {
    if started {
        let evidence = error.evidence.get_or_insert_with(|| serde_json::json!({}));
        if !evidence.is_object() {
            *evidence = serde_json::json!({});
        }
        evidence["action_started"] = serde_json::json!(true);
        evidence["sequence_index"] = serde_json::json!(index);
    }
    error
}

pub fn decode(call: &Invocation) -> Result<Option<Vec<Invocation>>, CtlError> {
    if call.command != COMMAND {
        return Ok(None);
    }
    if !call.bind.is_empty() || call.args.is_empty() || call.args.len() > 16 {
        return Err(CtlError::protocol(
            "sequence requires 1-16 calls and no output bindings",
        ));
    }
    let calls = call
        .args
        .iter()
        .map(|text| {
            serde_json::from_str::<Invocation>(text)
                .map_err(|_| CtlError::protocol("invalid sequence invocation"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if calls
        .iter()
        .any(|call| call.command == COMMAND || !call.bind.is_empty())
    {
        return Err(CtlError::protocol("sequences cannot nest or bind outputs"));
    }
    Ok(Some(calls))
}

pub fn expand(raw: &mut serde_json::Value) -> Result<(), CtlError> {
    if let Some(steps) = raw
        .get_mut("steps")
        .and_then(serde_json::Value::as_array_mut)
    {
        for step in steps {
            for field in ["run", "otherwise"] {
                if let Some(run) = step.get_mut(field)
                    && let Some(sequence) = run.get("sequence")
                {
                    if run.as_object().is_none_or(|object| object.len() != 1) {
                        return Err(CtlError::protocol(
                            "sequence cannot be combined with invocation fields",
                        ));
                    }
                    let calls = sequence
                        .as_array()
                        .ok_or_else(|| CtlError::protocol("sequence must be an array"))?;
                    let args = calls
                        .iter()
                        .map(|call| {
                            serde_json::to_string(call)
                                .map_err(|e| CtlError::internal(e.to_string()))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let call = Invocation {
                        command: COMMAND.into(),
                        args,
                        bind: Default::default(),
                    };
                    decode(&call)?;
                    *run = serde_json::to_value(call)
                        .map_err(|e| CtlError::internal(e.to_string()))?;
                }
            }
        }
    }
    Ok(())
}
