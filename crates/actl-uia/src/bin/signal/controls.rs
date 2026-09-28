//! Explicit, focusable control window using standard Windows accessible controls.
use super::{flow_ui, model, paint};
use std::sync::OnceLock;
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
    (102, "任务详情", 24, 20, 400, 32),
    (16, "最小化", 448, 20, 40, 32),
    (15, "关闭", 496, 20, 40, 32),
    (103, "查看当前进度，或暂停和继续流程", 24, 58, 512, 22),
    (100, "正在读取流程状态", 24, 96, 512, 160),
    (101, "", 24, 272, 512, 44),
    (10, "暂停流程", 24, 328, 148, 40),
    (11, "继续流程", 184, 328, 188, 40),
    (14, "关闭", 388, 328, 148, 40),
    (104, "紧急停止  ·  Ctrl+Alt+F12", 24, 386, 512, 22),
    (12, "停止操作", 24, 420, 148, 40),
    (13, "解除急停", 184, 420, 188, 40),
];
unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| paint::pixels(n, dpi);
    for &(id, _, x, y, width, height) in CONTROLS {
        if let Ok(child) = GetDlgItem(Some(hwnd), id) {
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
            bottom: px(148),
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
unsafe fn refresh(hwnd: HWND) {
    let paths = SignalPaths::default();
    let stopped = paths.stop_requested();
    let flow = current();
    let mut text = if let Some(id) = BOUND_FLOW.get().and_then(|id| id.as_deref()) {
        flow_ui::details(&paths, id)
    } else {
        control_view(&paths).details
    };
    if let Some(id) = BOUND_FLOW.get().and_then(|id| id.as_deref()) {
        let states: Vec<_> = paths
            .read_calls()
            .into_iter()
            .filter(|s| s.workflow.as_ref().is_some_and(|f| f.run_id == id))
            .collect();
        if !states.is_empty() {
            text.push_str(&format!(
                "\n\n{}",
                model::view(
                    &states,
                    unix_ms(),
                    super::shell::stop_timestamp(&paths),
                    actl_uia::display_support::input_busy()
                )
                .details
            ));
        }
    }
    text.push_str(&paths.stop_details());
    set_text(hwnd, 100, &text.replace('\n', "\r\n"));
    let message = flow_ui::message();
    if !message.is_empty() {
        set_text(
            hwnd,
            101,
            if flow.as_ref().is_some_and(|(_, status)| status == "paused")
                && message == "已请求暂停，等待当前操作结束"
            {
                "已暂停"
            } else {
                &message
            },
        );
    }
    let status = flow.as_ref().map(|(_, s)| s.as_str());
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 10).unwrap_or_default(),
        status == Some("running")
            && !stopped
            && flow.as_ref().is_none_or(|(id, _)| {
                !flow_ui::pending(id, false)
                    && !paths
                        .dir
                        .join("flows")
                        .join(id)
                        .join("pause-requested")
                        .exists()
            }),
    );
    let resume = matches!(
        status,
        Some("paused" | "needs_review" | "failed" | "stopped")
    );
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 11).unwrap_or_default(),
        resume
            && !stopped
            && flow
                .as_ref()
                .is_none_or(|(id, _)| !flow_ui::pending(id, true)),
    );
    set_text(
        hwnd,
        11,
        if flow
            .as_ref()
            .is_some_and(|(id, _)| flow_ui::pending(id, true))
        {
            "继续中…"
        } else if status == Some("needs_review") {
            "重新核对"
        } else {
            "继续流程"
        },
    );
    set_text(
        hwnd,
        10,
        if flow.as_ref().is_some_and(|(id, _)| {
            flow_ui::pending(id, false)
                || (status == Some("running")
                    && paths
                        .dir
                        .join("flows")
                        .join(id)
                        .join("pause-requested")
                        .exists())
        }) {
            "暂停中…"
        } else {
            "暂停流程"
        },
    );
    let _ = EnableWindow(GetDlgItem(Some(hwnd), 13).unwrap_or_default(), stopped);
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
            flow_ui::clear_message();
            set_text(hwnd, 101, "");
            let result = match wp.0 & 0xffff {
                10 | 11 => current()
                    .ok_or_else(|| "没有可操作的流程".into())
                    .and_then(|(id, _)| super::shell::flow_action(&id, wp.0 & 0xffff == 11)),
                12 => super::shell::request_emergency_stop("details_button"),
                13 => super::shell::clear_emergency_stop(),
                2 | 14 | 15 => {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                let text = paint::wide(&error);
                let _ = SetWindowTextW(
                    GetDlgItem(Some(hwnd), 101).unwrap_or_default(),
                    PCWSTR(text.as_ptr()),
                );
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
        w!("actl · 流程控制"),
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
        bottom: paint::pixels(478, dpi),
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
                    WINDOW_STYLE(0)
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
    let resume = GetDlgItem(Some(hwnd), 11)?;
    let focus = if IsWindowEnabled(resume).as_bool() {
        resume
    } else {
        GetDlgItem(Some(hwnd), 14)?
    };
    let _ = SetFocus(Some(focus));
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
                    bottom: paint::pixels(478, dpi),
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
                assert_eq!(rect.bottom - rect.top, paint::pixels(40, dpi));
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
        std::fs::write(paths.stop_file(), s.ts_ms.saturating_sub(1).to_string()).unwrap();
        let actual = control_view(&paths).mode;
        std::fs::remove_dir_all(paths.dir).unwrap();
        assert_eq!(actual, model::Mode::Stopped);
    }
}
