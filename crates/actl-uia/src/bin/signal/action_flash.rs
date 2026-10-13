//! Task lifecycle notifications; no per-action prompts or execution/consent state writes.
use super::flash_paint::{self, FlashRaster};
use super::paint;
use super::task_notice::{Cursor, Kind, Notice};
use actl_core::state::{SessionState, SignalPaths};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*};
use windows::core::{BOOL, w};

const TIMER_ID: usize = 42;
const SLIDE_IN_MS: u32 = 600;
const HOLD_MS: u32 = 1800;
const SLIDE_OUT_MS: u32 = 500;

#[derive(Clone)]
struct Flash {
    hwnd: usize,
    target_x: i32,
    y: i32,
    distance: i32,
    hold_ms: u32,
    started: std::time::Instant,
    raster: Arc<FlashRaster>,
    reduced: bool,
    last_frame: Option<(i32, i32, u8, u8)>,
}
static FLASH: Mutex<Option<Flash>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
struct Motion {
    offset: i32,
    alpha: u8,
    light: u8,
}
/// One-third -> center -> two-thirds. Text never scales or blurs during travel.
fn frame(elapsed: u32, reduced: bool, distance: i32, hold: u32) -> Option<Motion> {
    if elapsed >= SLIDE_IN_MS + hold + SLIDE_OUT_MS {
        return None;
    }
    if reduced {
        return Some(Motion {
            offset: 0,
            alpha: 255,
            light: 63,
        });
    }
    if elapsed < SLIDE_IN_MS {
        let t = elapsed as f32 / SLIDE_IN_MS as f32;
        let ease = 1. - (1. - t).powi(5);
        let fade = 1. - (1. - (t / 0.35).min(1.)).powi(3);
        Some(Motion {
            offset: -(distance as f32 * (1. - ease)).round() as i32,
            alpha: (fade * 255.) as u8,
            light: (elapsed / 32).min(63) as u8,
        })
    } else if elapsed < SLIDE_IN_MS + hold {
        Some(Motion {
            offset: 0,
            alpha: 255,
            light: (elapsed / 32).min(63) as u8,
        })
    } else {
        let t = (elapsed - SLIDE_IN_MS - hold) as f32 / SLIDE_OUT_MS as f32;
        let ease = t.powi(3);
        Some(Motion {
            offset: (distance as f32 * ease).round() as i32,
            alpha: ((1. - t * t) * 255.) as u8,
            light: 63,
        })
    }
}

struct Notices {
    cursor: Cursor,
    pending: VecDeque<Notice>,
}
static NOTICES: Mutex<Option<Notices>> = Mutex::new(None);

pub unsafe fn update(paths: &SignalPaths, states: &[SessionState], now: u64, visible: bool) {
    let next = if let Ok(mut slot) = NOTICES.lock() {
        let notices = slot.get_or_insert_with(|| Notices {
            cursor: Cursor::new(now),
            pending: VecDeque::new(),
        });
        let events = notices.cursor.take_named(states);
        if visible {
            notices.pending.extend(events);
            if FLASH.lock().ok().is_some_and(|f| f.is_none()) {
                notices.pending.pop_front()
            } else {
                None
            }
        } else {
            notices.pending.clear();
            hide();
            None
        }
    } else {
        None
    };
    if let Some(notice) = next {
        let title = task_text(paths, &notice);
        if let Err(error) = create(&title, notice.kind) {
            eprintln!("INTERNAL: task notification failed: {error}");
            hide();
        }
    }
}

fn task_text(paths: &SignalPaths, notice: &Notice) -> String {
    let plan = super::ui_text::plan(paths, &notice.run_id);
    plan.as_ref()
        .and_then(|p| p["name"].as_str())
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "任务名称未确认".into())
}

unsafe fn create(text: &str, kind: Kind) -> windows::core::Result<()> {
    let instance = GetModuleHandleW(None)?;
    let class = w!("actl_signal_flash");
    let _ = RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        lpszClassName: class,
        ..Default::default()
    });
    let mut work = RECT::default();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut work as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )?;
    let cx = work.left + (work.right - work.left) / 2;
    let cy = work.top + ((work.bottom - work.top) as f32 * 0.4) as i32;
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class,
        w!("actl flash"),
        WS_POPUP,
        cx,
        cy,
        1,
        1,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    let mut animation = BOOL(1);
    let reduced = SystemParametersInfoW(
        SPI_GETCLIENTAREAANIMATION,
        0,
        Some((&mut animation as *mut BOOL).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .is_ok()
        && !animation.as_bool();
    let dpi = GetDpiForWindow(hwnd).max(96);
    let available = (((work.right - work.left) * 2 / 3 - 64) * 96 / dpi as i32).clamp(300, 760);
    let raster = match flash_paint::raster(text, dpi, kind, available) {
        Ok(raster) => Arc::new(raster),
        Err(error) => {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
    };
    let target_x = cx - paint::pixels(raster.width, raster.dpi) / 2;
    let y = cy - paint::pixels(flash_paint::HEIGHT, raster.dpi) / 2;
    if let Ok(mut state) = FLASH.lock() {
        *state = Some(Flash {
            hwnd: hwnd.0 as usize,
            target_x,
            y,
            distance: (work.right - work.left) / 6,
            hold_ms: if matches!(kind, Kind::Started | Kind::Completed) {
                HOLD_MS
            } else {
                3000
            },
            started: std::time::Instant::now(),
            raster,
            reduced,
            last_frame: None,
        });
    } else {
        let _ = DestroyWindow(hwnd);
        return Err(E_FAIL.into());
    }
    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    advance(hwnd)?;
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    if SetTimer(Some(hwnd), TIMER_ID, 16, None) == 0 {
        return Err(windows::core::Error::from_thread());
    }
    Ok(())
}

pub unsafe fn hide() {
    let old = FLASH.lock().ok().and_then(|mut state| state.take());
    if let Some(flash) = old {
        let _ = DestroyWindow(HWND(flash.hwnd as *mut _));
    }
}

unsafe fn advance(hwnd: HWND) -> windows::core::Result<()> {
    let Some(flash) = FLASH.lock().ok().and_then(|state| state.clone()) else {
        return Ok(());
    };
    let elapsed = flash.started.elapsed().as_millis().min(u32::MAX as u128) as u32;
    let Some(motion) = frame(elapsed, flash.reduced, flash.distance, flash.hold_ms) else {
        hide();
        return Ok(());
    };
    let x = flash.target_x + motion.offset;
    let y = flash.y;
    let key = (x, y, motion.alpha, motion.light);
    if flash.last_frame == Some(key) {
        return Ok(());
    }
    SetWindowPos(
        hwnd,
        None,
        x,
        y,
        0,
        0,
        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
    )?;
    let data = flash_paint::illuminate(&flash.raster, motion.light);
    paint::present_alpha(
        hwnd,
        &data,
        flash.raster.dpi,
        flash.raster.width,
        flash_paint::HEIGHT,
        motion.alpha,
    )?;
    if let Ok(mut state) = FLASH.lock()
        && let Some(current) = state.as_mut().filter(|f| f.hwnd == hwnd.0 as usize)
    {
        current.last_frame = Some(key);
    }
    Ok(())
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_TIMER if wp.0 == TIMER_ID => {
            if let Err(error) = advance(hwnd) {
                eprintln!("INTERNAL: task ribbon animation failed: {error}");
                hide();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            if let Ok(mut state) = FLASH.lock()
                && state.as_ref().is_some_and(|f| f.hwnd == hwnd.0 as usize)
            {
                *state = None;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_ribbon_is_large_enough_to_read_over_a_terminal() {
        let raster =
            unsafe { flash_paint::raster("整理会议记录", 96, Kind::Started, 760) }.unwrap();
        assert!(
            raster.width >= 640,
            "task notice must have a substantial reading surface"
        );
        assert!(
            raster.data.len() / (raster.width as usize * 4) >= 100,
            "room for a single large line and an open light ribbon"
        );
    }
    #[test]
    fn motion_crosses_screen_thirds_and_reduced_motion_stays_still() {
        let total = SLIDE_IN_MS + HOLD_MS + SLIDE_OUT_MS;
        assert_eq!(
            frame(0, false, 320, HOLD_MS),
            Some(Motion {
                offset: -320,
                alpha: 0,
                light: 0
            })
        );
        assert_eq!(frame(SLIDE_IN_MS, false, 320, HOLD_MS).unwrap().offset, 0);
        let mut previous = -320;
        for ms in 0..total {
            let motion = frame(ms, false, 320, HOLD_MS).unwrap();
            assert!(motion.offset >= previous && motion.offset <= 320);
            previous = motion.offset;
            assert_eq!(
                frame(ms, true, 320, HOLD_MS),
                Some(Motion {
                    offset: 0,
                    alpha: 255,
                    light: 63
                })
            );
        }
        assert!(previous >= 315);
        assert_eq!(frame(total, false, 320, HOLD_MS), None);
        assert_eq!(frame(total, true, 320, HOLD_MS), None);
        assert!(
            frame(total, false, 320, 3000).is_some(),
            "exceptions stay readable for longer"
        );
    }
    #[test]
    fn text_uses_saved_task_name_and_never_step_title_or_another_task() {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let dir = paths.dir.join("flows").join("meeting");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plan.json"),
            serde_json::to_vec(
                &serde_json::json!({"name":"整理\n会议记录", "steps":[{"title":"读取窗口"}]}),
            )
            .unwrap(),
        )
        .unwrap();
        let notice = |kind| Notice {
            run_id: "meeting".into(),
            kind,
        };
        assert_eq!(task_text(&paths, &notice(Kind::Started)), "整理 会议记录");
        assert_eq!(task_text(&paths, &notice(Kind::Completed)), "整理 会议记录");
        assert_eq!(task_text(&paths, &notice(Kind::Failed)), "整理 会议记录");
        assert_eq!(task_text(&paths, &notice(Kind::Stopped)), "整理 会议记录");
        assert_eq!(
            task_text(
                &paths,
                &Notice {
                    run_id: "missing".into(),
                    kind: Kind::Started
                }
            ),
            "任务名称未确认"
        );
        std::fs::remove_dir_all(paths.dir).unwrap();
    }
    #[test]
    fn raster_handles_dpi_and_long_names_and_light_does_not_repaint_text() {
        for dpi in [96, 120, 144, 192] {
            let raster =
                unsafe { flash_paint::raster(&"会议记录".repeat(30), dpi, Kind::Started, 760) }
                    .unwrap();
            let pw = paint::pixels(raster.width, dpi) as usize;
            assert_eq!(
                raster.data.len(),
                pw * paint::pixels(flash_paint::HEIGHT, dpi) as usize * 4
            );
            assert_eq!(raster.data[3], 0);
            assert!(
                raster
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255)
            );
            for phase in [0, 31, 63] {
                let lit = flash_paint::illuminate(&raster, phase);
                assert!(
                    lit.as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[..3].iter().all(|c| *c <= p[3]))
                );
                let end = paint::pixels(73, dpi) as usize * pw * 4;
                assert_eq!(
                    lit[..end],
                    raster.data[..end],
                    "the traveling light must leave the glyphs unchanged"
                );
                assert_ne!(lit, raster.data);
            }
            let narrow =
                unsafe { flash_paint::raster("任务名称", dpi, Kind::Failed, 460) }.unwrap();
            assert_eq!(narrow.width, 460);
        }
    }
    #[test]
    fn long_task_names_cannot_push_exception_text_off_the_ribbon() {
        for kind in [Kind::Failed, Kind::Stopped] {
            let raster =
                unsafe { flash_paint::raster(&"整理会议记录".repeat(20), 96, kind, 760) }.unwrap();
            let pixels = raster.data.as_chunks::<4>().0;
            let colored_text = (30..70).any(|y| {
                (600..735).any(|x| {
                    let pixel = pixels[y * raster.width as usize + x];
                    pixel[2] > pixel[0].saturating_add(30) && pixel[3] > 180
                })
            });
            assert!(
                colored_text,
                "exception suffix must remain visible after a trimmed long name"
            );
        }
    }
    #[test]
    #[ignore = "exports production raster without showing a window or granting consent"]
    fn export_flash_previews() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/ribbon-review");
        std::fs::create_dir_all(&dir).unwrap();
        let frames = (0..=2900)
            .step_by(40)
            .map(|ms| frame(ms, false, 320, HOLD_MS))
            .collect::<Vec<_>>();
        std::fs::write(
            dir.join("frames.json"),
            serde_json::to_vec(&frames).unwrap(),
        )
        .unwrap();
        for (name, kind) in [
            ("started", Kind::Started),
            ("completed", Kind::Completed),
            ("failed", Kind::Failed),
            ("stopped", Kind::Stopped),
        ] {
            let raster = unsafe { flash_paint::raster("整理会议记录", 144, kind, 760) }.unwrap();
            std::fs::write(
                dir.join(format!("{name}.bgra")),
                flash_paint::illuminate(&raster, 48),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}.size")),
                format!(
                    "{} {}",
                    paint::pixels(raster.width, raster.dpi),
                    paint::pixels(flash_paint::HEIGHT, raster.dpi)
                ),
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
mod glyph_probe {
    use super::*;

    /// 候选设备字形 DirectWrite 渲染探针(显示层 glyph 裁定的前置实证,
    /// 2026-10-12):豆腐块/空渲染在 BGRA 里目视判定。非断言型——产物即证据。
    #[test]
    fn export_device_glyph_candidates() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/glyph-probe");
        std::fs::create_dir_all(&dir).unwrap();
        let candidates: [(&str, &str); 5] = [
            ("mouse_1f5b1", "\u{1F5B1}"),
            ("keyboard_2328", "\u{2328}"),
            ("keyboard_vs16", "\u{2328}\u{FE0F}"),
            ("keyboard_2328_text", "\u{2328} 键盘"),
            ("bracket_glyph", "⌨"),
        ];
        for (name, text) in candidates {
            let raster = unsafe { flash_paint::raster(text, 144, Kind::Stopped, 760) }.unwrap();
            std::fs::write(
                dir.join(format!("{name}.bgra")),
                flash_paint::illuminate(&raster, 48),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}.size")),
                format!(
                    "{} {}",
                    paint::pixels(raster.width, raster.dpi),
                    paint::pixels(flash_paint::HEIGHT, raster.dpi)
                ),
            )
            .unwrap();
        }
    }
}
