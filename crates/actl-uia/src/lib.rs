//! actl-uia —— Windows UIA 后端。
//!
//! 全仓库唯一接触 COM 的 crate(AGENTS.md §5.5)。
//! snapshot/定位/表格通过 fallible RawView 枚举区分空树与观察失败。
//! 窗口清单、身份基线和 PID 定位同样传播枚举与必要属性错误。
//! 性能注记(spike #3):本版逐属性跨进程读取,release 下金任务实测,超 500ms 再上 cache request。
//!
//! 模块划分(按命令域,单文件行数门禁见 xtask gate):
//! `window` 窗口发现/capture/快照/窗口管理 · `locate` 定位链 L1–L4 与元素等待 ·
//! `action` 语义动作(click/type/press/set-value)· `pointer` 物理指针 ·
//! `read` 读取与断言(get/verify/extract/剪贴板)· `input`/`kbd`/`mouse` 输入底座。
//! 公共 API 经 `pub use` 汇聚于 crate 根,crate 外路径不变。

#![allow(unsafe_code)] // 豁免点(AGENTS.md §5.5):COM/SendInput/Win32 直调集中于本 crate

mod action;
mod bg_style;
#[allow(unsafe_op_in_unsafe_fn)] // GDI/WIC 直调集中在该模块
mod capture;
pub mod desktop;
pub mod display_gate;
pub mod display_support;
mod feedback;
mod handoff;
mod identity;
mod idle;
mod input_verify;
mod locate;
pub use identity::ReplayGuard;
mod native_windows;
mod observation;
mod pointer;
pub mod preview;
#[allow(unsafe_op_in_unsafe_fn)] // Temporary no-activate preview windows and GDI resources.
mod preview_window;
mod read;
mod read_channel;
mod scoped_locate;
mod table;
pub use table::{TableResult, extract_table};
mod window;
mod window_observation;
mod window_ready;
pub use window_ready::wait_window_ready;

pub mod input;
pub mod scoped_snapshot;
pub use window::persist_snapshot_scoped;
pub mod kbd;
pub mod mouse;
pub use capture::{ShotResult, screenshot_element, screenshot_rect, screenshot_window};
pub use native_windows::{native_window, native_windows};

pub use action::{
    InputResult, TypeOutcome, click, paste, press_half, press_half_with_direction, press_keys,
    press_keys_targeted, select_option, set_value, toggle_element, type_text,
};
pub use idle::{idle_ms, wait_idle};
pub use locate::{
    Located, StableOutcome, WaitPredicate, WaitProp, locate, wait_element, wait_gone,
    wait_property, wait_stable,
};
pub use pointer::{
    CoordOutcome, PointerAction, click_physical, drag_physical, pointer_at_physical,
    pointer_physical, scroll_element,
};
pub use read::{
    PROPS, PropertyInfo, VerifyKind, VerifyOutcome, get_clipboard_text, read_property,
    set_clipboard_text, verify,
};
pub use window::{
    CaptureResult, MAX_DEPTH, MAX_ELEMENTS, WindowEntry, WindowInfo, WindowRef, capture,
    capture_hwnd, check_snapshot_freshness, check_snapshot_freshness_by_pattern, close_window,
    close_window_by, focus_window, focus_window_by, foreground_window, list_windows, parse_hwnd,
    persist_snapshot, resize_window, resolve_app_path, wait_new_window, wait_window,
    window_identities,
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
