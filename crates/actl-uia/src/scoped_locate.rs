//! Strict scoped queries use fallible child enumeration, never the legacy fuzzy chain.
use crate::{internal, locate::Located};
use actl_core::{
    CtlError, ErrorCode,
    selector::{Filter, Scope, Selector},
};
use serde_json::json;
use uiautomation::{UIAutomation, UIElement, types::TreeScope};

#[derive(Clone)]
struct Node {
    element: UIElement,
    condition: uiautomation::core::UICondition,
    cache: uiautomation::core::UICacheRequest,
}

trait Tree: Clone {
    fn children(&self) -> Result<Vec<Self>, CtlError>;
    fn matches(&self, filter: &Filter) -> Result<bool, CtlError>;
    fn evidence(&self) -> Result<serde_json::Value, CtlError>;
}

/// Only an observed absence may satisfy wait --gone.
pub(crate) fn observed<T>(result: Result<T, CtlError>) -> Result<Option<T>, CtlError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

impl Tree for Node {
    fn children(&self) -> Result<Vec<Self>, CtlError> {
        self.element
            .find_all_build_cache(TreeScope::Children, &self.condition, &self.cache)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|element| Self {
                        element,
                        condition: self.condition.clone(),
                        cache: self.cache.clone(),
                    })
                    .collect()
            })
            .map_err(internal)
    }
    fn matches(&self, filter: &Filter) -> Result<bool, CtlError> {
        // A failed required property read is unknown, not an absent match.
        let role = filter
            .role
            .as_ref()
            .map(|_| {
                self.element
                    .get_control_type()
                    .map(|v| format!("{v:?}"))
                    .map_err(internal)
            })
            .transpose()?;
        let id = filter
            .id
            .as_ref()
            .map(|_| self.element.get_automation_id().map_err(internal))
            .transpose()?;
        let name = (filter.name.is_some() || filter.name_contains.is_some())
            .then(|| self.element.get_name().map_err(internal))
            .transpose()?;
        Ok(filter.matches(role.as_deref(), id.as_deref(), name.as_deref()))
    }
    fn evidence(&self) -> Result<serde_json::Value, CtlError> {
        Ok(json!({
            "role": format!("{:?}", self.element.get_control_type().map_err(internal)?),
            "name": self.element.get_name().map_err(internal)?,
            "automation_id": self.element.get_automation_id().map_err(internal)?,
        }))
    }
}

fn collect<T: Tree>(
    root: &T,
    filter: &Filter,
    scope: Scope,
    include_root: bool,
) -> Result<Vec<T>, CtlError> {
    let mut pending = vec![(root.clone(), 0u32)];
    let mut count = 0usize;
    let mut hits = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        actl_core::wait_control::check()?;
        count += 1;
        if count > crate::window::MAX_ELEMENTS || depth > crate::window::MAX_DEPTH {
            return Err(CtlError::with_evidence(
                ErrorCode::NotActionable,
                "uia query exceeded traversal budget; narrow the window or container",
                json!({"reason":"selector_budget_exceeded"}),
            ));
        }
        if (include_root || depth > 0) && node.matches(filter)? {
            hits.push(node.clone());
        }
        if scope == Scope::Descendants || depth == 0 {
            let children = node.children()?;
            if count + pending.len() + children.len() > crate::window::MAX_ELEMENTS {
                return Err(CtlError::with_evidence(
                    ErrorCode::NotActionable,
                    "uia query exceeded traversal budget; narrow the window or container",
                    json!({"reason":"selector_budget_exceeded"}),
                ));
            }
            pending.extend(children.into_iter().rev().map(|n| (n, depth + 1)));
        }
    }
    Ok(hits)
}

fn unique<T: Tree>(mut hits: Vec<T>, phase: &str) -> Result<T, CtlError> {
    if hits.len() == 1 {
        return hits
            .pop()
            .ok_or_else(|| CtlError::internal("missing unique UIA candidate"));
    }
    let count = hits.len();
    let candidates = hits
        .iter()
        .take(20)
        .map(Tree::evidence)
        .collect::<Result<Vec<_>, _>>()?;
    Err(CtlError::with_evidence(
        if count == 0 {
            ErrorCode::NotFound
        } else {
            ErrorCode::Ambiguous
        },
        format!("uia {phase} requires exactly one match; found {count}"),
        json!({"phase":phase,"count":count,"candidates":candidates,"truncated":count > 20}),
    ))
}

fn resolve<T: Tree>(root: &T, selector: &Selector) -> Result<T, CtlError> {
    let container = if let Some(filter) = &selector.within {
        unique(collect(root, filter, Scope::Descendants, true)?, "within")?
    } else {
        root.clone()
    };
    // Container itself is excluded; an unscoped subtree includes the window root.
    unique(
        collect(
            &container,
            &selector.filter,
            selector.scope,
            selector.within.is_none() && selector.scope == Scope::Descendants,
        )?,
        "match",
    )
}

/// Do not inherit the legacy walker's error-as-end-of-tree window discovery.
pub(crate) fn window(auto: &UIAutomation, app: Option<&str>) -> Result<UIElement, CtlError> {
    let handle = match app {
        Some(pattern) if pattern.starts_with("hwnd:") => {
            Some(crate::window::parse_hwnd(&pattern[5..])?)
        }
        None => Some(
            crate::window::foreground_window()
                .ok_or_else(|| CtlError::internal("foreground window could not be observed"))?
                .1,
        ),
        _ => None,
    };
    if let Some(handle) = handle {
        crate::display_support::reject_display(handle)?;
        if crate::native_windows::native_window(handle)?.is_empty() {
            return Err(CtlError::new(
                ErrorCode::NotFound,
                "native target window is absent",
            ));
        }
        // An existing native window with an unavailable provider is unknown, not absent.
        return auto
            .element_from_handle(uiautomation::types::Handle::from(handle))
            .map_err(internal);
    }
    let pattern = app.ok_or_else(|| CtlError::internal("missing window query"))?;
    // 与 find_window 同源的桌面根枚举瞬态重试;此前此处把枚举失败误映射为 Internal
    let windows = crate::observation::desktop_children(auto)?;
    let mut matched = Vec::new();
    for element in windows {
        if let Some(hwnd) = crate::window::native_hwnd(&element)
            && crate::display_support::is_display_window(hwnd)
        {
            continue;
        }
        if element.get_name().map_err(internal)?.contains(pattern) {
            matched.push(element);
        }
    }
    // Node 的 condition/cache 只服务后续 children 遍历,命中后再构建
    let hits: Vec<Node> = if matched.is_empty() {
        Vec::new()
    } else {
        let condition = auto.create_true_condition().map_err(internal)?;
        let cache = raw_cache(auto, &condition)?;
        matched
            .into_iter()
            .map(|element| Node {
                element,
                condition: condition.clone(),
                cache: cache.clone(),
            })
            .collect()
    };
    Ok(unique(hits, "window")?.element)
}

fn raw_cache(
    auto: &UIAutomation,
    condition: &uiautomation::core::UICondition,
) -> Result<uiautomation::core::UICacheRequest, CtlError> {
    let cache = auto.create_cache_request().map_err(internal)?;
    // FindAll's implicit control view omits raw-only containers visible to snapshot.
    // Explicit RawView (true) preserves actual parent/child scope as well as membership.
    cache.set_tree_filter(condition.clone()).map_err(internal)?;
    cache.set_tree_scope(TreeScope::Element).map_err(internal)?;
    Ok(cache)
}

pub(crate) fn locate(
    auto: &UIAutomation,
    root: &UIElement,
    selector: &Selector,
) -> Result<Located, CtlError> {
    let condition = auto.create_true_condition().map_err(internal)?;
    let cache = raw_cache(auto, &condition)?;
    let node = Node {
        element: root.clone(),
        condition,
        cache,
    };
    let hit = resolve(&node, selector)?;
    Ok(Located {
        window_title: root.get_name().map_err(internal)?,
        role: format!("{:?}", hit.element.get_control_type().map_err(internal)?),
        name: Some(hit.element.get_name().map_err(internal)?),
        automation_id: Some(hit.element.get_automation_id().map_err(internal)?),
        resolved_by: "uia-composite",
        element: hit.element,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Fake {
        id: &'static str,
        role: &'static str,
        name: &'static str,
        children: Vec<Fake>,
        broken: bool,
    }
    impl Tree for Fake {
        fn children(&self) -> Result<Vec<Self>, CtlError> {
            if self.broken {
                Err(CtlError::internal("provider disconnected"))
            } else {
                Ok(self.children.clone())
            }
        }
        fn matches(&self, f: &Filter) -> Result<bool, CtlError> {
            Ok(f.matches(Some(self.role), Some(self.id), Some(self.name)))
        }
        fn evidence(&self) -> Result<serde_json::Value, CtlError> {
            Ok(json!({"id":self.id}))
        }
    }
    fn node(id: &'static str, role: &'static str, children: Vec<Fake>) -> Fake {
        Fake {
            id,
            role,
            name: "OK",
            children,
            broken: false,
        }
    }
    fn tree() -> Fake {
        node(
            "window",
            "Window",
            vec![
                node("other", "Button", vec![]),
                node(
                    "panel",
                    "Pane",
                    vec![
                        node("save", "Button", vec![]),
                        node("nested", "Pane", vec![node("deep", "Button", vec![])]),
                    ],
                ),
            ],
        )
    }
    fn query(json: &str) -> Selector {
        Selector::parse(json).unwrap()
    }
    #[test]
    fn container_and_conjunction_are_required() {
        let q = query(
            r#"{"match":{"role":"Button","name":"OK"},"within":{"id":"panel"},"scope":"children"}"#,
        );
        assert_eq!(resolve(&tree(), &q).unwrap().id, "save");
        let mut missing = q.clone();
        missing.within.as_mut().unwrap().id = Some("absent".into());
        let e = resolve(&tree(), &missing).err().unwrap();
        assert_eq!(e.code, ErrorCode::NotFound);
        assert_eq!(e.evidence.unwrap()["phase"], "within");
        let mut mismatch = q;
        mismatch.filter.name = Some("ok".into());
        assert_eq!(
            resolve(&tree(), &mismatch).err().unwrap().code,
            ErrorCode::NotFound
        );
    }
    #[test]
    fn descendants_and_ambiguous_containers_fail_closed() {
        let q = query(r#"{"match":{"role":"Button"},"within":{"id":"panel"}}"#);
        assert_eq!(
            resolve(&tree(), &q).err().unwrap().code,
            ErrorCode::Ambiguous
        );
        let q = query(r#"{"match":{"id":"save"},"within":{"role":"Pane"}}"#);
        let e = resolve(&tree(), &q).err().unwrap();
        assert_eq!(e.code, ErrorCode::Ambiguous);
        assert_eq!(e.evidence.unwrap()["phase"], "within");
    }
    #[test]
    fn exclude_container_and_relocate_after_rebuild() {
        let q = query(r#"{"match":{"id":"panel"},"within":{"id":"panel"}}"#);
        assert_eq!(
            resolve(&tree(), &q).err().unwrap().code,
            ErrorCode::NotFound
        );
        let q = query(r#"{"match":{"id":"save"}}"#);
        assert_eq!(resolve(&tree(), &q).unwrap().id, "save");
        let rebuilt = node("window", "Window", vec![]);
        assert_eq!(
            resolve(&rebuilt, &q).err().unwrap().code,
            ErrorCode::NotFound
        );
    }
    #[test]
    fn incomplete_observation_never_proves_uniqueness_or_absence() {
        let mut root = tree();
        root.children[0].broken = true;
        for q in [
            r#"{"match":{"id":"save"}}"#,
            r#"{"match":{"id":"missing"}}"#,
        ] {
            assert_eq!(
                resolve(&root, &query(q)).err().unwrap().code,
                ErrorCode::Internal
            );
        }
    }
    #[test]
    fn budget_is_an_error_not_partial_success() {
        let mut root = node("last", "Button", vec![]);
        for _ in 0..=crate::window::MAX_DEPTH {
            root = node("level", "Pane", vec![root]);
        }
        assert_eq!(
            resolve(&root, &query(r#"{"match":{"id":"last"}}"#))
                .err()
                .unwrap()
                .code,
            ErrorCode::NotActionable
        );
    }

    #[test]
    fn wait_does_not_convert_errors_to_absence() {
        for code in [
            ErrorCode::Internal,
            ErrorCode::NotActionable,
            ErrorCode::Ambiguous,
            ErrorCode::Protocol,
            ErrorCode::Aborted,
            ErrorCode::PermDenied,
            ErrorCode::StaleRef,
        ] {
            assert_eq!(
                observed::<()>(Err(CtlError::new(code, "unknown")))
                    .unwrap_err()
                    .code,
                code
            );
        }
        assert!(
            observed::<()>(Err(CtlError::new(ErrorCode::NotFound, "absent")))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ambiguity_envelope_contract() {
        let q = query(r#"{"match":{"role":"Button"},"within":{"id":"panel"}}"#);
        let error = resolve(&tree(), &q).err().unwrap();
        let envelope = actl_core::ErrorEnvelope::new("find", &error, 0);
        assert_eq!(
            serde_json::to_value(envelope).unwrap(),
            serde_json::from_str::<serde_json::Value>(include_str!(
                "../tests/golden/selector_ambiguous.json"
            ))
            .unwrap()
        );
    }
}
