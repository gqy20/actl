//! Bounded raw-tree observation. Missing children and failed enumeration are distinct.
use actl_core::{CtlError, ErrorCode};
use uiautomation::{UIAutomation, UIElement, types::TreeScope};

pub(crate) struct Entry<T> {
    pub element: T,
    pub depth: u32,
    pub parent: Option<usize>,
}

pub(crate) struct Observed<T> {
    pub entries: Vec<Entry<T>>,
    pub reasons: Vec<&'static str>,
}

pub(crate) fn incomplete(reasons: &[&str]) -> CtlError {
    CtlError::with_evidence(
        ErrorCode::NotActionable,
        "observation is incomplete; narrow the target scope",
        serde_json::json!({"reason":"observation_incomplete","causes":reasons}),
    )
}

impl<T> Observed<T> {
    pub fn require_complete(self) -> Result<Vec<Entry<T>>, CtlError> {
        if self.reasons.is_empty() {
            Ok(self.entries)
        } else {
            Err(incomplete(&self.reasons))
        }
    }
}

fn collect<T: Clone>(
    root: T,
    max_depth: u32,
    max_nodes: usize,
    mut children: impl FnMut(&T) -> Result<Vec<T>, CtlError>,
) -> Result<Observed<T>, CtlError> {
    let mut entries = Vec::new();
    let mut reasons = Vec::new();
    let mut pending = vec![(root, 0, None)];
    while let Some((element, depth, parent)) = pending.pop() {
        actl_core::wait_control::check()?;
        if entries.len() >= max_nodes {
            reasons.push("node_limit");
            break;
        }
        let kids = children(&element)?;
        let index = entries.len();
        entries.push(Entry {
            element,
            depth,
            parent,
        });
        if depth == max_depth && !kids.is_empty() {
            if !reasons.contains(&"depth_limit") {
                reasons.push("depth_limit");
            }
            continue;
        }
        // Bound queued work too; FindAll itself remains a synchronous provider call.
        let remaining = max_nodes.saturating_sub(entries.len() + pending.len());
        if kids.len() > remaining && !reasons.contains(&"node_limit") {
            reasons.push("node_limit");
        }
        let kids: Vec<_> = kids.into_iter().take(remaining).collect();
        pending.extend(kids.into_iter().rev().map(|c| (c, depth + 1, Some(index))));
    }
    Ok(Observed { entries, reasons })
}

pub(crate) fn tree(root: &UIElement) -> Result<Observed<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    let cache = auto.create_cache_request().map_err(crate::internal)?;
    cache
        .set_tree_filter(condition.clone())
        .map_err(crate::internal)?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(crate::internal)?;
    collect(
        root.clone(),
        crate::window::MAX_DEPTH,
        crate::window::MAX_ELEMENTS,
        |element| {
            element
                .find_all_build_cache(TreeScope::Children, &condition, &cache)
                .map_err(|e| crate::read_channel::failure("children", e))
        },
    )
}

pub(crate) fn children(root: &UIElement) -> Result<Vec<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    let cache = auto.create_cache_request().map_err(crate::internal)?;
    cache
        .set_tree_filter(condition.clone())
        .map_err(crate::internal)?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(crate::internal)?;
    root.find_all_build_cache(TreeScope::Children, &condition, &cache)
        .map_err(|e| crate::read_channel::failure("children", e))
}

pub(crate) fn parent(
    walker: &uiautomation::UITreeWalker,
    element: &UIElement,
) -> Result<Option<UIElement>, CtlError> {
    use windows::{Win32::UI::Accessibility::IUIAutomationElement, core::Interface};
    // The ABI distinguishes successful null parent from a failed provider call.
    // Adopt only a successful non-null COM reference; never construct a null UIElement.
    unsafe {
        let walker = walker.as_ref();
        let mut raw = std::ptr::null_mut();
        (walker.vtable().GetParentElement)(walker.as_raw(), element.as_ref().as_raw(), &mut raw)
            .ok()
            .map_err(|e| crate::read_channel::failure("parent", e.into()))?;
        Ok(if raw.is_null() {
            None
        } else {
            Some(UIElement::from(IUIAutomationElement::from_raw(raw)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn depth_cut_is_not_complete() {
        let result = collect(0, 1, 10, |n| Ok(if *n < 2 { vec![n + 1] } else { vec![] })).unwrap();
        assert_eq!(result.reasons, ["depth_limit"]);
        assert!(result.require_complete().is_err());
    }
    #[test]
    fn enumeration_failure_is_not_absence() {
        let result = collect(0, 5, 10, |_| {
            Err(CtlError::new(ErrorCode::PermDenied, "denied"))
        });
        assert!(matches!(result, Err(e) if e.code == ErrorCode::PermDenied));
    }
    #[test]
    fn exact_limit_is_complete() {
        let result = collect(0, 1, 2, |n| Ok(if *n == 0 { vec![1] } else { vec![] })).unwrap();
        assert!(result.reasons.is_empty());
    }

    #[test]
    fn node_cut_preserves_parent_indices_and_cannot_prove_absence() {
        let result = collect(0, 5, 3, |n| {
            Ok(match n {
                0 => vec![1, 2],
                1 => vec![3],
                _ => vec![],
            })
        })
        .unwrap();
        assert_eq!(result.reasons, ["node_limit"]);
        assert_eq!(
            result.entries.iter().map(|e| e.parent).collect::<Vec<_>>(),
            [None, Some(0), Some(0)]
        );
        let error = result.require_complete().err().unwrap();
        let envelope = actl_core::ErrorEnvelope::new("find", &error, 0);
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/observation_incomplete.json"))
                .unwrap();
        assert_eq!(serde_json::to_value(envelope).unwrap(), expected);
    }

    #[test]
    fn failure_after_a_match_is_not_a_unique_result() {
        let result = collect(0, 5, 10, |n| match n {
            0 => Ok(vec![1, 2]),
            1 => Ok(vec![]),
            _ => Err(CtlError::new(ErrorCode::StaleRef, "rebuilt")),
        });
        assert!(matches!(result, Err(e) if e.code == ErrorCode::StaleRef));
    }
}
