//! 输入动作前台快拒绝(2026-10-11,release-review-11 两轮 runner 被看门狗
//! 击杀的根因收口):Word 类 provider 在锚窗非前台时懒激活节流——同命令
//! 实测前台 ~2s / 后台 14-22.6s,全树 locate 里单次 COM 调用撞 20s UIA
//! 看门狗预算即击杀进程,flow 状态滞留 running。
//!
//! 守卫语义不变(type/press 本就要求目标窗前台,拒绝发生在 locate 之后的
//! set_focus+settle 检查),本模块把拒绝点提前到昂贵观察**之前**,并在
//! 拒绝前给两级激活机会(保留既有激活路径的成功面;用户空闲时前台锁
//! 无保护对象,激活即成功,流程自动继续):
//! ①原生 SetForegroundWindow(最小化先还原;进程有前台权时成功——
//!   如显示端授权点击后的窗口期内);
//! ②锚窗**单元素** UIA SetFocus(单次 COM 调用,不做树遍历;Word 实证
//!   可从后台激活);
//! ③仍非前台 → NOT_ACTIONABLE 让行(handoff::foreground_error 同款证据
//!   形态:expected/actual)——此刻让行是正确行为,OS 拒绝恰说明用户
//!   正在别处活跃。
//! 锚窗不存在时不抢答(NOT_FOUND 语义留给 locate 原路径);观察类命令
//! (find/snapshot/get/wait)不经本门——后台观察慢但合法,由调用方决定。

use actl_core::{CtlError, handoff};
use windows::Win32::UI::WindowsAndMessaging::{
    IsIconic, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

use crate::window::WindowEntry;

/// 锚匹配(纯逻辑,单测覆盖):`hwnd:<n>` 精确句柄,否则可见+标题子串。
fn anchor_matches(pattern: &str, w: &WindowEntry) -> bool {
    if let Some(raw) = pattern.strip_prefix("hwnd:") {
        return w
            .hwnd
            .is_some_and(|h| raw.parse::<isize>().is_ok_and(|n| n == h));
    }
    w.visible && w.title.contains(pattern)
}

/// 激活后沉降判定:任一锚匹配窗成为物理前台即通过(轮询,容忍激活过渡)。
fn settled(windows: &[WindowEntry], pattern: &str) -> bool {
    windows
        .iter()
        .any(|w| w.foreground && anchor_matches(pattern, w))
}

fn foreground_title(windows: &[WindowEntry]) -> String {
    windows
        .iter()
        .find(|w| w.foreground)
        .map(|w| w.title.clone())
        .unwrap_or_default()
}

/// type/press 输入动作的前台快门:锚窗非前台 → 尝试激活 → 仍非前台让行。
/// app=None(默认聚焦窗语义)直接放行,由既有守卫链负责。
pub fn ensure_input_foreground(app: Option<&str>) -> Result<(), CtlError> {
    let Some(pattern) = app else {
        return Ok(());
    };
    let windows = crate::native_windows::native_windows()?;
    let matched: Vec<&WindowEntry> = windows
        .iter()
        .filter(|w| anchor_matches(pattern, w))
        .collect();
    if matched.is_empty() {
        return Ok(()); // 锚不存在:NOT_FOUND 留给 locate 原路径如实上报
    }
    if matched.iter().any(|w| w.foreground) {
        return Ok(());
    }
    let target = matched[0];
    let Some(handle) = target.hwnd else {
        return Ok(()); // 无原生句柄(虚拟窗):回到原路径
    };
    // ①原生激活:最小化先还原,再抢前台。
    unsafe {
        let hwnd = windows::Win32::Foundation::HWND(handle as *mut _);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
    if settle_foreground(pattern) {
        return Ok(());
    }
    // ②单元素 UIA SetFocus:单次 COM 调用激活(不做树遍历,不进慢墙)。
    if let Ok(auto) = uiautomation::UIAutomation::new() {
        let hwnd = windows::Win32::Foundation::HWND(handle as *mut _);
        if let Ok(element) = auto.element_from_handle(uiautomation::types::Handle::from(hwnd)) {
            let _ = element.set_focus();
        }
    }
    if settle_foreground(pattern) {
        return Ok(());
    }
    let now = crate::native_windows::native_windows()?;
    Err(handoff::foreground_error(
        &target.title,
        &foreground_title(&now),
    ))
}

/// 激活过渡轮询:~1.2s/100ms 步进(Word 冷激活实测亚秒级;前台锁拒绝时
/// 两级尝试后即失败,不让空等放大延迟)。
fn settle_foreground(pattern: &str) -> bool {
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(windows) = crate::native_windows::native_windows()
            && settled(&windows, pattern)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, visible: bool, foreground: bool, hwnd: Option<isize>) -> WindowEntry {
        WindowEntry {
            title: title.into(),
            class: "TestClass".into(),
            pid: 1,
            foreground,
            topmost: false,
            minimized: false,
            visible,
            z_order: None,
            rect: None,
            hwnd,
            owner_title: None,
        }
    }

    #[test]
    fn anchor_title_substring_requires_visible() {
        let w = entry("文档1 - Word", true, false, Some(42));
        assert!(anchor_matches("Word", &w));
        let hidden = entry("文档1 - Word", false, false, Some(42));
        assert!(!anchor_matches("Word", &hidden));
        assert!(!anchor_matches("Excel", &w));
    }

    #[test]
    fn anchor_hwnd_exact_match() {
        let w = entry("任意标题", true, false, Some(4242));
        assert!(anchor_matches("hwnd:4242", &w));
        assert!(!anchor_matches("hwnd:4243", &w));
        assert!(!anchor_matches("hwnd:bad", &w));
    }

    #[test]
    fn settled_requires_foreground_anchor() {
        let bg = || entry("a - Word", true, false, Some(1));
        let fg = || entry("b - Word", true, true, Some(2));
        assert!(!settled(&[bg()], "Word"));
        assert!(settled(&[bg(), fg()], "Word"));
    }

    #[test]
    fn foreground_title_reports_front_window() {
        let bg = || entry("a", true, false, Some(1));
        let fg = || entry("ChatGPT", true, true, Some(2));
        assert_eq!(foreground_title(&[bg(), fg()]), "ChatGPT");
        assert_eq!(foreground_title(&[bg()]), "");
    }
}
