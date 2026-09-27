//! actl-uia —— Windows UIA 后端。
//!
//! 全仓库唯一接触 COM 的 crate(AGENTS.md §5.5)。
//! 已知限制(spike #4,docs/spike-findings.md):walker 在"无子/无兄弟"时返回 Err,
//! 与真错误不可区分——当前以 `.ok()` 视为尽头;HRESULT 分类列入 M1 遗留。
//! 性能注记(spike #3):本版逐属性跨进程读取,release 下金任务实测,超 500ms 再上 cache request。
//!
//! 模块划分(按命令域,单文件行数门禁见 xtask gate):
//! `window` 窗口发现/capture/快照/窗口管理 · `locate` 定位链 L1–L4 与元素等待 ·
//! `action` 语义动作(click/type/press/set-value)· `pointer` 物理指针 ·
//! `read` 读取与断言(get/verify/extract/剪贴板)· `input`/`kbd`/`mouse` 输入底座。
//! 公共 API 经 `pub use` 汇聚于 crate 根,crate 外路径不变。

#![allow(unsafe_code)] // 豁免点(AGENTS.md §5.5):COM/SendInput/Win32 直调集中于本 crate

mod action;
#[allow(unsafe_op_in_unsafe_fn)] // GDI/WIC 直调集中在该模块
mod capture;
mod locate;
mod pointer;
mod read;
mod window;

pub mod input;
pub mod kbd;
pub mod mouse;
pub use capture::{ShotResult, screenshot_element, screenshot_rect, screenshot_window};

pub use action::{
    InputResult, TypeOutcome, click, paste, press_half, press_keys, select_option, set_value,
    toggle_element, type_text,
};
pub use locate::{
    Located, WaitPredicate, WaitProp, locate, wait_element, wait_gone, wait_property,
};
pub use pointer::{PointerAction, click_physical, drag_physical, pointer_physical, scroll_element};
pub use read::{
    PROPS, PropertyInfo, VerifyKind, VerifyOutcome, extract_table, get_clipboard_text,
    read_property, set_clipboard_text, verify,
};
pub use window::{
    CaptureResult, MAX_DEPTH, MAX_ELEMENTS, WindowEntry, WindowInfo, WindowRef, capture,
    check_snapshot_freshness, check_snapshot_freshness_by_pattern, close_window, close_window_by,
    focus_window, focus_window_by, foreground_window, list_windows, parse_hwnd, persist_snapshot,
    resize_window, resolve_app_path, wait_new_window, wait_window, window_identities,
};

use actl_core::{CtlError, ErrorCode};

/// 全 crate 共用的时序参数(各模块懒加载点统一走这里)。
pub(crate) fn timing() -> &'static actl_core::timing::Timing {
    actl_core::timing::Timing::load()
}

/// COM/UIA 调用错误的统一 Internal 包装。
pub(crate) fn internal(e: impl std::fmt::Debug) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("{e:?}"))
}
