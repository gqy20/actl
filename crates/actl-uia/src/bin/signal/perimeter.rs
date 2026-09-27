//! Peripheral status light: four narrow, click-through surfaces per affected monitor.
use super::{
    model::{Mode, View},
    paint,
};
use actl_core::state::{Phase, SessionState};
use std::sync::Mutex;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{BOOL, w},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tone {
    Running,
    Attention,
    Paused,
}
fn tone(view: &View, states: &[SessionState], now: u64) -> Option<Tone> {
    let fresh = states
        .iter()
        .any(|s| s.phase == Phase::Running && now.saturating_sub(s.ts_ms) <= 10_000);
    match view.mode {
        Mode::Running | Mode::Preparing if fresh => Some(Tone::Running),
        Mode::StopRequested if fresh => Some(Tone::Attention),
        Mode::Partial | Mode::Failed => super::model::selected(states, now)
            .filter(|s| {
                s.workflow
                    .as_ref()
                    .is_some_and(|f| matches!(f.status.as_str(), "needs_review" | "failed"))
            })
            .map(|_| Tone::Attention),
        Mode::Idle => super::model::selected(states, now)
            .and_then(|s| s.workflow.as_ref())
            .and_then(|f| {
                (f.status == "paused").then_some(
                    if matches!(
                        f.reason.as_deref(),
                        Some(
                            "checkpoint" | "start_paused" | "precondition_failed" | "action_failed"
                        )
                    ) {
                        Tone::Attention
                    } else {
                        Tone::Paused
                    },
                )
            }),
        _ => None,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Screen {
    bounds: [i32; 4],
    dpi: u32,
}
struct Strip {
    hwnd: usize,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}
struct Frame {
    screens: Vec<Screen>,
    tone: Tone,
    strips: Vec<Strip>,
    fading: Option<(u64, u8)>,
    alpha: u8,
}
static FRAME: Mutex<Option<Frame>> = Mutex::new(None);

fn intersects(a: [i32; 4], b: [i32; 4]) -> bool {
    a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]
}
unsafe extern "system" fn enumerate(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(m, &mut info).as_bool() {
        let (mut dx, mut dy) = (96, 96);
        let _ = GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let r = info.rcMonitor;
        (*(data.0 as *mut Vec<Screen>)).push(Screen {
            bounds: [r.left, r.top, r.right, r.bottom],
            dpi: dx,
        });
    }
    BOOL(1)
}
unsafe fn screens(owner: HWND, states: &[SessionState], now: u64) -> Vec<Screen> {
    let mut all = Vec::<Screen>::new();
    let _ = EnumDisplayMonitors(
        None,
        None,
        Some(enumerate),
        LPARAM((&mut all as *mut Vec<Screen>) as isize),
    );
    let mut targets = vec![];
    for state in states
        .iter()
        .filter(|s| s.phase == Phase::Running && now.saturating_sub(s.ts_ms) <= 10_000)
    {
        if let Some(a) = &state.action {
            if let Some(b) = a.target.bounds {
                targets.push(b);
            }
            for [x, y] in [a.target.point, a.target.destination].into_iter().flatten() {
                targets.push([x, y, x.saturating_add(1), y.saturating_add(1)]);
            }
        }
    }
    // No target geometry (read-only waits / checkpoints): use the status bar's monitor.
    if targets.is_empty() {
        let mut r = RECT::default();
        if GetWindowRect(owner, &mut r).is_ok() {
            targets.push([r.left, r.top, r.right, r.bottom]);
        }
    }
    all.retain(|s| targets.iter().any(|t| intersects(s.bounds, *t)));
    all
}
fn strips(width: i32, height: i32, band: i32) -> [[i32; 4]; 4] {
    [
        [0, 0, width, band],
        [0, height - band, width, band],
        [0, band, band, height - 2 * band],
        [width - band, band, band, height - 2 * band],
    ]
}
// Rounded rectangle signed distance; premultiplied BGRA, zero pixels beyond the inner glow.
fn raster(width: i32, height: i32, part: [i32; 4], dpi: u32, tone: Tone) -> Vec<u8> {
    let scale = dpi.max(96) as f32 / 96.;
    let radius = 18. * scale;
    let rgb = match tone {
        Tone::Running => [145., 185., 209.],
        Tone::Attention => [197., 163., 107.],
        Tone::Paused => [115., 129., 140.],
    };
    let mut out = vec![0; (part[2] * part[3] * 4) as usize];
    for y in 0..part[3] {
        for x in 0..part[2] {
            let px = (part[0] + x) as f32 + 0.5;
            let py = (part[1] + y) as f32 + 0.5;
            let qx = (px - width as f32 / 2.).abs() - (width as f32 / 2. - radius - 1.);
            let qy = (py - height as f32 / 2.).abs() - (height as f32 / 2. - radius - 1.);
            let distance = qx.max(0.).hypot(qy.max(0.)) + qx.max(qy).min(0.) - radius;
            let line = (1. - distance.abs() / (1.25 * scale)).clamp(0., 1.);
            let glow = if tone == Tone::Paused || distance > 0. {
                0.
            } else {
                (distance / (7. * scale)).exp() * 0.18
            };
            let alpha = ((line * 0.62 + glow).min(0.78) * 255.) as u8;
            let i = ((y * part[2] + x) * 4) as usize;
            out[i] = (rgb[2] * alpha as f32 / 255.) as u8;
            out[i + 1] = (rgb[1] * alpha as f32 / 255.) as u8;
            out[i + 2] = (rgb[0] * alpha as f32 / 255.) as u8;
            out[i + 3] = alpha;
        }
    }
    out
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn destroy(frame: Frame) {
    for strip in frame.strips {
        let _ = DestroyWindow(HWND(strip.hwnd as *mut _));
    }
}
pub unsafe fn hide() {
    if let Some(frame) = FRAME.lock().ok().and_then(|mut f| f.take()) {
        destroy(frame);
    }
}
unsafe fn create(screens: Vec<Screen>, tone: Tone) -> windows::core::Result<Frame> {
    let instance = GetModuleHandleW(None)?;
    let class = w!("actl_signal_perimeter");
    let _ = RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    });
    let mut frame = Frame {
        screens,
        tone,
        strips: vec![],
        fading: None,
        alpha: 0,
    };
    let result = (|| -> windows::core::Result<()> {
        for screen in &frame.screens {
            let [left, top, right, bottom] = screen.bounds;
            let (width, height) = (right - left, bottom - top);
            let band = paint::pixels(32, screen.dpi);
            if width <= 2 * band || height <= 2 * band || width > 32000 || height > 32000 {
                continue;
            }
            for part in strips(width, height, band) {
                let hwnd = CreateWindowExW(
                    WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOOLWINDOW
                        | WS_EX_TOPMOST,
                    class,
                    w!("actl screen edge"),
                    WS_POPUP,
                    left + part[0],
                    top + part[1],
                    part[2],
                    part[3],
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )?;
                let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
                frame.strips.push(Strip {
                    hwnd: hwnd.0 as usize,
                    width: part[2],
                    height: part[3],
                    pixels: raster(width, height, part, screen.dpi, tone),
                });
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        destroy(frame);
        return Err(error);
    }
    Ok(frame)
}
fn opacity(tone: Tone, now: u64, fade: Option<(u64, u8)>, reduced: bool) -> u8 {
    if let Some((start, initial)) = fade {
        return (initial as f32 * (1. - now.saturating_sub(start) as f32 / 600.).clamp(0., 1.))
            as u8;
    }
    if tone != Tone::Running || reduced {
        255
    } else {
        (215. + 25. * ((now % 3600) as f32 / 3600. * std::f32::consts::TAU).sin()) as u8
    }
}
pub unsafe fn update(owner: HWND, view: &View, states: &[SessionState], now: u64, visible: bool) {
    if !visible {
        hide();
        return;
    }
    let desired = tone(view, states, now);
    let Ok(mut current) = FRAME.lock() else {
        return;
    };
    if let Some(tone) = desired {
        let monitors = screens(owner, states, now);
        if current
            .as_ref()
            .is_none_or(|f| f.screens != monitors || f.tone != tone)
        {
            if let Some(old) = current.take() {
                destroy(old);
            }
            match create(monitors, tone) {
                Ok(frame) => *current = Some(frame),
                Err(e) => eprintln!("INTERNAL: screen edge: {e}"),
            }
        }
        if let Some(f) = current.as_mut() {
            f.fading = None;
        }
    }
    let Some(frame) = current.as_mut() else {
        return;
    };
    let mut animation = BOOL(1);
    let _ = SystemParametersInfoW(
        SPI_GETCLIENTAREAANIMATION,
        0,
        Some((&mut animation as *mut BOOL).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    if desired.is_none() {
        if !animation.as_bool() {
            if let Some(old) = current.take() {
                destroy(old);
            }
            return;
        }
        frame.fading.get_or_insert((now, frame.alpha));
    }
    let alpha = opacity(frame.tone, now, frame.fading, !animation.as_bool());
    if alpha == 0 {
        if let Some(old) = current.take() {
            destroy(old);
        }
        return;
    }
    if alpha != frame.alpha {
        for strip in &frame.strips {
            let hwnd = HWND(strip.hwnd as *mut _);
            if paint::present_alpha(hwnd, &strip.pixels, 96, strip.width, strip.height, alpha)
                .is_ok()
            {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
        }
        frame.alpha = alpha;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_states_choose_distinct_static_tones() {
        let mut state = SessionState::now(1, "flow-run", None, None, Phase::Done);
        for (status, reason, expected) in [
            ("paused", "pause_requested", Some(Tone::Paused)),
            ("paused", "checkpoint", Some(Tone::Attention)),
            ("needs_review", "verification_failed", Some(Tone::Attention)),
            ("completed", "", None),
        ] {
            state.workflow = Some(serde_json::from_value(serde_json::json!({"run_id":"test","title":"test","status":status,"reason":reason})).unwrap());
            let states = std::slice::from_ref(&state);
            let view = super::super::model::view(states, state.ts_ms, None, Some(false));
            assert_eq!(tone(&view, states, state.ts_ms), expected);
        }
    }
    #[test]
    #[ignore = "requires an interactive desktop; briefly displays native screen edges"]
    fn native_edges_do_not_take_focus_or_intercept_hit_tests() {
        unsafe {
            let old_dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let foreground = GetForegroundWindow();
            let mut monitors = Vec::<Screen>::new();
            assert!(
                EnumDisplayMonitors(
                    None,
                    None,
                    Some(enumerate),
                    LPARAM((&mut monitors as *mut Vec<Screen>) as isize)
                )
                .as_bool()
            );
            let screen = monitors[0];
            for tone in [Tone::Running, Tone::Attention, Tone::Paused] {
                let frame = create(vec![screen], tone).unwrap();
                assert_eq!(frame.strips.len(), 4);
                for strip in &frame.strips {
                    let hwnd = HWND(strip.hwnd as *mut _);
                    paint::present_alpha(hwnd, &strip.pixels, 96, strip.width, strip.height, 255)
                        .unwrap();
                    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                    assert_ne!(style & WS_EX_TRANSPARENT.0, 0);
                    assert_ne!(style & WS_EX_NOACTIVATE.0, 0);
                    let mut rect = RECT::default();
                    GetWindowRect(hwnd, &mut rect).unwrap();
                    assert_ne!(
                        WindowFromPoint(POINT {
                            x: (rect.left + rect.right) / 2,
                            y: rect.top + 1
                        }),
                        hwnd
                    );
                    assert!(actl_uia::display_support::is_display_window(hwnd));
                }
                windows::Win32::Graphics::Dwm::DwmFlush().unwrap();
                assert_eq!(GetForegroundWindow(), foreground);
                std::thread::sleep(std::time::Duration::from_millis(350));
                destroy(frame);
            }
            let _ = SetThreadDpiAwarenessContext(old_dpi);
        }
    }
    #[test]
    fn geometry_and_pixels_are_bounded() {
        assert!(intersects([-1920, 0, 0, 1080], [-10, 100, 100, 200]));
        assert!(!intersects([-1920, 0, 0, 1080], [0, 100, 100, 200]));
        let parts = strips(1920, 1080, 32);
        assert_eq!(
            parts.iter().map(|p| p[2] * p[3]).sum::<i32>(),
            1920 * 1080 - (1920 - 64) * (1080 - 64)
        );
        let data = raster(1920, 1080, parts[0], 96, Tone::Running);
        assert!(
            data.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
        );
        assert!(data[(31 * 1920 + 960) * 4 + 3] < 3);
        assert!(data[(1920 + 960) * 4 + 3] > 100);
    }
    #[test]
    fn motion_and_completion() {
        assert_eq!(opacity(Tone::Paused, 100, None, false), 255);
        assert_eq!(opacity(Tone::Running, 100, Some((100, 190)), false), 190);
        assert_eq!(opacity(Tone::Running, 100, None, true), 255);
        assert_eq!(opacity(Tone::Running, 700, Some((100, 215)), false), 0);
    }
    #[test]
    fn stale_and_idle_do_not_light_the_screen() {
        let s = SessionState::now(1, "wait", None, None, Phase::Running);
        let now = s.ts_ms + 11000;
        let view = super::super::model::view(std::slice::from_ref(&s), now, None, Some(false));
        assert_eq!(tone(&view, &[s], now), None);
        let view = super::super::model::view(&[], now, Some(now), Some(false));
        assert_eq!(tone(&view, &[], now), None);
    }
}
