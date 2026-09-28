//! Explicit desktop preparation with instance-bound, conservative restoration.
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

fn unavailable(s: &str) -> CtlError {
    CtlError::new(ErrorCode::NotActionable, s)
}
fn placement(hwnd: HWND) -> Result<WINDOWPLACEMENT, CtlError> {
    let mut p = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    unsafe { GetWindowPlacement(hwnd, &mut p) }.map_err(crate::internal)?;
    Ok(p)
}
fn encode(p: &WINDOWPLACEMENT) -> Value {
    json!({"flags":p.flags.0,"show":p.showCmd,"min":[p.ptMinPosition.x,p.ptMinPosition.y],"max":[p.ptMaxPosition.x,p.ptMaxPosition.y],"rect":[p.rcNormalPosition.left,p.rcNormalPosition.top,p.rcNormalPosition.right,p.rcNormalPosition.bottom]})
}
fn decode(v: &Value) -> Result<WINDOWPLACEMENT, CtlError> {
    let number = |v: &Value| {
        v.as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| CtlError::protocol("invalid window placement record"))
    };
    Ok(WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        flags: WINDOWPLACEMENT_FLAGS(number(&v["flags"])? as u32),
        showCmd: number(&v["show"])? as u32,
        ptMinPosition: POINT {
            x: number(&v["min"][0])?,
            y: number(&v["min"][1])?,
        },
        ptMaxPosition: POINT {
            x: number(&v["max"][0])?,
            y: number(&v["max"][1])?,
        },
        rcNormalPosition: RECT {
            left: number(&v["rect"][0])?,
            top: number(&v["rect"][1])?,
            right: number(&v["rect"][2])?,
            bottom: number(&v["rect"][3])?,
        },
    })
}
fn identity(hwnd: isize) -> Result<(uiautomation::UIElement, Value), CtlError> {
    let win = crate::window::resolve_window(crate::WindowRef::Hwnd(hwnd))?;
    let rid = win.get_runtime_id().map_err(crate::internal)?;
    let pid = win.get_process_id().map_err(crate::internal)?;
    let class = win.get_classname().map_err(crate::internal)?;
    Ok((
        win,
        json!({"hwnd":hwnd,"pid":pid,"runtime_id":rid,"class":class}),
    ))
}
fn ordinary(hwnd: HWND) -> bool {
    unsafe {
        IsWindow(Some(hwnd)).as_bool()
            && IsWindowVisible(hwnd).as_bool()
            && !crate::display_support::is_display_window(hwnd)
            && GetWindow(hwnd, GW_OWNER).is_err()
            && GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_MINIMIZEBOX.0 != 0
            && GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & (WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0)
                == 0
            && hwnd != GetShellWindow()
            && hwnd != GetDesktopWindow()
    }
}
fn wait_iconic(hwnd: HWND, expected: bool) -> Result<(), CtlError> {
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(2) {
        crate::feedback::check_stop()?;
        if unsafe { IsIconic(hwnd).as_bool() } == expected {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(
            crate::timing().fast_poll_ms,
        ));
    }
    Err(unavailable(
        "window did not reach the requested visual state",
    ))
}
pub fn set_minimized(r: crate::WindowRef<'_>, minimize: bool) -> Result<Value, CtlError> {
    let win = crate::window::resolve_window(r)?;
    let hwnd =
        crate::window::hwnd_of(&win).ok_or_else(|| unavailable("window has no native handle"))?;
    if minimize && !ordinary(hwnd) {
        return Err(unavailable(
            "only ordinary unowned application windows can be minimized",
        ));
    }
    let _lock = crate::input::InputLock::acquire(3000)?;
    let mut action = crate::feedback::window_action(
        if minimize {
            "minimize-window"
        } else {
            "restore-window"
        },
        &win,
        true,
    )?;
    unsafe { ShowWindowAsync(hwnd, if minimize { SW_MINIMIZE } else { SW_RESTORE }) }
        .ok()
        .map_err(crate::internal)?;
    wait_iconic(hwnd, minimize)?;
    action.delivered();
    Ok(
        json!({"hwnd":hwnd.0 as isize,"window":win.get_name().unwrap_or_default(),"minimized":minimize,"input_effects":{"keyboard":"none","pointer":"none","clipboard":"untouched","focus":true}}),
    )
}
fn path(token: &str) -> Result<std::path::PathBuf, CtlError> {
    if token.is_empty()
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    {
        return Err(CtlError::protocol("invalid desktop session id"));
    }
    Ok(actl_core::state::SignalPaths::default()
        .dir
        .join("desktop")
        .join(format!("{token}.json")))
}
fn save(token: &str, doc: &Value) -> Result<(), CtlError> {
    let p = path(token)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(crate::internal)?;
    }
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec(doc).map_err(crate::internal)?)
        .map_err(crate::internal)?;
    std::fs::rename(tmp, p).map_err(crate::internal)
}
pub fn prepare(handles: &[isize]) -> Result<Value, CtlError> {
    let token = actl_core::snapshot::new_snapshot_id();
    let _lock = crate::input::InputLock::acquire(3000)?;
    let mut doc = json!({"version":1,"session":token,"windows":[],"foreground":unsafe { GetForegroundWindow() }.0 as isize});
    save(&token, &doc)?;
    let mut results = Vec::new();
    let windows = crate::list_windows()?;
    if handles
        .iter()
        .any(|h| !windows.iter().any(|w| w.hwnd == Some(*h)))
    {
        return Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            "a requested window is not available",
            json!({"session":token,"results":results}),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for w in windows {
        let Some(raw) = w.hwnd else { continue };
        let hwnd = HWND(raw as *mut _);
        if !seen.insert(raw) {
            continue;
        }
        if !handles.is_empty() && !handles.contains(&raw) {
            continue;
        }
        if !ordinary(hwnd) || unsafe { IsIconic(hwnd).as_bool() } {
            if !handles.is_empty() {
                results.push(
                    json!({"hwnd":raw,"status":"skipped","reason":"already_minimized_or_excluded"}),
                );
            }
            continue;
        }
        let outcome = (|| -> Result<(), CtlError> {
            let (win, id) = identity(raw)?;
            let before = placement(hwnd)?;
            let mut action = crate::feedback::window_action("minimize-window", &win, true)?;
            if !ordinary(hwnd) || encode(&placement(hwnd)?) != encode(&before) {
                return Err(unavailable("window changed during preview"));
            }
            let record =
                json!({"identity":id,"before":encode(&before),"after":null,"restored":false});
            let Some(records) = doc["windows"].as_array_mut() else {
                return Err(crate::internal("invalid session"));
            };
            records.push(record);
            let index = records.len() - 1;
            // Journal before changing the desktop, so a crash retains recovery data.
            save(&token, &doc)?;
            unsafe { ShowWindowAsync(hwnd, SW_MINIMIZE) }
                .ok()
                .map_err(crate::internal)?;
            wait_iconic(hwnd, true)?;
            doc["windows"][index]["after"] = encode(&placement(hwnd)?);
            save(&token, &doc)?;
            action.delivered();
            Ok(())
        })();
        match outcome {
            Ok(()) => results.push(json!({"hwnd":raw,"status":"minimized"})),
            Err(e) => {
                results.push(json!({"hwnd":raw,"status":"failed","code":e.code}));
                return Err(CtlError::with_evidence(
                    e.code,
                    e.message,
                    json!({"session":token,"results":results}),
                ));
            }
        }
    }
    Ok(json!({"session":token,"results":results}))
}
pub fn restore(token: &str) -> Result<Value, CtlError> {
    let bytes = std::fs::read(path(token)?).map_err(|e| {
        CtlError::new(
            ErrorCode::NotFound,
            format!("desktop session unavailable: {e}"),
        )
    })?;
    let mut doc: Value = serde_json::from_slice(&bytes)
        .map_err(|_| CtlError::protocol("invalid desktop session"))?;
    if doc["version"] != 1 || doc["session"] != token {
        return Err(CtlError::protocol(
            "desktop session version or identity mismatch",
        ));
    }
    let count = doc["windows"]
        .as_array()
        .ok_or_else(|| CtlError::protocol("missing window records"))?
        .len();
    let _lock = crate::input::InputLock::acquire(3000)?;
    let mut results = Vec::new();
    for index in (0..count).rev() {
        let record = doc["windows"][index].clone();
        if record["restored"] == true {
            continue;
        }
        let raw = record["identity"]["hwnd"]
            .as_i64()
            .ok_or_else(|| CtlError::protocol("missing hwnd"))? as isize;
        let hwnd = HWND(raw as *mut _);
        let current = identity(raw);
        let (win, id) = match current {
            Ok(v) => v,
            Err(_) => {
                results.push(json!({"hwnd":raw,"status":"skipped","reason":"closed_or_replaced"}));
                continue;
            }
        };
        let observed = encode(&placement(hwnd)?);
        if id != record["identity"]
            || !unsafe { IsIconic(hwnd).as_bool() }
            || record["after"].is_null()
            || observed != record["after"]
        {
            results.push(json!({"hwnd":raw,"status":"skipped","reason":"changed_or_unconfirmed"}));
            continue;
        }
        let result = (|| -> Result<(), CtlError> {
            let mut action = crate::feedback::window_action("restore-window", &win, true)?;
            if identity(raw)?.1 != id || encode(&placement(hwnd)?) != observed {
                return Err(unavailable("window changed during restore preview"));
            }
            unsafe { SetWindowPlacement(hwnd, &decode(&record["before"])?) }
                .map_err(crate::internal)?;
            wait_iconic(hwnd, false)?;
            action.delivered();
            doc["windows"][index]["restored"] = json!(true);
            save(token, &doc)?;
            Ok(())
        })();
        if let Err(e) = result {
            return Err(CtlError::with_evidence(
                e.code,
                e.message,
                json!({"session":token,"results":results}),
            ));
        }
        results.push(json!({"hwnd":raw,"status":"restored"}));
    }
    Ok(json!({"session":token,"results":results}))
}
