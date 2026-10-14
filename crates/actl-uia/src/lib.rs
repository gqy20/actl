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
mod collect;
pub mod desktop;
pub mod diagnostic;
pub mod display_gate;
pub mod display_support;
mod feedback;
#[allow(unsafe_op_in_unsafe_fn)] // WIC PNG 解码直调集中在该模块(与 capture 同栈)
pub mod find_image;
/// 输入动作前台快拒绝(2026-10-11):锚窗非前台时 provider 懒激活节流,
/// 昂贵 locate 前先廉价原生核对+两级激活,仍非前台让行。
mod foreground_gate;
mod handoff;
/// image-regions:盲区几何提议器(方差网格+连通域,零语义;算法核在 core)。
pub mod image_regions;
/// preview-map:全窗组件画框(先看见再设计;layered overlay 多 rect 绘制)。
#[allow(unsafe_op_in_unsafe_fn)]
pub mod map;
/// 自身进程资源快照(诊断旁路;trace 消费,不进信封)。
pub mod resource;
/// find-image 搜索范围可视化(2026-10-14):红框画在目标窗口面,Drop 擦除。
#[allow(unsafe_op_in_unsafe_fn)]
mod search_frame;
/// verify-visual:像素对比断言(verify 链视觉末端;算法核在 core::visual_diff)。
pub mod visual_verify;
/// 流程执行器注入计划指纹的窄通道(始终信任此流程)。
pub fn set_plan_hash(hash: Option<String>) {
    handoff::set_plan_hash(hash);
}
mod identity;
mod idle;
mod input_verify;
mod locate;
pub use identity::ReplayGuard;
mod clipboard_bank;
pub mod evidence;
mod native_windows;
mod observation;
mod observation_diagnostics;
pub mod observation_metadata;
mod observation_query;
mod pointer;
pub mod preview;
#[allow(unsafe_op_in_unsafe_fn)] // Temporary no-activate preview windows and GDI resources.
mod preview_window;
mod read;
/// GUI 操作录制会话捕获层(docs/06 R1;钩子+反查+trace)。
pub mod record;
pub use clipboard_bank::{clipboard_formats, clipboard_restore, clipboard_save};
pub use read::focus_report;
/// 分离式子进程派生前的 stdio 继承守卫(浏览器通道等长命子进程;docs/72)。
pub mod process_io;
mod read_channel;
mod scoped_locate;
mod table;
pub use table::{TableResult, extract_table};
/// UIA 调用看门狗(有界 COM 调用;docs/81 §8 僵尸事故根治)。
pub mod watchdog;
mod window;
mod window_geometry;
pub use window::front_report;
pub use window::residue_report;
pub mod window_check;
mod window_metadata;
mod window_observation;
mod window_ready;
pub use window_ready::wait_window_ready;

pub mod app_locate;
pub mod app_registry;
pub mod input;
pub mod scoped_snapshot;
pub use window::persist_snapshot_scoped;
pub mod kbd;
pub mod mouse;
pub use capture::{
    ShotResult, ensure_process_pmv2, screenshot_element, screenshot_rect, screenshot_screen,
    screenshot_window, screenshot_window_rect,
};
pub use collect::{CollectOptions, CollectReport, collect};
pub use native_windows::{monitors, native_window, native_windows};

pub use action::{
    InputResult, TypeOutcome, click, focus_element, paste, press_half, press_half_with_direction,
    press_keys, press_keys_targeted, select_option, set_value, toggle_element, type_text,
};
pub use idle::{idle_ms, wait_idle};
pub use locate::{
    Located, ResolvedInfo, StableOutcome, WaitPredicate, WaitProp, find_by_id_provider, locate,
    wait_element, wait_gone, wait_property, wait_stable,
};
pub use pointer::{
    CoordOutcome, DragPathOutcome, PointerAction, PointerSpec, click_physical, drag_path_physical,
    drag_physical, pointer_at_physical, pointer_at_window, pointer_physical, scroll_element,
};
pub use read::{
    ElementRect, PROPS, PropertyInfo, SelectionOutcome, VerifyKind, VerifyOutcome,
    get_clipboard_text, get_selection, read_property, set_clipboard_text, verify,
};
pub use window::{
    CaptureResult, MAX_DEPTH, MAX_ELEMENTS, WindowEntry, WindowInfo, WindowRef, capture,
    capture_hwnd, check_snapshot_freshness, check_snapshot_freshness_by_pattern, close_window,
    close_window_by, focus_window, focus_window_by, foreground_window, list_windows, parse_hwnd,
    persist_snapshot, resolve_app_path, wait_new_window, wait_window, window_identities,
};
pub use window_geometry::{ResizeOutcome, resize_window_rect};

use actl_core::{CtlError, ErrorCode};

/// 全 crate 共用的时序参数(各模块懒加载点统一走这里)。
pub(crate) fn timing() -> &'static actl_core::timing::Timing {
    actl_core::timing::Timing::load()
}

/// COM/UIA 调用错误的统一 Internal 包装。
pub(crate) fn internal(e: impl std::fmt::Debug) -> CtlError {
    CtlError::new(ErrorCode::Internal, format!("{e:?}"))
}
