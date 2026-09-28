//! window —— 窗口域:发现(find_window 家族)、capture(DFS 遍历)、
//! snapshot 身份持久化与回放校验、窗口管理命令(focus/close/resize/list/wait)。
//! 叠放事实(foreground/z_order 等)混走 UIA + Win32 物理层,来源在条目内注明。

use actl_core::{CtlError, ErrorCode, UiNode};
use uiautomation::types::ControlType;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};

use crate::{internal, timing};

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
    pub element_identities: Vec<serde_json::Value>,
    pub truncated: bool,
    pub truncation_reasons: Vec<&'static str>,
    /// 激活感知等待的毫秒数(0 = 目标本就在响应;>0 = 目标曾挂起,已唤醒)
    pub wake_ms: u32,
}

impl CaptureResult {
    pub fn require_complete(&self) -> Result<(), CtlError> {
        if self.truncated {
            Err(crate::observation::incomplete(&self.truncation_reasons))
        } else {
            Ok(())
        }
    }
}

/// 捕获一个顶层窗口的 UIA 子树。
/// `app`:窗口标题子串;None = 焦点元素所在顶层窗口(spike #2:焦点链可能落在后台 UI)。
pub fn capture(app: Option<&str>) -> Result<CaptureResult, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;

    let target = {
        let _t = actl_core::trace::scope("capture.window");
        match app {
            Some(pattern) => find_window(&auto, &walker, pattern)?,
            None => top_level_of_focused(&auto, &walker)?,
        }
    };

    let window = WindowInfo {
        title: target
            .get_name()
            .map_err(|e| crate::read_channel::failure("window.name", e))?,
        class: target
            .get_classname()
            .map_err(|e| crate::read_channel::failure("window.class", e))?,
        pid: target
            .get_process_id()
            .map_err(|e| crate::read_channel::failure("window.pid", e))?,
    };
    let wake_ms = {
        let _t = actl_core::trace::scope("capture.wake");
        ensure_window_responsive(&target)
    };
    let window_runtime_id = target
        .get_runtime_id()
        .map_err(|e| crate::read_channel::failure("window.runtime_id", e))?;

    let mut nodes = Vec::new();
    let mut element_identities = Vec::new();
    let truncation_reasons = {
        let _t = actl_core::trace::scope("capture.walk");
        walk(
            &walker,
            &target,
            0,
            None,
            &mut nodes,
            &mut element_identities,
        )?
    };
    let truncated = !truncation_reasons.is_empty();
    Ok(CaptureResult {
        window,
        window_runtime_id,
        nodes,
        element_identities,
        truncated,
        truncation_reasons,
        wake_ms,
    })
}

// ─── snapshot_id 版本化-lite(M2:窗口身份绑定)──────────────────────────
/// @eN 绑定的是"快照那一刻的窗口实例"。跨命令重放时窗口可能已关闭重开
/// (同标题 ≠ 同实例),纯计数重放会静默点进新窗口的巧合位置。方案:快照
/// 落盘 {snapshot_id → 窗口 RuntimeId};回放命令带 `--snapshot <id>` 时
/// 校验当前窗口实例与快照一致,不一致 → STALE_REF。
/// 元素身份与同次 DFS ref 序列一并保存，由 ReplayGuard 在实际 locate 命中时核验。
const SNAPSHOT_KEEP: usize = 20;

fn snapshot_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let dir = std::path::Path::new(&base).join("actl").join("snapshots");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// 快照记录落盘(保留最近 SNAPSHOT_KEEP 份,按修改时间淘汰)。
pub fn persist_snapshot(
    id: &str,
    window_title: &str,
    window_runtime_id: &[i32],
    elements: &[serde_json::Value],
) -> Result<(), CtlError> {
    let dir = snapshot_dir().ok_or_else(|| crate::internal("snapshot directory unavailable"))?;
    let record = serde_json::json!({
        "snapshot_id": id,
        "window_title": window_title,
        "window_runtime_id": window_runtime_id,
        "elements": elements,
    });
    std::fs::write(
        dir.join(format!("{id}.json")),
        serde_json::to_vec(&record).map_err(crate::internal)?,
    )
    .map_err(crate::internal)?;
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
    Ok(())
}

pub(crate) fn load_snapshot_record(id: &str) -> Option<serde_json::Value> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let dir = snapshot_dir()?;
    let text = std::fs::read_to_string(dir.join(format!("{id}.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_snapshot(id: &str) -> Option<(String, Vec<i32>)> {
    let v = load_snapshot_record(id)?;
    let rid = v["window_runtime_id"]
        .as_array()?
        .iter()
        .map(|x| x.as_i64().and_then(|n| i32::try_from(n).ok()))
        .collect::<Option<Vec<_>>>()?;
    Some((v["window_title"].as_str()?.to_string(), rid))
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
    if !runtime_ids_match(&recorded, &now) {
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
/// 先用 WM_NULL 探针测目标是否在泵消息，再做有界等待(≤1.6s)。
/// 观察路径不再调用 ShowWindowAsync，避免用户尚未交接就被抢焦点。把 stall 变成一次有界的、
/// 可上报的等待。无原生句柄的元素(UIA-only)直接放行。
pub(crate) fn ensure_window_responsive(elem: &UIElement) -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL};

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
    // Read-only observation must not steal focus while the user is typing.
    // Explicit focus-window performs guarded restoration/activation instead.
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
pub(crate) fn window_title_of(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 256];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if len <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

pub(crate) fn native_hwnd(elem: &UIElement) -> Option<windows::Win32::Foundation::HWND> {
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
pub(crate) fn top_windows(auto: &UIAutomation, walker: &UITreeWalker) -> Vec<UIElement> {
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
        if !native_hwnd(&w).is_some_and(crate::display_support::is_display_window) {
            out.push(w.clone());
        }
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
pub(crate) fn find_window(
    auto: &UIAutomation,
    _walker: &UITreeWalker,
    pattern: &str,
) -> Result<UIElement, CtlError> {
    if pattern.starts_with("hwnd:") {
        return crate::scoped_locate::window(auto, Some(pattern));
    }
    // 桌面根整批枚举对无关窗口的创建/销毁竞态敏感,重试吸收瞬态(见 observation::desktop_children)
    let hosts = crate::observation::desktop_children(auto)?;
    let mut matches = Vec::new();
    for w in &hosts {
        if native_hwnd(w).is_some_and(crate::display_support::is_display_window) {
            continue;
        }
        if w.get_name()
            .map_err(|e| crate::read_channel::failure("window.name", e))?
            .contains(pattern)
        {
            matches.push(w.clone());
        }
    }
    if matches.is_empty() {
        for host in hosts {
            if native_hwnd(&host).is_some_and(crate::display_support::is_display_window) {
                continue;
            }
            for w in crate::observation::children(&host)? {
                if w.get_control_type()
                    .map_err(|e| crate::read_channel::failure("window.role", e))?
                    == ControlType::Window
                    && w.get_name()
                        .map_err(|e| crate::read_channel::failure("window.name", e))?
                        .contains(pattern)
                {
                    matches.push(w);
                }
            }
        }
    }
    // Required classification failures must not remove an ambiguity candidate.
    let classes: Vec<String> = matches
        .iter()
        .map(|w| {
            w.get_classname()
                .map_err(|e| crate::read_channel::failure("window.class", e))
        })
        .collect::<Result<_, _>>()?;
    // 折叠 WinUI 辅助 HWND:存在非辅助窗口时,辅助件不参与解析
    let has_primary = classes
        .iter()
        .any(|cls| !WINUI_AUX_CLASSES.contains(&cls.as_str()));
    let pool: Vec<UIElement> = matches
        .into_iter()
        .zip(classes)
        .filter(|(_, cls)| !has_primary || !WINUI_AUX_CLASSES.contains(&cls.as_str()))
        .map(|(element, _)| element)
        .collect();
    match pool.len() {
        0 => Err(CtlError::new(
            ErrorCode::NotFound,
            format!("no top-level window title contains {pattern:?}"),
        )),
        1 => pool
            .into_iter()
            .next()
            .ok_or_else(|| CtlError::internal("window candidate missing")),
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
pub(crate) fn top_level_of_focused(
    auto: &UIAutomation,
    walker: &UITreeWalker,
) -> Result<UIElement, CtlError> {
    let mut cur = auto.get_focused_element().map_err(internal)?;
    for _ in 0..=MAX_DEPTH {
        actl_core::wait_control::check()?;
        match crate::observation::parent(walker, &cur)? {
            Some(p) => cur = p,
            None => return Ok(cur),
        }
    }
    Err(crate::observation::incomplete(&["depth_limit"]))
}

fn walk(
    _walker: &UITreeWalker,
    elem: &UIElement,
    _depth: u32,
    _parent: Option<usize>,
    nodes: &mut Vec<UiNode>,
    identities: &mut Vec<serde_json::Value>,
) -> Result<Vec<&'static str>, CtlError> {
    let observed = crate::observation::tree(elem)?;
    for entry in observed.entries {
        let element = entry.element;
        let node = UiNode {
            depth: entry.depth,
            role: format!(
                "{:?}",
                element
                    .get_control_type()
                    .map_err(|e| crate::read_channel::failure("control_type", e))?
            ),
            name: Some(
                element
                    .get_name()
                    .map_err(|e| crate::read_channel::failure("name", e))?,
            ),
            automation_id: Some(
                element
                    .get_automation_id()
                    .map_err(|e| crate::read_channel::failure("automation_id", e))?,
            ),
            parent: entry.parent,
        };
        if actl_core::is_interactive_role(&node.role) {
            identities.push(crate::identity::observed_identity(&element)?);
        }
        nodes.push(node);
    }
    Ok(observed.reasons)
}
/// 等待标题包含 `substr` 的顶层窗口出现(--expect 的执行体,06 §4:默认 2s 轮询)。
pub fn wait_window(substr: &str, timeout_ms: u64) -> Result<String, CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        actl_core::wait_control::check()?;
        if let Some(window) = crate::native_windows()?
            .into_iter()
            .find(|w| w.visible && w.title.contains(substr))
        {
            return Ok(window.title);
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

/// focus-window:经交接后请求 UIA SetFocus，并有界核验原生前台句柄。
/// press/type 等键盘命令作用于全局焦点——多窗口场景必须先 focus-window(06 §3.6 裁定表)。
pub fn focus_window(pattern: &str) -> Result<String, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    let mut action = crate::feedback::window_action("focus-window", &win, true)?;
    crate::window_ready::activate(&win)?;
    action.delivered();
    Ok(win.get_name().unwrap_or_default())
}

/// list-windows:枚举顶层窗口 + owned dialog(title/class/pid),供 agent 选择目标。
/// list-windows 条目:title/class/pid 来自 UIA(与定位链同源),叠放事实
/// 来自 Win32 物理层——agent 据此回答"谁在最上面/是否被挡/弹窗挂在谁下面"。
#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub title: String,
    pub class: String,
    pub pid: u32,
    /// 物理前台(GetForegroundWindow 的根;注入护栏的同一判据)
    pub foreground: bool,
    /// WS_EX_TOPMOST("总在最前"弹窗:输入法悬浮/overlay 等)
    pub topmost: bool,
    pub minimized: bool,
    /// 注意:UWP 后台挂起时 CoreWindow 被 cloak,visible=false 属预期(不代表已死)
    pub visible: bool,
    /// Win32 Z 序(0 = 最顶);无原生句柄的窗口为 None
    pub z_order: Option<u32>,
    /// 窗口矩形 [left, top, right, bottom](虚拟桌面坐标,多屏可为负)
    pub rect: Option<[i32; 4]>,
    /// owned dialog 的宿主窗口标题(None = 顶层窗口)
    pub owner_title: Option<String>,
    /// 原生 HWND(十进制;close/focus-window --hwnd 的取值来源)
    pub hwnd: Option<isize>,
}

/// list-windows:枚举顶层窗口 + owned dialog,附物理层叠放事实。
/// Z 序 = GetTopWindow→GetWindow(GW_HWNDNEXT) 链的位次;前台比对沿用
/// 标题兜底先例(WinUI 三件套分属不同 HWND,严格比对会漏)。
pub fn list_windows() -> Result<Vec<WindowEntry>, CtlError> {
    use std::collections::HashMap;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GW_HWNDNEXT, GW_OWNER, GWL_EXSTYLE, GetAncestor, GetForegroundWindow,
        GetTopWindow, GetWindow, GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible,
        WS_EX_TOPMOST,
    };

    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    // rect 统一物理像素(PMv2 线程上下文):与 screenshot / xy: 坐标寻址同
    // 坐标系——视觉兜底换算("窗口左上角 + PNG 坐标")不再跨缩放混层
    // (2026-09-27 实测踩坑:逻辑 rect × 200% 缩放屏差点点错,守卫拦下)
    let _dpi = crate::capture::Pmv2Guard::enter();

    // Z 链一次取全:HWND → 位次(0 = 最顶)
    let mut z_rank: HashMap<usize, u32> = HashMap::new();
    unsafe {
        let null = HWND::default();
        let mut h = GetTopWindow(None).unwrap_or(null);
        let mut i = 0u32;
        while !h.0.is_null() {
            z_rank.insert(h.0 as usize, i);
            i += 1;
            h = GetWindow(h, GW_HWNDNEXT).unwrap_or(null);
        }
    }
    // 物理前台(根 + 标题双判据,WinUI 兜底同 pointer_physical 先例)
    let (fg_root, fg_title) = unsafe {
        let fg = GetForegroundWindow();
        let root = GetAncestor(fg, GA_ROOT);
        (
            if root.0.is_null() { fg } else { root },
            window_title_of(fg).unwrap_or_default(),
        )
    };

    let rank_of_root = |hwnd: HWND| -> Option<u32> {
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        let key = if root.0.is_null() { hwnd } else { root }.0 as usize;
        z_rank.get(&key).copied()
    };

    Ok(top_windows(&auto, &walker)
        .into_iter()
        .filter_map(|w| {
            let title = w.get_name().ok()?;
            if title.is_empty() {
                return None;
            }
            let hwnd = native_hwnd(&w);
            let (foreground, topmost, minimized, visible, z_order, rect, owner_title) = match hwnd {
                Some(h) => unsafe {
                    let root = GetAncestor(h, GA_ROOT);
                    let root = if root.0.is_null() { h } else { root };
                    let is_fg = root.0 == fg_root.0
                        || window_title_of(h)
                            .zip(Some(fg_title.clone()))
                            .is_some_and(|(t, f)| !f.is_empty() && t == f);
                    let ex_style = GetWindowLongW(h, GWL_EXSTYLE);
                    let mut r = RECT::default();
                    let has_rect = GetWindowRect(h, &mut r).is_ok();
                    let owner = GetWindow(h, GW_OWNER).unwrap_or_default();
                    (
                        is_fg,
                        (ex_style as u32) & WS_EX_TOPMOST.0 != 0,
                        IsIconic(h).as_bool(),
                        IsWindowVisible(h).as_bool(),
                        rank_of_root(h),
                        has_rect.then_some([r.left, r.top, r.right, r.bottom]),
                        (!owner.0.is_null())
                            .then(|| window_title_of(owner).unwrap_or_default())
                            .filter(|t| !t.is_empty()),
                    )
                },
                None => (false, false, false, true, None, None, None),
            };
            Some(WindowEntry {
                title,
                class: w.get_classname().unwrap_or_default(),
                pid: w.get_process_id().unwrap_or_default(),
                hwnd: hwnd.map(|h| h.0 as isize),
                foreground,
                topmost,
                minimized,
                visible,
                z_order,
                rect,
                owner_title,
            })
        })
        .collect())
}

/// close-window:优雅关闭标题匹配的唯一窗口(WM_CLOSE 异步投递;未保存内容
/// 由应用自行弹提示——语义关闭,不是强杀)。
pub fn close_window(pattern: &str) -> Result<String, CtlError> {
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let win = find_window(&auto, &walker, pattern)?;
    let mut action = crate::feedback::window_action("close-window", &win, false)?;
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
    action.delivered();
    Ok(title)
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
    let mut action = crate::feedback::window_action("resize-window", &win, false)?;
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
    action.delivered();
    Ok(title)
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

// ─── FR3 补缺:窗口按 PID/句柄定位(蓝图 FR3"支持按句柄/标题/PID 定位") ──

/// 窗口引用三形态(与 FR3 对齐;标题子串沿用既有语义,PID/HWND 精确)。
#[derive(Debug, Clone, Copy)]
pub enum WindowRef<'a> {
    Title(&'a str),
    Pid(u32),
    /// 原生 HWND(十进制或 0x 前缀,由 CLI 层解析)
    Hwnd(isize),
}

/// 按引用解析唯一顶层窗口:PID 多窗口 → AMBIGUOUS 附候选(与标题多匹配
/// 同语义);HWND 天然唯一。list-windows 的 pid/hwnd 字段是 agent 侧的来源。
pub fn resolve_window(r: WindowRef<'_>) -> Result<UIElement, CtlError> {
    let auto = UIAutomation::new().map_err(crate::internal)?;
    let walker = auto.create_tree_walker().map_err(crate::internal)?;
    match r {
        WindowRef::Title(pattern) => find_window(&auto, &walker, pattern),
        WindowRef::Pid(pid) => {
            let matches: Vec<UIElement> = top_windows(&auto, &walker)
                .into_iter()
                .filter(|w| w.get_process_id().map(|p| p == pid).unwrap_or(false))
                .collect();
            match matches.len() {
                0 => Err(CtlError::new(
                    ErrorCode::NotFound,
                    format!("no top-level window owned by pid {pid}"),
                )),
                1 => Ok(matches.into_iter().next().unwrap_or_else(|| unreachable!())),
                n => {
                    let titles: Vec<String> = matches
                        .iter()
                        .filter_map(|w| w.get_name().ok())
                        .take(5)
                        .collect();
                    Err(CtlError::with_evidence(
                        ErrorCode::Ambiguous,
                        format!("pid {pid} owns {n} windows; pass --hwnd or a title"),
                        serde_json::json!({ "candidates": titles }),
                    ))
                }
            }
        }
        WindowRef::Hwnd(raw) => {
            crate::display_support::reject_display(raw)?;
            // 特殊窗(任务栏 XAML 岛/TrayNotifyWnd 等)不在 UIA root 枚举里,
            // 也不必非有标题——ElementFromHandle 直取(From<isize> 现成转换)
            auto.element_from_handle(uiautomation::types::Handle::from(raw))
                .map_err(|_| {
                    CtlError::new(ErrorCode::NotFound, format!("no window at hwnd {raw:#x}"))
                })
        }
    }
}

/// 解析 --hwnd 参数(十进制或 0x 前缀十六进制)。
pub fn parse_hwnd(raw: &str) -> Result<isize, CtlError> {
    let t = raw.trim();
    let v = if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        isize::from_str_radix(hex, 16).ok()
    } else {
        t.parse::<isize>().ok()
    };
    v.filter(|v| *v != 0).ok_or_else(|| {
        CtlError::protocol(format!(
            "invalid --hwnd {raw:?}: expected decimal or 0x-prefixed hex"
        ))
    })
}

/// close-window 的按引用入口(FR3)。
pub fn close_window_by(r: WindowRef<'_>) -> Result<String, CtlError> {
    let win = resolve_window(r)?;
    let mut action = crate::feedback::window_action("close-window", &win, false)?;
    let title = win.get_name().unwrap_or_default();
    let Some(hwnd) = native_hwnd(&win) else {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("window {title:?} exposes no native handle"),
        ));
    };
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        )
    }
    .map_err(|e| CtlError::internal(format!("PostMessageW(WM_CLOSE) failed for {title:?}: {e}")))?;
    action.delivered();
    Ok(title)
}

/// focus-window 的按引用入口(FR3)。
pub fn focus_window_by(r: WindowRef<'_>) -> Result<String, CtlError> {
    let win = resolve_window(r)?;
    let mut action = crate::feedback::window_action("focus-window", &win, true)?;
    crate::window_ready::activate(&win)?;
    action.delivered();
    Ok(win.get_name().unwrap_or_default())
}

/// 轻量前台窗口查询(物理层,零 UIA 往返):标题 + 原生 HWND。
/// batch 步间弹窗闸门用——每步一次,必须便宜。
pub fn foreground_window() -> Option<(String, isize)> {
    let hwnd = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    window_title_of(hwnd).map(|t| (t, hwnd.0 as isize))
}

/// App Paths 解析(launch 的通用增强):裸程序名(WINWORD/notepad)不在 PATH 时,
/// 查注册表 `HKLM\...\App Paths\<name[.exe]>` 默认值取完整路径——Office/
/// 商店别名类应用的注册位置。仅在名字不含路径分隔符时尝试。
pub fn resolve_app_path(program: &str) -> Option<String> {
    use windows::Win32::System::Registry::{
        HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
    };
    use windows::core::PCWSTR;
    let bs = char::from_u32(0x5C)?; // U+005C 反斜杠(补丁工具转义规避,见 git 史)
    if program.contains(bs) || program.contains('/') {
        return None;
    }
    let lookup = |subkey: String| -> Option<String> {
        let wpath: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
        let mut hkey = Default::default();
        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(wpath.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        };
        if opened.is_err() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        let mut ty = REG_SZ;
        let ok = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR::null(), // 默认值
                None,
                Some(&mut ty),
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
            .is_ok()
        };
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        ok.then(|| {
            let end = (len as usize / 2).saturating_sub(1);
            String::from_utf16_lossy(&buf[..end]).trim().to_string()
        })
        .filter(|s| !s.is_empty())
    };
    let sep = std::path::MAIN_SEPARATOR_STR;
    let base = format!("SOFTWARE{sep}Microsoft{sep}Windows{sep}CurrentVersion{sep}App Paths");
    lookup(format!("{base}{sep}{program}")).or_else(|| lookup(format!("{base}{sep}{program}.exe")))
}

/// 元素的原生 HWND 公开入口(capture 窗口域用)
pub fn hwnd_of(elem: &UIElement) -> Option<windows::Win32::Foundation::HWND> {
    native_hwnd(elem)
}

/// capture 的按句柄入口(特殊窗可见性:Win11 任务栏 XAML 岛/溢出托盘等
/// 无标题窗口被 list-windows 的空标题过滤遮蔽,snapshot --hwnd 直指)。
pub fn capture_hwnd(hwnd_raw: isize) -> Result<CaptureResult, CtlError> {
    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;
    let target = resolve_window(WindowRef::Hwnd(hwnd_raw))?;
    let window = WindowInfo {
        title: target
            .get_name()
            .map_err(|e| crate::read_channel::failure("window.name", e))?,
        class: target
            .get_classname()
            .map_err(|e| crate::read_channel::failure("window.class", e))?,
        pid: target
            .get_process_id()
            .map_err(|e| crate::read_channel::failure("window.pid", e))?,
    };
    let wake_ms = ensure_window_responsive(&target);
    let window_runtime_id = target
        .get_runtime_id()
        .map_err(|e| crate::read_channel::failure("window.runtime_id", e))?;
    let mut nodes = Vec::new();
    let mut element_identities = Vec::new();
    let truncation_reasons = {
        let _t = actl_core::trace::scope("capture.walk");
        walk(
            &walker,
            &target,
            0,
            None,
            &mut nodes,
            &mut element_identities,
        )?
    };
    let truncated = !truncation_reasons.is_empty();
    Ok(CaptureResult {
        window,
        window_runtime_id,
        nodes,
        element_identities,
        truncated,
        truncation_reasons,
        wake_ms,
    })
}

fn runtime_ids_match(recorded: &[i32], current: &[i32]) -> bool {
    !recorded.is_empty() && recorded == current
}

#[cfg(test)]
mod identity_regression_tests {
    #[test]
    fn missing_window_identity_must_not_authorize_replay() {
        assert!(!super::runtime_ids_match(&[], &[]));
        assert!(!super::runtime_ids_match(&[], &[42]));
    }
}
