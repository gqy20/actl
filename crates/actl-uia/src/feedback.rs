//! Resolved execution feedback. Uses only local action facts, never key/text contents.
use actl_core::activity::{ActionTarget, ClipboardEffect, InputEffects};
use actl_core::signal_session::ActionGuard as CoreActionGuard;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;

pub struct ActionGuard {
    inner: CoreActionGuard,
    _foreground: Option<crate::handoff::ForegroundLease>,
}
impl std::ops::Deref for ActionGuard {
    type Target = CoreActionGuard;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl std::ops::DerefMut for ActionGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

thread_local! { static KEYBOARD_ANCHOR: std::cell::Cell<Option<(usize,usize)>> = const { std::cell::Cell::new(None) }; }
fn physical_focus() -> Result<(usize, usize), CtlError> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo,
    };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(0, &mut info) }.map_err(crate::internal)?;
    Ok((
        unsafe { GetForegroundWindow() }.0 as usize,
        info.hwndFocus.0 as usize,
    ))
}
pub(crate) struct KeyboardAnchor(Option<(usize, usize)>);
impl KeyboardAnchor {
    pub(crate) fn capture() -> Result<Self, CtlError> {
        let focus = physical_focus()?;
        if focus.0 == 0 || focus.1 == 0 {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "no physical keyboard focus",
            ));
        }
        Ok(Self(KEYBOARD_ANCHOR.with(|a| a.replace(Some(focus)))))
    }
}
impl Drop for KeyboardAnchor {
    fn drop(&mut self) {
        KEYBOARD_ANCHOR.with(|a| a.set(self.0));
    }
}
pub(crate) fn check_keyboard_anchor() -> Result<(), CtlError> {
    check_stop()?;
    crate::identity::check_focus_anchor()?;
    if let Some(expected) = KEYBOARD_ANCHOR.with(|a| a.get())
        && physical_focus()? != expected
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "physical keyboard focus changed before injection",
        ));
    }
    Ok(())
}

pub(crate) fn window_action(
    kind: &str,
    win: &UIElement,
    focus: bool,
) -> Result<ActionGuard, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let identity = crate::identity::observed_identity_strict(win)?;
    let title = win.get_name().map_err(crate::internal)?;
    let before = target(win, &title);
    let mut action = prepare(
        kind,
        InputEffects {
            focus,
            ..Default::default()
        },
        before.clone(),
    )?;
    if !crate::identity::same_instance(&identity, &crate::identity::observed_identity_strict(win)?)
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "window changed during action preparation",
        ));
    }
    check_stop()?;
    action.executing();
    Ok(action)
}

pub(crate) fn check_stop() -> Result<(), CtlError> {
    actl_core::stop::check_current()?;
    crate::display_gate::check_active()?;
    crate::handoff::check()
}

pub(crate) fn target(elem: &UIElement, window: &str) -> ActionTarget {
    let bounds = elem.get_bounding_rectangle().ok().and_then(|r| {
        let b = [r.get_left(), r.get_top(), r.get_right(), r.get_bottom()];
        (b[2] > b[0] && b[3] > b[1]).then_some(b)
    });
    ActionTarget {
        window: Some(window.into()),
        bounds,
        ..Default::default()
    }
}

pub(crate) fn keyboard(paste: bool, write: bool) -> InputEffects {
    InputEffects {
        keyboard: true,
        focus: true,
        clipboard: if write {
            ClipboardEffect::Write
        } else if paste {
            ClipboardEffect::Read
        } else {
            ClipboardEffect::Untouched
        },
        ..Default::default()
    }
}

/// Require a matching visible companion and acknowledgement of this resolved action.
/// The caller MUST revalidate the target after this cancellable preparation interval.
pub(crate) fn prepare(
    kind: &str,
    effects: InputEffects,
    target: ActionTarget,
) -> Result<ActionGuard, CtlError> {
    check_stop()?;
    let lease = crate::display_gate::ensure()?;
    crate::handoff::prepare(target.window.as_deref().unwrap_or(kind), kind)?;
    let foreground = crate::handoff::foreground_lease()?;
    let guard = ActionGuard {
        inner: CoreActionGuard::begin(kind, effects, target),
        _foreground: foreground,
    };
    if guard.is_recording() {
        crate::display_gate::wait_presented(&guard, &lease.epoch)?;
    }
    Ok(guard)
}

pub(crate) fn check_foreground(expected: &str) -> Result<(), CtlError> {
    check_stop()?;
    if crate::kbd::foreground_title().as_deref() != Some(expected) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "foreground changed during action preparation; re-observe before retrying",
        ));
    }
    Ok(())
}

/// Bounds describe the preview. Semantic actions bind to component identity.
pub(crate) fn semantic<T>(
    kind: &str,
    elem: &UIElement,
    window: &str,
    execute: impl FnOnce() -> Result<T, CtlError>,
) -> Result<T, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let before = semantic_identity(elem)?;
    let mut action = prepare(kind, InputEffects::default(), target(elem, window))?;
    let after = semantic_identity(elem)?;
    let enabled = elem.is_enabled().map_err(crate::internal)?;
    validate_semantic(&before, &after, enabled)?;
    check_stop()?;
    action.executing();
    let value = execute()?;
    action.delivered();
    Ok(value)
}

fn semantic_identity(elem: &UIElement) -> Result<serde_json::Value, CtlError> {
    let identity = crate::identity::observed_identity_strict(elem)?;
    let auto = uiautomation::UIAutomation::new().map_err(crate::internal)?;
    let walker = auto.get_raw_view_walker().map_err(crate::internal)?;
    let mut current = elem.clone();
    for _ in 0..128 {
        let handle = current
            .get_native_window_handle()
            .map_err(|e| crate::read_channel::failure("semantic.host_handle", e))?;
        if !handle.is_invalid() {
            let host = crate::identity::observed_identity_strict(&current)?;
            return Ok(serde_json::json!({"target":identity,
                "host":{"runtime_id":host["runtime_id"], "pid":host["pid"]}}));
        }
        current = crate::observation::parent(&walker, &current)?.ok_or_else(|| {
            CtlError::new(
                ErrorCode::NotActionable,
                "semantic target has no native host",
            )
        })?;
    }
    Err(CtlError::new(
        ErrorCode::NotActionable,
        "semantic host ancestry exceeds limit",
    ))
}

fn validate_semantic(
    before: &serde_json::Value,
    after: &serde_json::Value,
    enabled: bool,
) -> Result<(), CtlError> {
    let same =
        crate::identity::same_instance(&before["target"], &after["target"]) && before == after;
    if !same || !enabled {
        // 同码两态(disabled/identity_changed)曾共用一句 message,agent 只读 message
        // 会误诊为漂移去重拍快照——对禁用字段完全无效(2026-10-13 门户预填锁定
        // 字段实证)。message/hint 按 reason 分型,evidence 结构不变。
        let (message, hint) = if !enabled {
            (
                "target is disabled at action time",
                "disabled elements cannot be written — account-prefilled/locked fields stay read-only; confirm with `get --property enabled` and pick a writable target instead of retrying",
            )
        } else {
            (
                "target changed during action preparation",
                "re-observe with `snapshot` — the ref likely drifted across a re-render; re-resolve the target before acting, do not replay blindly",
            )
        };
        return Err(CtlError::with_evidence_hint(
            ErrorCode::NotActionable,
            message,
            serde_json::json!({
                "reason": if !enabled { "target_disabled" } else { "target_identity_changed" },
                "enabled": enabled, "action_started": false,
            }),
            hint,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn semantic_target_rejects_replacement_reparenting_and_disable() {
        let before = json!({"target":{"runtime_id":[42,7],"pid":123,
            "role":"Edit","automation_id":"line","name":"Go to line."},
            "host":{"runtime_id":[42,1],"pid":123}});
        // Bounds intentionally belong only to presentation, not this action identity.
        assert!(validate_semantic(&before, &before, true).is_ok());
        for field in ["runtime_id", "pid", "role", "automation_id", "name"] {
            let mut replaced = before.clone();
            replaced["target"][field] = json!("changed");
            assert!(
                validate_semantic(&before, &replaced, true).is_err(),
                "{field}"
            );
        }
        let mut reparented = before.clone();
        reparented["host"]["runtime_id"] = json!([42, 2]);
        assert!(validate_semantic(&before, &reparented, true).is_err());
        let error = validate_semantic(&before, &before, false).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotActionable);
        // disabled 与身份漂移分离:message/evidence.reason/hint 三处可判读,
        // 只读 message 的 agent 不再误诊为漂移去重拍快照
        assert_eq!(error.message, "target is disabled at action time");
        assert_eq!(
            error.evidence.as_ref().unwrap()["reason"],
            "target_disabled"
        );
        assert!(error.hint.as_deref().unwrap().contains("enabled"));
        let mut replaced = before.clone();
        replaced["target"]["runtime_id"] = json!([9, 9]);
        let drifted = validate_semantic(&before, &replaced, true).unwrap_err();
        assert_eq!(drifted.message, "target changed during action preparation");
        assert_eq!(
            drifted.evidence.as_ref().unwrap()["reason"],
            "target_identity_changed"
        );
        assert!(drifted.hint.as_deref().unwrap().contains("snapshot"));
    }
    #[test]
    fn disabled_target_envelope_is_pinned_by_golden() {
        // spec 契约(硬规则 8):disabled 分型的 envelope 形状钉死 golden——
        // 与 identity 漂移同码不同 message/hint/evidence,agent 三处可判读
        let before = json!({"target":{"runtime_id":[42,7],"pid":123,
            "role":"Edit","automation_id":"line","name":"Go to line."},
            "host":{"runtime_id":[42,1],"pid":123}});
        let error = validate_semantic(&before, &before, false).unwrap_err();
        let output = actl_core::ErrorEnvelope::new("set-value", &error, 0);
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/golden/feedback_disabled_envelope.json"
        ))
        .unwrap();
        assert_eq!(serde_json::to_value(output).unwrap(), golden);
    }
}
