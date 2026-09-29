use actl_core::{CtlError, handoff::Policy};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfig {
    pub version: u32,
    pub handoff: Policy,
}
pub fn parse(text: &str) -> Result<TaskConfig, CtlError> {
    let config: TaskConfig = serde_json::from_value(super::decode_yaml(text)?)
        .map_err(|e| CtlError::protocol(format!("invalid task configuration: {e}")))?;
    if config.version != 1 {
        return Err(CtlError::protocol("task configuration requires version 1"));
    }
    Ok(config)
}
