//! actl-uia —— Windows UIA 后端:capture(窗口定位 + DFS 遍历)。
//!
//! 全仓库唯一接触 COM 的 crate(AGENTS.md §5.5)。
//! 已知限制(spike #4,docs/spike-findings.md):walker 在"无子/无兄弟"时返回 Err,
//! 与真错误不可区分——当前以 `.ok()` 视为尽头;HRESULT 分类列入 M1 遗留。
//! 性能注记(spike #3):本版逐属性跨进程读取,release 下金任务实测,超 500ms 再上 cache request。

#![allow(unsafe_code)] // 豁免点(AGENTS.md §5.5):kbd 模块的 SendInput/MapVirtualKeyW 注入

pub mod input;
pub mod kbd;
pub mod mouse;

use actl_core::keys::KeySpec;
use actl_core::{CtlError, ErrorCode, UiNode};
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

/// 收集上限与深度上限(防失控 UI;截断在输出中标记 truncated)。
pub const MAX_ELEMENTS: usize = 5000;
pub const MAX_DEPTH: u32 = 40;

pub struct WindowInfo {
    pub title: String,
    pub class: String,
    pub pid: u32,
}

pub struct CaptureResult {
    pub window: WindowInfo,
    /// 窗口 RuntimeId(snapshot_id 版本化的比对键;跨进程稳定)
    pub window_runtime_id: Vec<i32>,
    /// DFS 序节点流,`parent` 指向同流索引(供 core 层 skeleton 投影)
    pub nodes: Vec<UiNode>,
    pub truncated: bool,
    /// 激活感知等待的毫秒数(0 = 目标本就在响应;>0 = 目标曾挂起,已唤醒)
    pub wake_ms: u32,
}

/// 捕获一个顶层窗口的 UIA 子树。
/// `app`:窗口标题子串;None = 焦点元素所在顶层窗口(spike #2:焦点链可能落在后台 UI)。
pub fn capture(app: Option<&str>) -> Result<CaptureResult, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;

    let target = match app {
        Some(pattern) => find_window(&auto, &walker, pattern)?,
        None => top_level_of_focused(&auto, &walker)?,
    };

    let window = WindowInfo {
        title: target.get_name().unwrap_or_default(),
        class: target.get_classname().unwrap_or_default(),
        pid: target.get_process_id().unwrap_or_default(),
    };
    let wake_ms = ensure_window_responsive(&target);
    let window_runtime_id = target.get_runtime_id().unwrap_or_default();

    let mut nodes = Vec::new();
    walk(&walker, &target, 0, None, &mut nodes);
    let truncated = nodes.len() >= MAX_ELEMENTS;
    Ok(CaptureResult {
        window,
        window_runtime_id,
        nodes,
        truncated,
        wake_ms,
    })
}

// ─── snapshot_id 版本化-lite(M2:窗口身份绑定)──────────────────────────
/// @eN 绑定的是"快照那一刻的窗口实例"。跨命令重放时窗口可能已关闭重开
/// (同标题 ≠ 同实例),纯计数重放会静默点进新窗口的巧合位置。方案:快照
/// 落盘 {snapshot_id → 窗口 RuntimeId};回放命令带 `--snapshot <id>` 时
/// 校验当前窗口实例与快照一致,不一致 → STALE_REF。
/// 元素级校验(树前缀比对)留待完整会话设计,与 fallback 链的 evidence 演进同批。
const SNAPSHOT_KEEP: usize = 20;

fn snapshot_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let dir = std::path::Path::new(&base).join("actl").join("snapshots");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// 快照记录落盘(保留最近 SNAPSHOT_KEEP 份,按修改时间淘汰)。
pub fn persist_snapshot(id: &str, window_title: &str, window_runtime_id: &[i32]) {
    let Some(dir) = snapshot_dir() else { return };
    let record = serde_json::json!({
        "snapshot_id": id,
        "window_title": window_title,
        "window_runtime_id": window_runtime_id,
    });
    let _ = std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_string(&record).unwrap_or_default(),
    );
    // 淘汰旧记录:按修改时间留最新 SNAPSHOT_KEEP 份
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                let t = m.modified().ok()?;
                Some((t, e.path()))
            })
            .collect();
        files.sort();
        let excess = files.len().saturating_sub(SNAPSHOT_KEEP);
        for (_, path) in files.into_iter().take(excess) {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn load_snapshot(id: &str) -> Option<(String, Vec<i32>)> {
    let dir = snapshot_dir()?;
    let text = std::fs::read_to_string(dir.join(format!("{id}.json"))).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let title = v["window_title"].as_str()?.to_string();
    let rid = v["window_runtime_id"]
        .as_array()?
        .iter()
        .filter_map(|x| x.as_i64().map(|n| n as i32))
        .collect();
    Some((title, rid))
}

/// 回放校验:当前窗口实例须与快照记录一致(RuntimeId 比对)。
pub fn check_snapshot_freshness(snapshot_id: &str, current: &UIElement) -> Result<(), CtlError> {
    let Some((_, recorded)) = load_snapshot(snapshot_id) else {
        return Err(CtlError::new(
            ErrorCode::StaleRef,
            format!("snapshot {snapshot_id:?} not found on disk (expired or different machine)"),
        ));
    };
    let now = current.get_runtime_id().unwrap_or_default();
    if !recorded.is_empty() && now != recorded {
        return Err(CtlError::new(
            ErrorCode::StaleRef,
            format!(
                "window instance changed since snapshot {snapshot_id:?}                  (same-title window reopened); re-snapshot before replaying refs"
            ),
        ));
    }
    Ok(())
}

/// 激活感知目标解析(M2 第一批,docs/spike-findings"UWP 挂起 = UIA 停摆"):
/// 挂起应用的每个跨进程 UIA 属性调用都会 stall ~2s(DCOM 等待应用恢复)。
/// 先用不阻塞调用方的 WM_NULL 探针测目标是否在泵消息;不在 → ShowWindowAsync
/// 异步唤醒 + 有界等待(≤1.6s)。把隐形的逐调用 stall 变成一次有界的、
/// 可上报的等待。无原生句柄的元素(UIA-only)直接放行。
fn ensure_window_responsive(elem: &UIElement) -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{
        SMTO_ABORTIFHUNG, SW_SHOW, SendMessageTimeoutW, ShowWindowAsync, WM_NULL,
    };

    let Some(hwnd) = native_hwnd(elem) else {
        return 0;
    };
    // 探针 60ms:不影响正常路径,挂起应用恰好被这 60ms 暴露
    let pumps = |timeout_ms: u32| unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
            SMTO_ABORTIFHUNG,
            timeout_ms,
            None,
        )
        .0 != 0
    };
    if pumps(timing().probe_ms) {
        return 0;
    }
    let started = std::time::Instant::now();
    unsafe {
        // 异步投递不等待应用;激活请求本身是 UWP 恢复的最常见触发器
        let _ = ShowWindowAsync(hwnd, SW_SHOW);
    }
    while started.elapsed() < std::time::Duration::from_millis(timing().wake_bound_ms) {
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
        if pumps(60) {
            break;
        }
    }
    started.elapsed().as_millis() as u32
}

/// 元素的原生 HWND(None = 无句柄/无效)。crate 的 Handle 未暴露原始值;
/// Handle/HANDLE/HWND 均为单指针包装,布局一致(同 close_window 的先例)。
/// HWND 的窗口标题(命中检查的比对键;空标题返回 None)。
fn window_title_of(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

fn native_hwnd(elem: &UIElement) -> Option<windows::Win32::Foundation::HWND> {
    let handle = elem.get_native_window_handle().ok()?;
    if handle.is_invalid() {
        return None;
    }
    let raw: windows::Win32::Foundation::HANDLE = unsafe { std::mem::transmute(handle) };
    Some(windows::Win32::Foundation::HWND(raw.0))
}

/// 顶层窗口 + 挂靠宿主的 owned dialog(IFileSaveDialog 等不是 root 的直接子节点,
/// 实测新记事本的"另存为"在宿主窗口的 depth-1,以 role=Window 挂靠)。
/// 注意:不做可见性过滤——UWP 应用后台挂起时 CoreWindow 会被 cloak
/// (IsWindowVisible=false),硬过滤会让快照只剩空壳框架、内容全失联
/// (金任务 1 实测 0/10);同名噪声由 find_window 的 WINUI_AUX_CLASSES 折叠消化。
fn top_windows(auto: &UIAutomation, walker: &UITreeWalker) -> Vec<UIElement> {
    let mut out = Vec::new();
    for w in root_children(auto, walker) {
        out.push(w.clone());
        out.extend(owned_dialogs_of(walker, &w));
    }
    out
}

/// root 直接子节点(顶层窗口)。快路径只走这里:不触 owned-dialog 深扫。
fn root_children(auto: &UIAutomation, walker: &UITreeWalker) -> Vec<UIElement> {
    let mut out = Vec::new();
    let Ok(root) = auto.get_root_element() else {
        return out;
    };
    let mut child = walker.get_first_child(&root).ok();
    while let Some(w) = child {
        out.push(w.clone());
        child = walker.get_next_sibling(&w).ok();
    }
    out
}

/// 窗口的直接子 Window(owned dialog,如挂靠宿主的"另存为")。
fn owned_dialogs_of(walker: &UITreeWalker, host: &UIElement) -> Vec<UIElement> {
    let mut out = Vec::new();
    let mut sub = walker.get_first_child(host).ok();
    while let Some(d) = sub {
        if d.get_control_type()
            .map(|t| t == ControlType::Window)
            .unwrap_or(false)
        {
            out.push(d.clone());
        }
        sub = walker.get_next_sibling(&d).ok();
    }
    out
}

fn scan_top_window(auto: &UIAutomation, walker: &UITreeWalker, pattern: &str) -> Option<UIElement> {
    top_windows(auto, walker)
        .into_iter()
        .find(|w| name_matches(w, pattern))
}

fn name_matches(elem: &UIElement, pattern: &str) -> bool {
    elem.get_name()
        .map(|n| n.contains(pattern))
        .unwrap_or(false)
}

/// WinUI 同一可见窗口的辅助 HWND(标题相同;实测计算器 3 个同名"计算器":
/// ApplicationFrameWindow 空壳框架 + TitleBarWindow 标题栏 + CoreWindow 内容,
/// 分属框架/内容两个 pid)。目标解析时折叠辅助件,**保留 CoreWindow**——
/// 内容元素(按钮等)挂在 CoreWindow 子树,框架子树是空壳(金任务 1 实测:
/// 偏好框架时 num1Button 全部失联 0/10)。
const WINUI_AUX_CLASSES: [&str; 2] = ["ApplicationFrameWindow", "ApplicationFrameTitleBarWindow"];

/// 在顶层窗口(含 owned dialog)中按标题子串定位**唯一**窗口。
/// 无匹配 → NOT_FOUND;**多匹配 → AMBIGUOUS 并附候选标题**(fail-closed:
/// 此前首个命中静默胜出,同名/含同名子串的窗口会把操作引向错误目标,doc 09 §3.3)。
/// 两阶段:先只扫 root 直接子节点(快路径,常见一步命中);0 命中才对每个
/// 顶层窗口深扫 owned dialog(如"另存为"挂靠宿主 depth-1)——深扫是每窗口
/// 几十次跨进程 COM 往返,放热路径实测把命令耗时从 ~40ms 拖到 ~2s。
fn find_window(
    auto: &UIAutomation,
    walker: &UITreeWalker,
    pattern: &str,
) -> Result<UIElement, CtlError> {
    let mut matches: Vec<UIElement> = root_children(auto, walker)
        .into_iter()
        .filter(|w| name_matches(w, pattern))
        .collect();
    if matches.is_empty() {
        // owned dialog 兜底:只对有子 Window 的宿主扫
        for host in root_children(auto, walker) {
            matches.extend(
                owned_dialogs_of(walker, &host)
                    .into_iter()
                    .filter(|w| name_matches(w, pattern)),
            );
        }
    }
    // 折叠 WinUI 辅助 HWND:存在非辅助窗口时,辅助件不参与解析
    let has_primary = matches.iter().any(|w| {
        let cls = w.get_classname().unwrap_or_default();
        !WINUI_AUX_CLASSES.contains(&cls.as_str())
    });
    let pool: Vec<UIElement> = if has_primary {
        matches
            .into_iter()
            .filter(|w| {
                let cls = w.get_classname().unwrap_or_default();
                !WINUI_AUX_CLASSES.contains(&cls.as_str())
            })
            .collect()
    } else {
        matches
    };
    match pool.len() {
        0 => Err(CtlError::new(
            ErrorCode::NotFound,
            format!("no top-level window title contains {pattern:?}"),
        )),
        1 => Ok(pool.into_iter().next().expect("len == 1")),
        n => {
            let titles: Vec<String> = pool
                .iter()
                .filter_map(|w| w.get_name().ok())
                .take(5)
                .collect();
            Err(CtlError::new(
                ErrorCode::Ambiguous,
                format!(
                    "pattern {pattern:?} matches {n} windows: {titles:?}; tighten the selector"
                ),
            ))
        }
    }
}

/// 焦点元素沿父链上行到顶层窗口。
fn top_level_of_focused(auto: &UIAutomation, walker: &UITreeWalker) -> Result<UIElement, CtlError> {
    let mut cur = auto.get_focused_element().map_err(internal)?;
    loop {
        match walker.get_parent(&cur) {
            Ok(p) => cur = p,
            Err(_) => return Ok(cur), // 到达根(尽头 Err 语义,见模块注释)
        }
    }
}

fn walk(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: u32,
    parent: Option<usize>,
    nodes: &mut Vec<UiNode>,
) {
    if nodes.len() >= MAX_ELEMENTS || depth > MAX_DEPTH {
        return;
    }
    nodes.push(to_node(elem, depth, parent));
    let idx = nodes.len() - 1;
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        walk(walker, &c, depth + 1, Some(idx), nodes);
        child = walker.get_next_sibling(&c).ok();
    }
}

fn to_node(elem: &UIElement, depth: u32, parent: Option<usize>) -> UiNode {
    UiNode {
        depth,
        role: format!(
            "{:?}",
            elem.get_control_type().unwrap_or(ControlType::Custom)
        ),
        name: elem.get_name().ok().filter(|n| !n.is_empty()),
        automation_id: elem.get_automation_id().ok().filter(|a| !a.is_empty()),
        parent,
    }
}

fn timing() -> &'static actl_core::timing::Timing {
    actl_core::timing::Timing::load()
}

fn internal(e: impl std::fmt::Debug) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("{e:?}"))
}

// ---------- 定位与交互(locate / click / read_property) ----------

use actl_core::target::Target;
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UISelectionItemPattern, UITogglePattern,
    UIValuePattern,
};

/// 定位结果:命中元素 + 所在窗口。
pub struct Located {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    /// 定位链实际命中级别:"primary" | "fuzzy-name" | "role-ordinal" |
    /// "anchor" | "ref-replay"——agent 据此判断置信度并决定是否加验证
    pub resolved_by: &'static str,
    pub(crate) element: UIElement,
}

/// 在目标窗口内按 Target 定位元素(遍历可提前退出,优于全量 capture)。
/// `Ref(n)` 数不到 = 树已变化 → STALE_REF;语义选择器无命中 → NOT_FOUND(06 §4)。
pub fn locate(app: Option<&str>, target: &Target, near: Option<&str>) -> Result<Located, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let root = match app {
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
    };
    let window_title = root.get_name().unwrap_or_default();
    // 激活感知:挂起目标先唤醒(有界),避免 DFS 里逐调用 stall
    let _wake_ms = ensure_window_responsive(&root);

    // @eN:计数重放,不参与 fallback(ref 漂移 = STALE_REF,模糊化反而危险)
    if let Target::Ref(n) = target {
        let mut counter = 0u32;
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
    let mut counter = 0u32;
    let mut matches = Vec::new();
    collect_matches(&walker, &root, 0, target, false, &mut counter, &mut matches);

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
        return Err(CtlError::new(
            ErrorCode::NotFound,
            format!(
                "no element matches {} (exact and fuzzy levels)",
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
    if let Target::RoleAt(_, n) = target {
        return match matches.get((*n as usize).saturating_sub(1)) {
            Some(c) => Ok(located_from(
                c.element.clone(),
                window_title,
                "role-ordinal",
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
    let name = elem.get_name().ok().filter(|n| !n.is_empty());
    let id = elem.get_automation_id().ok().filter(|a| !a.is_empty());
    let hit = match target {
        Target::Ref(_) => false,
        Target::Name(s) => match (&name, fuzzy) {
            (Some(n), true) => actl_core::target::fuzzy_contains(n, s),
            (Some(n), false) => n.contains(s.as_str()),
            (None, _) => false,
        },
        Target::Id(s) => id.as_deref() == Some(s.as_str()),
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
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        collect_matches(walker, &c, depth + 1, target, fuzzy, counter, out);
        child = walker.get_next_sibling(&c).ok();
    }
}

fn role_of(elem: &UIElement) -> String {
    format!(
        "{:?}",
        elem.get_control_type().unwrap_or(ControlType::Custom)
    )
}

/// click:语义执行分发(06 M1 范围:Invoke/Value/Selection)。
/// Invoke 优先;无 Invoke 但支持 SelectionItem(列表项/树项/选项卡)→ select(),
/// 语义同为"点击选中"。实际使用的 pattern 随结果上报,agent 可归因。
pub fn click(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<(Located, &'static str), CtlError> {
    let loc = locate(app, target, near)?;
    if let Ok(invoke) = loc.element.get_pattern::<UIInvokePattern>() {
        invoke.invoke().map_err(internal)?;
        return Ok((loc, "invoke"));
    }
    if let Ok(sel) = loc.element.get_pattern::<UISelectionItemPattern>() {
        sel.select().map_err(internal)?;
        return Ok((loc, "selection-item"));
    }
    // 复选框/开关:点击语义 = Toggle(状态翻转)
    if let Ok(toggle) = loc.element.get_pattern::<UITogglePattern>() {
        toggle.toggle().map_err(internal)?;
        return Ok((loc, "toggle"));
    }
    // 折叠面板/树节点:点击语义 = ExpandCollapse
    if let Ok(exp) = loc.element.get_pattern::<UIExpandCollapsePattern>() {
        exp.expand().map_err(internal)?;
        return Ok((loc, "expand-collapse"));
    }
    Err(CtlError::new(
        ErrorCode::NotActionable,
        format!(
            "{} ({}) exposes none of Invoke/SelectionItem/Toggle/ExpandCollapse; \
             scroll/focus first, try `set-value`, or `--physical` for a real click",
            target.describe(),
            loc.role
        ),
    ))
}

/// get:读取元素属性;`property` 白名单,缺省返回基本全集。
pub const PROPS: &[&str] = &["name", "role", "automation_id", "offscreen", "value"];

pub struct PropertyInfo {
    pub name: Option<String>,
    pub role: String,
    pub automation_id: Option<String>,
    pub offscreen: Option<bool>,
    pub value: Option<String>,
}

pub fn read_property(
    app: Option<&str>,
    target: &Target,
    property: Option<&str>,
    near: Option<&str>,
) -> Result<PropertyInfo, CtlError> {
    if let Some(p) = property {
        if !PROPS.contains(&p) {
            return Err(CtlError::protocol(format!(
                "unknown property {p:?}: expected one of {PROPS:?}"
            )));
        }
    }
    let loc = locate(app, target, near)?;
    let want = |p: &str| property.map(|sel| sel == p).unwrap_or(true);
    let value = want("value")
        .then(|| {
            loc.element
                .get_pattern::<UIValuePattern>()
                .ok()
                .and_then(|pat| pat.get_value().ok())
                .filter(|v| !v.is_empty())
        })
        .flatten();
    Ok(PropertyInfo {
        name: want("name").then_some(loc.name.clone()).flatten(),
        role: if want("role") {
            loc.role.clone()
        } else {
            String::new()
        },
        automation_id: want("automation_id")
            .then_some(loc.automation_id.clone())
            .flatten(),
        offscreen: want("offscreen")
            .then(|| loc.element.is_offscreen().ok())
            .flatten(),
        value,
    })
}

// ---------- 输入组(type / press / set-value)与窗口等待 ----------

/// 键盘命令家族说明:type/press 走 SendInput 键盘事件(键盘语义命令的执行本体,
/// 非物理"兜底",ADR-007 的 --physical 约束针对 click/scroll 等 UIA 等价物的兜底路径)。
pub struct InputResult {
    pub window_title: String,
    pub role: String,
    pub name: Option<String>,
}

impl From<&Located> for InputResult {
    fn from(loc: &Located) -> Self {
        Self {
            window_title: loc.window_title.clone(),
            role: loc.role.clone(),
            name: loc.name.clone(),
        }
    }
}

/// type:定位 → SetFocus →(护栏)→ 逐字键盘输入或剪贴板粘贴。
/// `--paste`:文本经剪贴板 + ctrl+v 送达——**中文模式 IME 下唯一可靠的文本通道**
/// (UNICODE 包在 WinUI/TSF 仍会被 IME 组合改写:空格吞成上屏、标点全角化,
/// 战记实测);代价是占用共享剪贴板,输出透明上报 via 字段,不静默。
pub fn type_text(
    app: Option<&str>,
    target: &Target,
    text: &str,
    paste: bool,
    near: Option<&str>,
) -> Result<TypeOutcome, CtlError> {
    let loc = locate(app, target, near)?;
    loc.element.set_focus().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) cannot take keyboard focus",
                target.describe(),
                loc.role
            ),
        )
    })?;
    // 护栏与 press 相同:元素置焦后仍要物理前台核对,文本才允许注入
    if let Some(pattern) = app {
        std::thread::sleep(std::time::Duration::from_millis(timing().type_focus_ms));
        let fg = kbd::foreground_title().unwrap_or_default();
        if !fg.contains(pattern) {
            return Err(CtlError::new(
                ErrorCode::PermDenied,
                format!(
                    "text injection ABORTED by verify-then-inject guard: \
                     physical foreground is {fg:?}, expected a window matching {pattern:?}; \
                     no text was sent"
                ),
            ));
        }
    }
    // 输入占用锁(doc 09 §6.2):多 actl 实例互斥;持锁覆盖整个注入动作段
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    let total = text.chars().count();
    let outcome = |delivered: usize, stopped_early: bool| TypeOutcome {
        located: (&loc).into(),
        delivered,
        total,
        stopped_early,
    };
    if paste {
        set_clipboard_text(text)?;
        // ctrl+v 走 VK 组合通道(IME 不拦截带修饰键组合);单次原子动作不分批
        kbd::send_key_spec(&KeySpec {
            modifiers: vec!["ctrl".into()],
            keys: vec![actl_core::keys::Key::Char('v')],
        })?;
        Ok(outcome(total, false))
    } else {
        // 键盘逐字通道:分批注入,批间复核物理前台——焦点被抢(真人点走/弹窗
        // 夺焦)即停在批边界并如实上报已送达量(并发输入损坏注入的四次活体
        // 实证结论,docs/spike-findings)
        let chars: Vec<char> = text.chars().collect();
        let pattern = app.map(str::to_string);
        let delivered = input::send_in_segments(
            &chars,
            timing().segment_chars,
            |seg| {
                kbd::send_key_spec(&KeySpec {
                    modifiers: Vec::new(),
                    keys: seg.iter().map(|c| actl_core::keys::Key::Char(*c)).collect(),
                })
            },
            || {
                pattern
                    .as_ref()
                    .map(|p| {
                        kbd::foreground_title()
                            .map(|t| t.contains(p.as_str()))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            },
        )?;
        Ok(outcome(delivered, delivered < total))
    }
}

/// type 的结果:注入了多少、是否中途被焦点抢占截断(部分送达是如实上报,
/// 不是失败——ok:true + input_effects.keyboard="partial")。
pub struct TypeOutcome {
    pub located: InputResult,
    pub delivered: usize,
    pub total: usize,
    pub stopped_early: bool,
}

/// press:可选先置前台(--app)。实测链路(docs/spike-findings.md):窗口级 SetFocus
/// 只激活窗口、不建立键盘焦点(键会被丢);必须再对窗口内内容元素做**元素级** SetFocus。
/// 启发式:窗口内首个 Document/Edit;找不到(如纯按钮面板)退化为仅窗口前置。
///
/// **verify-then-inject 护栏(事故教训,docs/spike-findings.md #6)**:注入前用 Win32
/// 物理前台(GetForegroundWindow)验证前台确为目标窗口——UIA 焦点视图可能与物理前台
/// 不一致,"看起来聚焦了"不等于键会进目标。不匹配则拒绝注入,宁可失败不可错发。
pub fn press_keys(spec: &KeySpec, app: Option<&str>) -> Result<Option<String>, CtlError> {
    let focused = prepare_keyboard(app)?;
    // 输入占用锁(doc 09 §6.2):护栏之后、注入之前取得,覆盖整个按键序列
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    kbd::send_key_spec(spec)?;
    Ok(focused)
}

/// key-down/key-up:与 press 相同的护栏链,但只发半边事件(见 kbd::send_key_partial)。
pub fn press_half(spec: &KeySpec, app: Option<&str>) -> Result<Option<String>, CtlError> {
    let focused = prepare_keyboard(app)?;
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    // 半事件方向由调用方决定;此函数不持锁跨进程(key-up 是另一进程,锁不横跨)
    kbd::send_key_partial(spec, true)?;
    Ok(focused)
}

/// 键盘注入的前置链:置前台 → 元素级聚焦 → 物理前台护栏(verify-then-inject)。
fn prepare_keyboard(app: Option<&str>) -> Result<Option<String>, CtlError> {
    Ok(match app {
        Some(pattern) => {
            let title = focus_window(pattern)?;
            std::thread::sleep(std::time::Duration::from_millis(timing().press_focus_ms));
            // 元素聚焦是异步请求且存在竞态(实测固定延时忽好忽坏):
            // 反复 SetFocus 并验证 UIA 焦点元素真的变为可编辑角色,最多 3 轮
            ensure_element_focus(pattern)?;
            // 护栏:物理前台必须是目标窗口,否则注入会打进别处(已发生过真实事故)
            let fg = kbd::foreground_title().unwrap_or_default();
            if !fg.contains(pattern) {
                return Err(CtlError::new(
                    ErrorCode::PermDenied,
                    format!(
                        "keyboard injection ABORTED by verify-then-inject guard: \
                         physical foreground is {fg:?}, expected a window matching {pattern:?}; \
                         no keys were sent"
                    ),
                ));
            }
            Some(title)
        }
        None => None,
    })
}

/// 反复对窗口内可编辑元素 SetFocus,直到**物理**键盘焦点的根窗口是目标。
/// 判据用 GetGUIThreadInfo 物理层而非 UIA focused element:后者在 WinUI 上
/// 实测会谎报(物理层 RichEditD2DPT 已聚焦时仍报别窗口编辑框,导致成批误拒绝
/// ——战记"信源必须用可靠层"的又一次实例)。重试耗尽未就绪 → 放行,由
/// verify-then-inject 物理前台护栏做最终裁决(它才是注入前的权威闸门)。
fn ensure_element_focus(pattern: &str) -> Result<(), CtlError> {
    for _ in 0..timing().focus_retries {
        if let Some(edit) = find_editable_element(pattern) {
            let _ = edit.set_focus();
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
        if kbd::focused_root_title()
            .map(|t| t.contains(pattern))
            .unwrap_or(false)
        {
            return Ok(());
        }
    }
    Ok(())
}

/// 在标题匹配的窗口内找首个 Document/Edit(元素级键盘焦点的落点)。
fn find_editable_element(pattern: &str) -> Option<UIElement> {
    let auto = UIAutomation::new().ok()?;
    let walker = auto.create_tree_walker().ok()?;
    let win = find_window(&auto, &walker, pattern).ok()?;
    fn dfs(walker: &UITreeWalker, elem: &UIElement, depth: u32) -> Option<UIElement> {
        if depth > 12 {
            return None;
        }
        let role = format!("{:?}", elem.get_control_type().ok()?);
        if role == "Document" || role == "Edit" {
            return Some(elem.clone());
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            if let Some(hit) = dfs(walker, &c, depth + 1) {
                return Some(hit);
            }
            child = walker.get_next_sibling(&c).ok();
        }
        None
    }
    dfs(&walker, &win, 0)
}

/// set-value:UIValue pattern 直写,不经键盘(与 type 的分工见 06 §3.6)。
pub fn set_value(
    app: Option<&str>,
    target: &Target,
    value: &str,
    near: Option<&str>,
) -> Result<InputResult, CtlError> {
    let loc = locate(app, target, near)?;
    let pattern = loc.element.get_pattern::<UIValuePattern>().map_err(|_| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no Value pattern; try `type` (keyboard path) instead",
                target.describe(),
                loc.role
            ),
        )
    })?;
    pattern.set_value(value).map_err(internal)?;
    Ok((&loc).into())
}

/// 等待标题包含 `substr` 的顶层窗口出现(--expect 的执行体,06 §4:默认 2s 轮询)。
pub fn wait_window(substr: &str, timeout_ms: u64) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Some(title) = scan_top_windows(&auto, &walker, substr) {
            return Ok(title);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::AssertionFailed,
                format!(
                    "expected window containing {substr:?} did not appear within {timeout_ms}ms"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

/// 当前全部顶层窗口(含 owned dialog)的 UIA RuntimeId——--expect 的"新窗口"基线。
/// 用 RuntimeId 而非标题:标题是可变身份(如记事本脏标记"*无标题"、保存后改名),
/// 按标题比对会把标题变化的既有窗口误判为"新窗口"(实测踩坑)。
pub fn window_identities() -> Result<Vec<Vec<i32>>, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    Ok(top_windows(&auto, &walker)
        .into_iter()
        .filter_map(|w| w.get_runtime_id().ok())
        .collect())
}

/// 等待一个**基线中不存在**、标题包含 `substr` 的新窗口出现。
/// --expect 的后置断言语义:证明"本动作导致了新窗口",而不是"存在同名窗口"
/// (后者会被预先打开的同名窗口恒真欺骗,doc 09 §2)。
/// 基线按 RuntimeId 比对:同名新窗口能通过,标题变化的既有窗口不会误判。
pub fn wait_new_window(
    substr: &str,
    timeout_ms: u64,
    baseline: &[Vec<i32>],
) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if let Some(title) = top_windows(&auto, &walker)
            .into_iter()
            .filter(|w| name_matches(w, substr))
            .find(|w| {
                w.get_runtime_id()
                    .map(|id| !baseline.contains(&id))
                    .unwrap_or(false) // 读不到身份 → 无法证明"新",不当作命中
            })
            .and_then(|w| w.get_name().ok())
        {
            return Ok(title);
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::AssertionFailed,
                format!(
                    "no NEW window containing {substr:?} appeared within {timeout_ms}ms \
                     (a pre-existing window with that title does not satisfy --expect)"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

fn scan_top_windows(auto: &UIAutomation, walker: &UITreeWalker, substr: &str) -> Option<String> {
    scan_top_window(auto, walker, substr).and_then(|w| w.get_name().ok())
}

/// focus-window:把标题匹配的顶层窗口带到前台(UIA SetFocus,即等效前台)。
/// press/type 等键盘命令作用于全局焦点——多窗口场景必须先 focus-window(06 §3.6 裁定表)。
pub fn focus_window(pattern: &str) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    win.set_focus().map_err(|e| {
        CtlError::new(
            ErrorCode::NotActionable,
            format!("cannot focus window {pattern:?}: {e:?}"),
        )
    })?;
    Ok(win.get_name().unwrap_or_default())
}

/// list-windows:枚举顶层窗口 + owned dialog(title/class/pid),供 agent 选择目标。
pub fn list_windows() -> Result<Vec<(String, String, u32)>, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    Ok(top_windows(&auto, &walker)
        .into_iter()
        .filter_map(|w| {
            let title = w.get_name().ok()?;
            if title.is_empty() {
                return None;
            }
            Some((
                title,
                w.get_classname().unwrap_or_default(),
                w.get_process_id().unwrap_or_default(),
            ))
        })
        .collect())
}

// ─── 窗口/等待/剪贴板(M1 收尾) ───────────────────────────────────────────

/// CF_UNICODETEXT(=13)。不引 Win32_System_Ole 只为一个常量。
const CF_UNICODETEXT: u32 = 13;

/// close-window:优雅关闭标题匹配的唯一窗口(WM_CLOSE 异步投递;未保存内容
/// 由应用自行弹提示——语义关闭,不是强杀)。
pub fn close_window(pattern: &str) -> Result<String, CtlError> {
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    let title = win.get_name().unwrap_or_default();
    let Some(hwnd) = native_hwnd(&win) else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("window {pattern:?} exposes no native handle"),
        ));
    };
    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_CLOSE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        )
    }
    .map_err(|e| CtlError::internal(format!("PostMessageW(WM_CLOSE) failed for {title:?}: {e}")))?;
    Ok(title)
}

/// wait --element:轮询定位直到目标可解析(TIMEOUT 语义;与 --expect 的
/// "立即断言"分工见 06 §3.6"耐心等待")。
pub fn wait_element(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if locate(app, target, None).is_ok() {
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

// ─── 剪贴板(CF_UNICODETEXT;剪贴板是共享状态,doc 09 §6.2 提醒并发) ──────

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

/// 确保任何返回路径都 CloseClipboard(占用不释放会拖死系统剪贴板)。
struct ClipGuard;
impl Drop for ClipGuard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() }.ok();
    }
}

fn open_clipboard() -> Result<ClipGuard, CtlError> {
    if unsafe { OpenClipboard(None) }.is_ok() {
        Ok(ClipGuard)
    } else {
        Err(CtlError::new(
            ErrorCode::NotActionable,
            "clipboard is held by another process; retry shortly",
        ))
    }
}

/// get-clipboard:读文本(None = 无文本格式/空)。非文本格式(图片等)返回 None。
pub fn get_clipboard_text() -> Result<Option<String>, CtlError> {
    unsafe {
        let _guard = open_clipboard()?;
        if !windows::Win32::System::DataExchange::IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok()
        {
            return Ok(None);
        }
        let Ok(h) = GetClipboardData(CF_UNICODETEXT) else {
            return Ok(None);
        };
        let hg = HGLOBAL(h.0);
        let p = GlobalLock(hg) as *const u16;
        if p.is_null() {
            return Ok(None);
        }
        // 手动扫 nul 结尾(免引 Globalization 的 lstrlenW)
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        let _ = GlobalUnlock(hg);
        Ok(Some(text))
    }
}

/// set-clipboard:写入文本(整体替换当前剪贴板内容——这是本命令的语义)。
pub fn set_clipboard_text(text: &str) -> Result<(), CtlError> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};

    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let _guard = open_clipboard()?;
        EmptyClipboard().map_err(|e| CtlError::internal(format!("EmptyClipboard: {e}")))?;
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)
            .map_err(|e| CtlError::internal(format!("GlobalAlloc: {e}")))?;
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(CtlError::internal("GlobalLock failed"));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        let _ = GlobalUnlock(h);
        // 所有权移交系统;失败则自毁防泄漏
        if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
            return Err(CtlError::internal("SetClipboardData failed"));
        }
        Ok(())
    }
}

/// 指针物理动作(泛化自 click --physical;doc 09 §6 七步链)。
/// ①语义定位唯一目标 ②取 UIA clickable point(无 → 拒绝,绝不猜中心点)
/// ③命中检查:该坐标的最顶层窗口须属于目标根(标题比对;遮挡/最小化 → 拒绝)
/// ④持输入占用锁 + 修饰键空置检查 ⑤单批注入(竞态窗最小化)。
/// 命中检查与注入之间仍有竞态,如实设计:检查紧贴注入,不承诺零竞态。
pub enum PointerAction {
    LeftClick,
    RightClick,
    DoubleClick,
    Hover,
    Drag { to: (i32, i32) },
    Wheel { notches: i32 },
}

pub fn pointer_physical(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    action: PointerAction,
) -> Result<(Located, (i32, i32)), CtlError> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor, WindowFromPoint};

    let loc = locate(app, target, near)?;
    let Some(point) = loc.element.get_clickable_point().ok().flatten() else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) exposes no clickable point; refusing to guess a center point",
                target.describe(),
                loc.role
            ),
        ));
    };
    let (px, py) = (point.get_x(), point.get_y());

    // 命中检查:坐标处最顶层窗口的根标题 == 目标窗口根标题。
    // ①目标侧从元素沿父链上溯到窗口根(WinUI 控件本身没有 HWND);
    // ②两侧取根标题比对而非严格 HWND——WinUI 三件套(框架/CoreWindow
    //   分属不同 HWND)会让同窗口的严格比对必然失败(实测)。
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    // 目标窗口 HWND:①从元素沿父链爬到第一个带句柄的元素;②部分 provider
    // 不暴露 NativeWindowHandle(实测记事本),按窗口标题兜底重解析
    let target_title = {
        let mut root = loc.element.clone();
        while native_hwnd(&root).is_none() {
            match walker.get_parent(&root) {
                Ok(p) => root = p,
                Err(_) => break,
            }
        }
        let title = native_hwnd(&root).and_then(window_title_of);
        match title {
            Some(t) => t,
            None => find_window(&auto, &walker, &loc.window_title)
                .ok()
                .and_then(|w| native_hwnd(&w))
                .and_then(window_title_of)
                .ok_or_else(|| {
                    CtlError::new(
                        ErrorCode::NotActionable,
                        "target window has no native handle for hit-checking",
                    )
                })?,
        }
    };
    let root_of = |hwnd: windows::Win32::Foundation::HWND| {
        let r = unsafe { GetAncestor(hwnd, GA_ROOT) };
        (!r.0.is_null()).then_some(r)
    };
    let hit = unsafe { WindowFromPoint(POINT { x: px, y: py }) };
    let hit_title = (!hit.0.is_null())
        .then_some(hit)
        .and_then(root_of)
        .and_then(window_title_of);
    if hit_title.as_deref() != Some(target_title.as_str()) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "clickable point ({px},{py}) is occupied by another window \
                 (occluded or minimized); refusing physical pointer action"
            ),
        ));
    }

    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    // 修饰键空置检查:ctrl 按住时的物理点击 = ctrl+click(语义劫持)
    kbd::wait_modifiers_clear()?;
    match action {
        PointerAction::LeftClick => mouse::left_click_at(px, py)?,
        PointerAction::RightClick => mouse::right_click_at(px, py)?,
        PointerAction::DoubleClick => mouse::double_click_at(px, py)?,
        PointerAction::Hover => mouse::move_to(px, py)?,
        PointerAction::Drag { to: (tx, ty) } => mouse::drag_to(px, py, tx, ty)?,
        PointerAction::Wheel { notches } => mouse::wheel_at(px, py, notches)?,
    }
    Ok((loc, (px, py)))
}

/// click --physical 的便捷入口(物理左键)。
pub fn click_physical(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
) -> Result<(Located, (i32, i32)), CtlError> {
    pointer_physical(app, target, near, PointerAction::LeftClick)
}

/// 拖拽:from/to 各自定位取 clickable point,from 侧命中检查后一批注入。
pub fn drag_physical(
    app: Option<&str>,
    from: &Target,
    from_near: Option<&str>,
    to: &Target,
    to_near: Option<&str>,
) -> Result<(Located, (i32, i32)), CtlError> {
    let (_, (fx, fy)) = pointer_physical(app, from, from_near, PointerAction::Hover)?;
    let Some(dest) = locate(app, to, to_near)?
        .element
        .get_clickable_point()
        .ok()
        .flatten()
    else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} exposes no clickable point for drag destination",
                to.describe()
            ),
        ));
    };
    let point = (dest.get_x(), dest.get_y());
    let _lock = input::InputLock::acquire(timing().lock_wait_ms)?;
    mouse::drag_to(fx, fy, point.0, point.1)?;
    Ok((locate(app, from, from_near)?, (fx, fy)))
}

/// resize-window:标题唯一匹配窗口 → SetWindowPos(不动位置,只改尺寸)。
pub fn resize_window(pattern: &str, width: i32, height: i32) -> Result<String, CtlError> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos,
    };

    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    let title = win.get_name().unwrap_or_default();
    let Some(hwnd) = native_hwnd(&win) else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("window {pattern:?} has no native handle"),
        ));
    };
    // 保持当前左上角不动;目标尺寸裁到系统最小限制之上
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|e| CtlError::internal(format!("GetWindowRect: {e}")))?;
    let w = width.max(120);
    let h = height.max(120);
    unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND(std::ptr::null_mut())),
            rect.left,
            rect.top,
            w,
            h,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }
    .map_err(|e| CtlError::internal(format!("SetWindowPos: {e}")))?;
    Ok(title)
}

/// scroll:UIA Scroll pattern 优先(容器语义,后台);无 pattern → 物理滚轮。
/// 返回 (via, 行数) 供上报。
pub fn scroll_element(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    notches: i32,
) -> Result<(Located, &'static str), CtlError> {
    use uiautomation::patterns::UIScrollPattern;
    use uiautomation::types::ScrollAmount;

    let loc = locate(app, target, near)?;
    if let Ok(scroll) = loc.element.get_pattern::<UIScrollPattern>() {
        let amount = if notches >= 0 {
            ScrollAmount::SmallIncrement
        } else {
            ScrollAmount::SmallDecrement
        };
        // pattern 宣称支持但调用失败(实测记事本 Document)→ 物理滚轮兜底
        let mut ok = true;
        for _ in 0..notches.abs() {
            if scroll
                .scroll(uiautomation::types::ScrollAmount::NoAmount, amount)
                .is_err()
            {
                ok = false;
                break;
            }
        }
        if ok {
            return Ok((loc, "uia-scroll"));
        }
    }
    // 物理兜底:滚轮打在元素 clickable point 上
    let (_, _) = pointer_physical(app, target, near, PointerAction::Wheel { notches })?;
    Ok((loc, "physical-wheel"))
}

// ─── M2 第二批:verify / wait-gone / extract ──────────────────────────────

/// verify:立即断言(与 wait 的"耐心等待"分工,06 §3.6)。
/// 断言失败 → ASSERTION_FAILED,evidence 携带实际值。
pub enum VerifyKind {
    /// 元素存在(经定位链解析成功)
    Exists,
    /// ValuePattern 值 == 期望(exact)或包含期望(contains)
    Value { expected: String, contains: bool },
    /// Toggle 状态为 On
    Checked,
}

pub struct VerifyOutcome {
    pub located: Located,
    pub actual: Option<String>,
    pub mode: &'static str,
}

pub fn verify(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    kind: VerifyKind,
) -> Result<VerifyOutcome, CtlError> {
    use uiautomation::patterns::UITogglePattern;

    let fail = |what: &str, actual: Option<String>| {
        CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            format!("verify {what} failed"),
            serde_json::json!({ "actual": actual }),
        )
    };
    match kind {
        VerifyKind::Exists => {
            let loc = locate(app, target, near)?;
            Ok(VerifyOutcome {
                located: loc,
                actual: None,
                mode: "exists",
            })
        }
        VerifyKind::Value { expected, contains } => {
            let loc = locate(app, target, near)?;
            let actual = read_value(&loc.element);
            let ok = match (&actual, contains) {
                (Some(a), true) => a.contains(&expected),
                (Some(a), false) => a == &expected,
                (None, _) => false,
            };
            if !ok {
                return Err(fail("value", actual));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual,
                mode: if contains { "value-contains" } else { "value" },
            })
        }
        VerifyKind::Checked => {
            let loc = locate(app, target, near)?;
            let state = loc
                .element
                .get_pattern::<UITogglePattern>()
                .ok()
                .and_then(|t| t.get_toggle_state().ok())
                .map(|s| format!("{s:?}"));
            if state.as_deref() != Some("On") {
                return Err(fail("checked", state));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual: state,
                mode: "checked",
            })
        }
    }
}

fn read_value(elem: &UIElement) -> Option<String> {
    elem.get_pattern::<UIValuePattern>()
        .ok()
        .and_then(|v| v.get_value().ok())
}

/// wait --gone:轮询直到目标**不再**可解析(消失;TIMEOUT 语义)。
pub fn wait_gone(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if locate(app, target, None).is_err() {
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

/// extract --table:容器 → 行(DataItem/ListItem)→ 单元格(Text/Edit 值)。
/// 通用树表扁平化:行的所有 Text 后代按 DFS 序作为单元格。
pub fn extract_table(app: Option<&str>, target: &Target) -> Result<Vec<Vec<String>>, CtlError> {
    let loc = locate(app, target, None)?;
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;

    fn rows_under(walker: &UITreeWalker, elem: &UIElement, depth: u32, out: &mut Vec<UIElement>) {
        if depth > MAX_DEPTH {
            return;
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            let role = role_of(&c);
            if role == "DataItem" || role == "ListItem" {
                out.push(c.clone());
            } else {
                rows_under(walker, &c, depth + 1, out);
            }
            child = walker.get_next_sibling(&c).ok();
        }
    }
    fn cells_under(walker: &UITreeWalker, elem: &UIElement, depth: u32, out: &mut Vec<String>) {
        if depth > MAX_DEPTH {
            return;
        }
        let role = role_of(elem);
        if role == "Text" || role == "Edit" {
            if let Some(n) = elem.get_name().ok().filter(|n| !n.is_empty()) {
                out.push(n);
            }
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            cells_under(walker, &c, depth + 1, out);
            child = walker.get_next_sibling(&c).ok();
        }
    }

    let mut row_elems = Vec::new();
    rows_under(&walker, &loc.element, 0, &mut row_elems);
    if row_elems.is_empty() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) contains no DataItem/ListItem rows",
                target.describe(),
                loc.role
            ),
        ));
    }
    let mut table = Vec::new();
    for r in &row_elems {
        let mut cells = Vec::new();
        cells_under(&walker, r, 0, &mut cells);
        if !cells.is_empty() {
            table.push(cells);
        }
    }
    Ok(table)
}

/// check_snapshot_freshness 的按模式入口:解析窗口后比对 RuntimeId。
pub fn check_snapshot_freshness_by_pattern(
    snapshot_id: &str,
    pattern: &str,
) -> Result<(), CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    check_snapshot_freshness(snapshot_id, &win)
}
