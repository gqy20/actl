//! MCP 工具面(docs/82 §5.2):单一通用工具 + 资源模板(Stage 3)。
//!
//! 工具 `actl` 收 {command, args[]};排除动词先于 spawn 拒绝(明确错误);
//! 其余派发子进程,envelope 映射 tool result——`ok:false` → `isError:true`,
//! error 的 code/message/hint/evidence 全保留:远端 agent 拿到的失败信息
//! 与本地完全同构,自愈路径不变。

use crate::dispatch::{self, QUICKREF};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ErrorData as McpError, Implementation, ServerCapabilities,
    ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[allow(dead_code)]
pub struct ActlCall {
    /// The actl subcommand to run (e.g. "snapshot", "click", "flow-run").
    pub command: String,
    /// Literal command arguments in order (e.g. ["--app", "Notepad"]).
    #[serde(default)]
    pub args: Vec<String>,
}

/// 适配器配置(每连接的服务实例共享)。
#[derive(Clone)]
pub struct Adapter {
    exe: Arc<PathBuf>,
}

impl Adapter {
    pub fn new(exe: PathBuf) -> Self {
        Self { exe: Arc::new(exe) }
    }
}

#[tool_router]
impl Adapter {
    /// Run one actl CLI command on the controlled device.
    ///
    /// Every call spawns a real actl process: the local signal bar, action
    /// previews, the operator's start/continue approval, emergency stop and
    /// the watchdog all stay authoritative on THIS device — remote calls are
    /// supervised, never privileged. The result is the actl JSON envelope
    /// verbatim (`ok:false` → tool error with code/hint/evidence preserved).
    ///
    /// Command families (read actl://help/{command} for full arguments):
    /// {QUICKREF}
    #[tool(
        description = "Run one actl CLI command on the controlled Windows device (UIA-precise, JSON-verifiable). Pass command + literal args; errors carry code/hint/evidence for self-recovery. Full per-command docs: read resource template actl://help/{command}."
    )]
    async fn actl(
        &self,
        Parameters(call): Parameters<ActlCall>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if crate::is_excluded(&call.command) {
            let envelope = dispatch::excluded_error(&call.command);
            return Ok(envelope_to_tool_result(envelope));
        }
        let envelope = dispatch::dispatch(&self.exe, &call.command, &call.args).await;
        Ok(envelope_to_tool_result(envelope))
    }
}

#[tool_handler]
impl ServerHandler for Adapter {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(format!(
            "actl MCP adapter: run commands on the controlled Windows desktop. \
             Start with read-only observation (snapshot/find/list-windows), verify \
             after every write. Remote write actions require approval on the \
             local signal bar. Command families: {QUICKREF}"
        ))
    }
}

/// envelope → tool result 的唯一映射(单测钉住)。
fn envelope_to_tool_result(envelope: serde_json::Value) -> CallToolResult {
    let ok = envelope["ok"].as_bool().unwrap_or(false);
    let text = vec![ContentBlock::text(envelope.to_string())];
    let mut result = if ok {
        CallToolResult::success(text)
    } else {
        CallToolResult::error(text)
    };
    result.structured_content = Some(envelope);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_envelope_maps_to_success_with_structured_content() {
        let envelope =
            serde_json::json!({"version": "1", "ok": true, "command": "status", "data": {}});
        let result = envelope_to_tool_result(envelope);
        assert_eq!(result.is_error, Some(false));
        assert!(result.structured_content.is_some());
        assert_eq!(result.structured_content.unwrap()["ok"], true);
    }

    #[test]
    fn error_envelope_maps_to_is_error_with_code_preserved() {
        let envelope = serde_json::json!({
            "version": "1", "ok": false, "command": "click",
            "error": {"code": "NOT_FOUND", "message": "no match", "hint": "retry"}
        });
        let result = envelope_to_tool_result(envelope);
        assert_eq!(result.is_error, Some(true));
        let structured = result.structured_content.unwrap();
        assert_eq!(structured["error"]["code"], "NOT_FOUND");
        assert_eq!(structured["error"]["hint"], "retry");
    }
}
