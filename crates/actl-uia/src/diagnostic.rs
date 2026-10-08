//! 失败时刻的上下文汇流(2026-10-06 基建):诊断靠错误自述,不靠人回溯输出。
//! Word 兼容模式文档整轮根因的教训——答案原本就在窗口标题里,但从不进
//! 错误通道。本模块只做**纯本地/原生**采集(UIA 失败时不能再依赖 UIA):
//! OS 前台窗 + 标题语义分类(兼容模式/脏星号/最小化态迹象)。
use serde_json::{Value, json};

// 当前观察目标窗标题(capture 解析窗口后设置;单命令进程内生命周期)。
// (thread_local! 宏调用不携带文档注释——rustdoc 不为其生成文档)
thread_local! {
    static OBSERVATION_TARGET: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// capture/树扫描入口调用:登记本次观察的目标窗标题(失败时上下文汇流用)。
pub fn set_observation_target(title: &str) {
    OBSERVATION_TARGET.with(|t| *t.borrow_mut() = Some(title.to_string()));
}

/// 失败现场上下文:前台窗标题/类 + 观察目标窗 + 各自标题语义分类。
/// 前台两次原生调用零 UIA;目标窗来自登记(无额外调用)。
pub fn failure_context() -> Value {
    let foreground = foreground_window();
    let title = foreground
        .as_ref()
        .map(|(t, _)| t.as_str())
        .unwrap_or_default();
    let target = OBSERVATION_TARGET
        .with(|t| t.borrow().clone())
        .unwrap_or_default();
    json!({
        "foreground_window": {
            "title": title,
            "class": foreground.as_ref().map(|(_, c)| c.as_str()).unwrap_or(""),
            "flags": classify_title(title),
        },
        "observation_target": {
            "title": target,
            "flags": classify_title(&target),
        },
    })
}

/// 标题语义分类:已知的「需要人工注意」信号——兼容模式(Office 文档类,
/// 元素 RuntimeId 常为空,2026-10-06 Word 根因)、脏星号(未保存缓冲)。
fn classify_title(title: &str) -> Vec<&'static str> {
    let mut flags = Vec::new();
    if title.contains("兼容模式") || title.contains("Compatibility Mode") {
        flags.push("compat_mode");
    }
    if title.starts_with('*') {
        flags.push("dirty_buffer");
    }
    if flags.is_empty() {
        flags.push("none");
    }
    flags
}

/// 前台窗 (title, class):原生 GetForegroundWindow + 查询——不依赖 UIA
/// (观察失败时 UIA 通道本身不可信)。
fn foreground_window() -> Option<(String, String)> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let title = window_text(hwnd.0 as isize)?;
    let class = window_class(hwnd.0 as isize)?;
    Some((title, class))
}

fn window_text(hwnd: isize) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextLengthW;
    let hwnd = windows::Win32::Foundation::HWND(hwnd as *mut _);
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return Some(String::new());
    }
    let mut buf = vec![0u16; len as usize + 1];
    let copied = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf) };
    if copied <= 0 {
        return Some(String::new());
    }
    Some(String::from_utf16_lossy(&buf[..copied as usize]))
}

fn window_class(hwnd: isize) -> Option<String> {
    let mut buf = [0u16; 256];
    let copied = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetClassNameW(
            windows::Win32::Foundation::HWND(hwnd as *mut _),
            &mut buf,
        )
    };
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..copied as usize]))
}
