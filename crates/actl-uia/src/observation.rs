//! Bounded raw-tree observation. Missing children and failed enumeration are distinct.
use crate::observation_diagnostics::{Stage, observe};
use actl_core::{CtlError, ErrorCode};
use uiautomation::{
    UIAutomation, UIElement,
    core::{UICacheRequest, UICondition},
    types::{TreeScope, UIProperty},
};

pub(crate) fn cached_hwnd(element: &UIElement) -> Option<isize> {
    let handle: windows::Win32::Foundation::HWND =
        element.get_cached_native_window_handle().ok()?.into();
    (!handle.0.is_null()).then_some(handle.0 as isize)
}

/// 缓存优先属性读(见 tree_bounded 属性预取):walk 产物元素的三连读变零
/// 往返;缓存未命中(元素来自其他调用路径,无预取)回退实时读。
pub(crate) fn role_of(elem: &UIElement) -> Result<uiautomation::types::ControlType, CtlError> {
    elem.get_cached_control_type()
        .or_else(|_| elem.get_control_type())
        .map_err(|e| crate::read_channel::failure("control_type", e))
}

pub(crate) fn name_of(elem: &UIElement) -> Result<String, CtlError> {
    elem.get_cached_name()
        .or_else(|_| elem.get_name())
        .map_err(|e| crate::read_channel::failure("name", e))
}

pub(crate) fn automation_id_of(elem: &UIElement) -> Result<String, CtlError> {
    elem.get_cached_automation_id()
        .or_else(|_| elem.get_automation_id())
        .map_err(|e| crate::read_channel::failure("automation_id", e))
}

pub(crate) struct Entry<T> {
    pub element: T,
    pub depth: u32,
    pub parent: Option<usize>,
}

/// 候选元素的"上下文行":沿索引父链上溯,最近**具名**祖先的 name。
/// AMBIGUOUS 证据消歧用(重复标签靠"所在行/容器"文本区分,docs/06 §4):
/// 遍历根(索引 0,窗口)豁免——标题对调用方已知,无消歧价值;上溯有界
/// 8 跳;超长截断 120 字符加省略号。名字读走缓存闭包(walk 产物零往返)。
pub(crate) fn nearest_named_ancestor<T>(
    entries: &[Entry<T>],
    index: usize,
    name: impl Fn(&T) -> Result<String, CtlError>,
) -> Result<Option<String>, CtlError> {
    const MAX_HOPS: usize = 8;
    const MAX_CHARS: usize = 120;
    let mut hops = 0usize;
    let mut idx = index;
    while let Some(parent) = entries[idx].parent {
        if parent == 0 || hops >= MAX_HOPS {
            break;
        }
        let ancestor_name = name(&entries[parent].element)?;
        if !ancestor_name.trim().is_empty() {
            return Ok(Some(truncate_chars(&ancestor_name, MAX_CHARS)));
        }
        idx = parent;
        hops += 1;
    }
    Ok(None)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(max)
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    format!("{}…", &text[..cut])
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
        // 进程看门狗心跳:前沿每弹出一个节点 = 进展——全树扫描合法地可达
        // 分钟级(网易云全树 126s 实证),无心跳会被默认期限误杀;单点卡死
        // (children 调用挂起)则心跳停止,照常触顶。
        actl_core::watchdog::reset();
        // UIA 区段重臂:同上——整段 wall clock 口径会误杀合法慢扫描
        // (冷 provider 实证三度 20s 中止);重臂后口径 = 单次跨进程调用。
        crate::watchdog::rearm();
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

/// 观察树(默认物化):纯观察消费——snapshot/find 的输出序列化后元素即弃,
/// 物化安全且拿满提速(N→1 往返)。
pub(crate) fn tree(root: &UIElement) -> Result<Observed<UIElement>, CtlError> {
    tree_bounded(root, crate::window::MAX_DEPTH, crate::window::MAX_ELEMENTS)
}

/// 动作链树(恒逐节点 live):元素将被携带出函数做 pattern 调用与身份核验
/// (locate 链的 click/type/focus/set-value、extract 的 Grid 读取)——
/// GetCachedChildren 的缓存视图存根对 live 调用在部分 provider(Word 的
/// MSAA 桥,2026-10-06 实证:focus NOT_ACTIONABLE/身份读空,legacy 即恢复)
/// 上失败,动作链一律 live 遍历(消费分轨,docs/06)。
pub(crate) fn tree_live(root: &UIElement) -> Result<Observed<UIElement>, CtlError> {
    tree_bounded_mode(
        root,
        crate::window::MAX_DEPTH,
        crate::window::MAX_ELEMENTS,
        true,
    )
}

pub(crate) fn tree_bounded(
    root: &UIElement,
    depth: u32,
    limit: usize,
) -> Result<Observed<UIElement>, CtlError> {
    tree_bounded_mode(root, depth, limit, false)
}

/// `live` = 动作链(逐节点);false = 观察(物化,ACTL_WALK_LEGACY 可全局
/// 强制 legacy——对照组/逃生舱)。
fn tree_bounded_mode(
    root: &UIElement,
    depth: u32,
    limit: usize,
    live: bool,
) -> Result<Observed<UIElement>, CtlError> {
    // 看门狗区段:树观察遍历 provider 全树,是 COM 挂起的高发面(docs/81 §8)。
    // 2026-10-07 子树物化(方案2,屏幕阅读器同款技术):缓存请求 TreeScope=
    // Subtree——一次 build_updated_cache 把整棵子树+属性+父子链接拉进本地,
    // collect 的 children 闭包改走 get_cached_children()(纯本地,零跨进程
    // 往返)——N 次往返变 1 次。collect 循环原封不动(上限/顺序/refs 编号
    // 语义完全保留)。物化是单次长调用,UIA 区段 20s 限即它的界(巨树/冷
    // Office 可 ACTL_UIA_WATCHDOG_MS 放宽,010 实测 60s 有效)。
    let _watchdog = crate::watchdog::enter();
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let legacy = live || std::env::var_os("ACTL_WALK_LEGACY").is_some_and(|v| v != "0");
    // 观察分支:物化默认,provider 拒答/结构损坏时自动回落逐节点(观察只读,
    // 回落代价只是一次慢遍历——Word 的 MSAA 桥在大树/兼容模式上报
    // 「Malformed Cacheresponse pTreeStructure」类缓存结构损坏,2026-10-06
    // 实证;legacy 的错误如实上抛,不静默吞)。
    if legacy {
        let cache = make_cache(&auto, TreeScope::Element)?;
        collect_per_node(root, depth, limit, &cache)
    } else {
        let cache = make_cache(&auto, TreeScope::Subtree)?;
        match root
            .build_updated_cache(&cache)
            .map_err(crate::internal)
            .and_then(|materialized| {
                collect(materialized, depth, limit, |element| {
                    Ok(element.get_cached_children().unwrap_or_default())
                })
            }) {
            Ok(observed) => Ok(observed),
            Err(error) => {
                eprintln!("· 物化被 provider 拒答({error:?}),回落逐节点遍历");
                let cache = make_cache(&auto, TreeScope::Element)?;
                collect_per_node(root, depth, limit, &cache)
            }
        }
    }
}

/// 构建带属性预取的缓存请求(scope 决定观察物化形态或逐节点属性)。
fn make_cache(auto: &UIAutomation, scope: TreeScope) -> Result<UICacheRequest, CtlError> {
    let cache = auto.create_cache_request().map_err(crate::internal)?;
    for property in [
        UIProperty::Name,
        UIProperty::ControlType,
        UIProperty::AutomationId,
        UIProperty::ProcessId,
    ] {
        cache.add_property(property).map_err(crate::internal)?;
    }
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    cache.set_tree_filter(condition).map_err(crate::internal)?;
    cache.set_tree_scope(scope).map_err(crate::internal)?;
    Ok(cache)
}

/// 逐节点 RPC 遍历(动作链默认/原实现,属性预取保留)。
fn collect_per_node(
    root: &UIElement,
    depth: u32,
    limit: usize,
    cache: &UICacheRequest,
) -> Result<Observed<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let condition = auto.create_true_condition().map_err(crate::internal)?;
    collect(root.clone(), depth, limit, |element| {
        crate::observation_query::children(
            element,
            &condition,
            cache,
            crate::observation_query::Scope::ElementChildren,
        )
    })
}

pub(crate) fn children(root: &UIElement) -> Result<Vec<UIElement>, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    ChildrenQuery::new(&auto, &[UIProperty::NativeWindowHandle])?.children(root)
}

/// One condition/cache request per observation, with no cross-call element reuse.
pub(crate) struct ChildrenQuery {
    condition: UICondition,
    cache: UICacheRequest,
}
impl ChildrenQuery {
    pub fn new(auto: &UIAutomation, properties: &[UIProperty]) -> Result<Self, CtlError> {
        let condition = auto.create_true_condition().map_err(crate::internal)?;
        let cache = auto.create_cache_request().map_err(crate::internal)?;
        for property in properties {
            cache.add_property(*property).map_err(crate::internal)?;
        }
        cache
            .set_tree_filter(condition.clone())
            .map_err(crate::internal)?;
        cache
            .set_tree_scope(TreeScope::Element)
            .map_err(crate::internal)?;
        Ok(Self { condition, cache })
    }
    pub fn children(&self, root: &UIElement) -> Result<Vec<UIElement>, CtlError> {
        crate::observation_query::children(
            root,
            &self.condition,
            &self.cache,
            crate::observation_query::Scope::ElementChildren,
        )
    }
    pub fn desktop_children(&self, auto: &UIAutomation) -> Result<Vec<UIElement>, CtlError> {
        let _t = actl_core::trace::scope("window.desktop_children");
        with_transient_retry(|| {
            let root = observe(Stage::DesktopRoot, None, || {
                auto.get_root_element()
                    .map_err(|e| crate::read_channel::failure("desktop.root", e))
            })?;
            crate::observation_query::children(
                &root,
                &self.condition,
                &self.cache,
                crate::observation_query::Scope::DesktopChildren,
            )
        })
    }
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
    ChildrenQuery::new(auto, &[UIProperty::NativeWindowHandle])?.desktop_children(auto)
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

    fn chain(names: &[&str]) -> Vec<Entry<usize>> {
        // 线性链:0=根(窗口),i 的父是 i-1;names[i] 即该索引的名字
        names
            .iter()
            .enumerate()
            .map(|(i, _)| Entry {
                element: i,
                depth: i as u32,
                parent: if i == 0 { None } else { Some(i - 1) },
            })
            .collect()
    }

    #[test]
    fn nearest_named_ancestor_picks_closest_named_skipping_unnamed() {
        // 根(名) - 空名 Pane - "List A" - 空名 Cell - 候选:最近具名 = List A
        let entries = chain(&["Window", "", "List A", "", ""]);
        let ctx = nearest_named_ancestor(&entries, 4, |i| {
            Ok(["Window", "", "List A", "", ""][*i].into())
        });
        assert_eq!(ctx.unwrap().as_deref(), Some("List A"));
    }

    #[test]
    fn nearest_named_ancestor_skips_root_even_when_only_named() {
        // 直接父就是根:窗口标题不作为上下文(调用方已知 --app)
        let entries = chain(&["Window", "", ""]);
        let ctx = nearest_named_ancestor(&entries, 2, |i| Ok(["Window", "", ""][*i].into()));
        assert_eq!(ctx.unwrap(), None);
    }

    #[test]
    fn nearest_named_ancestor_none_when_all_unnamed() {
        let entries = chain(&["Window", "", "", ""]);
        let ctx = nearest_named_ancestor(&entries, 3, |i| Ok(["Window", "", "", ""][*i].into()));
        assert_eq!(ctx.unwrap(), None);
    }

    #[test]
    fn nearest_named_ancestor_is_hop_bounded() {
        // 候选 idx10:祖先 idx9..2 是 8 跳内(全空名),idx1="Far" 在第 9 跳
        // ——不采(上下文取近不取远,超出即 None)
        let names = ["Root", "Far", "", "", "", "", "", "", "", "", ""];
        let entries = chain(&names);
        let ctx = nearest_named_ancestor(&entries, 10, |i| Ok(names[*i].to_string()));
        assert_eq!(ctx.unwrap(), None);
    }

    #[test]
    fn nearest_named_ancestor_truncates_long_names() {
        let long = "x".repeat(200);
        let names = vec!["Window", long.as_str(), ""];
        let entries = chain(&names);
        let ctx = nearest_named_ancestor(&entries, 2, |i| Ok(names[*i].to_string()));
        let ctx = ctx.unwrap().unwrap();
        assert_eq!(ctx.chars().count(), 121); // 120 + 省略号
        assert!(ctx.ends_with('…'));
    }

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
