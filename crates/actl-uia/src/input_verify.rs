//! Optional post-input readback on the same UIA instance; never retries the write.
use actl_core::{CtlError, ErrorCode};
use serde_json::json;
use uiautomation::UIElement;

/// 换行规范化:换行在剪贴板/UIA 提供方之间有三种表示——期望值用 \n,
/// Win32 EDIT 报 \r\n,Store 记事本的 ValuePattern 实测报孤立 \r。
/// 比较前统一为 \n,语义(行内容)不变、表示无关。
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn readback(
    result: Result<Option<String>, CtlError>,
    delivered: usize,
) -> Result<String, CtlError> {
    result
        .and_then(|value| crate::read_channel::require(value, "value-or-text-pattern"))
        .map_err(|mut error| {
            let mut evidence = error.evidence.take().unwrap_or_else(|| json!({}));
            evidence["delivered"] = json!(delivered);
            evidence["value_verified"] = json!(false);
            evidence["retry"] = json!("never");
            error.evidence = Some(evidence);
            error.message = "input was delivered but readback failed; do not replay input".into();
            error
        })
}

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
        let value = readback(crate::read::read_value(element), delivered)?;
        if normalize_newlines(&value) == normalize_newlines(expected) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::with_evidence(
                ErrorCode::AssertionFailed,
                "input was delivered but value readback did not match within 2000ms; do not replay input",
                json!({"delivered":delivered,"value_verified":false,"readable":true}),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(crate::timing().poll_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newline_normalization_matches_across_line_ending_forms() {
        // 期望值用 \n;Win32 EDIT 报 \r\n;Store 记事本 ValuePattern 实测报孤立 \r
        // (golden-task-2 2026-10-01 现场:回读 "a\rb" 而期望 "a\nb")——三种统一。
        assert_eq!(normalize_newlines("a\r\nb"), normalize_newlines("a\nb"));
        assert_eq!(normalize_newlines("a\r\nb"), "a\nb");
        assert_eq!(normalize_newlines("a\rb"), "a\nb");
        assert_eq!(normalize_newlines("a\rb\r\nc\nd"), "a\nb\nc\nd");
        // 无换行与空串不受影响。
        assert_eq!(normalize_newlines("abc"), "abc");
        assert_eq!(normalize_newlines(""), "");
    }
    #[test]
    fn failed_readback_retains_delivery_and_provider_evidence() {
        let error = CtlError::with_evidence(
            ErrorCode::StaleRef,
            "provider stale",
            json!({"channel":"value-pattern"}),
        );
        let failure = readback(Err(error), 3).unwrap_err();
        assert_eq!(failure.code, ErrorCode::StaleRef);
        let evidence = failure.evidence.unwrap();
        assert_eq!(evidence["delivered"], 3);
        assert_eq!(evidence["channel"], "value-pattern");
        assert_eq!(evidence["retry"], "never");
        assert_eq!(readback(Ok(Some(String::new())), 0).unwrap(), "");
        assert_eq!(
            readback(Ok(None), 0).unwrap_err().code,
            ErrorCode::NotActionable
        );
    }
}
