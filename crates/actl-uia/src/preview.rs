//! Observation-only preview. Never obtains consent, focuses, or injects input.
use actl_core::{CtlError, ErrorCode, Target};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub fn validate(
    target: &Target,
    snapshot: Option<&str>,
    message: &str,
    duration: u64,
) -> Result<(), CtlError> {
    if matches!(target, Target::Coord(..)) {
        return Err(CtlError::protocol(
            "preview requires an element, not coordinates",
        ));
    }
    if matches!(target, Target::Ref(_)) && snapshot.is_none_or(str::is_empty) {
        return Err(CtlError::protocol("preview refs require --snapshot"));
    }
    if !(500..=30_000).contains(&duration) {
        return Err(CtlError::protocol("preview duration must be 500..30000 ms"));
    }
    if message.trim().is_empty()
        || message.chars().count() > 160
        || message.chars().any(char::is_control)
    {
        return Err(CtlError::protocol(
            "preview message requires 1..160 characters without controls",
        ));
    }
    Ok(())
}

fn observe(element: &uiautomation::UIElement) -> Result<([i32; 4], Value), CtlError> {
    let identity = crate::identity::observed_identity(element)?;
    if element
        .is_offscreen()
        .map_err(|e| crate::read_channel::failure("preview.offscreen", e))?
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "preview target is offscreen",
        ));
    }
    let r = element
        .get_bounding_rectangle()
        .map_err(|e| crate::read_channel::failure("preview.bounds", e))?;
    let bounds = [r.get_left(), r.get_top(), r.get_right(), r.get_bottom()];
    if bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || i64::from(bounds[2]) - i64::from(bounds[0]) > 16000
        || i64::from(bounds[3]) - i64::from(bounds[1]) > 16000
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "preview target has invalid bounds",
        ));
    }
    Ok((bounds, identity))
}

fn check_stop() -> Result<(), CtlError> {
    if actl_core::state::SignalPaths::default().stop_requested() {
        return Err(CtlError::new(
            ErrorCode::Aborted,
            "preview interrupted by stop",
        ));
    }
    actl_core::wait_control::check()
}

pub fn show(app: &str, target: &Target, message: &str, duration: u64) -> Result<Value, CtlError> {
    check_stop()?;
    let _dpi = crate::capture::Pmv2Guard::enter();
    let located = crate::locate(Some(app), target, None)?;
    let (bounds, identity) = observe(&located.element)?;
    let owner = crate::preview_window::Window::target_owner(&located.element)?;
    crate::preview_window::Window::check_initial_target(owner, bounds)?;
    check_stop()?;
    let caption = format!(
        "仅预览 · {}",
        located.window_title.chars().take(60).collect::<String>()
    );
    let window = crate::preview_window::Window::open(bounds, &caption, message)?;
    let started = Instant::now();
    let reason = loop {
        if !window.pump() {
            break "dismissed";
        }
        check_stop()?;
        let (current_bounds, current_identity) = observe(&located.element)?;
        unchanged(bounds, &identity, current_bounds, &current_identity)?;
        window.check_target(owner, bounds)?;
        if started.elapsed() >= Duration::from_millis(duration) {
            break "expired";
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(
        json!({"previewed":true,"executed":false,"exit_reason":reason,
        "target":target.describe(),"window":located.window_title,"bounds":bounds,
        "resolved_by":located.resolved_by,"message":message}),
    )
}

fn unchanged(
    before: [i32; 4],
    identity: &Value,
    after: [i32; 4],
    current: &Value,
) -> Result<(), CtlError> {
    if before != after || identity != current {
        return Err(CtlError::with_evidence(
            ErrorCode::StaleRef,
            "preview target changed; observe again",
            json!({"reason":"preview_target_changed","executed":false}),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duration_and_message_limits_are_inclusive() {
        for duration in [500, 30_000] {
            assert!(validate(&Target::Id("a".into()), None, &"字".repeat(160), duration).is_ok());
        }
        assert!(validate(&Target::Id("a".into()), None, &"字".repeat(161), 500).is_err());
        assert!(validate(&Target::Ref(1), Some("s1"), "preview", 500).is_ok());
    }
    #[test]
    fn movement_and_identity_drift_stop_old_outline() {
        let bounds = [10, 20, 100, 200];
        let id = json!({"runtime_id":[1,2],"pid":3,"name":"original"});
        assert!(unchanged(bounds, &id, bounds, &id).is_ok());
        assert_eq!(
            unchanged(bounds, &id, [11, 20, 101, 200], &id)
                .unwrap_err()
                .code,
            ErrorCode::StaleRef
        );
        assert_eq!(
            unchanged(bounds, &id, bounds, &json!({"runtime_id":[1,4]}))
                .unwrap_err()
                .code,
            ErrorCode::StaleRef
        );
    }
}
