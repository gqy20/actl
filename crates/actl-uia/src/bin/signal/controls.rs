//! Explicit, focusable control window using standard Windows accessible controls.
use super::{flow_ui, model, paint};
use std::sync::OnceLock;
thread_local! {
    static EXPANDED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LOCAL_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}
static BOUND_FLOW: OnceLock<Option<String>> = OnceLock::new();
use actl_core::state::{SignalPaths, unix_ms};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{EM_GETFIRSTVISIBLELINE, EM_GETSEL, EM_LINESCROLL, EM_SETSEL},
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

const CONTROLS: &[(i32, &str, i32, i32, i32, i32)] = &[
    // 头行:标题与状态小字同一行(小字垂直居中对齐标题行);
    // 底行:紧急停止/修复 · 快捷键提示 · 详细信息 单行排布(2026-09-30 重排)。
    (102, "桌面状态", 24, 20, 160, 32),
    (16, "最小化", 448, 20, 40, 32),
    (15, "关闭", 496, 20, 40, 32),
    (103, "正在读取状态", 192, 25, 248, 22),
    (100, "正在读取流程状态", 24, 58, 512, 160),
    (101, "", 24, 272, 512, 44),
    (10, "暂停", 24, 330, 148, 32),
    (11, "继续", 24, 330, 148, 32),
    (14, "详细信息", 388, 388, 148, 32),
    (104, "Ctrl+Alt+F12", 184, 393, 200, 22),
    (12, "紧急停止", 24, 388, 148, 32),
    (13, "修复运行状态", 24, 388, 148, 32),
];
unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| paint::pixels(n, dpi);
    let compact = if EXPANDED.with(|v| v.get()) { 0 } else { 80 };
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        px(560),
        px(444 - compact),
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    for &(id, _, x, y, width, height) in CONTROLS {
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
            let height = if id == 100 {
                (if has_feedback(hwnd) { 160 } else { 216 }) - compact
            } else {
                height
            };
            let y = if y >= 272 { y - compact } else { y };
            let _ = SetWindowPos(
                child,
                None,
                px(x),
                px(y),
                px(width),
                px(height),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
    super::panel::fonts(hwnd, dpi, &CONTROLS.iter().map(|c| c.0).collect::<Vec<_>>());
    if let Ok(edit) = GetDlgItem(Some(hwnd), 100) {
        let rect = RECT {
            left: px(14),
            top: px(12),
            right: px(484),
            bottom: px((if has_feedback(hwnd) { 148 } else { 204 }) - compact),
        };
        SendMessageW(
            edit,
            windows::Win32::UI::Controls::EM_SETRECT,
            None,
            Some(LPARAM((&rect as *const RECT) as isize)),
        );
        super::panel::scroll(edit);
    }
}

fn current() -> Option<(String, String)> {
    let paths = SignalPaths::default();
    let id = BOUND_FLOW
        .get_or_init(|| {
            let args: Vec<_> = std::env::args().collect();
            args.windows(2)
                .find(|a| a[0] == "--flow")
                .map(|a| a[1].clone())
                .filter(|id| actl_core::history::valid_id(id))
                .or_else(|| flow_ui::discover(&paths))
        })
        .as_ref()?;
    let state = flow_ui::read(&paths, id)?;
    Some((id.clone(), state["status"].as_str()?.into()))
}
unsafe fn set_text(hwnd: HWND, id: i32, text: &str) {
    let Ok(child) = GetDlgItem(Some(hwnd), id) else {
        return;
    };
    let mut old = vec![0u16; GetWindowTextLengthW(child).max(0) as usize + 1];
    let len = GetWindowTextW(child, &mut old).max(0) as usize;
    if String::from_utf16_lossy(&old[..len]) != text {
        let (mut start, mut end) = (0u32, 0u32);
        let first = SendMessageW(child, EM_GETFIRSTVISIBLELINE, None, None);
        SendMessageW(
            child,
            EM_GETSEL,
            Some(WPARAM((&mut start as *mut u32) as usize)),
            Some(LPARAM((&mut end as *mut u32) as isize)),
        );
        if id == 100 {
            SendMessageW(child, WM_SETREDRAW, Some(WPARAM(0)), None);
        }
        let text = paint::wide(text);
        let _ = SetWindowTextW(child, PCWSTR(text.as_ptr()));
        if id == 100 {
            SendMessageW(
                child,
                EM_SETSEL,
                Some(WPARAM(start as usize)),
                Some(LPARAM(end as isize)),
            );
            let current = SendMessageW(child, EM_GETFIRSTVISIBLELINE, None, None);
            SendMessageW(
                child,
                EM_LINESCROLL,
                Some(WPARAM(0)),
                Some(LPARAM(first.0 - current.0)),
            );
            SendMessageW(child, WM_SETREDRAW, Some(WPARAM(1)), None);
            super::panel::scroll(child);
            let _ = InvalidateRect(Some(child), None, true);
        }
    }
}
fn control_view(paths: &SignalPaths) -> model::View {
    model::view(
        &paths.read_calls(),
        unix_ms(),
        super::shell::stop_timestamp(paths),
        actl_uia::display_support::input_busy(),
    )
}
fn has_feedback(hwnd: HWND) -> bool {
    unsafe { GetDlgItem(Some(hwnd), 101).is_ok_and(|c| GetWindowTextLengthW(c) > 0) }
}
unsafe fn visible(hwnd: HWND, id: i32, show: bool) {
    if let Ok(child) = GetDlgItem(Some(hwnd), id) {
        let _ = ShowWindow(child, if show { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
}
unsafe fn refresh(hwnd: HWND) {
    let paths = SignalPaths::default();
    let _ = current();
    let id = BOUND_FLOW.get().and_then(|id| id.as_deref());
    refresh_with(hwnd, &paths, id);
}
pub(super) unsafe fn refresh_with(hwnd: HWND, paths: &SignalPaths, id: Option<&str>) {
    let stopped = paths.stop_requested();
    let repair = paths.execution_generation().is_err();
    let state = id.and_then(|id| flow_ui::read(paths, id));
    let flow = id.and_then(|id| {
        state
            .as_ref()
            .and_then(|s| s["status"].as_str())
            .map(|status| (id.to_owned(), status.to_owned()))
    });
    let plan = id.and_then(|id| super::ui_text::plan(paths, id));
    let status = state.as_ref().and_then(|s| s["status"].as_str());
    let summary = if id.is_some() {
        super::ui_text::summary(state.as_ref(), plan.as_ref(), stopped)
    } else {
        let view = control_view(paths);
        super::ui_text::Summary {
            title: "桌面状态".into(),
            status: view.label,
            body: if stopped {
                "上一轮执行已停止。新任务可直接发起并点击开始；旧任务不会自动恢复。"
            } else {
                "当前没有已绑定的可恢复任务。\r\n单次调用请在状态条查看或暂停；结束后由调用方接续。"
            }
            .into(),
        }
    };
    let request_message = flow_ui::message();
    let local_error = LOCAL_ERROR.with(|e| e.borrow().clone());
    let message = if local_error.is_empty() {
        request_message
    } else {
        local_error
    };
    let feedback = super::ui_text::feedback(&message, status);
    let had_feedback = has_feedback(hwnd);
    set_text(hwnd, 101, &feedback);
    visible(hwnd, 101, !feedback.is_empty());
    if had_feedback != !feedback.is_empty() {
        layout(hwnd, GetDpiForWindow(hwnd));
    }
    set_text(hwnd, 102, &summary.title);
    set_text(hwnd, 103, &summary.status);
    let expanded = EXPANDED.with(|v| v.get());
    let mut text = summary.body;
    if expanded {
        let technical = id
            .map(|id| flow_ui::details(paths, id))
            .unwrap_or_else(|| control_view(paths).details);
        text.push_str(&format!("\r\n\r\n{}\r\n{technical}", summary.title));
        text.push_str(&paths.stop_details());
        if !message.is_empty() {
            text.push_str(&format!("\r\n{message}"));
        }
    }
    set_text(hwnd, 100, &text.replace("\r\n", "\n").replace('\n', "\r\n"));
    set_text(
        hwnd,
        14,
        if expanded {
            "收起详情"
        } else {
            "详细信息"
        },
    );
    let running = status == Some("running") && !stopped;
    let resumable = matches!(status, Some("paused" | "needs_review" | "stopped")) && !repair;
    visible(hwnd, 10, running);
    visible(hwnd, 11, resumable);
    visible(hwnd, 13, stopped || repair);
    visible(hwnd, 12, !stopped);
    visible(hwnd, 104, !stopped);
    let pause_pending = flow.as_ref().is_some_and(|(id, _)| {
        flow_ui::pending(id, false)
            || paths
                .dir
                .join("flows")
                .join(id)
                .join("pause-requested")
                .exists()
    });
    let resume_pending = flow
        .as_ref()
        .is_some_and(|(id, _)| flow_ui::pending(id, true));
    set_text(
        hwnd,
        10,
        if pause_pending {
            "正在暂停"
        } else {
            "暂停"
        },
    );
    set_text(
        hwnd,
        11,
        if resume_pending {
            "正在检查"
        } else if matches!(status, Some("needs_review" | "stopped")) {
            "核对结果"
        } else {
            "继续"
        },
    );
    let ready = state
        .as_ref()
        .is_some_and(|s| flow_ui::continue_ready(s, unix_ms()));
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 10).unwrap_or_default(),
        running && !pause_pending,
    );
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 11).unwrap_or_default(),
        resumable && ready && !resume_pending,
    );
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 13).unwrap_or_default(),
        stopped || repair,
    );
    let _ = EnableWindow(GetDlgItem(Some(hwnd), 12).unwrap_or_default(), !stopped);
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Some(result) = super::panel::message(hwnd, msg, wp, lp) {
        return result;
    }
    match msg {
        WM_DPICHANGED => {
            let rect = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            layout(hwnd, wp.0 as u32 & 0xffff);
            LRESULT(0)
        }
        WM_TIMER => {
            refresh(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            if wp.0 >> 16 != BN_CLICKED as usize {
                return LRESULT(0);
            }
            if wp.0 & 0xffff == 16 {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
                return LRESULT(0);
            }
            if wp.0 & 0xffff == 14 {
                EXPANDED.with(|v| v.set(!v.get()));
                layout(hwnd, GetDpiForWindow(hwnd));
                refresh(hwnd);
                return LRESULT(0);
            }
            LOCAL_ERROR.with(|e| e.borrow_mut().clear());
            flow_ui::clear_message();
            set_text(hwnd, 101, "");
            let result = match wp.0 & 0xffff {
                10 | 11 => current()
                    .ok_or_else(|| "没有可操作的流程".into())
                    .and_then(|(id, _)| super::shell::flow_action(&id, wp.0 & 0xffff == 11)),
                12 => super::shell::request_emergency_stop("details_button"),
                13 => super::shell::clear_emergency_stop(),
                2 | 15 => {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                LOCAL_ERROR.with(|e| *e.borrow_mut() = error);
            }
            refresh(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            super::panel::release(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub(super) unsafe fn create_window() -> windows::core::Result<HWND> {
    EXPANDED.with(|v| v.set(false));
    LOCAL_ERROR.with(|v| v.borrow_mut().clear());
    let instance = GetModuleHandleW(None)?;
    let class = w!("actl_signal_controls");
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        style: CS_DROPSHADOW,
        hbrBackground: HBRUSH::default(),
        lpszClassName: class,
        ..Default::default()
    });
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
        class,
        w!("actl · 任务详情"),
        WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        420,
        360,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    let dpi = GetDpiForWindow(hwnd);
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: paint::pixels(560, dpi),
        bottom: paint::pixels(444, dpi),
    };
    let _ = AdjustWindowRectExForDpi(
        &mut bounds,
        WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX,
        false,
        WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
        dpi,
    );
    let mut work = RECT::default();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )?;
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    let _ = SetWindowPos(
        hwnd,
        None,
        work.left + ((work.right - work.left - width) / 2).max(0),
        work.top + ((work.bottom - work.top - height) / 2).max(0),
        width,
        height,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    for &(id, label, _, _, _, _) in CONTROLS {
        let button = id < 100;
        let text = paint::wide(label);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            if button {
                w!("BUTTON")
            } else if id == 100 {
                w!("EDIT")
            } else {
                w!("STATIC")
            },
            PCWSTR(text.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | if button {
                    WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32)
                } else if id == 100 {
                    WS_TABSTOP
                        | WS_VSCROLL
                        | WINDOW_STYLE(
                            ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32,
                        )
                } else {
                    WINDOW_STYLE(if matches!(id, 102 | 103) { 0x4000 } else { 0 }) // SS_ENDELLIPSIS
                },
            0,
            0,
            0,
            0,
            Some(hwnd),
            Some(HMENU(id as *mut _)),
            Some(instance.into()),
            None,
        )?;
    }
    layout(hwnd, dpi);
    Ok(hwnd)
}
pub unsafe fn run() -> windows::core::Result<()> {
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let class = w!("actl_signal_controls");
    if let Ok(existing) = FindWindowW(class, None) {
        let _ = ShowWindow(existing, SW_RESTORE);
        let _ = SetForegroundWindow(existing);
        return Ok(());
    }
    let hwnd = create_window()?;
    refresh(hwnd);
    let _ = ShowWindow(hwnd, SW_SHOW);
    // StartupInfo may request a hidden helper; this explicit user window must be visible.
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    // Reading the panel must not focus an action that could resume desktop writes.
    let _ = SetFocus(Some(GetDlgItem(Some(hwnd), 100)?));
    SetTimer(Some(hwnd), 1, 500, None);
    let mut msg = MSG::default();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0).0;
        if result == -1 {
            return Err(windows::core::Error::from_thread());
        }
        if result == 0 {
            break;
        }
        if !IsDialogMessageW(hwnd, &msg).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dpi_change_updates_native_control_bounds_and_font() {
        unsafe {
            let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let instance = GetModuleHandleW(None).unwrap();
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("dpi-test"),
                WS_POPUP,
                0,
                0,
                420,
                360,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("BUTTON"),
                w!("pause"),
                WS_CHILD,
                0,
                0,
                0,
                0,
                Some(parent),
                Some(HMENU(10usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .unwrap();
            for dpi in [96, 120, 144, 192, 96] {
                let bounds = RECT {
                    left: 0,
                    top: 0,
                    right: paint::pixels(560, dpi),
                    bottom: paint::pixels(444, dpi),
                };
                proc(
                    parent,
                    WM_DPICHANGED,
                    WPARAM(dpi as usize),
                    LPARAM((&bounds as *const RECT) as isize),
                );
                let mut rect = RECT::default();
                GetWindowRect(child, &mut rect).unwrap();
                assert_eq!(rect.right - rect.left, paint::pixels(148, dpi));
                // 2026-09-30 轻量化:动作按钮高度 40 → 32。
                assert_eq!(rect.bottom - rect.top, paint::pixels(32, dpi));
                let font = SendMessageW(child, WM_GETFONT, None, None);
                let mut info = LOGFONTW::default();
                assert_ne!(
                    GetObjectW(
                        HGDIOBJ(font.0 as *mut _),
                        std::mem::size_of::<LOGFONTW>() as i32,
                        Some((&mut info as *mut LOGFONTW).cast())
                    ),
                    0
                );
                assert_eq!(info.lfHeight, -paint::pixels(16, dpi));
            }
            super::super::panel::release(parent);
            DestroyWindow(parent).unwrap();
            let _ = SetThreadDpiAwarenessContext(previous);
        }
    }
    #[test]
    fn live_details_preserve_selection_and_unchanged_text() {
        unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let parent = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("details-test"),
                WS_POPUP,
                0,
                0,
                420,
                360,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WINDOW_STYLE(ES_MULTILINE as u32 | ES_READONLY as u32),
                0,
                0,
                388,
                220,
                Some(parent),
                Some(HMENU(100usize as *mut _)),
                Some(instance.into()),
                None,
            )
            .unwrap();
            set_text(parent, 100, "first details");
            SendMessageW(child, EM_SETSEL, Some(WPARAM(1)), Some(LPARAM(4)));
            set_text(parent, 100, "first details");
            set_text(parent, 100, "newer details");
            let selected = SendMessageW(child, EM_GETSEL, None, None).0 as u32;
            DestroyWindow(parent).unwrap();
            assert_eq!(selected & 0xffff, 1);
            assert_eq!(selected >> 16, 4);
        }
    }
    #[test]
    fn acknowledged_stop_is_not_still_pending() {
        use actl_core::state::{Phase, SessionState};
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let mut s = SessionState::now(1, "wait", None, None, Phase::Stopped);
        s.ts_ms = unix_ms();
        paths.write_state(&s);
        paths.request_stop().unwrap();
        let mut stop = paths.stop_record().unwrap();
        stop.requested_ms = s.ts_ms.saturating_sub(1);
        std::fs::write(paths.stop_file(), serde_json::to_vec(&stop).unwrap()).unwrap();
        let actual = control_view(&paths).mode;
        std::fs::remove_dir_all(paths.dir).unwrap();
        assert_eq!(actual, model::Mode::Stopped);
    }
}
