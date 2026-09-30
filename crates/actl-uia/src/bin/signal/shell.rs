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

static PRESSED: Mutex<Option<model::Control>> = Mutex::new(None);
static VIEW: Mutex<Option<model::View>> = Mutex::new(None);
static REQUEST_MESSAGE: Mutex<String> = Mutex::new(String::new());
static MENU_FLOW: Mutex<Option<(String, bool)>> = Mutex::new(None);
static CURSOR: Mutex<Option<ActionCursor>> = Mutex::new(None);
static PREPARING: Mutex<String> = Mutex::new(String::new());
static CAPTURES: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static WAS_VISIBLE: AtomicBool = AtomicBool::new(false);
static HOTKEY: AtomicBool = AtomicBool::new(false);
static STOP_HOVERED: AtomicBool = AtomicBool::new(false);
static DRAG_STOP: AtomicBool = AtomicBool::new(false);
static FRAME_READY: AtomicBool = AtomicBool::new(false);
static TRAY_ICON: Mutex<Option<(usize, i32, u32)>> = Mutex::new(None);
static EPOCH: std::sync::OnceLock<String> = std::sync::OnceLock::new();

// Normal replacement is distinct from user close. Older displays do not acknowledge
// this message; never fall back to WM_CLOSE or clear a pre-existing stop request.
const REPLACE_CLOSE: u32 = WM_APP + 45;

fn allow_close(_paths: &SignalPaths, message: u32, input_busy: Option<bool>) -> bool {
    message == WM_CLOSE || (message == REPLACE_CLOSE && input_busy == Some(false))
}

pub(super) fn request_emergency_stop(source: &str) -> Result<(), String> {
    super::handoff::revoke();
    let lease = SignalPaths::default().display_lease();
    SignalPaths::default()
        .request_stop_from(
            source,
            EPOCH
                .get()
                .map(String::as_str)
                .or_else(|| lease.as_ref().map(|l| l.epoch.as_str())),
        )
        .map_err(|e| format!("INTERNAL：{}", e.message))
}

pub(super) fn clear_emergency_stop() -> Result<(), String> {
    super::handoff::revoke();
    actl_uia::display_gate::recover_execution()
        .map(|_| ())
        .map_err(|e| e.message)
}

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

pub(super) fn stop_timestamp(paths: &SignalPaths) -> Option<u64> {
    paths.stop_timestamp()
}

// Filesystem snapshots arrive on a worker; the window thread only takes the latest value.
fn call_snapshot() -> Vec<actl_core::state::SessionState> {
    type States = Vec<actl_core::state::SessionState>;
    static CACHE: std::sync::OnceLock<std::sync::Arc<Mutex<States>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        let cache = std::sync::Arc::new(Mutex::new(vec![]));
        let output = cache.clone();
        if std::thread::Builder::new()
            .name("signal-history-retention".into())
            .spawn(|| {
                loop {
                    if let Err(error) = actl_core::history::maintain(&SignalPaths::default().dir) {
                        eprintln!("INTERNAL: display history retention failed: {error}");
                    }
                    std::thread::sleep(actl_core::history::RETENTION_INTERVAL);
                }
            })
            .is_err()
        {
            eprintln!("INTERNAL: cannot start display history retention");
        }
        if std::thread::Builder::new()
            .name("signal-state-reader".into())
            .spawn(move || {
                loop {
                    let paths = SignalPaths::default();
                    let mut states = paths.read_calls();
                    super::flow_display::retain(&paths, &mut states);
                    if let Ok(mut latest) = output.lock() {
                        *latest = states;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            })
            .is_err()
        {
            eprintln!("INTERNAL: cannot start display state reader");
        }
        cache
    });
    cache
        .lock()
        .map(|states| states.clone())
        .unwrap_or_default()
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
    let states = call_snapshot();
    let stop = stop_timestamp(&paths);
    let mut v = model::view(&states, now, stop, actl_uia::display_support::input_busy());
    v.details.push_str(&paths.stop_details());
    if paths.preauthorized() {
        // 常驻预授权的可见性补偿:横条常显模式标记,动作预告照常上屏。
        v.label = format!("预授权·{}", v.label);
    }
    if matches!(
        v.mode,
        model::Mode::Running | model::Mode::Background | model::Mode::Desktop
    ) && let Some(flow) = model::selected(&states, now).and_then(|s| s.workflow.as_ref())
        && actl_core::history::valid_id(&flow.run_id)
        && paths
            .dir
            .join("flows")
            .join(&flow.run_id)
            .join("pause-requested")
            .exists()
    {
        v.label = "正在暂停".into();
        v.details.push_str("\n等待当前操作结束");
    }
    if stop.is_none() {
        DRAG_STOP.store(false, Ordering::Relaxed);
    }
    if DRAG_STOP.load(Ordering::Relaxed) {
        v.label = "移动状态条 · 已请求暂停".into();
        v.details.push_str("\n移动后已请求暂停；继续时请重新确认");
    }
    let request_message = super::flow_ui::message();
    if !request_message.is_empty() {
        v.details.push_str(&format!("\n{request_message}"));
        let is_new = REQUEST_MESSAGE
            .lock()
            .map(|mut old| {
                let changed = *old != request_message;
                *old = request_message.clone();
                changed
            })
            .unwrap_or(false);
        if is_new && request_message.contains('：') {
            super::popup::show(
                hwnd,
                vec!["流程操作未完成".into(), request_message.clone()],
                false,
            );
        }
    }
    let changed = if let Ok(mut old) = VIEW.lock() {
        let changed = old.as_ref().is_none_or(|o| {
            o.mode != v.mode
                || o.label != v.label
                || o.effects != v.effects
                || o.control != v.control
        });
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
                "右键菜单：紧急停止"
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
    if ready
        && stop.is_none()
        && let Some((s, a)) = preparing
        && s.call_id
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
        super::perimeter::hide();
        return;
    }
    super::perimeter::update(hwnd, &v, &states, now, ready);
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
        model::Mode::Failed | model::Mode::Blocked => 0xE59188,
        model::Mode::StopRequested
        | model::Mode::Preparing
        | model::Mode::Partial
        | model::Mode::Desktop
        | model::Mode::Paused
        | model::Mode::Waiting => 0xD4B879,
        model::Mode::Done => 0x8AB698,
        model::Mode::Running | model::Mode::Background => 0x93B4CA,
        _ => 0xB4BAC0,
    };
    let Ok(mut old) = TRAY_ICON.lock() else {
        return HICON::default();
    };
    if let Some((icon, s, c)) = *old
        && s == size
        && c == color
    {
        return HICON(icon as *mut _);
    }
    let Ok(icon) = super::icon::create(size, color) else {
        return old.map(|v| HICON(v.0 as *mut _)).unwrap_or_default();
    };
    if let Some((handle, _, _)) = old.replace((icon.0 as usize, size, color)) {
        let _ = DestroyIcon(HICON(handle as *mut _));
    }
    icon
}

unsafe fn details(_hwnd: HWND) {
    use std::os::windows::process::CommandExt;
    if let Ok(exe) = std::env::current_exe() {
        let mut command = std::process::Command::new(exe);
        command.arg("--controls").creation_flags(0x08000000);
        if let Some(id) = super::flow_ui::discover(&SignalPaths::default()) {
            command.args(["--flow", &id]);
        }
        if command.spawn().is_err() {
            super::popup::show(_hwnd, vec!["INTERNAL：无法打开详情".into()], false);
        }
    }
}

unsafe fn menu(hwnd: HWND) {
    let mut lines = vec![
        "紧急停止".into(),
        "修复运行状态".into(),
        "任务详情".into(),
        if IsWindowVisible(hwnd).as_bool() {
            "隐藏".into()
        } else {
            "显示".into()
        },
        "关闭".into(),
    ];
    let paths = SignalPaths::default();
    let action = super::flow_ui::discover(&paths).and_then(|id| {
        let state = super::flow_ui::read(&paths, &id)?;
        match state["status"].as_str()? {
            "running" => Some((id, false)),
            "paused" | "needs_review" => Some((id, true)),
            _ => None,
        }
    });
    lines.push(if paths.preauthorized() {
        "预授权写入：开（点此关闭）".into()
    } else {
        "预授权写入：关（点此开启）".into()
    });
    if let Some((_, resume)) = &action {
        lines.push(if *resume { "继续" } else { "暂停" }.into());
    }
    if let Ok(mut stored) = MENU_FLOW.lock() {
        *stored = action;
    }
    super::popup::show(hwnd, lines, true);
}

unsafe fn menu_action(hwnd: HWND, cmd: usize) {
    match cmd {
        1 | 2 => {
            let result = if cmd == 1 {
                request_emergency_stop("menu")
            } else {
                clear_emergency_stop()
            };
            if let Err(error) = result {
                super::popup::show(hwnd, vec!["急停状态更新失败".into(), error], false);
            }
        }
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
            super::handoff::revoke();
            publish_lease(&SignalPaths::default(), "closed");
            let _ = DestroyWindow(hwnd);
        }
        6 => {
            // 预授权开关(固定第 6 项,流程操作在它之后动态追加):授予动作只发生在
            // 显示端 UI(本菜单),agent/CLI 无写入通道。关闭时立即撤销常驻同意,
            // 回到点击模式;急停在任何模式下都有效。
            let paths = SignalPaths::default();
            let next = !paths.preauthorized();
            match paths.set_preauth(next) {
                Ok(()) => {
                    if !next {
                        super::handoff::revoke();
                    }
                    super::popup::show(
                        hwnd,
                        vec![if next {
                            "预授权写入已开启：桌面写入不再要求开始；让行与急停仍然有效".into()
                        } else {
                            "预授权写入已关闭".into()
                        }],
                        false,
                    );
                }
                Err(error) => super::popup::show(
                    hwnd,
                    vec!["预授权开关写入失败".into(), error.to_string()],
                    false,
                ),
            }
        }
        7 => {
            let action = MENU_FLOW.lock().ok().and_then(|a| a.clone());
            if let Some((id, resume)) = action {
                let result = flow_action(&id, resume);
                if let Err(error) = result {
                    super::popup::show(hwnd, vec!["流程操作未完成".into(), error], false);
                }
            }
        }
        _ => {}
    }
    tick(hwnd);
}

unsafe fn primary(hwnd: HWND, control: model::Control) {
    let result = match control {
        model::Control::Pause { call, flow } => {
            super::handoff::pause_call(&call);
            match flow {
                Some(id) => flow_action(&id, false),
                None => Ok(()),
            }
        }
        model::Control::Continue { flow } => {
            let current = VIEW
                .lock()
                .ok()
                .and_then(|v| v.as_ref().map(|v| v.control.clone()));
            if current != Some(model::Control::Continue { flow: flow.clone() }) {
                super::popup::show(hwnd, vec!["显示任务已变化，请重新查看".into()], false);
                return;
            }
            // Re-read persisted state. A stale button must never revive completed/stopped work.
            let state = super::flow_ui::read(&SignalPaths::default(), &flow);
            if !state
                .as_ref()
                .is_some_and(|s| matches!(s["status"].as_str(), Some("paused" | "needs_review")))
            {
                Err("任务状态已变化，请重新查看".into())
            } else if !state
                .as_ref()
                .is_some_and(|s| super::flow_ui::continue_ready(s, unix_ms()))
            {
                Err("刚刚暂停，请确认任务状态后再点继续".into())
            } else if super::flow_ui::pending(&flow, true) {
                Ok(())
            } else {
                let result = flow_action(&flow, true);
                if result.is_err() {
                    super::handoff::revoke();
                }
                result
            }
        }
        model::Control::Details => {
            details(hwnd);
            Ok(())
        }
    };
    if let Err(error) = result {
        super::popup::show(hwnd, vec!["任务状态未改变".into(), error], false);
    }
}

pub(super) fn flow_action(id: &str, resume: bool) -> Result<(), String> {
    let paths = SignalPaths::default();
    let generation = if resume {
        let state = super::flow_ui::read(&paths, id);
        super::flow_ui::resume_guard(state.as_ref(), unix_ms())?;
        super::flow_start::allow(id)?
    } else {
        paths.execution_generation().map_err(|e| e.message)?
    };
    super::flow_ui::request(id, resume, &generation)
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
        super::flow_start::MESSAGE => LRESULT(isize::from(super::flow_start::receive(wp.0 as u32))),
        actl_core::handoff::REQUEST_MESSAGE
        | actl_core::handoff::CHECK_MESSAGE
        | actl_core::handoff::FOREGROUND_MESSAGE => {
            LRESULT(super::handoff::message(msg, wp.0 as u32) as isize)
        }
        actl_core::handoff::RECOVER_MESSAGE => {
            place(hwnd);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            LRESULT(1)
        }
        msg if msg == WM_APP + 44 => {
            menu(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            super::handoff::tick();
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
            DRAG_STOP.store(true, Ordering::Relaxed);
            let control = VIEW
                .lock()
                .ok()
                .and_then(|v| v.as_ref().map(|v| v.control.clone()));
            if let Some(control @ model::Control::Pause { .. }) = control {
                primary(hwnd, control);
            }
            overlay::hide();
            super::perimeter::hide();
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
        WM_LBUTTONDOWN => {
            let x = (lp.0 as u16 as i16) as i32;
            if x >= paint::pixels(246, windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd))
                && let Ok(mut pressed) = PRESSED.lock()
            {
                *pressed = VIEW
                    .lock()
                    .ok()
                    .and_then(|v| v.as_ref().map(|v| v.control.clone()));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let x = (lp.0 as u16 as i16) as i32;
            let captured = PRESSED.lock().ok().and_then(|mut p| p.take());
            if x >= paint::pixels(246, windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd)) {
                if let Some(control) = captured {
                    primary(hwnd, control);
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
            if let Err(error) = request_emergency_stop("hotkey") {
                super::popup::show(hwnd, vec!["急停保存失败".into(), error], false);
            }
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
            super::perimeter::hide();
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
        WM_CLOSE | REPLACE_CLOSE => {
            if allow_close(
                &SignalPaths::default(),
                msg,
                actl_uia::display_support::input_busy(),
            ) {
                super::handoff::revoke();
                publish_lease(&SignalPaths::default(), "closed");
                let _ = DestroyWindow(hwnd);
                LRESULT(1)
            } else {
                LRESULT(0)
            }
        }

        WM_DESTROY => {
            super::handoff::revoke();
            super::popup::hide();
            overlay::hide();
            super::perimeter::hide();
            let _ = KillTimer(Some(hwnd), 1);
            let _ = UnregisterHotKey(Some(hwnd), 1);
            let _ = Shell_NotifyIconW(NIM_DELETE, &tray(hwnd));
            if let Ok(mut icon) = TRAY_ICON.lock()
                && let Some((h, _, _)) = icon.take()
            {
                let _ = DestroyIcon(HICON(h as *mut _));
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
    if std::env::args().any(|a| a == "--menu")
        && !std::env::args().any(|a| a == "--replace")
        && let Ok(existing) = FindWindowW(w!("actl_signal_window"), None)
    {
        PostMessageW(Some(existing), WM_APP + 44, WPARAM(0), LPARAM(0))?;
        return Ok(());
    }
    if std::env::args().any(|a| a == "--replace") {
        if actl_uia::display_support::input_busy() != Some(false) {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80004005u32 as i32),
                "input busy; replacement refused",
            ));
        }
        if let Ok(old) = FindWindowW(w!("actl_signal_window"), None) {
            let mut pid = 0;
            GetWindowThreadProcessId(old, Some(&mut pid));
            let process = windows::Win32::System::Threading::OpenProcess(
                windows::Win32::System::Threading::PROCESS_SYNCHRONIZE,
                false,
                pid,
            )?;
            let mut acknowledged = 0;
            let sent = SendMessageTimeoutW(
                old,
                REPLACE_CLOSE,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                2000,
                Some(&mut acknowledged),
            );
            let waited = if sent.0 != 0 && acknowledged == 1 {
                windows::Win32::System::Threading::WaitForSingleObject(process, 4000)
            } else {
                WAIT_FAILED
            };
            let _ = CloseHandle(process);
            if waited != WAIT_OBJECT_0 {
                return Err(windows::core::Error::new(
                    windows::core::HRESULT(0x80004005u32 as i32),
                    "display replacement not acknowledged or exit timed out; old builds require explicit user close (stop state preserved)",
                ));
            }
        }
    }
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
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(Some(hwnd), 1, 100, None);
    if std::env::args().any(|a| a == "--menu") {
        menu(hwnd);
    }
    let _input_watch = super::handoff::watch()?;
    let mut msg = MSG::default();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0).0;
        if result == -1 {
            return Err(windows::core::Error::from_thread());
        }
        if result == 0 {
            break;
        }
        if !super::popup::dialog_message(&msg) {
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
    fn revoke_message_does_not_collide_with_handoff_or_close() {
        for message in [
            actl_core::handoff::REQUEST_MESSAGE,
            actl_core::handoff::CHECK_MESSAGE,
            actl_core::handoff::RECOVER_MESSAGE,
            REPLACE_CLOSE,
            CAPTURE_HIDE,
            CAPTURE_RESTORE,
        ] {
            assert_ne!(crate::flow_start::MESSAGE, message);
        }
    }
    #[test]
    fn replacement_must_not_send_user_close() {
        let source = include_str!("shell.rs");
        let replacement = source.split("pub unsafe fn run()").nth(1).unwrap();
        let replacement = replacement.split("struct Instance").next().unwrap();
        assert!(
            !replacement.contains("PostMessageW(Some(old), WM_CLOSE"),
            "replacement must not take the user-close path that sets emergency stop"
        );
    }
    #[test]
    fn ordinary_close_does_not_create_stop_and_preserves_existing_stop() {
        let dir = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
        let paths = SignalPaths::at(dir.clone());
        assert!(allow_close(&paths, REPLACE_CLOSE, Some(false)));
        assert!(!paths.stop_requested());
        assert!(!allow_close(&paths, REPLACE_CLOSE, Some(true)));
        assert!(!allow_close(&paths, REPLACE_CLOSE, None));
        assert!(!paths.stop_requested());
        assert!(allow_close(&paths, WM_CLOSE, Some(true)));
        assert!(
            !paths.stop_requested(),
            "ordinary close must not create sticky emergency stop"
        );
        paths.request_stop().unwrap();
        let original = std::fs::read(paths.stop_file()).unwrap();
        assert!(allow_close(&paths, REPLACE_CLOSE, Some(false)));
        assert_eq!(std::fs::read(paths.stop_file()).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }
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
