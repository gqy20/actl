//! locate —— 定位链(L1 精确 → L2 名称模糊 → L3 序数/锚点 → L4 虚拟化容器),
//! 以及基于定位的元素等待(wait/wait --gone)。候选 ref 编号与 snapshot 同源。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};

use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use crate::window::{ensure_window_responsive, find_window, top_level_of_focused};
use crate::{internal, timing};

/// 定位结果:命中元素 + 所在窗口。
pub struct Located {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    /// 定位链实际命中级别:"primary" | "fuzzy-name" | "role-ordinal" |
    /// "anchor" | "ref-replay" | "item-container" | "focused" | "cache"——
    /// agent 据此判断置信度并决定是否加验证;"cache" = 进程内定位缓存命中
    /// (RuntimeId + 归属窗口复核通过,等价于 primary 语义)
    pub resolved_by: &'static str,
    pub(crate) element: UIElement,
}

/// 定位结果的元数据面。COM 元素(UIElement)不出本 crate 域:跨函数边界携带
/// 会让它的 Drop(跨进程 Release)裸奔在无看门狗区段——2026-10-06 真机实证
/// (get 在读取成功后挂死在析构,内核态不可杀)。调用方只消费这些字段。
pub struct ResolvedInfo {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub resolved_by: &'static str,
}

impl Located {
    /// 消费式转元数据:显式 drop COM 元素。**调用点必须已在看门狗区内**——
    /// Release 是跨进程调用,提供程序不泵消息时无限阻塞。
    pub(crate) fn into_info(self) -> ResolvedInfo {
        let Located {
            window_title,
            role,
            name,
            automation_id,
            resolved_by,
            element,
        } = self;
        drop(element);
        ResolvedInfo {
            window_title,
            role,
            name,
            automation_id,
            resolved_by,
        }
    }
}

/// 进程内定位缓存(docs/81 提速轮):同 (app,target) 的连续解析在键入序列中
/// 每次都重扫目标窗全树(Word 级巨型树实测 DFS 2-3s/次,是每键 4.4s 的大头)。
/// 缓存最近一次成功解析的元素;命中时以 RuntimeId + 归属根窗口双复核替代
/// 全树 DFS,失配/失效即回落原链路(fail-closed 语义不变)。
struct CachedResolve {
    key: String,
    element: UIElement,
    runtime_id: serde_json::Value,
}
// UIElement 持 COM 指针(!Send):CLI/batch/flow 的命令派发同线程,thread_local 即可
thread_local! {
    static RESOLVE_CACHE: std::cell::RefCell<Option<CachedResolve>> =
        const { std::cell::RefCell::new(None) };
}

fn cache_key(app: Option<&str>, target: &Target) -> String {
    format!("{}\u{1f}{}", app.unwrap_or(""), target.describe())
}

/// 元素沿 ControlView 向上走到根(有界 32 层,防御环/深树)。
fn root_element(walker: &UITreeWalker, element: &UIElement) -> UIElement {
    let mut current = element.clone();
    for _ in 0..32 {
        match walker.get_parent(&current) {
            Ok(parent) => current = parent,
            _ => break,
        }
    }
    current
}

fn cache_lookup(app: Option<&str>, key: &str) -> Option<Located> {
    let app = app?;
    let (element, cached_rid) = RESOLVE_CACHE.with(|cache| {
        let guard = cache.borrow();
        let cached = guard.as_ref()?;
        if cached.key != key {
            return None;
        }
        Some((cached.element.clone(), cached.runtime_id.clone()))
    })?;
    // 复核 1:元素仍活着且 RuntimeId 未变(元素重建 = 失效,回落全链路)
    let rid = element.get_runtime_id().ok()?;
    if rid.is_empty() || serde_json::json!(rid) != cached_rid {
        return None;
    }
    // 复核 2:app 锚解析到的当前窗口仍是该元素的归属根(窗口重建/换锚 = 失效)
    let auto = UIAutomation::new().ok()?;
    let walker = auto.create_tree_walker().ok()?;
    let window = find_window(&auto, &walker, app).ok()?;
    let root = root_element(&walker, &element);
    let window_rid = window.get_runtime_id().ok()?;
    let root_rid = root.get_runtime_id().ok()?;
    if window_rid.is_empty()
        || root_rid.is_empty()
        || serde_json::json!(window_rid) != serde_json::json!(root_rid)
    {
        return None;
    }
    let title = window.get_name().ok()?;
    located_from(element, title, "cache").ok()
}

fn cache_store(key: &str, located: &Located) {
    let rid = match located.element.get_runtime_id() {
        Ok(rid) if !rid.is_empty() => serde_json::json!(rid),
        _ => return,
    };
    RESOLVE_CACHE.with(|cache| {
        *cache.borrow_mut() = Some(CachedResolve {
            key: key.to_string(),
            element: located.element.clone(),
            runtime_id: rid,
        });
    });
}

/// 在目标窗口内按 Target 定位元素；遍历须完整，不能用截断结果证明唯一性。
/// `Ref(n)` 数不到 = 树已变化 → STALE_REF;语义选择器无命中 → NOT_FOUND(06 §4)。
pub fn locate(app: Option<&str>, target: &Target, near: Option<&str>) -> Result<Located, CtlError> {
    // 缓存仅覆盖静态语义选择器(name:/id:/role: 序数形态):@eN 编号、uia: 严格
    // 选择器、focused:(随焦点漂移)、--near 锚语义都走原链路
    let cacheable = near.is_none()
        && app.is_some()
        && matches!(target, Target::Name(_) | Target::Id(_) | Target::Role(..));
    if !cacheable {
        return locate_full(app, target, near);
    }
    let key = cache_key(app, target);
    if let Some(hit) = cache_lookup(app, &key) {
        return Ok(hit);
    }
    let located = locate_full(app, target, near)?;
    {
        // cache_store 的 RuntimeId 读取是跨进程 COM 调用,进看门狗区段
        // (locate_full 的区段已随其返回退出,这里是裸段)。
        let _guard = crate::watchdog::enter();
        cache_store(&key, &located);
    }
    Ok(located)
}

fn locate_full(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<Located, CtlError> {
    // 看门狗区段:定位链含全树 DFS(Word 级巨型树秒级)+ provider 直连,
    // 是无界 COM 挂起的高发面(docs/81 §8)
    let _watchdog = crate::watchdog::enter();
    if matches!(target, Target::Uia(_)) && near.is_some() {
        return Err(CtlError::protocol(
            "uia selectors do not support --near; use within and unique conditions",
        ));
    }
    // xy: 坐标目标无元素可解析——物理专用通道(pointer_at_physical),
    // 误入元素定位路径 = 协议错误(所有元素类命令对 xy 的统一拒绝)
    if let Target::Coord(..) = target {
        return Err(CtlError::protocol(format!(
            "{} cannot be resolved to an element; coordinate targets are physical-only (click xy:x,y --app <win> --physical)",
            target.describe()
        )));
    }
    let auto = {
        let _t = actl_core::trace::scope("locate.initialize");
        UIAutomation::new().map_err(internal)?
    };
    if let Target::Uia(selector) = target {
        let root = {
            let _t = actl_core::trace::scope("locate.uia.window");
            crate::scoped_locate::window(&auto, app)?
        };
        let _t = actl_core::trace::scope("locate.uia.selector");
        return crate::scoped_locate::locate(&auto, &root, selector);
    }
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
    let window_title = root
        .get_name()
        .map_err(|e| crate::read_channel::failure("window.name", e))?;
    // 激活感知:挂起目标先唤醒(有界),避免 DFS 里逐调用 stall
    let _wake_ms = {
        let _t = actl_core::trace::scope("locate.wake");
        ensure_window_responsive(&root)
    };

    // @eN:计数重放,不参与 fallback(ref 漂移 = STALE_REF,模糊化反而危险)
    if let Target::Ref(n) = target {
        if let Some(hit) = crate::identity::scoped_ref(&auto, &root, *n)? {
            crate::identity::check_ref(&root, &hit, *n)?;
            return located_from(hit, window_title, "scoped-ref-replay");
        }
        let mut counter = 0u32;
        let _t = actl_core::trace::scope("locate.ref-replay");
        return match collect_until_ref(&walker, &root, 0, *n, &mut counter)? {
            Some(hit) => {
                crate::identity::check_ref(&root, &hit, *n)?;
                located_from(hit, window_title, "ref-replay")
            }
            None => Err(CtlError::new(
                ErrorCode::StaleRef,
                format!(
                    "{} does not resolve — the tree changed since the snapshot",
                    target.describe()
                ),
            )),
        };
    }

    // focused:OS 键盘焦点元素——树对语义选择器不可见但焦点可达的应用
    // (Electron 懒树,uTools 实证)的唯一寻址通道。属主判据用物理焦点根
    // (GUITHREADINFO):UIA focused 视图在 WinUI 实测会谎报(kbd 文档);
    // 属主(给了 --app 时焦点根 HWND 必须即 app 窗)与角色校验 fail-closed。
    if let Target::Focused(role) = target {
        let _t = actl_core::trace::scope("locate.focused");
        let (focused_hwnd, focused_title) = crate::kbd::focused_root_window().ok_or_else(|| {
            CtlError::new(
                ErrorCode::NotFound,
                "focused: no keyboard focus or focus root resolvable",
            )
        })?;
        if app.is_some() {
            let root_hwnd = crate::window::native_hwnd(&root)
                .ok_or_else(|| internal("app window has no native hwnd"))?;
            if root_hwnd.0 as isize != focused_hwnd {
                return Err(CtlError::with_evidence(
                    ErrorCode::NotFound,
                    "focused: keyboard focus belongs to another window",
                    serde_json::json!({
                        "focused_window": focused_title,
                        "app_window": window_title,
                    }),
                ));
            }
        }
        let focused = auto
            .get_focused_element()
            .map_err(|e| crate::read_channel::failure("focused_element", e))?;
        if let Some(expected) = role {
            let actual = strict_role(&focused)?;
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(CtlError::with_evidence(
                    ErrorCode::NotFound,
                    "focused: focused element role does not match",
                    serde_json::json!({
                        "expected": expected,
                        "actual": actual,
                    }),
                ));
            }
        }
        return located_from(focused, focused_title, "focused");
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
            fast_exact_ids(&auto, &root, target)?
        };
        match hits {
            Some(hits) if hits.len() == 1 => {
                return located_from(hits[0].clone(), window_title, "primary");
            }
            Some(hits) if hits.is_empty() => {
                // L1 确定空手(id: 无 fuzzy 层),直落 L4 虚拟化容器
                let _t = actl_core::trace::scope("locate.l4");
                if let Some(hit) = locate_virtualized(&walker, &root, target)? {
                    return located_from(hit, window_title, "item-container");
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
        collect_matches(&walker, &root, 0, target, false, &mut counter, &mut matches)?;
    }

    // L2 名称模糊(仅 name: 目标且 L1 空手时):大小写/空白不敏感
    if matches.is_empty() {
        if let Target::Name(_) = target {
            let mut fuzzy = Vec::new();
            collect_matches(&walker, &root, 0, target, true, &mut counter, &mut fuzzy)?;
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
            locate_virtualized(&walker, &root, target)?
        };
        if let Some(hit) = l4 {
            return located_from(hit, window_title, "item-container");
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
            Some(c) => located_from(
                c.element.clone(),
                window_title,
                if matches!(target, Target::IdAt(..)) {
                    "id-ordinal"
                } else {
                    "role-ordinal"
                },
            ),
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
        return located_from(matches[0].element.clone(), window_title, level);
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
        )?;
        if anchors.len() != 1 {
            return Err(CtlError::new(
                ErrorCode::NotFound,
                format!(
                    "--near anchor {anchor_pat:?} must match exactly one element, got {}",
                    anchors.len()
                ),
            ));
        }
        {
            let ar = anchors[0]
                .element
                .get_bounding_rectangle()
                .map_err(|e| crate::read_channel::failure("anchor.bounds", e))?;
            let (ax, ay) = center(&ar);
            let mut best: Option<(f64, usize)> = None;
            for (i, c) in matches.iter().enumerate() {
                {
                    let r = c
                        .element
                        .get_bounding_rectangle()
                        .map_err(|e| crate::read_channel::failure("candidate.bounds", e))?;
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
                return located_from(matches[i].element.clone(), window_title, "anchor");
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

fn located_from(
    elem: UIElement,
    window_title: String,
    resolved_by: &'static str,
) -> Result<Located, CtlError> {
    Ok(Located {
        window_title,
        role: strict_role(&elem)?,
        name: Some(
            elem.get_name()
                .map_err(|e| crate::read_channel::failure("name", e))?,
        ),
        automation_id: Some(
            elem.get_automation_id()
                .map_err(|e| crate::read_channel::failure("automation_id", e))?,
        ),
        resolved_by,
        element: elem,
    })
}
/// 定位链候选(ref 与 snapshot 同源编号,可直接重放)
struct Candidate {
    element: UIElement,
    ref_no: u32,
    role: String,
    name: Option<String>,
    automation_id: Option<String>,
}

/// @eN 在完整观察的树中重放；观察失败与 ref 消失分开。
fn collect_until_ref(
    _walker: &UITreeWalker,
    elem: &UIElement,
    _depth: u32,
    want: u32,
    counter: &mut u32,
) -> Result<Option<UIElement>, CtlError> {
    for entry in crate::observation::tree(elem)?.require_complete()? {
        let role = strict_role(&entry.element)?;
        if actl_core::is_interactive_role(&role) {
            *counter += 1;
            if *counter == want {
                return Ok(Some(entry.element));
            }
        }
    }
    Ok(None)
}

fn strict_role(elem: &UIElement) -> Result<String, CtlError> {
    elem.get_control_type()
        .map(|v| format!("{v:?}"))
        .map_err(|e| crate::read_channel::failure("control_type", e))
}

fn collect_matches(
    _walker: &UITreeWalker,
    elem: &UIElement,
    _depth: u32,
    target: &Target,
    fuzzy: bool,
    counter: &mut u32,
    out: &mut Vec<Candidate>,
) -> Result<(), CtlError> {
    *counter = 0;
    for entry in crate::observation::tree(elem)?.require_complete()? {
        let element = entry.element;
        let role = strict_role(&element)?;
        let ref_no = if actl_core::is_interactive_role(&role) {
            *counter += 1;
            *counter
        } else {
            0
        };
        let name = element
            .get_name()
            .map_err(|e| crate::read_channel::failure("name", e))?;
        let id = element
            .get_automation_id()
            .map_err(|e| crate::read_channel::failure("automation_id", e))?;
        let hit = match target {
            Target::Name(s) if fuzzy => actl_core::target::fuzzy_contains(&name, s),
            Target::Name(s) => name.contains(s.as_str()),
            Target::Id(s) | Target::IdAt(s, _) => id == *s,
            Target::Role(s) | Target::RoleAt(s, _) => role == *s,
            _ => false,
        };
        if hit {
            out.push(Candidate {
                element,
                ref_no,
                role,
                name: Some(name),
                automation_id: Some(id),
            });
        }
    }
    Ok(())
}
/// id: 目标的 provider 侧精确匹配(UIA FindAll + AutomationId 属性条件,
/// TreeScope::Subtree 与 DFS 全遍历同覆盖含根自身)。
/// 条件创建/查询失败直接传播，不能触发静默回退。
fn fast_exact_ids(
    auto: &UIAutomation,
    root: &UIElement,
    target: &Target,
) -> Result<Option<Vec<UIElement>>, CtlError> {
    use uiautomation::types::{TreeScope, UIProperty};
    use uiautomation::variants::Variant;

    let Target::Id(s) = target else {
        return Ok(None);
    };
    let cond = auto
        .create_property_condition(UIProperty::AutomationId, Variant::from(s.clone()), None)
        .map_err(|e| crate::read_channel::failure("id.condition", e))?;
    let cache = auto.create_cache_request().map_err(internal)?;
    cache
        .set_tree_filter(auto.create_true_condition().map_err(internal)?)
        .map_err(internal)?;
    cache.set_tree_scope(TreeScope::Element).map_err(internal)?;
    root.find_all_build_cache(TreeScope::Subtree, &cond, &cache)
        .map(Some)
        .map_err(|e| crate::read_channel::failure("id.query", e))
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
    _walker: &UITreeWalker,
    root: &UIElement,
    target: &Target,
) -> Result<Option<UIElement>, CtlError> {
    use uiautomation::{types::UIProperty, variants::Variant};
    use windows::{
        Win32::UI::Accessibility::{
            IUIAutomationElement, IUIAutomationItemContainerPattern, UIA_ItemContainerPatternId,
        },
        core::Interface,
    };
    let Some((prop, value)) = container_query_for(target) else {
        return Ok(None);
    };
    let value = Variant::from(value);
    let entries = crate::observation::tree(root)?.require_complete()?;
    let mut hit = None;
    for entry in entries {
        let c = entry.element;
        if !crate::read_channel::available(
            &c,
            UIProperty::IsItemContainerPatternAvailable,
            "item-container",
        )? {
            continue;
        }
        // Raw ABI preserves S_OK + null (observed absence), unlike a non-null interface wrapper.
        // The returned non-null COM reference is adopted exactly once; no null UIElement is made.
        unsafe {
            let pattern: IUIAutomationItemContainerPattern = c
                .as_ref()
                .GetCurrentPattern(UIA_ItemContainerPatternId)
                .and_then(|p| p.cast())
                .map_err(|e| crate::read_channel::failure("item-container.pattern", e.into()))?;
            let mut raw = std::ptr::null_mut();
            (pattern.vtable().FindItemByProperty)(
                pattern.as_raw(),
                std::ptr::null_mut(),
                prop,
                std::mem::transmute_copy(value.as_ref()),
                &mut raw,
            )
            .ok()
            .map_err(|e| crate::read_channel::failure("item-container.query", e.into()))?;
            if !raw.is_null() {
                let element = UIElement::from(IUIAutomationElement::from_raw(raw));
                let mut second = std::ptr::null_mut();
                (pattern.vtable().FindItemByProperty)(
                    pattern.as_raw(),
                    element.as_ref().as_raw(),
                    prop,
                    std::mem::transmute_copy(value.as_ref()),
                    &mut second,
                )
                .ok()
                .map_err(|e| crate::read_channel::failure("item-container.next", e.into()))?;
                if !second.is_null() {
                    // Adopt and release the extra returned reference before rejecting ambiguity.
                    let _second = IUIAutomationElement::from_raw(second);
                    return Err(CtlError::new(
                        ErrorCode::Ambiguous,
                        "multiple virtualized items matched",
                    ));
                }
                if hit.is_some() {
                    return Err(CtlError::new(
                        ErrorCode::Ambiguous,
                        "multiple virtualized containers matched",
                    ));
                }
                hit = Some(element);
            }
        }
    }
    Ok(hit)
}
/// wait 族轮询的定位语义:仅 NOT_FOUND 表示缺席；其他观察错误立即传播。
/// **AMBIGUOUS = 确定性多命中,等待不可能收敛 → 立即失败**(候选 evidence
/// 已随错误携带,无需等满超时;2026-09-27 抖音多文档页实测:samples 冻结
/// 在 1 的误导性 TIMEOUT 根因)。
fn poll(app: Option<&str>, target: &Target) -> Result<Option<Located>, CtlError> {
    actl_core::wait_control::check()?;
    crate::scoped_locate::observed(locate(app, target, None))
}

/// wait --element:轮询定位直到目标可解析(TIMEOUT 语义;与 --expect 的
/// "立即断言"分工见 06 §3.6"耐心等待")。
pub fn wait_element(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    // 观察瞬态有界重试(docs/81 §8):首次解析在弹窗收尾/窗口重构过渡期可
    // 报 STALE_REF/INTERNAL(Word open 步每轮必现、稍后同命令即通)。等待是
    // 耐心命令——瞬态观察错误按"尚未出现"继续轮询(wait --gone 同款语义),
    // 超时仍如实失败;非瞬态错误立即上抛。
    let mut last_transient: Option<CtlError> = None;
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        match poll(app, target) {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error)
                if matches!(error.code, ErrorCode::StaleRef | ErrorCode::Internal)
                    && std::time::Instant::now() < deadline =>
            {
                last_transient = Some(error);
            }
            Err(error) => return Err(error),
        }
        if std::time::Instant::now() >= deadline {
            if let Some(transient) = last_transient {
                return Err(transient);
            }
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
/// 观察失败(如窗口拆除期 STALE_REF)视作"尚未消失"继续轮询——耐心命令
/// 不被瞬态观察错误打断,超时仍如实失败。
pub fn wait_gone(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        match poll(app, target) {
            Ok(None) => return Ok(()),
            Ok(Some(_)) => {}
            Err(_) if std::time::Instant::now() < deadline => {}
            Err(error) => return Err(error),
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
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        if let Some(loc) = poll(app, target)? {
            let actual = predicate.read(&loc.element)?;
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
                serde_json::json!({ "last_actual": last, "actual":last, "target":target.describe(), "expected":predicate.expected, "property":predicate.describe().split('=').next(), "channel":match predicate.prop { WaitProp::Value => "value-or-text-pattern", WaitProp::Name => "name", WaitProp::Text => "text-pattern", WaitProp::Checked => "toggle-pattern" }, "comparison":if predicate.contains {"contains"} else {"exact"} }),
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
/// (采样间隔 settle_ms,默认 250ms，跨一个采样间隔)。样本通道:text 优先、
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
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        if let Some(loc) = poll(app, target)? {
            let sample = stability_sample(&loc.element)?;
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
fn stability_sample(elem: &UIElement) -> Result<String, CtlError> {
    if let Some(text) = crate::read::read_text(elem)? {
        return Ok(text);
    }
    if let Some(value) = crate::read::read_value(elem)? {
        return Ok(value);
    }
    elem.get_name()
        .map_err(|e| crate::read_channel::failure("name", e))
}

/// wait/verify 共用的属性谓词(解析与判定分离,便于单测)。
pub struct WaitPredicate {
    pub prop: WaitProp,
    pub expected: String,
    pub contains: bool,
}

pub enum WaitProp {
    Text,
    Value,
    Name,
    Checked,
}

impl WaitPredicate {
    /// 解析 "prop=expected" 形态(prop ∈ value|name|text|checked;checked 期望 on/off)
    pub fn parse(raw: &str) -> Result<Self, CtlError> {
        let Some((prop, expected)) = raw.split_once('=') else {
            return Err(CtlError::protocol(format!(
                "invalid predicate {raw:?}: expected <prop>=<expected> with prop in value|name|text|checked"
            )));
        };
        let prop = match prop.trim() {
            "value" => WaitProp::Value,
            "name" => WaitProp::Name,
            "text" => WaitProp::Text,
            "checked" => WaitProp::Checked,
            other => {
                return Err(CtlError::protocol(format!(
                    "unknown predicate property {other:?}: expected value|name|text|checked"
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
            WaitProp::Text => "text",
            WaitProp::Checked => "checked",
        };
        format!("{p}={}", self.expected)
    }

    /// 读元素当前值(Checked 读 Toggle 状态,Value 走双通道,Name 读 UIA Name)
    pub fn read(&self, elem: &uiautomation::UIElement) -> Result<Option<String>, CtlError> {
        let (value, channel) = match self.prop {
            WaitProp::Value => (crate::read::read_value(elem)?, "value-or-text-pattern"),
            WaitProp::Text => (crate::read::read_text(elem)?, "text-pattern"),
            WaitProp::Name => (
                Some(
                    elem.get_name()
                        .map_err(|e| crate::read_channel::failure("name", e))?,
                ),
                "name",
            ),
            WaitProp::Checked => (
                crate::read_channel::toggle(elem)?.map(|v| v.to_lowercase()),
                "toggle-pattern",
            ),
        };
        Ok(Some(crate::read_channel::require(value, channel)?))
    }

    /// 判定(读不到值 = 未满足,fail-closed)
    pub fn satisfied(&self, actual: Option<&str>) -> bool {
        let Some(a) = actual else { return false };
        match (&self.prop, self.contains) {
            (WaitProp::Checked, _)
            | (WaitProp::Value, false)
            | (WaitProp::Name | WaitProp::Text, false) => a == self.expected,
            (WaitProp::Value, true) | (WaitProp::Name | WaitProp::Text, true) => {
                a.contains(&self.expected)
            }
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
