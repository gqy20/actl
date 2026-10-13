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

/// 比对失败信封:与 verify 命令的证据标准对齐(docs/06)——失败必须能直接
/// 看出差异本体(expected/actual),换行根因(孤立 \r)在首份信封即可见,
/// 不必再活体观测(2026-10-01 金任务2教训:实际值只报布尔多花一整轮)。
fn mismatch_error(delivered: usize, expected: &str, actual: &str) -> CtlError {
    // 2026-10-13 Phase 1.4 三态区分(VMware 密码框实证):读回**通道工作但
    // 返回空串**(密码框/provider 背书 Edit 不暴露内容)≠ 读回不匹配——
    // 前者写入可能已生效,hint 指向截图核验,不再伪装成真失败误导重放排查。
    if actual.is_empty() {
        return CtlError::with_evidence_hint(
            ErrorCode::AssertionFailed,
            "input was delivered but the readback channel returned empty on this control",
            json!({
                "delivered": delivered,
                "value_verified": false,
                "readback": "empty",
                "reason": "readback_empty",
                "expected": expected,
                "actual": "",
                "comparison": "newline_folded_exact"
            }),
            "password/provider-backed edits read back empty by design — the write may have succeeded; verify visually with `screenshot` or re-assert via another channel instead of replaying; see docs/06 §5.2 for the channel-selection boundary",
        );
    }
    CtlError::with_evidence(
        ErrorCode::AssertionFailed,
        "input was delivered but value readback did not match within 2000ms; do not replay input",
        json!({
            "delivered": delivered,
            "value_verified": false,
            "readback": "mismatch",
            "readable": true,
            "expected": expected,
            "actual": actual,
            "comparison": "newline_folded_exact"
        }),
    )
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
            return Err(mismatch_error(delivered, expected, &value));
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
    fn mismatch_evidence_carries_expected_and_actual() {
        // 与 verify 命令证据标准对齐:expected/actual 直接入信封。
        let error = mismatch_error(19, "golden task 2\n第二行中文", "golden task 2\r第二行中文");
        assert_eq!(error.code, ErrorCode::AssertionFailed);
        assert_eq!(error.evidence.as_ref().unwrap()["readback"], "mismatch");
        let evidence = error.evidence.unwrap();
        assert_eq!(evidence["expected"], "golden task 2\n第二行中文");
        assert_eq!(evidence["actual"], "golden task 2\r第二行中文");
        assert_eq!(evidence["delivered"], 19);
        assert_eq!(evidence["value_verified"], false);
        assert_eq!(evidence["comparison"], "newline_folded_exact");
    }
    #[test]
    fn empty_readback_is_not_a_mismatch() {
        // 2026-10-13 Phase 1.4:密码框/provider 背书 Edit 读回恒空(VMware
        // Configure Encryption 实证:写入生效、掩码点出现,但读回 Some(""),
        // 旧语义把它当 mismatch 报"读回不匹配"——误导了整整两轮排查)。
        // 现在 reason=readback_empty + hint 指向截图核验,不再伪装真失败。
        let error = mismatch_error(15, "actl-eval-26h2", "");
        assert_eq!(error.code, ErrorCode::AssertionFailed);
        let evidence = error.evidence.as_ref().unwrap();
        assert_eq!(evidence["readback"], "empty");
        assert_eq!(evidence["reason"], "readback_empty");
        assert_eq!(evidence["actual"], "");
        let hint = error.hint.as_deref().unwrap();
        assert!(
            hint.contains("screenshot"),
            "hint must point to visual verification"
        );
        assert!(
            hint.contains("replay"),
            "hint must warn against blind replay"
        );
        // 真失败(非空不等)保持旧消息形态与 readable:true
        let real = mismatch_error(3, "abc", "abd");
        assert_eq!(real.evidence.as_ref().unwrap()["readback"], "mismatch");
        assert_eq!(real.evidence.as_ref().unwrap()["readable"], true);
        assert!(
            real.hint.is_none(),
            "mismatch keeps the code-level default hint"
        );
    }
    #[test]
    fn readback_empty_envelope_is_pinned_by_golden() {
        // spec 契约(硬规则 8):readback_empty 分型的信封形状钉死 golden,
        // 与 feedback_disabled_envelope 同族——message/hint/reason 三处可判读
        let error = mismatch_error(15, "actl-eval-26h2", "");
        let output = actl_core::ErrorEnvelope::new("set-value", &error, 0);
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/readback_empty_envelope.json"))
                .unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), golden);
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
