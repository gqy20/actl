//! locate —— 定位链(L1 精确 → L2 名称模糊 → L3 序数/锚点 → L4 虚拟化容器),
//! 以及基于定位的元素等待(wait/wait --gone)。候选 ref 编号与 snapshot 同源。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use crate::window::{MAX_DEPTH, ensure_window_responsive, find_window, top_level_of_focused};
use crate::{internal, timing};

/// 定位结果:命中元素 + 所在窗口。
pub struct Located {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    /// 定位链实际命中级别:"primary" | "fuzzy-name" | "role-ordinal" |
    /// "anchor" | "ref-replay" | "item-container"——agent 据此判断置信度并决定是否加验证
    pub resolved_by: &'static str,
    pub(crate) element: UIElement,
}

/// 在目标窗口内按 Target 定位元素(遍历可提前退出,优于全量 capture)。
/// `Ref(n)` 数不到 = 树已变化 → STALE_REF;语义选择器无命中 → NOT_FOUND(06 §4)。
pub fn locate(app: Option<&str>, target: &Target, near: Option<&str>) -> Result<Located, CtlError> {
    // xy: 坐标目标无元素可解析——物理专用通道(pointer_at_physical),
    // 误入元素定位路径 = 协议错误(所有元素类命令对 xy 的统一拒绝)
    if let Target::Coord(..) = target {
        return Err(CtlError::protocol(format!(
            "{} cannot be resolved to an element; coordinate targets are physical-only (click xy:x,y --app <win> --physical)",
            target.describe()
        )));
    }
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let root = {
        let _t = actl_core::trace::scope("locate.window");
        match app {
            Some(pattern) => find_window(&auto, &walker, pattern)?,
            None => {
                // fail-closed:@eN 的编号只对快照来源窗口的遍历顺序有意义;对着
                // "当前恰好聚焦的窗口"重放,前台一变就会静默点进别的应用(已实际发生)。
                if matches!(target, Target::Ref(_)) {
                    return Err(CtlError::protocol(format!(
                        "{} requires --app: refs are scoped to the window the snapshot came from; \
                         pass --app <title-substr> or re-snapshot",
                        target.describe()
                    )));
                }
                top_level_of_focused(&auto, &walker)?
            }
        }
    };
    let window_title = root.get_name().unwrap_or_default();
    // 激活感知:挂起目标先唤醒(有界),避免 DFS 里逐调用 stall
    let _wake_ms = {
        let _t = actl_core::trace::scope("locate.wake");
        ensure_window_responsive(&root)
    };

    // @eN:计数重放,不参与 fallback(ref 漂移 = STALE_REF,模糊化反而危险)
    if let Target::Ref(n) = target {
        let mut counter = 0u32;
        let _t = actl_core::trace::scope("locate.ref-replay");
        return match collect_until_ref(&walker, &root, 0, *n, &mut counter) {
            Some(hit) => Ok(located_from(hit, window_title, "ref-replay")),
            None => Err(CtlError::new(
                ErrorCode::StaleRef,
                format!(
                    "{} does not resolve — the tree changed since the snapshot",
                    target.describe()
                ),
            )),
        };
    }

    // L1 精确:全量收集(ref 编号与 snapshot 同源,候选可直接重放)
    // id: 快路径:provider 侧条件匹配(TreeScope::Subtree,单次 COM 往返替代
    // 逐元素三连读)——AutomationId 语义 = 精确匹配,与属性条件语义一致;
    // name: 的子串语义 provider 做不了,不走快路径。
    let mut counter = 0u32;
    let mut matches = Vec::new();
    if let Target::Id(_) = target {
        let hits = {
            let _t = actl_core::trace::scope("locate.fastpath");
            fast_exact_ids(&auto, &root, target)
        };
        match hits {
            Some(hits) if hits.len() == 1 => {
                return Ok(located_from(hits[0].clone(), window_title, "primary"));
            }
            Some(hits) if hits.is_empty() => {
                // L1 确定空手(id: 无 fuzzy 层),直落 L4 虚拟化容器
                let _t = actl_core::trace::scope("locate.l4");
                if let Some(hit) = locate_virtualized(&walker, &root, target) {
                    return Ok(located_from(hit, window_title, "item-container"));
                }
                return Err(CtlError::new(
                    ErrorCode::NotFound,
                    format!(
                        "no element matches {} (exact and virtualized-container levels)",
                        target.describe()
                    ),
                ));
            }
            _ => {} // 多命中(候选需 ref 编号)或快路径不可用 → 全遍历
        }
    }
    {
        let _t = actl_core::trace::scope("locate.dfs");
        collect_matches(&walker, &root, 0, target, false, &mut counter, &mut matches);
    }

    // L2 名称模糊(仅 name: 目标且 L1 空手时):大小写/空白不敏感
    if matches.is_empty() {
        if let Target::Name(_) = target {
            let mut fuzzy = Vec::new();
            collect_matches(&walker, &root, 0, target, true, &mut counter, &mut fuzzy);
            if !fuzzy.is_empty() {
                fuzzy.sort_by_key(|c: &Candidate| c.ref_no);
                return select(
                    fuzzy,
                    target,
                    near,
                    &walker,
                    &root,
                    window_title,
                    "fuzzy-name",
                );
            }
        }
        // L4 虚拟化容器(spike 2026-09-26 证真:Explorer 文件列表的 ListItem 不物化,
        // walker 遍历恒 0 命中;ItemContainerPattern::FindItemByProperty 精确命中)
        let l4 = {
            let _t = actl_core::trace::scope("locate.l4");
            locate_virtualized(&walker, &root, target)
        };
        if let Some(hit) = l4 {
            return Ok(located_from(hit, window_title, "item-container"));
        }
        return Err(CtlError::new(
            ErrorCode::NotFound,
            format!(
                "no element matches {} (exact, fuzzy and virtualized-container levels)",
                target.describe()
            ),
        ));
    }
    matches.sort_by_key(|c: &Candidate| c.ref_no);
    select(
        matches,
        target,
        near,
        &walker,
        &root,
        window_title,
        "primary",
    )
}

/// 分级裁决:唯一命中 → 命中;多命中且有锚点 → 最近者;多命中 → AMBIGUOUS
/// 附结构化候选(ref/role/name,docs/12 §4 协议评审项);RoleAt 越界 → NOT_FOUND。
fn select(
    matches: Vec<Candidate>,
    target: &Target,
    near: Option<&str>,
    walker: &UITreeWalker,
    root: &UIElement,
    window_title: String,
    level: &'static str,
) -> Result<Located, CtlError> {
    // L3 序数直选
    if let Target::RoleAt(_, n) | Target::IdAt(_, n) = target {
        return match matches.get((*n as usize).saturating_sub(1)) {
            Some(c) => Ok(located_from(
                c.element.clone(),
                window_title,
                if matches!(target, Target::IdAt(..)) {
                    "id-ordinal"
                } else {
                    "role-ordinal"
                },
            )),
            None => Err(CtlError::new(
                ErrorCode::NotFound,
                format!(
                    "{}: only {} match(es) in the window",
                    target.describe(),
                    matches.len()
                ),
            )),
        };
    }
    if matches.len() == 1 {
        return Ok(located_from(
            matches[0].element.clone(),
            window_title,
            level,
        ));
    }
    // 多命中 + 锚点:取与锚点矩形中心距最近者
    if let Some(anchor_pat) = near {
        let mut counter = 0u32;
        let mut anchors = Vec::new();
        // 锚点接受完整选择器语法;裸串按 name: 子串兜底
        let anchor_target =
            actl_core::parse_target(anchor_pat).unwrap_or(Target::Name(anchor_pat.to_string()));
        collect_matches(
            walker,
            root,
            0,
            &anchor_target,
            false,
            &mut counter,
            &mut anchors,
        );
        if anchors.len() != 1 {
            return Err(CtlError::new(
                ErrorCode::NotFound,
                format!(
                    "--near anchor {anchor_pat:?} must match exactly one element, got {}",
                    anchors.len()
                ),
            ));
        }
        if let Ok(ar) = anchors[0].element.get_bounding_rectangle() {
            let (ax, ay) = center(&ar);
            let mut best: Option<(f64, usize)> = None;
            for (i, c) in matches.iter().enumerate() {
                if let Ok(r) = c.element.get_bounding_rectangle() {
                    let (cx, cy) = center(&r);
                    let dx = (cx - ax) as f64;
                    let dy = (cy - ay) as f64;
                    let d = dx * dx + dy * dy;
                    if best.map(|(bd, _)| d < bd).unwrap_or(true) {
                        best = Some((d, i));
                    }
                }
            }
            if let Some((_, i)) = best {
                return Ok(located_from(
                    matches[i].element.clone(),
                    window_title,
                    "anchor",
                ));
            }
        }
    }
    // AMBIGUOUS + 结构化候选(每个含可重放的 ref)
    let evidence: Vec<serde_json::Value> = matches
        .iter()
        .take(8)
        .map(|c| {
            serde_json::json!({
                "ref": format!("@e{}", c.ref_no),
                "role": c.role,
                "name": c.name,
                "automation_id": c.automation_id,
            })
        })
        .collect();
    Err(CtlError::with_evidence(
        ErrorCode::Ambiguous,
        format!(
            "{} matches {} elements in the window; pick one by @eN or role ordinal              (--near anchor also applies)",
            target.describe(),
            matches.len()
        ),
        serde_json::json!({ "candidates": evidence }),
    ))
}

fn center(r: &uiautomation::types::Rect) -> (i32, i32) {
    (
        (r.get_left() + r.get_right()) / 2,
        (r.get_top() + r.get_bottom()) / 2,
    )
}

fn located_from(elem: UIElement, window_title: String, resolved_by: &'static str) -> Located {
    Located {
        window_title,
        role: format!(
            "{:?}",
            elem.get_control_type().unwrap_or(ControlType::Custom)
        ),
        name: elem.get_name().ok().filter(|n| !n.is_empty()),
        automation_id: elem.get_automation_id().ok().filter(|a| !a.is_empty()),
        resolved_by,
        element: elem,
    }
}

/// 定位链候选(ref 与 snapshot 同源编号,可直接重放)
struct Candidate {
    element: UIElement,
    ref_no: u32,
    role: String,
    name: Option<String>,
    automation_id: Option<String>,
}

/// @eN 重放的提前退出收集(语义同旧 locate_walk 的 Ref 分支)
fn collect_until_ref(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    want: u32,
    counter: &mut u32,
) -> Option<UIElement> {
    if depth > MAX_DEPTH {
        return None;
    }
    let role = role_of(elem);
    if actl_core::is_interactive_role(&role) {
        *counter += 1;
        if *counter == want {
            return Some(elem.clone());
        }
    }
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        if let Some(hit) = collect_until_ref(walker, &c, depth + 1, want, counter) {
            return Some(hit);
        }
        child = walker.get_next_sibling(&c).ok();
    }
    None
}

/// 全量收集语义匹配(不做提前退出——AMBIGUOUS 判定与候选上报需要全集)。
/// fuzzy=true 时 name 走 L2 模糊匹配(core::fuzzy_contains)。
/// 性能:属性惰性读——非命中元素只读定位所需属性(每元素 2 次 COM 往返,
/// 替代旧的无差别三连读;命中元素读全量供候选上报)。
fn collect_matches(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    target: &Target,
    fuzzy: bool,
    counter: &mut u32,
    out: &mut Vec<Candidate>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let role = role_of(elem);
    let ref_no = if actl_core::is_interactive_role(&role) {
        *counter += 1;
        *counter
    } else {
        0 // 非交互匹配仍可作候选上报,但不可 ref 重放
    };
    // 惰性属性读:按目标类型先读匹配所需的那一个属性
    let (name, id) = match target {
        Target::Name(s) => {
            let name = elem.get_name().ok().filter(|n| !n.is_empty());
            let hit = match (&name, fuzzy) {
                (Some(n), true) => actl_core::target::fuzzy_contains(n, s),
                (Some(n), false) => n.contains(s.as_str()),
                (None, _) => false,
            };
            if hit {
                (
                    name,
                    elem.get_automation_id().ok().filter(|a| !a.is_empty()),
                )
            } else {
                return dfs_children(walker, elem, depth, target, fuzzy, counter, out);
            }
        }
        Target::Id(s) | Target::IdAt(s, _) => {
            let id = elem.get_automation_id().ok().filter(|a| !a.is_empty());
            if id.as_deref() != Some(s.as_str()) {
                return dfs_children(walker, elem, depth, target, fuzzy, counter, out);
            }
            (elem.get_name().ok().filter(|n| !n.is_empty()), id)
        }
        Target::Role(s) | Target::RoleAt(s, _) => {
            if role != *s {
                return dfs_children(walker, elem, depth, target, fuzzy, counter, out);
            }
            (
                elem.get_name().ok().filter(|n| !n.is_empty()),
                elem.get_automation_id().ok().filter(|a| !a.is_empty()),
            )
        }
        Target::Ref(_) | Target::Coord(..) => (None, None),
    };
    let hit = match target {
        Target::Ref(_) | Target::Coord(..) => false,
        Target::Name(s) => match (&name, fuzzy) {
            (Some(n), true) => actl_core::target::fuzzy_contains(n, s),
            (Some(n), false) => n.contains(s.as_str()),
            (None, _) => false,
        },
        Target::Id(s) | Target::IdAt(s, _) => id.as_deref() == Some(s.as_str()),
        Target::Role(s) | Target::RoleAt(s, _) => role == *s,
    };
    if hit {
        out.push(Candidate {
            element: elem.clone(),
            ref_no,
            role: role.clone(),
            name: name.clone(),
            automation_id: id.clone(),
        });
    }
    dfs_children(walker, elem, depth, target, fuzzy, counter, out);
}

fn dfs_children(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    target: &Target,
    fuzzy: bool,
    counter: &mut u32,
    out: &mut Vec<Candidate>,
) {
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        collect_matches(walker, &c, depth + 1, target, fuzzy, counter, out);
        child = walker.get_next_sibling(&c).ok();
    }
}

pub(crate) fn role_of(elem: &UIElement) -> String {
    format!(
        "{:?}",
        elem.get_control_type().unwrap_or(ControlType::Custom)
    )
}

/// id: 目标的 provider 侧精确匹配(UIA FindAll + AutomationId 属性条件,
/// TreeScope::Subtree 与 DFS 全遍历同覆盖含根自身)。
/// None = 条件创建/查询失败,调用方回退全遍历(正确性优先)。
fn fast_exact_ids(
    auto: &UIAutomation,
    root: &UIElement,
    target: &Target,
) -> Option<Vec<UIElement>> {
    use uiautomation::types::{TreeScope, UIProperty};
    use uiautomation::variants::Variant;

    let Target::Id(s) = target else { return None };
    let cond = auto
        .create_property_condition(UIProperty::AutomationId, Variant::from(s.clone()), None)
        .ok()?;
    root.find_all(TreeScope::Subtree, &cond).ok()
}

// ─── L4 虚拟化容器兜底(M2 观察层强化,spike 2026-09-26 证真) ───────────────
// Explorer/任务管理器等大列表的 ListItem 不物化(US1 实测:等 6s + F5 后 walker
// 遍历仍 0 命中),ItemContainerPattern::FindItemByProperty 由 provider 侧
// 直接命中未物化条目(61 文件目录实测:树内 0 命中 → 精确查询命中末位文件)。

/// L4 支持的属性查询映射:Name → UIA Name 精确;Id → AutomationId 精确。
/// Role/Ref 无属性查询语义(Name/AutomationId 之外的属性 provider 普遍不支持),
/// 返回 None(定位链保持原 NotFound/STALE_REF 语义)。
fn container_query_for(
    target: &Target,
) -> Option<(windows::Win32::UI::Accessibility::UIA_PROPERTY_ID, String)> {
    use windows::Win32::UI::Accessibility::{UIA_AutomationIdPropertyId, UIA_NamePropertyId};
    match target {
        Target::Name(s) => Some((UIA_NamePropertyId, s.clone())),
        Target::Id(s) => Some((UIA_AutomationIdPropertyId, s.clone())),
        _ => None,
    }
}

/// L4 执行体:窗口内收集 ItemContainer 容器,逐个精确查询。
/// 仅在 L1/L2 双落空后进入(不在热路径);无命中返回 None → NotFound。
/// FindItemByProperty 为**精确匹配**(provider 侧无子串语义)——name: 的
/// 子串语义在 L1/L2 已消耗,进入 L4 的 name 值必须是完整名称(如完整文件名)。
///
/// unsafe 理由(AGENTS §5,crate 豁免点):crate 安全包装不接受 NULL 起查元素
/// (MS 文档语义 = "从头查",且零值 UIElement 的 Drop 会 Release(null) 崩溃),
/// 经 `UIElement::as_ref()` 取原生 IUIAutomationElement 直调 GetCurrentPattern
/// + FindItemByProperty(None, ...);类型与 crate 同源(均 windows 0.62)。
fn locate_virtualized(
    walker: &UITreeWalker,
    root: &UIElement,
    target: &Target,
) -> Option<UIElement> {
    use uiautomation::variants::Variant;
    use windows::Win32::UI::Accessibility::{
        IUIAutomationItemContainerPattern, UIA_ItemContainerPatternId,
    };

    let (prop, value) = container_query_for(target)?;
    let mut containers = Vec::new();
    collect_containers(walker, root, 0, &mut containers);
    let value = Variant::from(value);
    for c in &containers {
        let pattern: Option<IUIAutomationItemContainerPattern> = (|| unsafe {
            use windows::core::Interface;
            let raw = c.as_ref();
            raw.GetCurrentPattern(UIA_ItemContainerPatternId)
                .ok()?
                .cast()
                .ok()
        })();
        let Some(pattern) = pattern else { continue };
        // 无命中:provider 返回 null 元素(crate 包装为 Err)→ 试下一个容器
        if let Ok(elem) = unsafe { pattern.FindItemByProperty(None, prop, value.as_ref()) } {
            return Some(UIElement::from(elem));
        }
    }
    None
}

/// DFS 收集实现 ItemContainerPattern 的元素(容器数上限防失控 UI)。
fn collect_containers(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    out: &mut Vec<UIElement>,
) {
    use uiautomation::patterns::UIItemContainerPattern;
    if depth > MAX_DEPTH || out.len() >= 32 {
        return;
    }
    if elem.get_pattern::<UIItemContainerPattern>().is_ok() {
        out.push(elem.clone());
    }
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        collect_containers(walker, &c, depth + 1, out);
        child = walker.get_next_sibling(&c).ok();
    }
}

/// wait 族轮询的定位语义:普通失败(NOT_FOUND 等)= 继续等(元素可能晚到);
/// **AMBIGUOUS = 确定性多命中,等待不可能收敛 → 立即失败**(候选 evidence
/// 已随错误携带,无需等满超时;2026-09-27 抖音多文档页实测:samples 冻结
/// 在 1 的误导性 TIMEOUT 根因)。
fn poll(app: Option<&str>, target: &Target) -> Result<Option<Located>, CtlError> {
    match locate(app, target, None) {
        Ok(loc) => Ok(Some(loc)),
        Err(e) if e.code == ErrorCode::Ambiguous => Err(e),
        Err(_) => Ok(None),
    }
}

/// wait --element:轮询定位直到目标可解析(TIMEOUT 语义;与 --expect 的
/// "立即断言"分工见 06 §3.6"耐心等待")。
pub fn wait_element(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if poll(app, target)?.is_some() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::Timeout,
                format!(
                    "element {} did not resolve within {timeout_ms}ms",
                    target.describe()
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

/// wait --gone:轮询直到目标**不再**可解析(消失;TIMEOUT 语义)。
pub fn wait_gone(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if poll(app, target)?.is_none() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::Timeout,
                format!(
                    "{} still resolves after {timeout_ms}ms (expected it to disappear)",
                    target.describe()
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

#[cfg(test)]
mod tests {
    use super::container_query_for;
    use actl_core::target::Target;
    use windows::Win32::UI::Accessibility::{UIA_AutomationIdPropertyId, UIA_NamePropertyId};

    #[test]
    fn l4_maps_name_and_id_to_exact_property_queries() {
        let (prop, v) = container_query_for(&Target::Name("report.xlsx".into())).expect("name");
        assert_eq!(prop, UIA_NamePropertyId);
        assert_eq!(v, "report.xlsx");

        let (prop, v) = container_query_for(&Target::Id("SystemListItem".into())).expect("id");
        assert_eq!(prop, UIA_AutomationIdPropertyId);
        assert_eq!(v, "SystemListItem");
    }

    #[test]
    fn l4_excludes_role_and_ref_targets() {
        assert!(container_query_for(&Target::Role("ListItem".into())).is_none());
        assert!(container_query_for(&Target::RoleAt("ListItem".into(), 2)).is_none());
        assert!(container_query_for(&Target::Ref(7)).is_none());
    }
}

/// wait --property:轮询直到元素可解析且属性满足期望(06 §3.4 完整谓词语法)。
/// 谓词:`value=<expected>`(Value/TextPattern 双通道)| `name=<substr>` | `checked=on|off`;
/// contains=true 时 value/name 用子串语义。元素尚未可解析视为"未满足"继续等
/// (与 --element 的分工:property 隐含存在性)。
pub fn wait_property(
    app: Option<&str>,
    target: &Target,
    predicate: &WaitPredicate,
    timeout_ms: u64,
) -> Result<String, CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut last: Option<String> = None;
    loop {
        if let Some(loc) = poll(app, target)? {
            let actual = predicate.read(&loc.element);
            if predicate.satisfied(actual.as_deref()) {
                return Ok(actual.unwrap_or_default());
            }
            last = actual;
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::with_evidence(
                ErrorCode::Timeout,
                format!(
                    "{} did not satisfy {} within {timeout_ms}ms",
                    target.describe(),
                    predicate.describe()
                ),
                serde_json::json!({ "last_actual": last }),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

/// wait --stable 的结果(settle 证据:采样数/变化数/末次样本长度)。
pub struct StableOutcome {
    pub samples: usize,
    pub changes: usize,
    pub last_len: usize,
}

/// wait --stable:settle 谓词——轮询直到元素可解析且观测样本连续两次不变
/// (采样间隔 settle_ms,默认 250ms → 稳定窗 500ms)。样本通道:text 优先、
/// value 次之、name 兜底(Chromium Document 的 Value=URL 对内容 settle 无
/// 意义,正文只能走 TextPattern,与 get 的 text/value 分工一致)。元素消失
/// 期间稳定计数清零(隐含存在性);超时 TIMEOUT 附 samples/changes evidence。
pub fn wait_stable(
    app: Option<&str>,
    target: &Target,
    timeout_ms: u64,
) -> Result<StableOutcome, CtlError> {
    const STABLE_SAMPLES: usize = 2;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut last: Option<String> = None;
    let mut stable = 0usize;
    let mut samples = 0usize;
    let mut changes = 0usize;
    loop {
        if let Some(loc) = poll(app, target)? {
            let sample = stability_sample(&loc.element);
            samples += 1;
            stable = match &last {
                Some(prev) if prev == &sample => stable + 1,
                Some(_) => {
                    changes += 1;
                    1
                }
                None => 1,
            };
            last = Some(sample);
            if stable >= STABLE_SAMPLES {
                return Ok(StableOutcome {
                    samples,
                    changes,
                    last_len: last.as_deref().map(str::len).unwrap_or(0),
                });
            }
        } else {
            stable = 0;
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::with_evidence(
                ErrorCode::Timeout,
                format!(
                    "{} did not settle within {timeout_ms}ms (content still changing or element absent)",
                    target.describe()
                ),
                serde_json::json!({
                    "samples": samples,
                    "changes": changes,
                    "last_len": last.as_deref().map(str::len),
                }),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().settle_ms));
    }
}

/// 稳定性采样:text > value > name(富文本正文只有 TextPattern 可读;
/// value 通道兜底纯控件,name 最后)。
fn stability_sample(elem: &UIElement) -> String {
    use uiautomation::patterns::UITextPattern;
    if let Some(t) = elem
        .get_pattern::<UITextPattern>()
        .ok()
        .and_then(|t| t.get_document_range().ok())
        .and_then(|r| r.get_text(-1).ok())
    {
        return t;
    }
    crate::read::read_value(elem).unwrap_or_else(|| elem.get_name().unwrap_or_default())
}

/// wait/verify 共用的属性谓词(解析与判定分离,便于单测)。
pub struct WaitPredicate {
    pub prop: WaitProp,
    pub expected: String,
    pub contains: bool,
}

pub enum WaitProp {
    Value,
    Name,
    Checked,
}

impl WaitPredicate {
    /// 解析 "prop=expected" 形态(prop ∈ value|name|checked;checked 期望 on/off)
    pub fn parse(raw: &str) -> Result<Self, CtlError> {
        let Some((prop, expected)) = raw.split_once('=') else {
            return Err(CtlError::protocol(format!(
                "invalid predicate {raw:?}: expected <prop>=<expected> with prop in value|name|checked"
            )));
        };
        let prop = match prop.trim() {
            "value" => WaitProp::Value,
            "name" => WaitProp::Name,
            "checked" => WaitProp::Checked,
            other => {
                return Err(CtlError::protocol(format!(
                    "unknown predicate property {other:?}: expected value|name|checked"
                )));
            }
        };
        let expected = expected.trim().to_string();
        if expected.is_empty() {
            return Err(CtlError::protocol(format!(
                "predicate {raw:?} has an empty expectation"
            )));
        }
        if matches!(prop, WaitProp::Checked) && !matches!(expected.as_str(), "on" | "off") {
            return Err(CtlError::protocol(
                "checked predicate expects on|off (e.g. checked=on)",
            ));
        }
        Ok(Self {
            prop,
            expected,
            contains: false,
        })
    }

    pub fn describe(&self) -> String {
        let p = match self.prop {
            WaitProp::Value => "value",
            WaitProp::Name => "name",
            WaitProp::Checked => "checked",
        };
        format!("{p}={}", self.expected)
    }

    /// 读元素当前值(Checked 读 Toggle 状态,Value 走双通道,Name 读 UIA Name)
    pub fn read(&self, elem: &uiautomation::UIElement) -> Option<String> {
        match self.prop {
            WaitProp::Value => crate::read::read_value(elem),
            WaitProp::Name => elem.get_name().ok().filter(|n| !n.is_empty()),
            WaitProp::Checked => elem
                .get_pattern::<uiautomation::patterns::UITogglePattern>()
                .ok()
                .and_then(|t| t.get_toggle_state().ok())
                .map(|s| if s.to_string() == "On" { "on" } else { "off" }.to_string()),
        }
    }

    /// 判定(读不到值 = 未满足,fail-closed)
    pub fn satisfied(&self, actual: Option<&str>) -> bool {
        let Some(a) = actual else { return false };
        match (&self.prop, self.contains) {
            (WaitProp::Checked, _) | (WaitProp::Value, false) | (WaitProp::Name, false) => {
                a == self.expected
            }
            (WaitProp::Value, true) | (WaitProp::Name, true) => a.contains(&self.expected),
        }
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;
    use actl_core::CtlError;

    #[test]
    fn predicate_parses_and_rejects() {
        let p = WaitPredicate::parse("value=OK").unwrap();
        assert!(p.satisfied(Some("OK")));
        assert!(!p.satisfied(Some("ok"))); // exact by default
        let p = WaitPredicate::parse("checked=on").unwrap();
        assert!(p.satisfied(Some("on")));
        assert!(WaitPredicate::parse("checked=maybe").is_err());
        assert!(WaitPredicate::parse("bogus=1").is_err());
        assert!(WaitPredicate::parse("noequalsign").is_err());
        assert!(WaitPredicate::parse("value=").is_err());
    }

    #[test]
    fn contains_semantics_apply_to_value_and_name_only() {
        let mut p = WaitPredicate::parse("value=56,877").unwrap();
        p.contains = true;
        assert!(p.satisfied(Some("显示为 56,877")));
        let mut p = WaitPredicate::parse("name=保存").unwrap();
        p.contains = true;
        assert!(p.satisfied(Some("另存为 - 保存")));
    }

    #[test]
    fn missing_actual_is_unsatisfied() {
        let p = WaitPredicate::parse("name=x").unwrap();
        assert!(!p.satisfied(None));
        let _ = CtlError::protocol(""); // silence unused import in tests
    }
}
