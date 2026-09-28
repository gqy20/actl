//! Selection between observed text channels; absence is distinct from read failure.
use actl_core::{CtlError, ErrorCode};
use uiautomation::{
    UIElement,
    patterns::{UITextPattern, UITogglePattern, UIValuePattern},
    types::UIProperty,
};
use windows::Win32::{Foundation::E_ACCESSDENIED, UI::Accessibility::UIA_E_ELEMENTNOTAVAILABLE};

pub(crate) fn failure(channel: &str, error: uiautomation::errors::Error) -> CtlError {
    let code = match error.code() {
        n if n == UIA_E_ELEMENTNOTAVAILABLE as i32 => ErrorCode::StaleRef,
        n if n == E_ACCESSDENIED.0 => ErrorCode::PermDenied,
        _ => ErrorCode::Internal,
    };
    CtlError::with_evidence(
        code,
        "UIA observation failed",
        serde_json::json!({
            "reason":"observation_failed", "channel":channel, "native_code":error.code(), "detail":error.message()
        }),
    )
}

fn supported<T>(
    available: Result<bool, CtlError>,
    read: impl FnOnce() -> Result<T, CtlError>,
) -> Result<Option<T>, CtlError> {
    if available? {
        read().map(Some)
    } else {
        Ok(None)
    }
}

pub(crate) fn available(
    element: &UIElement,
    property: UIProperty,
    channel: &str,
) -> Result<bool, CtlError> {
    let value = element
        .get_property_value(property)
        .map_err(|e| failure(channel, e))?;
    // The availability flag must be a boolean. Unknown/malformed provider data
    // is not proof that a pattern is unsupported.
    match value.get_value().map_err(|e| failure(channel, e))? {
        uiautomation::variants::Value::BOOL(value) => Ok(value),
        _ => Err(CtlError::with_evidence(
            ErrorCode::Internal,
            "invalid pattern availability",
            serde_json::json!({"reason":"observation_failed","channel":channel}),
        )),
    }
}

pub(crate) fn value(element: &UIElement) -> Result<Option<String>, CtlError> {
    supported(
        available(
            element,
            UIProperty::IsValuePatternAvailable,
            "value-pattern",
        ),
        || {
            element
                .get_pattern::<UIValuePattern>()
                .and_then(|p| p.get_value())
                .map_err(|e| failure("value-pattern", e))
        },
    )
}
pub(crate) fn text(element: &UIElement) -> Result<Option<String>, CtlError> {
    supported(
        available(element, UIProperty::IsTextPatternAvailable, "text-pattern"),
        || {
            element
                .get_pattern::<UITextPattern>()
                .and_then(|p| p.get_document_range())
                .and_then(|r| r.get_text(-1))
                .map_err(|e| failure("text-pattern", e))
        },
    )
}
pub(crate) fn toggle(element: &UIElement) -> Result<Option<String>, CtlError> {
    supported(
        available(
            element,
            UIProperty::IsTogglePatternAvailable,
            "toggle-pattern",
        ),
        || {
            element
                .get_pattern::<UITogglePattern>()
                .and_then(|p| p.get_toggle_state())
                .map(|v| format!("{v:?}"))
                .map_err(|e| failure("toggle-pattern", e))
        },
    )
}
pub(crate) fn require<T>(value: Option<T>, channel: &str) -> Result<T, CtlError> {
    value.ok_or_else(|| {
        CtlError::with_evidence(
            ErrorCode::NotActionable,
            "UIA read channel is unsupported",
            serde_json::json!({"reason":"pattern_unsupported","channel":channel}),
        )
    })
}

pub(crate) fn choose(
    value: Result<Option<String>, CtlError>,
    text: impl FnOnce() -> Result<Option<String>, CtlError>,
) -> Result<(Option<String>, Option<&'static str>), CtlError> {
    if let Some(value) = value? {
        return Ok((Some(value), Some("value-pattern")));
    }
    let value = text()?;
    let channel = value.as_ref().map(|_| "text-pattern");
    Ok((value, channel))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_value_read_never_falls_back() {
        let error = choose(Err(CtlError::internal("provider failed")), || {
            panic!("must not fall back")
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Internal);
        assert!(
            supported::<String>(Err(CtlError::internal("unknown availability")), || panic!(
                "must not read"
            ))
            .is_err()
        );
        assert!(supported::<String>(Ok(true), || Err(CtlError::internal("read failed"))).is_err());
        assert_eq!(
            supported(Ok(false), || -> Result<String, CtlError> {
                panic!("unsupported")
            })
            .unwrap(),
            None
        );
    }
    #[test]
    fn stale_and_permission_errors_remain_distinct() {
        assert_eq!(
            failure(
                "text-pattern",
                windows::core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32).into()
            )
            .code,
            ErrorCode::StaleRef
        );
        assert_eq!(
            failure("text-pattern", E_ACCESSDENIED.into()).code,
            ErrorCode::PermDenied
        );
    }
    #[test]
    fn unsupported_channel_contract() {
        let error = require::<String>(None, "text-pattern").unwrap_err();
        let output = actl_core::ErrorEnvelope::new("get", &error, 0);
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/read_unsupported.json")).unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), golden);
    }
    #[test]
    fn empty_value_is_observed_and_must_not_fall_back() {
        let result = choose(Ok(Some(String::new())), || {
            panic!("must not read TextPattern")
        });
        assert_eq!(
            result.unwrap(),
            (Some(String::new()), Some("value-pattern"))
        );
    }
    #[test]
    fn empty_document_is_an_observation() {
        assert_eq!(
            choose(Ok(None), || Ok(Some(String::new()))).unwrap(),
            (Some(String::new()), Some("text-pattern"))
        );
    }
}
