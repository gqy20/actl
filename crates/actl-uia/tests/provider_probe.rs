//! Manual, read-only COM phase diagnosis; does not inject input or resume flows.
#![allow(unsafe_code)] // COM inspection is confined to actl-uia (ADR-005).

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::{io::Write, path::PathBuf, time::Instant};
use uiautomation::{UIAutomation, errors::Error, types::TreeScope, types::UIProperty};
use windows::{
    Win32::{Foundation::HWND, UI::Accessibility::IUIAutomation2},
    core::Interface,
};

fn endpoint() -> Value {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: native metadata reads only; initialized local output buffers.
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0;
        let thread = GetWindowThreadProcessId(hwnd, Some(&mut pid));
        json!({"foreground_hwnd":(!hwnd.0.is_null()).then_some(hwnd.0 as isize),
            "foreground_pid":(thread!=0).then_some(pid),
            "last_input_tick":GetLastInputInfo(&mut input).as_bool().then_some(input.dwTime)})
    }
}

fn client(version: &str) -> Result<UIAutomation, Error> {
    if version == "legacy" {
        return UIAutomation::new();
    }
    use windows::Win32::{
        System::Com::{CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
        UI::Accessibility::{CUIAutomation8, IUIAutomation},
    };
    // SAFETY: acquire this thread's MTA as the legacy wrapper does; COM lifetime
    // ends with this short-lived probe process. No provider policy is changed.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let native: IUIAutomation = CoCreateInstance(&CUIAutomation8, None, CLSCTX_ALL)?;
        Ok(UIAutomation::from(native))
    }
}

fn error_code(native: i32) -> ErrorCode {
    use windows::Win32::{
        Foundation::E_ACCESSDENIED,
        UI::Accessibility::{UIA_E_ELEMENTNOTAVAILABLE, UIA_E_TIMEOUT},
    };
    match native {
        n if n == UIA_E_TIMEOUT as i32 => ErrorCode::Timeout,
        n if n == UIA_E_ELEMENTNOTAVAILABLE as i32 => ErrorCode::StaleRef,
        n if n == E_ACCESSDENIED.0 => ErrorCode::PermDenied,
        _ => ErrorCode::Internal,
    }
}

fn phase<T>(
    name: &str,
    events: &mut Vec<Value>,
    operation: impl FnOnce() -> Result<T, Error>,
) -> Result<T, CtlError> {
    eprintln!("provider_probe begin {name}");
    let start = Instant::now();
    let result = operation();
    let mut event = json!({"phase":name,"duration_ms":start.elapsed().as_millis()});
    if let Err(error) = &result {
        event["outcome"] = json!("failed");
        event["native_code"] = json!(error.code());
    } else {
        event["outcome"] = json!("succeeded");
    }
    eprintln!("provider_probe finish {event}");
    events.push(event);
    result.map_err(|error| {
        CtlError::with_evidence(
            error_code(error.code()),
            "provider probe failed",
            json!({"phase":name,"native_code":error.code()}),
        )
    })
}

fn observe(
    version: &str,
    mode: &str,
    hwnd: Option<isize>,
    events: &mut Vec<Value>,
) -> Result<Value, CtlError> {
    let auto = phase("initialize", events, || client(version))?;
    // SAFETY: query the owned COM client on this MTA thread; no policy mutation.
    let timeouts = match auto.as_ref().cast::<IUIAutomation2>() {
        Ok(client) => unsafe {
            let connection = client.ConnectionTimeout();
            let transaction = client.TransactionTimeout();
            json!({"supported":true,"connection_ms":connection.as_ref().ok(),
                "connection_error":connection.as_ref().err().map(|e|e.code().0),
                "transaction_ms":transaction.as_ref().ok(),
                "transaction_error":transaction.as_ref().err().map(|e|e.code().0)})
        },
        Err(error) => json!({"supported":false,"native_code":error.code().0}),
    };
    events.push(json!({"phase":"client_configuration","timeouts":timeouts}));
    let root = phase("root", events, || match hwnd {
        Some(raw) => auto.element_from_handle(uiautomation::types::Handle::from(raw)),
        None => auto.get_root_element(),
    })?;
    let condition = phase("condition", events, || auto.create_true_condition())?;
    let cache = phase("cache", events, || {
        let cache = auto.create_cache_request()?;
        cache.set_tree_filter(condition.clone())?;
        cache.set_tree_scope(TreeScope::Element)?;
        if mode == "handle" {
            cache.add_property(UIProperty::NativeWindowHandle)?;
        }
        Ok(cache)
    })?;
    // SAFETY: live COM objects are owned on this thread; Children never traverses
    // desktop descendants. Keep the raw result to distinguish query vs conversion.
    let array = phase("find_all_build_cache", events, || unsafe {
        root.as_ref()
            .FindAllBuildCache(TreeScope::Children.into(), &condition, &cache)
            .map_err(Into::into)
    })?;
    let length = phase("array_length", events, || unsafe {
        array.Length().map_err(Into::into)
    })?;
    if !(0..=5000).contains(&length) {
        return Err(CtlError::protocol("provider result exceeds probe budget"));
    }
    let start = Instant::now();
    let mut identities = Vec::new();
    eprintln!("provider_probe begin array_elements count={length}");
    for index in 0..length {
        // SAFETY: index is bounded by the returned array length; no property reads.
        let element = match unsafe { array.GetElement(index) } {
            Ok(element) => element,
            Err(error) => {
                events.push(json!({"phase":"array_elements","index":index,
                "duration_ms":start.elapsed().as_millis(),"outcome":"failed",
                "native_code":error.code().0}));
                return Err(CtlError::with_evidence(
                    error_code(error.code().0),
                    "provider array conversion failed",
                    json!({"index":index,"native_code":error.code().0}),
                ));
            }
        };
        if mode == "handle" {
            let element = uiautomation::UIElement::from(element);
            match element.get_cached_native_window_handle() {
                Ok(handle) => {
                    let handle: HWND = handle.into();
                    identities.push(json!({"index":index,
                        "hwnd":(!handle.0.is_null()).then_some(handle.0 as isize)}));
                }
                Err(error) => identities.push(json!({"index":index,"hwnd":null,
                    "native_code":error.code()})),
            }
        }
    }
    events.push(
        json!({"phase":"array_elements","duration_ms":start.elapsed().as_millis(),
        "outcome":"succeeded","count":length}),
    );
    eprintln!("provider_probe finish array_elements");
    Ok(json!({"count":length,"timeouts":timeouts,"cached_handles":identities}))
}

#[test]
#[ignore = "manual read-only desktop diagnosis; set ACTL_PROVIDER_PROBE_OUTPUT"]
fn desktop_provider_phases() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::var("ACTL_PROVIDER_PROBE_MODE")?;
    let version = std::env::var("ACTL_PROVIDER_PROBE_CLIENT").unwrap_or_else(|_| "legacy".into());
    if !["legacy", "modern"].contains(&version.as_str()) {
        return Err("client must be legacy or modern".into());
    }
    if !["minimal", "handle"].contains(&mode.as_str()) {
        return Err("mode must be minimal or handle".into());
    }
    let output = PathBuf::from(std::env::var("ACTL_PROVIDER_PROBE_OUTPUT")?);
    if output.exists() {
        return Err("probe output already exists; preserve prior evidence".into());
    }
    let hwnd = std::env::var("ACTL_PROVIDER_PROBE_HWND")
        .ok()
        .map(|v| v.parse::<isize>())
        .transpose()?;
    let mut file = std::fs::File::create_new(output)?;
    let before = match hwnd.map(actl_uia::native_window).transpose() {
        Ok(windows) => windows,
        Err(error) => {
            file.write_all(&serde_json::to_vec_pretty(&json!({"status":"blocked",
                "phase":"native_preflight","hwnd":hwnd,"client":version,"mode":mode,
                "error":{"code":error.code,"evidence":error.evidence}}))?)?;
            return Err(error.into());
        }
    };
    if before
        .as_ref()
        .is_some_and(|v| v.len() != 1 || !v[0].visible || v[0].minimized)
    {
        let reason = "target unavailable, hidden or minimized; no activation";
        file.write_all(&serde_json::to_vec_pretty(&json!({"status":"blocked",
            "phase":"native_preflight","hwnd":hwnd,"client":version,"mode":mode,
            "error":{"code":"NOT_ACTIONABLE","reason":reason},
            "native":before.as_ref().and_then(|v|v.first()).map(|w|json!({
                "pid":w.pid,"visible":w.visible,"minimized":w.minimized})),
            "cleanup":"no_resources_created","input_injected":false}))?)?;
        return Err(CtlError::new(ErrorCode::NotActionable, reason).into());
    }
    let process_before = match before
        .as_ref()
        .and_then(|v| v.first())
        .map(|w| actl_uia::evidence::process(w.pid, false))
        .transpose()
    {
        Ok(process) => process,
        Err(error) => {
            file.write_all(&serde_json::to_vec_pretty(&json!({"status":"blocked",
                "phase":"process_preflight","hwnd":hwnd,"client":version,"mode":mode,
                "error":{"code":error.code,"evidence":error.evidence}}))?)?;
            return Err(error.into());
        }
    };
    let mut events = Vec::new();
    let context_before = endpoint();
    let result = observe(&version, &mode, hwnd, &mut events);
    let context_after = endpoint();
    let after = hwnd.map(actl_uia::native_window).transpose();
    let process_after = match &after {
        Ok(Some(windows)) => windows
            .first()
            .map(|w| actl_uia::evidence::process(w.pid, false))
            .transpose(),
        _ => Ok(None),
    };
    let stable = match (&before, &after) {
        (Some(a), Ok(Some(b))) => {
            a.len() == 1
                && b.len() == 1
                && a[0].pid == b[0].pid
                && a[0].class == b[0].class
                && process_before
                    .as_ref()
                    .is_some_and(|p| p["start"].is_string())
                && process_after.as_ref().is_ok_and(|p| p == &process_before)
        }
        (None, Ok(None)) => true,
        _ => false,
    };
    let context_stable = context_before["last_input_tick"].is_number()
        && context_before["foreground_hwnd"].is_number()
        && context_before == context_after;
    let record = json!({"client":version,"mode":mode,"hwnd":hwnd,
        "status":if result.is_err(){"failed"}else if !stable{"inconclusive"}else{"observed"},
        "native_identity_stable":hwnd.map(|_|stable),
        "comparison_context_stable":context_stable,
        "context_before":context_before,"context_after":context_after,
        "target_pid":before.as_ref().and_then(|v|v.first()).map(|w|w.pid),
        "process_before":process_before,"process_after":process_after.as_ref().ok(),
        "native_after_error":after.as_ref().err().map(|e|e.code),
        "process_after_error":process_after.as_ref().err().map(|e|e.code),
        "events":events,"observation":result.as_ref().ok(),
        "error":result.as_ref().err().map(|e|json!({"code":e.code,"evidence":e.evidence})),
        "cleanup":"no_resources_created","input_injected":false});
    file.write_all(&serde_json::to_vec_pretty(&record)?)?;
    result?;
    if !stable {
        return Err("target identity changed; sample is inconclusive".into());
    }
    Ok(())
}
