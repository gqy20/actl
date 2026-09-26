//! M0 spike probe —— UIA 冒险探测器(docs/ROADMAP.md Phase 0)
//!
//! 这是**一次性人类诊断工具**(examples 不受 envelope 契约约束,AGENTS 硬规则 1 限命令层):
//!
//! ```bash
//! cargo run -p actl-uia --example probe -- windows          # 枚举顶层窗口(读不出的 ≈ UIPI 墙)
//! cargo run -p actl-uia --example probe -- focused          # 当前焦点元素全属性(中文/编码观察)
//! cargo run -p actl-uia --example probe -- tree <pid|名称子串> [max_depth]   # dump 树 + 统计
//! ```
//!
//! 五场景对照:① Electron=tree 对 VSCode;② UIPI=windows 的 UNREADABLE 项;
//! ③ 中文=tree 对系统应用;④ Canvas=tree 对 mspaint;⑤ RDP=需会话环境,本机默认跳过。
//!
//! 遍历约定:COM walker 在"无子/无兄弟"时返回 Err,探测工具将其视为尽头(可能吞掉真错误,
//! 生产实现必须区分错误类型——这是坑清单要记录的点之一)。

use std::collections::HashMap;

use uiautomation::types::ControlType;
use uiautomation::{Result, UIAutomation, UIElement, UITreeWalker};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect(); // argv[0]=probe.exe,其后即模式
    let mode = args.first().map(String::as_str).unwrap_or("help");
    let auto = UIAutomation::new().expect("CoInitialize/IUIAutomation init");

    let result = match mode {
        "windows" => list_windows(&auto),
        "focused" => show_focused(&auto),
        "fginfo" => {
            fginfo::run();
            Ok(())
        }
        "tree" => dump_tree(&auto, args.get(1).map(String::as_str), args.get(2)),
        "ic" => item_container(
            &auto,
            args.get(1).map(String::as_str),
            args.get(2).map(String::as_str),
        ),
        _ => {
            eprintln!(
                "modes: windows | focused | tree <pid|name-substr> [max_depth] | ic <window-substr> <name>"
            );
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("✗ {mode} failed: {e:?}");
        std::process::exit(1);
    }
}

/// 场景②:枚举顶层窗口;读不出 Name 的窗口 ≈ 跨完整性级别(UIPI)证据。
fn list_windows(auto: &UIAutomation) -> Result<()> {
    let walker = auto.create_tree_walker()?;
    let root = auto.get_root_element()?;
    let mut total = 0;
    let mut unreadable = 0;
    let mut child = walker.get_first_child(&root).ok();
    while let Some(w) = child {
        let pid = w.get_process_id().unwrap_or_default();
        let class = w.get_classname().unwrap_or_default();
        // 读取即探测:UIPI 阻隔时属性读取报错
        let name = match w.get_name() {
            Ok(n) if !n.is_empty() => n,
            Ok(_) => "(empty name)".to_string(),
            Err(_) => {
                unreadable += 1;
                "«UNREADABLE — UIPI/权限墙?»".to_string()
            }
        };
        println!("pid={pid:<7} class={class:<28} {name}");
        total += 1;
        child = walker.get_next_sibling(&w).ok();
    }
    println!("--- windows={total} unreadable={unreadable}(后者多为更高完整性级别进程) ---");
    Ok(())
}

/// 场景③:焦点元素全属性 —— 观察中文 Name、AutomationId、RuntimeId 形态。
fn show_focused(auto: &UIAutomation) -> Result<()> {
    let f = auto.get_focused_element()?;
    println!("name          = {:?}", f.get_name());
    println!("class         = {:?}", f.get_classname());
    println!("control_type  = {:?}", f.get_control_type());
    println!("automation_id = {:?}", f.get_automation_id());
    println!("runtime_id    = {:?}", f.get_runtime_id());
    println!("process_id    = {}", f.get_process_id()?);
    println!("offscreen     = {:?}", f.is_offscreen());
    Ok(())
}

struct Stats {
    total: usize,
    interactive: usize,
    empty_name: usize,
    max_seen_depth: usize,
    by_type: HashMap<String, usize>,
}

/// 场景①③④:dump 指定窗口子树。目标按 pid 或名称子串匹配首个顶层窗口。
fn dump_tree(auto: &UIAutomation, target: Option<&str>, max_depth: Option<&String>) -> Result<()> {
    let Some(target) = target else {
        eprintln!("tree 需要目标:pid 数字或名称子串");
        return Ok(());
    };
    let max_depth: usize = max_depth.and_then(|s| s.parse().ok()).unwrap_or(12);

    let walker = auto.create_tree_walker()?;
    let root = auto.get_root_element()?;
    let mut child = walker.get_first_child(&root).ok();
    let start = loop {
        let Some(w) = child else {
            eprintln!("未找到匹配窗口:{target}");
            return Ok(());
        };
        let hit = match target.parse::<u32>() {
            Ok(pid) => w.get_process_id().map(|p| p == pid).unwrap_or(false),
            Err(_) => w.get_name().map(|n| n.contains(target)).unwrap_or(false),
        };
        if hit {
            break w;
        }
        child = walker.get_next_sibling(&w).ok();
    };

    println!(
        "target: name={:?} class={:?} pid={}",
        start.get_name().unwrap_or_default(),
        start.get_classname().unwrap_or_default(),
        start.get_process_id().unwrap_or_default()
    );
    let mut stats = Stats {
        total: 0,
        interactive: 0,
        empty_name: 0,
        max_seen_depth: 0,
        by_type: HashMap::new(),
    };
    let mut printed = 0;
    walk(&walker, &start, 0, max_depth, &mut stats, &mut printed);
    println!("\n=== stats(depth≤{max_depth}) ===");
    println!(
        "total={} interactive={} empty_name={} real_depth={}",
        stats.total, stats.interactive, stats.empty_name, stats.max_seen_depth
    );
    let mut types: Vec<_> = stats.by_type.into_iter().collect();
    types.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (t, n) in types.iter().take(15) {
        println!("  {t:<18} ×{n}");
    }
    Ok(())
}

const INTERACTIVE: &[&str] = &[
    "Button",
    "CheckBox",
    "RadioButton",
    "ComboBox",
    "Edit",
    "ListItem",
    "MenuItem",
    "TabItem",
    "Hyperlink",
    "ToggleButton",
    "DataGrid",
    "Spinner",
    "Slider",
    "Document",
];

fn walk(
    walker: &UITreeWalker,
    elem: &UIElement,
    depth: usize,
    max_depth: usize,
    stats: &mut Stats,
    printed: &mut usize,
) {
    stats.total += 1;
    stats.max_seen_depth = stats.max_seen_depth.max(depth);
    let ty = format!(
        "{:?}",
        elem.get_control_type().unwrap_or(ControlType::Custom)
    );
    *stats.by_type.entry(ty.clone()).or_default() += 1;
    let name = elem.get_name().unwrap_or_default();
    if name.is_empty() {
        stats.empty_name += 1;
    }
    if INTERACTIVE.contains(&ty.as_str()) {
        stats.interactive += 1;
    }
    if *printed < 220 && depth <= max_depth {
        let aid = elem.get_automation_id().unwrap_or_default();
        let shown = if name.is_empty() {
            "·".to_string()
        } else {
            name
        };
        println!("{}{}[{aid}] {shown}", "  ".repeat(depth), ty);
        *printed += 1;
    }
    if depth >= max_depth {
        return;
    }
    let mut child = walker.get_first_child(elem).ok();
    while let Some(c) = child {
        walk(walker, &c, depth + 1, max_depth, stats, printed);
        child = walker.get_next_sibling(&c).ok();
    }
}

// fginfo:物理层焦点诊断(GetForegroundWindow + GetGUIThreadInfo.hwndFocus)
// fginfo:物理层焦点诊断(GetForegroundWindow + GetGUIThreadInfo.hwndFocus 的真实类名)
mod fginfo {
    #![allow(unsafe_code)] // 诊断工具,Win32 直调

    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowTextW,
    };

    pub fn run() {
        unsafe {
            let fg = GetForegroundWindow();
            let mut buf = [0u16; 256];
            GetWindowTextW(fg, &mut buf);
            println!("foreground : {}", String::from_utf16_lossy(&buf));
            let mut class = [0u16; 256];
            GetClassNameW(fg, &mut class);
            println!("fg class   : {}", String::from_utf16_lossy(&class));

            let mut info = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // 0 = 前台线程
            if GetGUIThreadInfo(0, &mut info).is_ok() {
                let mut cbuf = [0u16; 256];
                let n = GetClassNameW(info.hwndFocus, &mut cbuf);
                println!(
                    "focus hwnd : {} class={}",
                    info.hwndFocus.0 as usize,
                    String::from_utf16_lossy(&cbuf[..n.max(0) as usize])
                );
            } else {
                println!("GetGUIThreadInfo failed");
            }
        }
    }
}

#[allow(unsafe_code)] // 诊断工具:原生 COM 直调 FindItemByProperty
/// 虚拟化列表探测(P0 spike):验证目标窗口内
/// ① 有哪些元素实现 ItemContainerPattern;
/// ② 原生 FindItemByProperty(NULL 起查,Name 精确匹配)能否命中未物化条目;
/// ③ 普通 walker 遍历是否看不见该条目(复现虚拟化缺口)。
///
/// 起查元素传 NULL 是 MS 文档语义("从头查"),uiautomation crate 的安全包装
/// 不接受空 UIElement,故经 UIElement::as_ref() 拿原生 IUIAutomationElement
/// 直调 GetCurrentPattern + FindItemByProperty。
fn item_container(auto: &UIAutomation, window: Option<&str>, needle: Option<&str>) -> Result<()> {
    use uiautomation::patterns::UIItemContainerPattern;
    use uiautomation::variants::Variant;
    use windows::Win32::UI::Accessibility::{
        IUIAutomationItemContainerPattern, UIA_ItemContainerPatternId, UIA_NamePropertyId,
    };
    use windows::core::Interface;

    let (Some(window), Some(needle)) = (window, needle) else {
        eprintln!("ic 需要参数:<window-substr> <name-to-find>");
        return Ok(());
    };
    let walker = auto.create_tree_walker()?;
    let root = auto.get_root_element()?;
    let mut child = walker.get_first_child(&root).ok();
    let start = loop {
        let Some(w) = child else {
            eprintln!("未找到匹配窗口:{window}");
            return Ok(());
        };
        if w.get_name().map(|n| n.contains(window)).unwrap_or(false) {
            break w;
        }
        child = walker.get_next_sibling(&w).ok();
    };
    println!(
        "window: {:?} class={:?} pid={}",
        start.get_name().unwrap_or_default(),
        start.get_classname().unwrap_or_default(),
        start.get_process_id().unwrap_or_default()
    );

    // ① + ③:一遍 DFS——记录 ItemContainer 容器,同时数普通遍历能否看到 needle
    let mut containers: Vec<UIElement> = Vec::new();
    let mut realized_hits = 0usize;
    let mut total = 0usize;
    fn dfs(
        walker: &UITreeWalker,
        elem: &UIElement,
        depth: usize,
        needle: &str,
        containers: &mut Vec<UIElement>,
        realized_hits: &mut usize,
        total: &mut usize,
    ) {
        if depth > 40 || *total > 5000 {
            return;
        }
        *total += 1;
        let name = elem.get_name().unwrap_or_default();
        if name.contains(needle) {
            *realized_hits += 1;
        }
        if elem.get_pattern::<UIItemContainerPattern>().is_ok() {
            containers.push(elem.clone());
        }
        let mut c = walker.get_first_child(elem).ok();
        while let Some(x) = c {
            dfs(
                walker,
                &x,
                depth + 1,
                needle,
                containers,
                realized_hits,
                total,
            );
            c = walker.get_next_sibling(&x).ok();
        }
    }
    dfs(
        &walker,
        &start,
        0,
        needle,
        &mut containers,
        &mut realized_hits,
        &mut total,
    );
    println!(
        "walker: total={total} containers={} realized_name_hits={realized_hits}",
        containers.len()
    );
    for c in containers.iter().take(5) {
        println!(
            "  container: {:?} [{:?}] class={:?}",
            c.get_name().unwrap_or_default(),
            c.get_control_type()
                .map(|t| format!("{t:?}"))
                .unwrap_or_default(),
            c.get_classname().unwrap_or_default()
        );
    }

    // ②:对每个容器做原生 FindItemByProperty(NULL, Name, needle)
    let value = Variant::from(needle.to_string());
    for c in &containers {
        let raw = c.as_ref();
        let pattern: IUIAutomationItemContainerPattern =
            match (|| unsafe { raw.GetCurrentPattern(UIA_ItemContainerPatternId)?.cast() })() {
                Ok(p) => p,
                Err(e) => {
                    println!("  ! GetCurrentPattern failed: {e:?}");
                    continue;
                }
            };
        let hit = unsafe { pattern.FindItemByProperty(None, UIA_NamePropertyId, value.as_ref()) };
        match hit.map(UIElement::from) {
            Ok(elem) => {
                println!(
                    "  ✓ FindItemByProperty HIT: name={:?} role={:?} class={:?} offscreen={:?}",
                    elem.get_name().unwrap_or_default(),
                    elem.get_control_type()
                        .map(|t| format!("{t:?}"))
                        .unwrap_or_default(),
                    elem.get_classname().unwrap_or_default(),
                    elem.is_offscreen().unwrap_or(true)
                );
            }
            Err(e) => println!("  ✗ no hit: {e:?}"),
        }
    }
    Ok(())
}
