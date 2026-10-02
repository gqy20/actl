//! Task lifecycle notifications; no per-action prompts or execution/consent state writes.
use super::paint::{self, FlashRaster};
use super::task_notice::{Cursor, Kind};
use actl_core::state::SessionState;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::{HiDpi::GetDpiForWindow, WindowsAndMessaging::*};
use windows::core::{BOOL, w};

const TIMER_ID: usize = 42;
const SLIDE_IN_MS: u32 = 320;
const HOLD_MS: u32 = 1600;
const SLIDE_OUT_MS: u32 = 180;
const SLIDE_DISTANCE: i32 = 12;

#[derive(Clone)]
struct Flash {
    hwnd: usize,
    target_x: i32,
    y: i32,
    started: std::time::Instant,
    raster: Arc<FlashRaster>,
    reduced: bool,
    last_frame: Option<(i32, i32, i32, i32, u8)>,
}
static FLASH: Mutex<Option<Flash>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
struct Motion {
    offset: i32,
    alpha: u8,
    scale: f32,
}
/// Small spring settling on entry; faster withdrawal. Reduced motion stays static.
fn frame(elapsed: u32, reduced: bool, distance: i32) -> Option<Motion> {
    if elapsed >= SLIDE_IN_MS + HOLD_MS + SLIDE_OUT_MS {
        return None;
    }
    if reduced {
        return Some(Motion {
            offset: 0,
            alpha: 255,
            scale: 1.,
        });
    }
    if elapsed < SLIDE_IN_MS {
        let t = elapsed as f32 / SLIDE_IN_MS as f32;
        let p = t - 1.;
        let spring = 1. + 1.8 * p.powi(3) + 0.8 * p.powi(2);
        let fade = 1. - (1. - (t / 0.7).min(1.)).powi(3);
        Some(Motion {
            offset: (distance as f32 * (1. - spring)).round() as i32,
            alpha: (fade * 255.) as u8,
            scale: 0.94 + 0.06 * spring,
        })
    } else if elapsed < SLIDE_IN_MS + HOLD_MS {
        Some(Motion {
            offset: 0,
            alpha: 255,
            scale: 1.,
        })
    } else {
        let t = (elapsed - SLIDE_IN_MS - HOLD_MS) as f32 / SLIDE_OUT_MS as f32;
        let ease = t * t;
        Some(Motion {
            offset: (distance as f32 * 0.5 * ease).round() as i32,
            alpha: ((1. - ease) * 255.) as u8,
            scale: 1. - 0.02 * ease,
        })
    }
}

struct Notices {
    cursor: Cursor,
    pending: VecDeque<Kind>,
}
static NOTICES: Mutex<Option<Notices>> = Mutex::new(None);

pub unsafe fn update(states: &[SessionState], now: u64, visible: bool) {
    let next = if let Ok(mut slot) = NOTICES.lock() {
        let notices = slot.get_or_insert_with(|| Notices {
            cursor: Cursor::new(now),
            pending: VecDeque::new(),
        });
        let events = notices.cursor.take(states);
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
    if let Some(kind) = next
        && let Err(error) = create(kind.label(), kind)
    {
        eprintln!("INTERNAL: task notification failed: {error}");
        hide();
    }
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
    let cy = work.top + ((work.bottom - work.top) as f32 * 0.28) as i32;
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
    let raster = match paint::raster_flash(text, GetDpiForWindow(hwnd), kind) {
        Ok(raster) => Arc::new(raster),
        Err(error) => {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
    };
    let target_x = cx - paint::pixels(raster.width, raster.dpi) / 2;
    let y = cy - paint::pixels(raster.height, raster.dpi) / 2;
    if let Ok(mut state) = FLASH.lock() {
        *state = Some(Flash {
            hwnd: hwnd.0 as usize,
            target_x,
            y,
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
    let Some(motion) = frame(
        elapsed,
        flash.reduced,
        paint::pixels(SLIDE_DISTANCE, flash.raster.dpi),
    ) else {
        hide();
        return Ok(());
    };
    let sw = paint::pixels(flash.raster.width, flash.raster.dpi);
    let sh = paint::pixels(flash.raster.height, flash.raster.dpi);
    let dw = (sw as f32 * motion.scale).round() as i32;
    let dh = (sh as f32 * motion.scale).round() as i32;
    let x = flash.target_x + (sw - dw) / 2;
    let y = flash.y + motion.offset + (sh - dh) / 2;
    let key = (x, y, dw, dh, motion.alpha);
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
    if dw == sw && dh == sh {
        paint::present_alpha(
            hwnd,
            &flash.raster.data,
            flash.raster.dpi,
            flash.raster.width,
            flash.raster.height,
            motion.alpha,
        )?;
    } else {
        let (data, w, h) = paint::scaled_flash(&flash.raster, motion.scale);
        paint::present_alpha(hwnd, &data, 96, w, h, motion.alpha)?;
    }
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
                eprintln!("INTERNAL: action-start animation failed: {error}");
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
    fn motion_settles_and_reduced_motion_has_no_translation_scaling_or_fade() {
        assert_eq!(
            frame(0, false, 12),
            Some(Motion {
                offset: 12,
                alpha: 0,
                scale: 0.94
            })
        );
        assert_eq!(
            frame(320, false, 12),
            Some(Motion {
                offset: 0,
                alpha: 255,
                scale: 1.
            })
        );
        for elapsed in 0..2100 {
            let motion = frame(elapsed, false, 12).unwrap();
            assert!((-1..=12).contains(&motion.offset));
            assert!((0.939..=1.004).contains(&motion.scale));
            assert_eq!(
                frame(elapsed, true, 12),
                Some(Motion {
                    offset: 0,
                    alpha: 255,
                    scale: 1.
                })
            );
        }
        assert_eq!(frame(2100, false, 12), None);
        assert_eq!(frame(2100, true, 12), None);
    }
    #[test]
    fn raster_scales_and_clips_long_text_with_smooth_transparent_corners() {
        for dpi in [96, 120, 144, 192] {
            let short = unsafe { paint::raster_flash("任务开始", dpi, Kind::Started) }.unwrap();
            let long =
                unsafe { paint::raster_flash(&"任务完成".repeat(20), dpi, Kind::Completed) }
                    .unwrap();
            assert!(long.width > short.width && long.width <= 420);
            assert_eq!(short.height, long.height);
            assert_eq!(
                short.data.len(),
                (paint::pixels(short.width, dpi) * paint::pixels(short.height, dpi) * 4) as usize
            );
            assert_eq!(short.data[3], 0);
            assert!(short.data.as_chunks::<4>().0.iter().any(|p| p[3] == 255));
            assert!(
                short
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[3] > 0 && p[3] < 255)
            );
            assert!(
                short
                    .data
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[..3].iter().all(|c| *c <= p[3]))
            );
            for scale in [0.94, 0.98, 1.002] {
                let (scaled, w, h) = paint::scaled_flash(&short, scale);
                assert_eq!(scaled.len(), (w * h * 4) as usize);
                assert!(
                    scaled
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p[..3].iter().all(|c| *c <= p[3]))
                );
            }
        }
    }
    #[test]
    #[ignore = "exports production raster without showing a window or granting consent"]
    fn export_flash_previews() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/flash-review");
        std::fs::create_dir_all(&dir).unwrap();
        let frames = (0..=2100)
            .step_by(40)
            .map(|ms| frame(ms, false, 24))
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
        ] {
            let raster = unsafe { paint::raster_flash(kind.label(), 192, kind) }.unwrap();
            std::fs::write(dir.join(format!("{name}.bgra")), &raster.data).unwrap();
            std::fs::write(
                dir.join(format!("{name}.size")),
                format!(
                    "{} {}",
                    paint::pixels(raster.width, raster.dpi),
                    paint::pixels(raster.height, raster.dpi)
                ),
            )
            .unwrap();
        }
    }
}
