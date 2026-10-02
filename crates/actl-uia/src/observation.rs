//! Bounded raw-tree observation. Missing children and failed enumeration are distinct.
use crate::observation_diagnostics::{Stage, observe};
use actl_core::{CtlError, ErrorCode};
use uiautomation::{
    UIAutomation, UIElement,
    types::{TreeScope, UIProperty},
};

pub(crate) fn cached_hwnd(element: &UIElement) -> Option<isize> {
    let handle: windows::Win32::Foundation::HWND =
        element.get_cached_native_window_handle().ok()?.into();
    (!handle.0.is_null()).then_some(handle.0 as isize)
}

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
    tree_bounded(root, crate::window::MAX_DEPTH, crate::window::MAX_ELEMENTS)
}

pub(crate) fn tree_bounded(
    root: &UIElement,
    depth: u32,
    limit: usize,
) -> Result<Observed<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    let cache = auto.create_cache_request().map_err(crate::internal)?;
    cache
        .set_tree_filter(condition.clone())
        .map_err(crate::internal)?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(crate::internal)?;
    collect(root.clone(), depth, limit, |element| {
        observe(Stage::Children, cached_hwnd(element), || {
            element
                .find_all_build_cache(TreeScope::Children, &condition, &cache)
                .map_err(|e| crate::read_channel::failure("children", e))
        })
    })
}

pub(crate) fn children(root: &UIElement) -> Result<Vec<UIElement>, CtlError> {
    observe(Stage::Children, cached_hwnd(root), || children_inner(root))
}

fn children_inner(root: &UIElement) -> Result<Vec<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    let cache = auto.create_cache_request().map_err(crate::internal)?;
    // Native identity is returned by this existing batch, never fetched by an
    // additional live provider call merely to populate diagnostic metadata.
    cache
        .add_property(UIProperty::NativeWindowHandle)
        .map_err(crate::internal)?;
    cache
        .set_tree_filter(condition.clone())
        .map_err(crate::internal)?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(crate::internal)?;
    root.find_all_build_cache(TreeScope::Children, &condition, &cache)
        .map_err(|e| crate::read_channel::failure("children", e))
}

/// 桌面根整批枚举的有界瞬态重试。一次 FindAll(Children) 是全有或全无:
/// 任何**无关**顶层窗口在创建/销毁中途,其 provider 会以
/// UIA_E_ELEMENTNOTAVAILABLE 让整批枚举失败,进而让所有应用的标题定位
/// 报 STALE_REF(窗口不存在时还会把 NOT_FOUND 误报成 STALE_REF;
/// 2026-09-28 记事本开关联扰动下 9/10 复现,HWND 路径不受影响)。
/// 仅重试 StaleRef,与 flow 层 stale_read 同参数(3 次/200ms);其他错误
/// 立即上抛,持久失败保留错误并附尝试次数。窗口子树枚举不走此重试:
/// 目标应用自身的控件失效是另一个故障域,由既有语义处理。
pub(crate) fn with_transient_retry<T>(
    mut attempt: impl FnMut() -> Result<T, CtlError>,
) -> Result<T, CtlError> {
    const MAX_ATTEMPTS: usize = 3;
    const RETRY_INTERVAL_MS: u64 = 200;
    let mut stale = None;
    for index in 0..MAX_ATTEMPTS {
        actl_core::wait_control::check()?;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) if error.code == ErrorCode::StaleRef => stale = Some(error),
            Err(error) => return Err(error),
        }
        if index + 1 < MAX_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(RETRY_INTERVAL_MS));
        }
    }
    let mut error =
        stale.ok_or_else(|| CtlError::internal("retry finished without an observation"))?;
    if let Some(serde_json::Value::Object(map)) = error.evidence.as_mut() {
        map.insert("attempts".into(), serde_json::json!(MAX_ATTEMPTS));
    }
    Err(error)
}

/// 桌面根的直接子窗口(带瞬态重试);标题定位与 uia: 作用域解析共用。
pub(crate) fn desktop_children(auto: &UIAutomation) -> Result<Vec<UIElement>, CtlError> {
    let _t = actl_core::trace::scope("window.desktop_children");
    with_transient_retry(|| {
        let root = observe(Stage::DesktopRoot, None, || {
            auto.get_root_element()
                .map_err(|e| crate::read_channel::failure("desktop.root", e))
        })?;
        children(&root)
    })
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

    #[test]
    fn transient_root_staleness_is_retried_then_recovers() {
        let calls = std::cell::Cell::new(0);
        let result = with_transient_retry(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(CtlError::with_evidence(
                    ErrorCode::StaleRef,
                    "UIA observation failed",
                    serde_json::json!({"reason":"observation_failed","channel":"children"}),
                ))
            } else {
                Ok(vec![1u32])
            }
        });
        assert_eq!(result.unwrap(), vec![1]);
        assert_eq!(calls.get(), 2);
    }
    #[test]
    fn persistent_root_staleness_keeps_error_with_attempt_count() {
        let calls = std::cell::Cell::new(0);
        let error = with_transient_retry(|| {
            calls.set(calls.get() + 1);
            Err::<(), _>(CtlError::with_evidence(
                ErrorCode::StaleRef,
                "UIA observation failed",
                serde_json::json!({"reason":"observation_failed","channel":"children"}),
            ))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleRef);
        assert_eq!(calls.get(), 3);
        assert_eq!(error.evidence.unwrap()["attempts"], 3);
    }
    #[test]
    fn non_stale_errors_are_not_retried() {
        let calls = std::cell::Cell::new(0);
        let error = with_transient_retry(|| {
            calls.set(calls.get() + 1);
            Err::<(), _>(CtlError::new(ErrorCode::PermDenied, "denied"))
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermDenied);
        assert_eq!(calls.get(), 1);
        assert!(error.evidence.is_none());
    }
}
