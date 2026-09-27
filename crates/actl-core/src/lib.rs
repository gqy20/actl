//! actl-core —— 协议层:JSON envelope、错误码、错误映射。
//!
//! 本 crate 是纯逻辑层,禁止依赖任何 Windows 相关 crate(AGENTS.md §5.5)。
//! 契约的权威定义见 docs/06-design-blueprint.md §4/§5;变更必须走 `spec` 提交。

use serde::{Deserialize, Serialize};

pub mod activity;
pub mod display;
pub mod history;
pub mod keys;
pub mod signal_session;
pub mod snapshot;
pub mod state;
pub mod target;
pub mod timing;
pub mod trace;
pub mod wait_control;

pub use keys::{Key, KeySpec, parse_key_expr};
pub use snapshot::{
    ElementOut, INTERACTIVE_ROLES, Projection, SnapshotBuilder, SnapshotOutput, UiNode,
    is_interactive_role, new_snapshot_id,
};
pub use target::{Target, parse_target};

/// envelope 协议版本,与 CLI semver 解耦(ADR-006)。
pub const ENVELOPE_VERSION: &str = "1";

/// 机器可读错误码(docs/06 §4 错误码表,全集变更属 `spec` 提交)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// ref 失效(fallback 链已穷尽)→ agent 应重新 snapshot
    StaleRef,
    /// 选择器无匹配 → 用 find 放宽条件
    NotFound,
    /// 权限不足(UIPI/管理员窗口)→ 提示用户,勿重试
    PermDenied,
    /// wait/谓词超时 → 重估前置状态
    Timeout,
    /// 匹配到多个元素 → 收紧条件
    Ambiguous,
    /// 元素存在但不可操作(disabled/offscreen)→ 先 scroll/focus;hint 会指向 `set-value`
    NotActionable,
    /// verify/--expect 断言未成立 → 检查前置步骤
    AssertionFailed,
    /// 协议/参数错误(不可恢复)→ 修正命令参数
    Protocol,
    /// 内部/UIA/系统层错误 → 重试一次;持续出现附 evidence 报 issue
    Internal,
    /// 用户主动停止(紧急热键/停止文件)→ 清除停止标志后可安全重试
    Aborted,
}

impl ErrorCode {
    /// 恢复建议(envelope 面向 agent,输出英文;docs 中文档为中文)。
    pub fn recovery_hint(self) -> &'static str {
        match self {
            Self::StaleRef => "run `snapshot` to refresh refs, then retry",
            Self::NotFound => "relax the selector or run `find` to inspect candidates",
            Self::PermDenied => {
                "the target runs at a higher integrity level; ask the user to relaunch actl elevated — do not retry"
            }
            Self::Timeout => "re-evaluate preconditions; the UI may be busy or slow",
            Self::Ambiguous => "tighten the selector or pick by index",
            Self::NotActionable => {
                "scroll or focus the element first; for value writes consider `set-value`"
            }
            Self::AssertionFailed => {
                "inspect the prior step's result; the expected state did not materialize"
            }
            Self::Protocol => "fix the command arguments (see `actl <cmd> -h`)",
            Self::Internal => {
                "retry once; if it persists, report an issue with the `evidence` payload"
            }
            Self::Aborted => {
                "user requested stop; clear the stop flag (actl-signal UI or delete the file) and retry"
            }
        }
    }
}

/// 统一错误类型(ADR-004):单一枚举,`code()` 映射与恢复建议的唯一处。
/// 变体随命令实现扩充;运行路径禁止裸 unwrap/expect。
#[derive(Debug, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct CtlError {
    pub code: ErrorCode,
    pub message: String,
    /// 结构化证据(如 AMBIGUOUS 的候选列表,docs/12 §4 协议评审项):
    /// agent 可免重取 snapshot 直接裁决。
    pub evidence: Option<serde_json::Value>,
}

impl CtlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            evidence: None,
        }
    }

    pub fn with_evidence(
        code: ErrorCode,
        message: impl Into<String>,
        evidence: serde_json::Value,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            evidence: Some(evidence),
        }
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Protocol, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}

/// 统一成功 envelope。stdout 只允许出现本结构(或其错误变体)的序列化结果(ADR-002;
/// 唯一例外 `--help/--version` 走人类文本,docs/06 §3.5)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub version: String,
    pub ok: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    pub duration_ms: u64,
}

impl<T> Envelope<T> {
    pub fn success(command: impl Into<String>, data: Option<T>, duration_ms: u64) -> Self {
        Self {
            version: ENVELOPE_VERSION.to_owned(),
            ok: true,
            command: command.into(),
            data,
            snapshot_id: None,
            duration_ms,
        }
    }

    pub fn with_snapshot_id(mut self, id: impl Into<String>) -> Self {
        self.snapshot_id = Some(id.into());
        self
    }
}

/// 失败 envelope(docs/06 §4):错误码 + 消息 + 恢复建议 + evidence 引用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub version: String,
    pub ok: bool,
    pub command: String,
    pub error: ErrorBody,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    /// 缺省取 `ErrorCode::recovery_hint()`;显式覆盖用于上下文更准的提示
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// 失败留证:出错时的局部元素树/截图引用(RPA 借鉴,docs/03 §3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<serde_json::Value>,
}

impl ErrorEnvelope {
    pub fn new(command: impl Into<String>, err: &CtlError, duration_ms: u64) -> Self {
        Self {
            version: ENVELOPE_VERSION.to_owned(),
            ok: false,
            command: command.into(),
            error: ErrorBody {
                code: err.code,
                message: err.message.clone(),
                hint: Some(err.code.recovery_hint().to_owned()),
                evidence: err.evidence.clone(),
            },
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn error_codes_serialize_as_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::StaleRef).unwrap(),
            r#""STALE_REF""#
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::AssertionFailed).unwrap(),
            r#""ASSERTION_FAILED""#
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::Internal).unwrap(),
            r#""INTERNAL""#
        );
    }

    #[test]
    fn every_error_code_has_a_recovery_hint() {
        let all = [
            ErrorCode::StaleRef,
            ErrorCode::NotFound,
            ErrorCode::PermDenied,
            ErrorCode::Timeout,
            ErrorCode::Ambiguous,
            ErrorCode::NotActionable,
            ErrorCode::AssertionFailed,
            ErrorCode::Protocol,
            ErrorCode::Aborted,
            ErrorCode::Internal,
        ];
        for c in all {
            assert!(!c.recovery_hint().is_empty(), "{c:?} missing hint");
        }
    }

    #[test]
    fn success_envelope_shape_matches_contract() {
        let data = json!({ "action": "click", "ref": "@e3" });
        let env = Envelope::success("click", Some(&data), 42);
        let json: serde_json::Value = serde_json::to_value(&env).unwrap();
        assert_eq!(json["version"], "1");
        assert_eq!(json["ok"], true);
        assert_eq!(json["command"], "click");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["data"]["action"], "click");
    }

    #[test]
    fn none_data_and_snapshot_id_are_omitted() {
        let env = Envelope::success("snapshot", None::<u8>, 5);
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("\"data\""));
        assert!(!json.contains("\"snapshot_id\""));
    }

    #[test]
    fn snapshot_id_round_trips() {
        let env = Envelope::success("snapshot", None::<u8>, 5).with_snapshot_id("s8f3k2p9");
        let json: serde_json::Value = serde_json::to_value(&env).unwrap();
        assert_eq!(json["snapshot_id"], "s8f3k2p9");
    }

    #[test]
    fn error_envelope_shape_matches_contract() {
        let err = CtlError::new(ErrorCode::StaleRef, "element @e3 no longer resolves");
        let env = ErrorEnvelope::new("click", &err, 17);
        let json: serde_json::Value = serde_json::to_value(&env).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["command"], "click");
        assert_eq!(json["error"]["code"], "STALE_REF");
        assert!(json["error"]["hint"].as_str().unwrap().contains("snapshot"));
        assert!(json.get("evidence").is_none());
    }
}
