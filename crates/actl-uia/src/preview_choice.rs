//! Explicit selection among refs from one snapshot. Selection never performs an action.
use super::*;
use crate::preview_window::{Choice, Window};

pub fn validate_candidates(
    first: &Target,
    additional: &[String],
    snapshot: Option<&str>,
) -> Result<Vec<Target>, CtlError> {
    if additional.is_empty() {
        return Ok(vec![]);
    }
    if additional.len() >= 20 || snapshot.is_none_or(str::is_empty) {
        return Err(CtlError::protocol(
            "candidate selection requires --snapshot and 2..20 distinct refs",
        ));
    }
    let mut targets = vec![first.clone()];
    for raw in additional {
        targets.push(actl_core::parse_target(raw)?);
    }
    for (i, target) in targets.iter().enumerate() {
        if !matches!(target, Target::Ref(_)) || targets[..i].contains(target) {
            return Err(CtlError::protocol(
                "candidate selection requires distinct snapshot-bound @eN refs",
            ));
        }
    }
    Ok(targets)
}

fn outcome(reason: &str, snapshot: &str, count: usize, selection: Option<Value>) -> Value {
    json!({"previewed":true,"executed":false,"exit_reason":reason,"snapshot_id":snapshot,
        "candidate_count":count,"selected":selection.is_some(),"selection":selection,
        "requires_revalidation":true})
}

pub fn choose(
    app: &str,
    targets: &[Target],
    snapshot: &str,
    message: &str,
    duration: u64,
) -> Result<Value, CtlError> {
    if !(2..=20).contains(&targets.len()) || snapshot.is_empty() {
        return Err(CtlError::protocol(
            "candidate selection requires --snapshot and 2..20 refs",
        ));
    }
    let _replay = crate::identity::ReplayGuard::enter(snapshot, app)?;
    let _dpi = crate::capture::Pmv2Guard::enter();
    let mut candidates = Vec::new();
    for target in targets {
        validate(target, Some(snapshot), message, duration)?;
        if !matches!(target, Target::Ref(_)) {
            return Err(CtlError::protocol("candidate selection requires refs"));
        }
        check_stop()?;
        let located = crate::locate(Some(app), target, None)?;
        let (bounds, identity) = observe(&located.element)?;
        let owner = Window::target_owner(&located.element)?;
        candidates.push((located, bounds, identity, owner));
    }
    let mut index = 0;
    let started = Instant::now();
    loop {
        check_stop()?;
        if started.elapsed() >= Duration::from_millis(duration) {
            return Ok(outcome("expired", snapshot, targets.len(), None));
        }
        let (located, bounds, identity, owner) = &candidates[index];
        let (now_bounds, now_identity) = observe(&located.element)?;
        unchanged(*bounds, identity, now_bounds, &now_identity)?;
        Window::check_initial_target(*owner, *bounds)?;
        let name: String = located
            .name
            .as_deref()
            .unwrap_or(&located.role)
            .chars()
            .filter(|c| !c.is_control())
            .take(60)
            .collect();
        let app_name: String = located
            .window_title
            .chars()
            .filter(|c| !c.is_control())
            .take(30)
            .collect();
        let caption = format!(
            "选择目标 · 候选 {}/{} · {} · {}",
            index + 1,
            targets.len(),
            app_name,
            name
        );
        let window = Window::open_choice(*bounds, &caption, message, index, targets.len())?;
        loop {
            if !window.pump() {
                return Ok(outcome("dismissed", snapshot, targets.len(), None));
            }
            check_stop()?;
            let (now_bounds, now_identity) = observe(&located.element)?;
            unchanged(*bounds, identity, now_bounds, &now_identity)?;
            window.check_target(*owner, *bounds)?;
            if started.elapsed() >= Duration::from_millis(duration) {
                return Ok(outcome("expired", snapshot, targets.len(), None));
            }
            match window.choice() {
                Some(Choice::Previous) if index > 0 => {
                    index -= 1;
                    break;
                }
                Some(Choice::Next) if index + 1 < targets.len() => {
                    index += 1;
                    break;
                }
                Some(Choice::Select) => {
                    // Re-resolve the selected ref; the retained COM pointer alone cannot
                    // prove the current snapshot traversal still maps to the same element.
                    let current = crate::locate(Some(app), &targets[index], None)?;
                    let (now_bounds, now_identity) = observe(&current.element)?;
                    unchanged(*bounds, identity, now_bounds, &now_identity)?;
                    check_stop()?;
                    window.check_target(*owner, *bounds)?;
                    return Ok(outcome(
                        "selected",
                        snapshot,
                        targets.len(),
                        Some(json!({
                            "index":index+1,"target":targets[index].describe(),"window":located.window_title,
                            "bounds":bounds,"identity":identity
                        })),
                    ));
                }
                _ => {}
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_never_implies_execution_and_expiry_never_selects() {
        let v = outcome(
            "selected",
            "s123",
            2,
            Some(json!({"index":2,"target":"@e3"})),
        );
        let envelope = actl_core::Envelope::success("preview", Some(v), 0);
        assert_eq!(
            serde_json::to_value(envelope).unwrap(),
            serde_json::from_str::<Value>(include_str!("../tests/golden/preview_selection.json"))
                .unwrap()
        );
        for reason in ["expired", "dismissed"] {
            let result = outcome(reason, "s123", 2, None);
            assert_eq!(result["selected"], false);
            assert!(result["selection"].is_null());
            assert_eq!(result["executed"], false);
        }
    }
}
