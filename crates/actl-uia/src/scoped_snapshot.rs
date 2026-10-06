//! Local observations keep a separate, snapshot-bound ref namespace.
use actl_core::{CtlError, ErrorCode, Target, selector::Selector};
use serde_json::{Value, json};
use uiautomation::{UIAutomation, UIElement};

pub fn validate(within: &str, depth: u32, limit: usize) -> Result<Selector, CtlError> {
    if depth > crate::MAX_DEPTH || !(1..=crate::MAX_ELEMENTS).contains(&limit) {
        return Err(CtlError::protocol(
            "local snapshot requires depth 0..40 and limit 1..5000",
        ));
    }
    match actl_core::parse_target(within)? {
        Target::Uia(selector) => Ok(*selector),
        _ => Err(CtlError::protocol(
            "snapshot --within requires a strict uia: selector",
        )),
    }
}

pub fn capture(
    app: &str,
    within: &str,
    depth: u32,
    limit: usize,
) -> Result<(crate::window::CaptureResult, Value), CtlError> {
    let selector = validate(within, depth, limit)?;
    actl_core::stop::check_current()?;
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let root = crate::scoped_locate::window(&auto, Some(app))?;
    let window_identity = crate::identity::observed_identity_strict(&root)?;
    let located = crate::scoped_locate::locate(&auto, &root, &selector)?;
    let identity = crate::identity::observed_identity_strict(&located.element)?;
    let scope =
        json!({"version":1,"selector":within,"identity":identity,"depth":depth,"limit":limit});
    let observed = crate::observation::tree_bounded(&located.element, depth, limit)?;
    let mut nodes = Vec::new();
    let mut element_identities = Vec::new();
    let truncation_reasons =
        crate::window::append_observed(observed, &mut nodes, &mut element_identities)?;
    ensure_identity(
        &identity,
        &crate::identity::observed_identity_strict(&located.element)?,
    )?;
    ensure_identity(
        &window_identity,
        &crate::identity::observed_identity_strict(&root)?,
    )?;
    actl_core::stop::check_current()?;
    Ok((
        crate::window::CaptureResult {
            window: crate::window::WindowInfo {
                title: located.window_title,
                class: root
                    .get_classname()
                    .map_err(|e| crate::read_channel::failure("window.class", e))?,
                pid: root
                    .get_process_id()
                    .map_err(|e| crate::read_channel::failure("window.pid", e))?,
            },
            window_runtime_id: root
                .get_runtime_id()
                .map_err(|e| crate::read_channel::failure("window.runtime_id", e))?,
            nodes,
            element_identities,
            truncated: !truncation_reasons.is_empty(),
            truncation_reasons,
            wake_ms: 0,
        },
        scope,
    ))
}

fn stale(message: &str) -> CtlError {
    CtlError::with_evidence(
        ErrorCode::StaleRef,
        message,
        json!({"reason":"snapshot_scope_changed","retry_input":false}),
    )
}
fn ensure_identity(before: &Value, after: &Value) -> Result<(), CtlError> {
    if crate::identity::same_instance(before, after) && before == after {
        Ok(())
    } else {
        Err(stale("snapshot container identity changed; observe again"))
    }
}

/// Re-observe with the original scope and budgets; never reinterpret a local ref globally.
pub(crate) fn replay(
    auto: &UIAutomation,
    root: &UIElement,
    scope: &Value,
    want: u32,
) -> Result<UIElement, CtlError> {
    let within = scope["selector"]
        .as_str()
        .ok_or_else(|| stale("invalid snapshot scope"))?;
    let depth = scope["depth"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| stale("invalid snapshot depth"))?;
    let limit = scope["limit"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| stale("invalid snapshot limit"))?;
    if scope["version"] != 1 {
        return Err(stale("unsupported snapshot scope"));
    }
    let selector = validate(within, depth, limit).map_err(|_| stale("invalid snapshot scope"))?;
    let located = crate::scoped_locate::locate(auto, root, &selector).map_err(|e| {
        if matches!(e.code, ErrorCode::NotFound | ErrorCode::Ambiguous) {
            stale("snapshot container is absent or ambiguous")
        } else {
            e
        }
    })?;
    ensure_identity(
        &scope["identity"],
        &crate::identity::observed_identity_strict(&located.element)?,
    )?;
    let observed = crate::observation::tree_bounded(&located.element, depth, limit)?;
    let mut counter = 0;
    for entry in observed.entries {
        let role = entry
            .element
            .get_control_type()
            .map_err(|e| crate::read_channel::failure("control_type", e))?;
        if actl_core::is_interactive_role(&format!("{role:?}")) {
            counter += 1;
            if counter == want {
                ensure_identity(
                    &scope["identity"],
                    &crate::identity::observed_identity_strict(&located.element)?,
                )?;
                actl_core::stop::check_current()?;
                return Ok(entry.element);
            }
        }
    }
    Err(stale("ref is outside the recorded local snapshot"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn container_replacement_and_missing_identity_are_stale() {
        let first = json!({"runtime_id":[1,2],"pid":4,"name":"pane"});
        assert!(ensure_identity(&first, &first).is_ok());
        for other in [
            Value::Null,
            json!({"runtime_id":[1,3],"pid":4,"name":"pane"}),
        ] {
            assert_eq!(
                ensure_identity(&first, &other).unwrap_err().code,
                ErrorCode::StaleRef
            );
        }
    }
}
