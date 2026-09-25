//! actl-core —— 协议层:JSON envelope、错误码、ref 语义。
//!
//! 本 crate 是纯逻辑层,禁止依赖任何 Windows 相关 crate(AGENTS.md §5.5)。
//! 契约的权威定义见 docs/06-design-blueprint.md §4/§5;变更必须走 `spec` 提交。

use serde::{Deserialize, Serialize};

/// envelope 协议版本,与 CLI semver 解耦(ADR-006)。
pub const ENVELOPE_VERSION: &str = "1";

/// 机器可读错误码(docs/06 §4 错误码表)。全集变更属 `spec` 提交。
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
    /// 元素存在但不可操作(disabled/offscreen)
    NotActionable,
    /// 协议/参数错误(不可恢复)
    Protocol,
}

/// 统一成功 envelope。stdout 只允许出现本结构(或其错误变体)的序列化结果(ADR-002)。
///
/// TODO(M1):错误变体 `ErrorEnvelope { code, message, hint, evidence }`(docs/06 §4)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub version: String,
    pub ok: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    pub duration_ms: u64,
}

impl<T> Envelope<T> {
    pub fn success(command: impl Into<String>, data: Option<T>, duration_ms: u64) -> Self {
        Self {
            version: ENVELOPE_VERSION.to_owned(),
            ok: true,
            command: command.into(),
            data,
            duration_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_serialize_as_screaming_snake_case() {
        assert_eq!(
            serde_json::to_string(&ErrorCode::StaleRef).unwrap(),
            r#""STALE_REF""#
        );
        assert_eq!(
            serde_json::to_string(&ErrorCode::PermDenied).unwrap(),
            r#""PERM_DENIED""#
        );
    }

    #[test]
    fn success_envelope_shape_matches_contract() {
        let data = json_data();
        let env = Envelope::success("click", Some(&data), 42);
        let json: serde_json::Value = serde_json::to_value(&env).unwrap();
        assert_eq!(json["version"], "1");
        assert_eq!(json["ok"], true);
        assert_eq!(json["command"], "click");
        assert_eq!(json["duration_ms"], 42);
        assert_eq!(json["data"]["action"], "click");
    }

    #[test]
    fn none_data_is_omitted_from_output() {
        let env = Envelope::success("snapshot", None::<u8>, 5);
        let json = serde_json::to_string(&env).unwrap();
        assert!(!json.contains("\"data\""));
    }

    fn json_data() -> serde_json::Value {
        serde_json::json!({ "action": "click", "ref": "@e3" })
    }
}
