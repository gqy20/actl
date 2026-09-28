//! Optional post-input readback on the same UIA instance; never retries the write.
use actl_core::{CtlError, ErrorCode};
use serde_json::json;
use uiautomation::{
    UIElement,
    patterns::{UITextPattern, UIValuePattern},
};

pub(crate) fn verify_value(
    element: &UIElement,
    identity: &serde_json::Value,
    expected: Option<&str>,
    delivered: usize,
) -> Result<(), CtlError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        crate::feedback::check_stop()?;
        let current = crate::identity::identity(element);
        if !crate::identity::same_instance(identity, &current) {
            return Err(CtlError::with_evidence(
                ErrorCode::StaleRef,
                "input was delivered but target identity changed during readback; do not replay input",
                json!({"delivered":delivered,"value_verified":false}),
            ));
        }
        let value = element
            .get_pattern::<UIValuePattern>()
            .ok()
            .and_then(|p| p.get_value().ok())
            .or_else(|| {
                element
                    .get_pattern::<UITextPattern>()
                    .ok()
                    .and_then(|p| p.get_document_range().ok())
                    .and_then(|r| r.get_text(-1).ok())
            });
        if value.as_deref() == Some(expected) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::with_evidence(
                ErrorCode::AssertionFailed,
                "input was delivered but value readback did not match within 2000ms; do not replay input",
                json!({"delivered":delivered,"value_verified":false,"readable":value.is_some()}),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(crate::timing().poll_ms));
    }
}
