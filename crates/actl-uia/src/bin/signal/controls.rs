//! Explicit, focusable control window using standard Windows accessible controls.
use super::{flow_ui, model, paint};
use std::sync::OnceLock;
thread_local! {
    static EXPANDED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static RECORDS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FONT_DPI: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static LAST_LAYOUT: std::cell::Cell<Option<[i64; 9]>> = const { std::cell::Cell::new(None) };
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
    (102, "桌面状态", 24, 20, 412, 32),
    (16, "最小化", 448, 20, 40, 32),
    (15, "关闭", 496, 20, 40, 32),
    (103, "正在读取状态", 24, 72, 512, 32),
    (100, "正在读取流程状态", 24, 112, 512, 32),
    (101, "", 24, 272, 512, 44),
    (10, "暂停", 24, 330, 148, 32),
    (11, "继续", 24, 330, 148, 32),
    (17, "步骤记录  ›", 24, 220, 148, 32),
    (106, "", 24, 256, 512, 160),
    (14, "技术详情  ›", 24, 220, 148, 32),
    (107, "", 24, 256, 512, 160),
    (104, "Ctrl+Alt+F12", 184, 393, 200, 22),
    (12, "急停", 24, 388, 80, 32),
    (13, "修复运行状态", 24, 388, 148, 32),
];
unsafe fn text_height(hwnd: HWND, id: i32, dpi: u32, cap: i32) -> i32 {
    let Ok(child) = GetDlgItem(Some(hwnd), id) else {
        return 32;
    };
    let mut text = vec![0u16; GetWindowTextLengthW(child).max(0) as usize + 1];
    let len = GetWindowTextW(child, &mut text).max(0) as usize;
    let dc = GetDC(Some(child));
    let font = SendMessageW(child, WM_GETFONT, None, None);
    let old = SelectObject(dc, HGDIOBJ(font.0 as *mut _));
    let mut r = RECT {
        right: paint::pixels(492, dpi),
        ..Default::default()
    };
    DrawTextW(
        dc,
        &mut text[..len],
        &mut r,
        DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
    );
    SelectObject(dc, old);
    ReleaseDC(Some(child), dc);
    ((r.bottom * 96 + dpi as i32 - 1) / dpi as i32 + 8).clamp(32, cap)
}
unsafe fn shown(hwnd: HWND, id: i32) -> bool {
    GetDlgItem(Some(hwnd), id)
        .is_ok_and(|c| GetWindowLongW(c, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0)
}
unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| paint::pixels(n, dpi);
    if FONT_DPI.with(|v| v.replace(dpi)) != dpi {
        super::panel::fonts(hwnd, dpi, &CONTROLS.iter().map(|c| c.0).collect::<Vec<_>>());
    }
    let mut monitor = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let monitor_height = if GetMonitorInfoW(
        MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    )
    .as_bool()
    {
        (monitor.rcWork.bottom - monitor.rcWork.top) * 96 / dpi as i32 - 32
    } else {
        720
    };
    let web = super::web_details::active(hwnd);
    let body_height = if web {
        super::web_details::height(hwnd).clamp(64, (monitor_height - 180).max(64))
    } else {
        text_height(hwnd, 100, dpi, (monitor_height - 300).clamp(32, 216))
    };
    let mut next = (if web { 64 } else { 112 }) + body_height + 12;
    let feedback_y = next;
    if !web && has_feedback(hwnd) {
        next += text_height(hwnd, 101, dpi, 64) + 12;
    }
    let has_action = shown(hwnd, 10) || shown(hwnd, 11);
    let records_y = next;
    if !web {
        next += 36;
    }
    let both = RECORDS.with(|v| v.get()) && EXPANDED.with(|v| v.get());
    let budget = (monitor_height - next - 36 - 72 - 24).max(64);
    let records_height = if web {
        0
    } else {
        text_height(
            hwnd,
            106,
            dpi,
            (if both { budget / 2 } else { budget }).clamp(32, 160),
        )
    };
    if !web && RECORDS.with(|v| v.get()) {
        next += records_height + 12;
    }
    let technical_y = next;
    if !web {
        next += 36;
    }
    let technical_height = if web {
        0
    } else {
        text_height(
            hwnd,
            107,
            dpi,
            (monitor_height - next - 72 - 12).clamp(32, 200),
        )
    };
    if !web && EXPANDED.with(|v| v.get()) {
        next += technical_height + 12;
    }
    let footer_y = next + 16;
    let key = [
        hwnd.0 as i64,
        i64::from(dpi),
        i64::from(body_height),
        i64::from(feedback_y),
        i64::from(has_action) + 2 * i64::from(web),
        i64::from(records_y),
        i64::from(technical_y),
        i64::from(footer_y),
        i64::from(records_height) * 1000 + i64::from(technical_height),
    ];
    if LAST_LAYOUT.with(|v| v.replace(Some(key))) == Some(key) {
        return;
    }
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        px(560),
        px(footer_y + 56),
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
    );
    super::frame::update(hwnd, dpi);
    super::web_details::bounds(
        hwnd,
        RECT {
            left: px(24),
            top: px(64),
            right: px(536),
            bottom: px(64 + body_height),
        },
    );
    for &(id, _, x, y, width, height) in CONTROLS {
        if web && matches!(id, 100 | 101 | 103 | 14 | 17 | 106 | 107) {
            continue;
        }
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
            let x = match id {
                12 | 13 if has_action => 184,
                104 => {
                    if has_action {
                        276
                    } else {
                        116
                    }
                }
                _ => x,
            };
            let (y, height) = match id {
                100 => (112, body_height),
                101 => (feedback_y, text_height(hwnd, 101, dpi, 64)),
                10 | 11 => (footer_y, height),
                17 => (records_y, height),
                106 => (records_y + 36, records_height),
                14 => (technical_y, height),
                107 => (technical_y + 36, technical_height),
                12 | 13 => (footer_y, height),
                104 => (footer_y + 5, height),
                _ => (y, height),
            };
            let _ = SetWindowPos(
                child,
                None,
                px(x),
                px(y),
                px(width),
                px(height),
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if matches!(id, 100 | 106 | 107) {
                let rect = RECT {
                    left: 0,
                    top: 0,
                    right: px(492),
                    bottom: px(height),
                };
                SendMessageW(
                    child,
                    windows::Win32::UI::Controls::EM_SETRECT,
                    None,
                    Some(LPARAM((&rect as *const RECT) as isize)),
                );
                super::panel::scroll(child);
            }
        }
    }
    let _ = InvalidateRect(Some(hwnd), None, true);
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
        if matches!(id, 100 | 106 | 107) {
            SendMessageW(child, WM_SETREDRAW, Some(WPARAM(0)), None);
        }
        let text = paint::wide(text);
        let _ = SetWindowTextW(child, PCWSTR(text.as_ptr()));
        if matches!(id, 100 | 106 | 107) {
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
unsafe fn text(hwnd: HWND, id: i32) -> String {
    let Ok(child) = GetDlgItem(Some(hwnd), id) else {
        return String::new();
    };
    let mut buffer = vec![0u16; GetWindowTextLengthW(child).max(0) as usize + 1];
    let length = GetWindowTextW(child, &mut buffer).max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
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
    let mut feedback = super::ui_text::feedback(&message, status);
    let backend_error = super::web_details::error(hwnd);
    if feedback.is_empty() && backend_error.is_some() {
        feedback = "WebView2 未就绪，当前使用原生详情。".into();
    }
    set_text(hwnd, 101, &feedback);
    visible(hwnd, 101, !feedback.is_empty());
    set_text(hwnd, 102, &summary.title);
    set_text(hwnd, 103, &summary.status);
    let expanded = EXPANDED.with(|v| v.get());
    set_text(hwnd, 100, &summary.body);
    let technical = id
        .map(|id| flow_ui::details(paths, id))
        .unwrap_or_else(|| control_view(paths).details);
    let mut diagnostic = technical;
    diagnostic.push_str(&paths.stop_details());
    if !message.is_empty() {
        diagnostic.push_str(&format!("\r\n{message}"));
    }
    if let Some(error) = backend_error {
        diagnostic.push_str(&format!("\r\n{error}"));
    }
    set_text(
        hwnd,
        107,
        &diagnostic.replace("\r\n", "\n").replace('\n', "\r\n"),
    );
    set_text(
        hwnd,
        106,
        &state
            .as_ref()
            .map(|s| super::ui_text::step_records(s, plan.as_ref()))
            .unwrap_or_else(|| "步骤记录暂不可读。".into()),
    );
    let records = RECORDS.with(|v| v.get());
    visible(hwnd, 106, records);
    visible(hwnd, 107, expanded);
    set_text(
        hwnd,
        17,
        if records {
            "步骤记录  ∨"
        } else {
            "步骤记录  ›"
        },
    );
    set_text(
        hwnd,
        14,
        if expanded {
            "技术详情  ∨"
        } else {
            "技术详情  ›"
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
        } else if state.as_ref().and_then(|s| s["reason"].as_str()) == Some("action_failed") {
            // 失败暂停:点继续会自动重发该步动作——文案明示语义。
            "已修复，重试继续"
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
    let primary = if running {
        10
    } else if resumable {
        11
    } else {
        0
    };
    let json = serde_json::json!({
        "version":1, "title":text(hwnd, 102), "status":text(hwnd, 103),
        "tone":if stopped { "error" } else { match status { Some("completed") => "done", Some("running") => "working", Some("paused" | "needs_review") => "waiting", Some("failed" | "stopped") => "error", _ => "neutral" } },
        "body":text(hwnd,100), "records":text(hwnd,106), "diagnostics":text(hwnd,107), "feedback":text(hwnd,101),
        "primary":text(hwnd,primary), "primaryEnabled":GetDlgItem(Some(hwnd),primary).is_ok_and(|c| IsWindowEnabled(c).as_bool()),
        "stop":text(hwnd,if stopped || repair { 13 } else { 12 }), "shortcut":if stopped { "" } else { "Ctrl+Alt+F12" }
    }).to_string();
    super::web_details::tick(hwnd, &json);
    let web = super::web_details::active(hwnd);
    let first_web_frame = web && shown(hwnd, 103);
    for id in [100, 103, 14, 17] {
        visible(hwnd, id, !web);
    }
    visible(hwnd, 101, !web && has_feedback(hwnd));
    visible(hwnd, 106, !web && records);
    visible(hwnd, 107, !web && expanded);
    layout(hwnd, GetDpiForWindow(hwnd));
    if first_web_frame && GetForegroundWindow() == hwnd {
        super::web_details::focus(hwnd);
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if super::web_details::is_update(msg) {
        if wp.0 == 1 {
            super::web_details::acknowledge_layout(hwnd);
            layout(hwnd, GetDpiForWindow(hwnd));
        } else {
            refresh(hwnd);
        }
        return LRESULT(0);
    }
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
            if matches!(wp.0 & 0xffff, 14 | 17) {
                if wp.0 & 0xffff == 14 {
                    EXPANDED.with(|v| v.set(!v.get()));
                } else {
                    RECORDS.with(|v| v.set(!v.get()));
                }
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
            super::web_details::close(hwnd);
            super::panel::release(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub(super) unsafe fn create_window() -> windows::core::Result<HWND> {
    EXPANDED.with(|v| v.set(false));
    RECORDS.with(|v| v.set(false));
    FONT_DPI.with(|v| v.set(0));
    LAST_LAYOUT.with(|v| v.set(None));
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
            } else if matches!(id, 100 | 106 | 107) {
                w!("EDIT")
            } else {
                w!("STATIC")
            },
            PCWSTR(text.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | if button {
                    WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32)
                } else if matches!(id, 100 | 106 | 107) {
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
    let com = super::web_details::Apartment::new();
    if com.initialized() {
        if let Err(error) = super::web_details::start(hwnd, &SignalPaths::default()) {
            eprintln!("NOT_ACTIONABLE: WebView2 initialization: {error}");
        }
    } else {
        eprintln!("NOT_ACTIONABLE: WebView2 COM apartment unavailable; using native details");
    }
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
        let handled = super::web_details::keyboard(hwnd, &msg)
            .unwrap_or_else(|| IsDialogMessageW(hwnd, &msg).as_bool());
        if !handled {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(super) unsafe fn preview_sections(
    hwnd: HWND,
    paths: &SignalPaths,
    records: bool,
    technical: bool,
) {
    RECORDS.with(|v| v.set(records));
    EXPANDED.with(|v| v.set(technical));
    refresh_with(hwnd, paths, Some("preview"));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_height_notification_preserves_the_loaded_task_snapshot() {
        unsafe {
            let hwnd = create_window().unwrap();
            set_text(hwnd, 100, "loaded snapshot must survive layout");
            proc(hwnd, WM_APP + 61, WPARAM(1), LPARAM(0));
            assert_eq!(text(hwnd, 100), "loaded snapshot must survive layout");
            DestroyWindow(hwnd).unwrap();
        }
    }
    #[test]
    fn terminal_state_reclaims_action_space_and_expanded_records_keep_footer_aligned() {
        unsafe {
            let paths =
                SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
            let dir = paths.dir.join("flows").join("preview");
            std::fs::create_dir_all(&dir).unwrap();
            let paused =
                serde_json::json!({"status":"paused", "steps":[{"id":"save", "status":"pending"}]});
            let done = serde_json::json!({"status":"completed", "steps":[{"id":"save", "status":"completed"}]});
            std::fs::write(dir.join("state.json"), serde_json::to_vec(&paused).unwrap()).unwrap();
            let hwnd = create_window().unwrap();
            refresh_with(hwnd, &paths, Some("preview"));
            let bounds = |id| {
                let mut r = RECT::default();
                GetWindowRect(GetDlgItem(Some(hwnd), id).unwrap(), &mut r).unwrap();
                r
            };
            assert!(shown(hwnd, 11));
            assert!(!shown(hwnd, 10));
            assert_eq!(bounds(11).top, bounds(12).top);
            assert!(bounds(11).right < bounds(12).left);
            assert!(bounds(12).right < bounds(104).left);
            let mut before = RECT::default();
            GetWindowRect(hwnd, &mut before).unwrap();
            std::fs::write(dir.join("state.json"), serde_json::to_vec(&done).unwrap()).unwrap();
            refresh_with(hwnd, &paths, Some("preview"));
            assert!(!shown(hwnd, 11));
            let mut after = RECT::default();
            GetWindowRect(hwnd, &mut after).unwrap();
            assert!(after.bottom - after.top <= before.bottom - before.top);
            assert_eq!(bounds(12).left, bounds(100).left);
            preview_sections(hwnd, &paths, true, true);
            assert!(shown(hwnd, 106) && shown(hwnd, 107));
            assert!(bounds(106).bottom <= bounds(14).top);
            assert!(bounds(107).bottom < bounds(12).top);
            let region = CreateRectRgn(0, 0, 0, 0);
            GetWindowRgn(hwnd, region);
            GetWindowRect(hwnd, &mut after).unwrap();
            assert!(PtInRegion(region, 40, after.bottom - after.top - 30).as_bool());
            let _ = DeleteObject(region.into());
            DestroyWindow(hwnd).unwrap();
            std::fs::remove_dir_all(paths.dir).unwrap();
        }
    }
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
