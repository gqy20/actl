use super::{model, overlay, paint};
use actl_core::activity::{ActionCursor, ActionPhase};
use actl_core::state::{SignalPaths, unix_ms};
use actl_uia::display_support::{CAPTURE_HIDE, CAPTURE_RESTORE};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

static VIEW: Mutex<Option<model::View>> = Mutex::new(None);
static CURSOR: Mutex<Option<ActionCursor>> = Mutex::new(None);
static PREPARING: Mutex<String> = Mutex::new(String::new());
static CAPTURES: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static WAS_VISIBLE: AtomicBool = AtomicBool::new(false);
static HOTKEY: AtomicBool = AtomicBool::new(false);
static STOP_HOVERED: AtomicBool = AtomicBool::new(false);
static FRAME_READY: AtomicBool = AtomicBool::new(false);
static TRAY_ICON: Mutex<Option<(usize, i32, u32)>> = Mutex::new(None);
static EPOCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn publish_lease(paths: &SignalPaths, state: &str) {
    let lease = actl_core::display::DisplayLease {
        protocol: actl_core::display::DISPLAY_PROTOCOL,
        build: env!("CARGO_PKG_VERSION").into(),
        pid: std::process::id(),
        epoch: EPOCH
            .get_or_init(actl_core::snapshot::new_snapshot_id)
            .clone(),
        state: state.into(),
        ts_ms: unix_ms(),
    };
    let _ = paths.write_display("display.json", &lease);
}

fn stop_timestamp(paths: &SignalPaths) -> Option<u64> {
    paths.stop_requested().then(|| {
        std::fs::read_to_string(paths.stop_file())
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(u64::MAX)
    })
}

unsafe fn tick(hwnd: HWND) {
    let mut pointer = POINT::default();
    let mut bounds = RECT::default();
    let hovered = GetCursorPos(&mut pointer).is_ok()
        && GetClientRect(hwnd, &mut bounds).is_ok()
        && ScreenToClient(hwnd, &mut pointer).as_bool()
        && pointer.x >= paint::pixels(246, windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd))
        && pointer.x < bounds.right
        && pointer.y >= 0
        && pointer.y < bounds.bottom;
    if STOP_HOVERED.swap(hovered, Ordering::Relaxed) != hovered {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    let paths = SignalPaths::default();
    let now = unix_ms();
    let states = paths.read_calls();
    let stop = stop_timestamp(&paths);
    let v = model::view(&states, now, stop, actl_uia::display_support::input_busy());
    let changed = if let Ok(mut old) = VIEW.lock() {
        let changed = old.as_ref().is_none_or(|o| o != &v);
        *old = Some(v.clone());
        changed
    } else {
        false
    };
    if changed {
        let _ = InvalidateRect(Some(hwnd), None, false);
        let title = paint::wide(&format!("actl · {}", v.label));
        let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
        let mut nid = tray(hwnd);
        nid.uFlags = NIF_TIP | NIF_ICON;
        nid.hIcon = update_icon(hwnd, v.mode);
        let tip = paint::wide(&format!(
            "{}\n{}\n右键：更多",
            v.label,
            if HOTKEY.load(Ordering::Relaxed) {
                "Ctrl+Alt+F12 请求停止"
            } else {
                "点击方框停止"
            }
        ));
        let n = tip.len().saturating_sub(1).min(126);
        nid.szTip[..n].copy_from_slice(&tip[..n]);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
    let suppressed = if let Ok(mut captures) = CAPTURES.lock() {
        let had = !captures.is_empty();
        captures.retain(|pid| process_alive(*pid));
        if had && captures.is_empty() && WAS_VISIBLE.load(Ordering::Relaxed) {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        !captures.is_empty()
    } else {
        true
    };
    let visible = IsWindowVisible(hwnd).as_bool() && !suppressed;
    let selected = model::selected(&states, now);
    let preparing = selected
        .filter(|s| s.phase == actl_core::state::Phase::Running)
        .and_then(|s| {
            s.action
                .as_ref()
                .filter(|a| a.phase == ActionPhase::Preparing && a.expires_ms > now)
                .map(|a| (s, a))
        });
    if visible && (changed || preparing.is_some() || !FRAME_READY.load(Ordering::Relaxed)) {
        FRAME_READY.store(
            paint::draw(hwnd, &v, hovered) && windows::Win32::Graphics::Dwm::DwmFlush().is_ok(),
            Ordering::Relaxed,
        );
    }
    let ready = visible
        && FRAME_READY.load(Ordering::Relaxed)
        && actl_uia::display_support::surface_exposed(hwnd);
    publish_lease(
        &paths,
        if suppressed {
            "capture"
        } else if ready {
            "ready"
        } else {
            "hidden"
        },
    );
    if ready && stop.is_none() {
        if let Some((s, a)) = preparing {
            if s.call_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                && !s.call_id.is_empty()
            {
                let ack = actl_core::display::DisplayAck {
                    epoch: EPOCH
                        .get_or_init(actl_core::snapshot::new_snapshot_id)
                        .clone(),
                    call_id: s.call_id.clone(),
                    action_id: a.id,
                    ts_ms: unix_ms(),
                };
                let _ = paths.write_display(&format!("ack-{}.json", s.call_id), &ack);
            }
        }
    }
    if visible {
        let _ = std::fs::write(paths.dir.join("display-live"), now.to_string());
    } else {
        let _ = std::fs::remove_file(paths.dir.join("display-live"));
    }
    let events = CURSOR
        .lock()
        .ok()
        .map(|mut c| {
            c.get_or_insert_with(|| ActionCursor::new(now))
                .take(&states, now)
        })
        .unwrap_or_default();
    if suppressed || !visible {
        overlay::hide();
        return;
    }
    let key = preparing
        .map(|(s, a)| format!("{}:{}", s.call_id, a.id))
        .unwrap_or_default();
    let changed = if let Ok(mut old) = PREPARING.lock() {
        let changed = *old != key;
        *old = key;
        changed
    } else {
        false
    };
    if changed {
        overlay::hide();
        if let Some((_, a)) = preparing {
            overlay::show(a);
        }
    }
    // Coalesce bursts to the most recent delivered spatial action; never accumulate a screenful.
    if let Some(a) = events
        .iter()
        .rev()
        .find(|a| a.target.point.is_some() || a.target.bounds.is_some())
    {
        overlay::show(a);
    }
}

unsafe fn process_alive(pid: u32) -> bool {
    use windows::Win32::System::Threading::*;
    match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
        Ok(h) => {
            let alive = WaitForSingleObject(h, 0) != WAIT_OBJECT_0;
            let _ = CloseHandle(h);
            alive
        }
        Err(e) => e.code() != windows::core::HRESULT::from_win32(87),
    }
}

unsafe fn tray(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

unsafe fn update_icon(hwnd: HWND, mode: model::Mode) -> HICON {
    let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
    let size = windows::Win32::UI::HiDpi::GetSystemMetricsForDpi(SM_CXSMICON, dpi).max(16);
    let color = match mode {
        model::Mode::Failed => 0xE59188,
        model::Mode::StopRequested | model::Mode::Preparing | model::Mode::Partial => 0xD4B879,
        model::Mode::Done => 0x8AB698,
        model::Mode::Running => 0x93B4CA,
        _ => 0xB4BAC0,
    };
    let Ok(mut old) = TRAY_ICON.lock() else {
        return HICON::default();
    };
    if let Some((icon, s, c)) = *old {
        if s == size && c == color {
            return HICON(icon as *mut _);
        }
    }
    let Ok(icon) = super::icon::create(size, color) else {
        return old.map(|v| HICON(v.0 as *mut _)).unwrap_or_default();
    };
    if let Some((handle, _, _)) = old.replace((icon.0 as usize, size, color)) {
        let _ = DestroyIcon(HICON(handle as *mut _));
    }
    icon
}

unsafe fn details(hwnd: HWND) {
    let text = VIEW
        .lock()
        .ok()
        .and_then(|v| v.as_ref().map(|v| v.details.clone()))
        .unwrap_or_default();
    super::popup::show(hwnd, text.lines().map(str::to_owned).collect(), false);
}

unsafe fn menu(hwnd: HWND) {
    super::popup::show(
        hwnd,
        vec![
            "停止操作".into(),
            "允许后续操作".into(),
            "详情".into(),
            if IsWindowVisible(hwnd).as_bool() {
                "隐藏".into()
            } else {
                "显示".into()
            },
            "停止并退出".into(),
        ],
        true,
    );
}

unsafe fn menu_action(hwnd: HWND, cmd: usize) {
    match cmd {
        1 => SignalPaths::default().request_stop(),
        2 => SignalPaths::default().clear_stop(),
        3 => details(hwnd),
        4 => {
            let _ = ShowWindow(
                hwnd,
                if IsWindowVisible(hwnd).as_bool() {
                    SW_HIDE
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
        }
        5 => {
            SignalPaths::default().request_stop();
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
    tick(hwnd);
}

unsafe fn place(hwnd: HWND) {
    let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
    let w = paint::pixels(paint::WIDTH, dpi);
    let h = paint::pixels(paint::HEIGHT, dpi);
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return;
    }
    let mut rc = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rc);
    let x = rc.left.clamp(
        info.rcWork.left,
        (info.rcWork.right - w).max(info.rcWork.left),
    );
    let y = rc.top.clamp(
        info.rcWork.top,
        (info.rcWork.bottom - h).max(info.rcWork.top),
    );
    let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
    // Rounded edges come from the premultiplied alpha bitmap, never a hard region clip.
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            tick(hwnd);
            LRESULT(0)
        }
        WM_PAINT => {
            let view = VIEW
                .lock()
                .ok()
                .and_then(|v| v.clone())
                .unwrap_or_else(|| model::view(&[], unix_ms(), None, Some(false)));
            FRAME_READY.store(
                paint::draw(hwnd, &view, STOP_HOVERED.load(Ordering::Relaxed)),
                Ordering::Relaxed,
            );
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            let mut pt = POINT {
                x: (lp.0 as u16 as i16) as i32,
                y: ((lp.0 >> 16) as u16 as i16) as i32,
            };
            let _ = ScreenToClient(hwnd, &mut pt);
            let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
            // Only the middle text is a drag handle; the stop hit area is never draggable.
            LRESULT(
                if pt.x >= paint::pixels(31, dpi) && pt.x < paint::pixels(181, dpi) {
                    HTCAPTION
                } else {
                    HTCLIENT
                } as isize,
            )
        }
        WM_ENTERSIZEMOVE => {
            SignalPaths::default().request_stop();
            overlay::hide();
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            place(hwnd);
            let mut rc = RECT::default();
            if GetWindowRect(hwnd, &mut rc).is_ok() {
                let _ = std::fs::write(
                    SignalPaths::default().dir.join("signal-pos.txt"),
                    format!("{},{}", rc.left, rc.top),
                );
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = (lp.0 as u16 as i16) as i32;
            if x >= paint::pixels(246, windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd)) {
                let paths = SignalPaths::default();
                if !paths.stop_requested() {
                    paths.request_stop();
                }
                tick(hwnd);
            } else {
                details(hwnd);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            menu(hwnd);
            LRESULT(0)
        }
        WM_HOTKEY => {
            SignalPaths::default().request_stop();
            tick(hwnd);
            LRESULT(0)
        }
        WM_APP => {
            if (lp.0 as u32 & 0xffff) == WM_RBUTTONUP {
                menu(hwnd);
            } else if (lp.0 as u32 & 0xffff) == WM_LBUTTONUP {
                details(hwnd);
            }
            LRESULT(0)
        }
        super::popup::CHOOSE => {
            menu_action(hwnd, wp.0);
            LRESULT(0)
        }
        CAPTURE_HIDE => {
            super::popup::hide();
            publish_lease(&SignalPaths::default(), "capture");
            if let Ok(mut captures) = CAPTURES.lock() {
                if captures.is_empty() {
                    WAS_VISIBLE.store(IsWindowVisible(hwnd).as_bool(), Ordering::Relaxed);
                }
                captures.push(wp.0 as u32);
            }
            let _ = ShowWindow(hwnd, SW_HIDE);
            overlay::hide();
            LRESULT(1)
        }
        CAPTURE_RESTORE => {
            if let Ok(mut captures) = CAPTURES.lock() {
                if let Some(i) = captures.iter().position(|pid| *pid == wp.0 as u32) {
                    captures.remove(i);
                }
                if captures.is_empty() && WAS_VISIBLE.load(Ordering::Relaxed) {
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            place(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            SignalPaths::default().request_stop();
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            super::popup::hide();
            overlay::hide();
            let _ = KillTimer(Some(hwnd), 1);
            let _ = UnregisterHotKey(Some(hwnd), 1);
            let _ = Shell_NotifyIconW(NIM_DELETE, &tray(hwnd));
            if let Ok(mut icon) = TRAY_ICON.lock() {
                if let Some((h, _, _)) = icon.take() {
                    let _ = DestroyIcon(HICON(h as *mut _));
                }
            }
            publish_lease(&SignalPaths::default(), "closed");
            let _ = std::fs::remove_file(SignalPaths::default().dir.join("display-live"));
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn run() -> windows::core::Result<()> {
    struct Instance(HANDLE);
    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let instance_lock = Instance(windows::Win32::System::Threading::CreateMutexW(
        None,
        false,
        w!("Local\\actl-display-instance"),
    )?);
    if GetLastError() == ERROR_ALREADY_EXISTS {
        return Ok(());
    }
    let _instance_lock = instance_lock;
    let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
        windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    );
    if FindWindowW(w!("actl_signal_window"), None).is_ok() {
        return Ok(());
    }
    let instance = GetModuleHandleW(None)?;
    let class = w!("actl_signal_window");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: class,
        ..Default::default()
    };
    if RegisterClassW(&wc) == 0 {
        return Err(windows::core::Error::from_thread());
    }
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    let position = std::fs::read_to_string(SignalPaths::default().dir.join("signal-pos.txt"))
        .ok()
        .and_then(|s| {
            let (x, y) = s.split_once(',')?;
            Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))
        });
    let (x, y) = position.unwrap_or((work.right - 320, work.top + 16));
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        class,
        w!("actl"),
        WS_POPUP,
        x,
        y,
        280,
        34,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    // actl screenshots use an acknowledged hide; human screen sharing may show the strip.
    place(hwnd);
    if let Ok(mut c) = CURSOR.lock() {
        *c = Some(ActionCursor::new(unix_ms()));
    }
    let mut nid = tray(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP;
    nid.hIcon = update_icon(hwnd, model::Mode::Idle);
    let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    HOTKEY.store(
        RegisterHotKey(Some(hwnd), 1, MOD_CONTROL | MOD_ALT, VK_F12.0 as u32).is_ok(),
        Ordering::Relaxed,
    );
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    tick(hwnd);
    SetTimer(Some(hwnd), 1, 100, None);
    let mut msg = MSG::default();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0).0;
        if result == -1 {
            return Err(windows::core::Error::from_thread());
        }
        if result == 0 {
            break;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_stop_file_still_means_stop_requested() {
        let dir = std::env::temp_dir().join(format!(
            "actl-stop-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        let paths = SignalPaths::at(dir.clone());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(paths.stop_file(), "").unwrap();
        let observed = stop_timestamp(&paths);
        let _ = std::fs::remove_dir_all(dir);
        assert!(observed.is_some());
    }
}
