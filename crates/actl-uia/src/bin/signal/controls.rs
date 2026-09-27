//! Explicit, focusable control window using standard Windows accessible controls.
use super::{model, paint};
use actl_core::state::{SignalPaths, unix_ms};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

const CONTROLS: &[(i32, &str, i32, i32, i32, i32)] = &[
    (100, "正在读取流程状态", 16, 16, 388, 220),
    (101, "", 16, 240, 388, 24),
    (10, "暂停流程", 16, 272, 120, 32),
    (11, "继续流程", 148, 272, 120, 32),
    (12, "停止操作", 280, 272, 120, 32),
    (13, "允许后续操作", 16, 312, 180, 32),
    (14, "关闭", 280, 312, 120, 32),
];

unsafe fn layout(hwnd: HWND, dpi: u32) {
    let px = |n| paint::pixels(n, dpi);
    let font = CreateFontW(
        -px(16),
        0,
        0,
        0,
        400,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_DEFAULT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        0,
        w!("Segoe UI"),
    );
    let old = SetWindowLongPtrW(hwnd, GWLP_USERDATA, font.0 as isize);
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
            SendMessageW(
                child,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
    if old != 0 {
        let _ = DeleteObject(HGDIOBJ(old as *mut _));
    }
}

fn current() -> Option<(String, String)> {
    let paths = SignalPaths::default();
    let states = paths.read_calls();
    let flow = model::selected(&states, unix_ms())?.workflow.as_ref()?;
    if !actl_core::history::valid_id(&flow.run_id) {
        return None;
    }
    let bytes = std::fs::read(
        paths
            .dir
            .join("flows")
            .join(&flow.run_id)
            .join("state.json"),
    )
    .ok()?;
    let state: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some((flow.run_id.clone(), state["status"].as_str()?.into()))
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
    let view = control_view(&paths);
    let text = paint::wide(&view.details);
    let _ = SetWindowTextW(
        GetDlgItem(Some(hwnd), 100).unwrap_or_default(),
        PCWSTR(text.as_ptr()),
    );
    let flow = current();
    let status = flow.as_ref().map(|(_, s)| s.as_str());
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 10).unwrap_or_default(),
        status == Some("running") && !stopped,
    );
    let resume = matches!(
        status,
        Some("paused" | "needs_review" | "failed" | "stopped")
    );
    let _ = EnableWindow(
        GetDlgItem(Some(hwnd), 11).unwrap_or_default(),
        resume && !stopped,
    );
    let label = paint::wide(if status == Some("needs_review") {
        "重新核对"
    } else {
        "继续流程"
    });
    let _ = SetWindowTextW(
        GetDlgItem(Some(hwnd), 11).unwrap_or_default(),
        PCWSTR(label.as_ptr()),
    );
    let _ = EnableWindow(GetDlgItem(Some(hwnd), 13).unwrap_or_default(), stopped);
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
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
            let result = match wp.0 & 0xffff {
                10 | 11 => current()
                    .ok_or_else(|| "没有可操作的流程".into())
                    .and_then(|(id, _)| super::shell::flow_action(&id, wp.0 & 0xffff == 11)),
                12 => {
                    SignalPaths::default().request_stop();
                    Ok(())
                }
                13 => {
                    SignalPaths::default().clear_stop();
                    Ok(())
                }
                2 | 14 => {
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
            let font = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if font != 0 {
                let _ = DeleteObject(HGDIOBJ(font as *mut _));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
pub unsafe fn run() -> windows::core::Result<()> {
    let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let instance = GetModuleHandleW(None)?;
    let class = w!("actl_signal_controls");
    if let Ok(existing) = FindWindowW(class, None) {
        let _ = ShowWindow(existing, SW_RESTORE);
        let _ = SetForegroundWindow(existing);
        return Ok(());
    }
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
        lpszClassName: class,
        ..Default::default()
    });
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
        class,
        w!("actl · 流程控制"),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
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
        right: paint::pixels(420, dpi),
        bottom: paint::pixels(360, dpi),
    };
    let _ = AdjustWindowRectExForDpi(
        &mut bounds,
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        false,
        WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
        dpi,
    );
    let _ = SetWindowPos(
        hwnd,
        None,
        0,
        0,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOMOVE | SWP_NOZORDER,
    );
    for &(id, label, _, _, _, _) in CONTROLS {
        let button = id < 100;
        let text = paint::wide(label);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            if button { w!("BUTTON") } else { w!("STATIC") },
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | if button { WS_TABSTOP } else { WINDOW_STYLE(0) },
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
    SetTimer(Some(hwnd), 1, 250, None);
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
                    right: paint::pixels(420, dpi),
                    bottom: paint::pixels(360, dpi),
                };
                proc(
                    parent,
                    WM_DPICHANGED,
                    WPARAM(dpi as usize),
                    LPARAM((&bounds as *const RECT) as isize),
                );
                let mut rect = RECT::default();
                GetWindowRect(child, &mut rect).unwrap();
                assert_eq!(rect.right - rect.left, paint::pixels(120, dpi));
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
            let font = SetWindowLongPtrW(parent, GWLP_USERDATA, 0);
            DestroyWindow(parent).unwrap();
            let _ = DeleteObject(HGDIOBJ(font as *mut _));
            let _ = SetThreadDpiAwarenessContext(previous);
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
