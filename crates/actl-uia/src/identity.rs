//! Snapshot-bound refs and exact keyboard focus. No ordinal fallback on mismatch.
use std::cell::RefCell;

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use uiautomation::{UIAutomation, UIElement};

thread_local! {
    static REPLAY: RefCell<Option<Value>> = const { RefCell::new(None) };
    static FOCUS: RefCell<Option<Value>> = const { RefCell::new(None) };
}

pub(crate) fn identity(element: &UIElement) -> Value {
    json!({
        "runtime_id": element.get_runtime_id().ok(),
        "pid": element.get_process_id().ok(),
        "role": element.get_control_type().ok().map(|r| format!("{r:?}")),
        "automation_id": element.get_automation_id().ok(),
        "name": element.get_name().ok(),
    })
}

/// 身份判定三态(2026-10-06 spec 决策落地):`None` = 元素无 RuntimeId/
/// pid(Word 任务窗格 MenuItem 实证——非异常,降级为 null 身份入树,ref
/// 编号照常但回放守卫会如实拒绝该元素);`Some(v)` = 正常身份;
/// `Err` = 读取本身的通道故障(如实上抛)。
pub(crate) fn observed_identity(element: &UIElement) -> Result<Option<Value>, CtlError> {
    use crate::read_channel::failure;
    let runtime_id = element
        .get_runtime_id()
        .map_err(|e| failure("identity.runtime_id", e))?;
    // pid 缓存优先(walk 产物经 tree_bounded 预取;其他来源回退实时)。
    let pid = element
        .get_cached_process_id()
        .or_else(|_| element.get_process_id().map(|p| p as i32))
        .map_err(|e| failure("identity.pid", e))?;
    if runtime_id.is_empty() || pid == 0 {
        eprintln!(
            "· identity-less interactive element (null 身份入树): role={:?} name={:?}",
            crate::observation::role_of(element)
                .map(|r| format!("{r:?}"))
                .unwrap_or_default(),
            crate::observation::name_of(element).unwrap_or_default()
        );
        return Ok(None);
    }
    Ok(Some(json!({
        "runtime_id":runtime_id, "pid":pid,
        "role":format!("{:?}", element.get_control_type().map_err(|e| failure("identity.role", e))?),
        "automation_id":element.get_automation_id().map_err(|e| failure("identity.automation_id", e))?,
        "name":element.get_name().map_err(|e| failure("identity.name", e))?,
    })))
}

/// 动作链严格版:身份必须真实存在(消费分轨纪律——动作/预览需要可校验
/// 身份;None 在此路径 = NOT_ACTIONABLE 带证据,如实拒绝)。
pub(crate) fn observed_identity_strict(element: &UIElement) -> Result<Value, CtlError> {
    match observed_identity(element)? {
        Some(v) => Ok(v),
        None => Err(CtlError::with_evidence(
            ErrorCode::NotActionable,
            "action target has no runtime identity; re-observe or choose another element",
            json!({ "diagnostic": crate::diagnostic::failure_context() }),
        )),
    }
}

pub(crate) fn same_instance(expected: &Value, current: &Value) -> bool {
    expected["runtime_id"]
        .as_array()
        .is_some_and(|r| !r.is_empty())
        && expected["pid"].as_u64().is_some_and(|p| p > 0)
        && expected["runtime_id"] == current["runtime_id"]
        && expected["pid"] == current["pid"]
}

fn validate_ref(expected: &Value, current: &Value, n: u32) -> Result<(), CtlError> {
    if same_instance(expected, current)
        && expected["role"].is_string()
        && expected["role"] == current["role"]
        && expected["automation_id"] == current["automation_id"]
        && expected["name"] == current["name"]
    {
        return Ok(());
    }
    Err(CtlError::with_evidence(
        ErrorCode::StaleRef,
        "snapshot element identity changed or unavailable; refresh the snapshot",
        json!({"ref": format!("@e{n}"), "expected": expected, "current": current}),
    ))
}

/// Scoped to one command; Drop also restores the prior context on error.
pub struct ReplayGuard(Option<Value>);
impl ReplayGuard {
    pub fn enter(id: &str, app: &str) -> Result<Self, CtlError> {
        crate::window::check_snapshot_freshness_by_pattern(id, app)?;
        let record = crate::window::load_snapshot_record(id)
            .ok_or_else(|| CtlError::new(ErrorCode::StaleRef, "snapshot record unavailable"))?;
        Ok(Self(REPLAY.with(|r| r.replace(Some(record)))))
    }
}
impl Drop for ReplayGuard {
    fn drop(&mut self) {
        REPLAY.with(|r| r.replace(self.0.take()));
    }
}

pub(crate) fn scoped_ref(
    auto: &UIAutomation,
    root: &UIElement,
    n: u32,
) -> Result<Option<UIElement>, CtlError> {
    REPLAY.with(|r| {
        let record = r.borrow();
        let Some(scope) = record.as_ref().and_then(|v| v.get("scope")) else {
            return Ok(None);
        };
        crate::scoped_snapshot::replay(auto, root, scope, n).map(Some)
    })
}

pub(crate) fn check_ref(root: &UIElement, element: &UIElement, n: u32) -> Result<(), CtlError> {
    REPLAY.with(|r| {
        let record = r.borrow();
        let Some(record) = record.as_ref() else {
            return Ok(());
        };
        let rid = root.get_runtime_id().map_err(|_| {
            CtlError::new(
                ErrorCode::StaleRef,
                "window identity unavailable during replay",
            )
        })?;
        if rid.is_empty() || json!(rid) != record["window_runtime_id"] {
            return Err(CtlError::new(
                ErrorCode::StaleRef,
                "window changed during replay",
            ));
        }
        let expected = n
            .checked_sub(1)
            .and_then(|i| record["elements"].get(i as usize))
            .unwrap_or(&Value::Null);
        validate_ref(expected, &identity(element), n)
    })
}

/// Captures the intended element, not whichever field happens to have focus.
pub(crate) struct FocusGuard(Option<Value>);
impl FocusGuard {
    pub(crate) fn enter(element: &UIElement) -> Result<Self, CtlError> {
        let expected = identity(element);
        check_focus(&expected)?;
        Ok(Self(FOCUS.with(|f| f.replace(Some(expected)))))
    }
}
impl Drop for FocusGuard {
    fn drop(&mut self) {
        FOCUS.with(|f| f.replace(self.0.take()));
    }
}
fn check_focus(expected: &Value) -> Result<(), CtlError> {
    // 看门狗区段:焦点元素查询直连 provider,挂起面之一(docs/81 §8)
    let _watchdog = crate::watchdog::enter();
    let current = UIAutomation::new()
        .and_then(|a| a.get_focused_element())
        .map(|e| identity(&e))
        .unwrap_or(Value::Null);
    validate_focus(expected, &current)
}
fn validate_focus(expected: &Value, current: &Value) -> Result<(), CtlError> {
    if same_instance(expected, current) {
        return Ok(());
    }
    Err(CtlError::with_evidence(
        ErrorCode::NotActionable,
        "keyboard focus is not the intended element; input refused",
        json!({"expected":expected, "current":current}),
    ))
}
pub(crate) fn check_focus_anchor() -> Result<(), CtlError> {
    FOCUS.with(|f| match f.borrow().as_ref() {
        Some(expected) => check_focus(expected),
        None => Ok(()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        json!({"runtime_id":[42,7],"pid":123,"role":"Edit","automation_id":"search","name":"Search"})
    }
    #[test]
    fn same_ordinal_and_name_cannot_replace_instance() {
        let expected = sample();
        let mut current = expected.clone();
        current["runtime_id"] = json!([42, 8]);
        assert_eq!(
            validate_ref(&expected, &current, 3).unwrap_err().code,
            ErrorCode::StaleRef
        );
        assert!(!same_instance(&expected, &current));
    }
    #[test]
    fn recycled_virtual_row_or_missing_identity_is_rejected() {
        let expected = sample();
        let mut current = expected.clone();
        current["name"] = json!("Different item");
        assert!(validate_ref(&expected, &current, 3).is_err());
        assert!(validate_ref(&Value::Null, &current, 3).is_err());
        current = expected.clone();
        current["pid"] = json!(456);
        assert!(!same_instance(&expected, &current));
        assert!(!same_instance(&Value::Null, &Value::Null));
        assert!(validate_ref(&expected, &expected, 3).is_ok());
    }
    #[test]
    fn refusal_envelopes_match_golden() {
        let expected = sample();
        let mut current = expected.clone();
        current["runtime_id"] = json!([42, 8]);
        let stale = validate_ref(&expected, &current, 3).unwrap_err();
        let focus = validate_focus(&expected, &current).unwrap_err();
        let actual = json!([
            actl_core::ErrorEnvelope::new("get", &stale, 0),
            actl_core::ErrorEnvelope::new("press", &focus, 0)
        ]);
        let golden: Value =
            serde_json::from_str(include_str!("../tests/golden/identity_refusals.json")).unwrap();
        assert_eq!(actual, golden);
        assert!(validate_focus(&expected, &expected).is_ok());
        assert!(validate_focus(&expected, &Value::Null).is_err());
    }
    #[test]
    fn replay_context_is_restored_even_on_early_return() {
        REPLAY.with(|r| r.replace(Some(json!("outer"))));
        {
            let _guard = ReplayGuard(REPLAY.with(|r| r.replace(Some(json!("inner")))));
        }
        REPLAY.with(|r| assert_eq!(r.borrow().as_ref(), Some(&json!("outer"))));
        REPLAY.with(|r| r.replace(None));
    }
}
