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
        "tree" => dump_tree(&auto, args.get(1).map(String::as_str), args.get(2)),
        _ => {
            eprintln!("modes: windows | focused | tree <pid|name-substr> [max_depth]");
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
