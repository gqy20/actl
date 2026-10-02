//! Read-only root identity and query-source comparison; never runs a flow.
#![allow(unsafe_code)] // Native/COM metadata remains inside actl-uia (ADR-005).

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::{fs::File, io::Write, path::PathBuf, time::Instant};
use uiautomation::{
    UIAutomation, UIElement,
    errors::Error,
    types::{Handle, TreeScope, UIProperty},
};
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::GetDesktopWindow};

struct Recorder(File);
fn failure(phase: &str, error: &Error) -> CtlError {
    use windows::Win32::{
        Foundation::E_ACCESSDENIED,
        UI::Accessibility::{UIA_E_ELEMENTNOTAVAILABLE, UIA_E_TIMEOUT},
    };
    let code = match error.code() {
        n if n == UIA_E_TIMEOUT as i32 => ErrorCode::Timeout,
        n if n == UIA_E_ELEMENTNOTAVAILABLE as i32 => ErrorCode::StaleRef,
        n if n == E_ACCESSDENIED.0 => ErrorCode::PermDenied,
        _ => ErrorCode::Internal,
    };
    CtlError::with_evidence(
        code,
        "root probe failed",
        json!({"phase":phase,"native_code":error.code()}),
    )
}
impl Recorder {
    fn note(&mut self, value: Value) -> Result<(), Box<dyn std::error::Error>> {
        serde_json::to_writer(&mut self.0, &value)?;
        self.0.write_all(b"\n")?;
        self.0.flush()?;
        Ok(())
    }
    fn phase<T>(
        &mut self,
        name: &str,
        operation: impl FnOnce() -> Result<T, Error>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        self.note(json!({"event":"started","phase":name,"ts_ms":actl_core::state::unix_ms()}))?;
        let start = Instant::now();
        let result = operation();
        self.note(
            json!({"event":"finished","phase":name,"duration_ms":start.elapsed().as_millis(),
            "outcome":if result.is_ok(){"succeeded"}else{"failed"},
            "native_code":result.as_ref().err().map(|e|e.code()),
            "error_code":result.as_ref().err().map(|e|failure(name,e).code)}),
        )?;
        Ok(result.map_err(|e| failure(name, &e))?)
    }
}

fn identity(element: &UIElement) -> Result<Value, Error> {
    let hwnd: HWND = element.get_native_window_handle()?.into();
    Ok(
        json!({"hwnd":hwnd.0 as isize,"pid":element.get_process_id()?,
        "role":format!("{:?}",element.get_control_type()?),
        "class":element.get_classname()?,"runtime_id":element.get_runtime_id()?}),
    )
}

fn query(
    auto: &UIAutomation,
    root: &UIElement,
    label: &str,
    mode: &str,
    r: &mut Recorder,
) -> Result<Value, Box<dyn std::error::Error>> {
    let cache_phase = format!("{label}.cache");
    let condition = auto
        .create_true_condition()
        .map_err(|e| failure(&cache_phase, &e))?;
    let cache = auto
        .create_cache_request()
        .map_err(|e| failure(&cache_phase, &e))?;
    cache
        .set_tree_filter(condition.clone())
        .map_err(|e| failure(&cache_phase, &e))?;
    cache
        .set_tree_scope(TreeScope::Element)
        .map_err(|e| failure(&cache_phase, &e))?;
    let properties = if mode == "handle" {
        vec![UIProperty::NativeWindowHandle]
    } else {
        vec![
            UIProperty::NativeWindowHandle,
            UIProperty::ProcessId,
            UIProperty::ClassName,
            UIProperty::ControlType,
        ]
    };
    for property in properties {
        cache
            .add_property(property)
            .map_err(|e| failure(&cache_phase, &e))?;
    }
    let array = r.phase(&format!("{label}.query"), || {
        // SAFETY: owned COM objects on the calling MTA; only direct children.
        unsafe {
            root.as_ref()
                .FindAllBuildCache(TreeScope::Children.into(), &condition, &cache)
        }
        .map_err(Into::into)
    })?;
    let length = r.phase(&format!("{label}.length"), || {
        // SAFETY: successful native query returned the owned array.
        unsafe { array.Length() }.map_err(Into::into)
    })?;
    if !(0..=5000).contains(&length) {
        return Err("invalid/unbounded root array length".into());
    }
    let mut children = Vec::new();
    for index in 0..length.min(16) {
        let child = r.phase(&format!("{label}.element.{index}"), || {
            // SAFETY: validated length bounds every index; properties are cached.
            let element = UIElement::from(unsafe { array.GetElement(index) }?);
            let hwnd: HWND = element.get_cached_native_window_handle()?.into();
            let mut value = json!({"index":index,"hwnd":hwnd.0 as isize});
            if mode == "metadata" {
                value["pid"] = json!(element.get_cached_process_id()?);
                value["class"] = json!(element.get_cached_classname()?);
                value["role"] = json!(format!("{:?}", element.get_cached_control_type()?));
            }
            Ok(value)
        })?;
        children.push(child);
    }
    Ok(json!({"count":length,"sample":children,"sample_truncated":length>16}))
}

fn native_context() -> Result<Value, actl_core::CtlError> {
    use windows::Win32::UI::{
        Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
        WindowsAndMessaging::GetForegroundWindow,
    };
    let mut input = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: initialized metadata buffer, with no input injection.
    let (foreground, input_tick) = unsafe {
        (
            GetForegroundWindow().0 as isize,
            GetLastInputInfo(&mut input)
                .as_bool()
                .then_some(input.dwTime),
        )
    };
    let windows = actl_uia::native_windows()?;
    Ok(
        json!({"foreground_hwnd":foreground,"last_input_tick":input_tick,"visible":windows.iter().filter(|w|w.visible && !w.minimized)
        .map(|w|json!({"hwnd":w.hwnd,"pid":w.pid,"class":w.class,"foreground":w.foreground})).collect::<Vec<_>>()}),
    )
}

#[test]
#[ignore = "manual incremental root probe; set ACTL_ROOT_PROBE_OUTPUT/CACHE; no input"]
fn root_walk() -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::UI::Accessibility::IUIAutomationElement;
    use windows::core::Interface;
    let mode = std::env::var("ACTL_ROOT_PROBE_CACHE")?;
    if !["handle", "metadata"].contains(&mode.as_str()) {
        return Err("ACTL_ROOT_PROBE_CACHE must be handle or metadata".into());
    }
    let mut r = Recorder(File::create_new(std::env::var("ACTL_ROOT_PROBE_OUTPUT")?)?);
    r.note(
        json!({"event":"context_before","cache_mode":mode,"native":native_context()?,
        "binary":std::env::current_exe()?,"input_injected":false}),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let auto = r.phase("initialize", UIAutomation::new)?;
        let root = r.phase("get_root", || auto.get_root_element())?;
        let root_id = r.phase("root.identity", || identity(&root))?;
        r.note(json!({"event":"root_identity","identity":root_id}))?;
        let walker = r.phase("walker", || auto.get_raw_view_walker())?;
        let cache = r.phase("cache", || {
            let cache = auto.create_cache_request()?;
            cache.set_tree_scope(TreeScope::Element)?;
            cache.add_property(UIProperty::NativeWindowHandle)?;
            if mode == "metadata" {
                for p in [
                    UIProperty::ProcessId,
                    UIProperty::ClassName,
                    UIProperty::ControlType,
                ] {
                    cache.add_property(p)?;
                }
            }
            Ok(cache)
        })?;
        let mut current = root;
        for index in 0..256 {
            let next = r.phase(&format!("walk.navigate.{index}"), || {
                let mut raw = std::ptr::null_mut();
                // SAFETY: owned interfaces on this MTA. Inspect the HRESULT and nullable
                // out pointer separately: S_OK/null is end-of-list, not a provider error.
                unsafe {
                    let w = walker.as_ref();
                    let method = if index == 0 {
                        w.vtable().GetFirstChildElementBuildCache
                    } else {
                        w.vtable().GetNextSiblingElementBuildCache
                    };
                    method(
                        w.as_raw(),
                        current.as_ref().as_raw(),
                        cache.as_ref().as_raw(),
                        &mut raw,
                    )
                    .ok()?;
                    Ok((!raw.is_null())
                        .then(|| UIElement::from(IUIAutomationElement::from_raw(raw))))
                }
            })?;
            let Some(next) = next else {
                r.note(json!({"event":"walk_complete","count":index}))?;
                return Ok(());
            };
            let value = r.phase(&format!("walk.identity.{index}"), || {
                let hwnd: HWND = next.get_cached_native_window_handle()?.into();
                let mut value = json!({"index":index,"hwnd":hwnd.0 as isize});
                if mode == "metadata" {
                    value["pid"] = json!(next.get_cached_process_id()?);
                    value["class"] = json!(next.get_cached_classname()?);
                    value["role"] = json!(format!("{:?}", next.get_cached_control_type()?));
                }
                Ok(value)
            })?;
            r.note(json!({"event":"walk_element","identity":value}))?;
            current = next;
        }
        Err("root walk exceeded 256 elements; incomplete".into())
    })();
    r.note(json!({"event":"context_after","native":native_context()?,
        "status":if result.is_ok(){"observed"}else{"failed"},
        "error":result.as_ref().err().map(|e|e.to_string()),
        "cleanup":"no_business_resources_created"}))?;
    result
}

#[test]
#[ignore = "manual root comparison; set ACTL_ROOT_PROBE_OUTPUT/CACHE; no input/activation"]
fn root_sources() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::var("ACTL_ROOT_PROBE_CACHE")?;
    if !["handle", "metadata"].contains(&mode.as_str()) {
        return Err("ACTL_ROOT_PROBE_CACHE must be handle or metadata".into());
    }
    let output = PathBuf::from(std::env::var("ACTL_ROOT_PROBE_OUTPUT")?);
    let mut r = Recorder(File::create_new(output)?);
    // SAFETY: metadata read only, with no input or desktop authorization.
    let desktop = unsafe { GetDesktopWindow() };
    r.note(
        json!({"event":"context_before","cache_mode":mode,"native_desktop_hwnd":desktop.0 as isize,
        "native":native_context()?,"input_injected":false,"resources_created":false,
        "binary":std::env::current_exe()?,"ts_ms":actl_core::state::unix_ms()}),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let auto = r.phase("initialize", UIAutomation::new)?;
        let root = r.phase("get_root", || auto.get_root_element())?;
        let root_identity = r.phase("get_root.identity", || identity(&root))?;
        r.note(json!({"event":"root_identity","source":"get_root","identity":root_identity}))?;
        let by_handle = r.phase("desktop_from_handle", || {
            auto.element_from_handle(Handle::from(desktop.0 as isize))
        })?;
        let handle_identity = r.phase("desktop_from_handle.identity", || identity(&by_handle))?;
        r.note(json!({"event":"root_identity","source":"desktop_from_handle","identity":handle_identity,
            "same_runtime_id":root_identity["runtime_id"]==handle_identity["runtime_id"]}))?;
        let from_root = query(&auto, &root, "get_root", &mode, &mut r)?;
        r.note(json!({"event":"query_result","source":"get_root","result":from_root}))?;
        let from_handle = query(&auto, &by_handle, "desktop_from_handle", &mode, &mut r)?;
        r.note(
            json!({"event":"query_result","source":"desktop_from_handle","result":from_handle}),
        )?;
        Ok(())
    })();
    r.note(json!({"event":"context_after","native":native_context()?,
        "status":if result.is_ok(){"observed"}else{"failed"},
        "error":result.as_ref().err().map(|e|e.to_string()),
        "error_code":result.as_ref().err().and_then(|e|e.downcast_ref::<CtlError>()).map(|e|e.code),
        "cleanup":"no_business_resources_created"}))?;
    result
}

#[test]
#[ignore = "manual DPI context comparison; set ACTL_ROOT_PROBE_OUTPUT/CACHE"]
fn root_sources_pmv2() -> Result<(), Box<dyn std::error::Error>> {
    use windows::Win32::UI::HiDpi::{
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
    };
    // SAFETY: only this short-lived probe thread's coordinate virtualization changes.
    let previous =
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.is_invalid() {
        return Err(windows::core::Error::from_thread().into());
    }
    let result = root_sources();
    unsafe {
        SetThreadDpiAwarenessContext(previous);
    }
    result
}

#[test]
#[ignore = "manual production/root comparison; set ACTL_ROOT_PROBE_OUTPUT"]
fn production_inventory() -> Result<(), Box<dyn std::error::Error>> {
    let mut r = Recorder(File::create_new(std::env::var("ACTL_ROOT_PROBE_OUTPUT")?)?);
    r.note(json!({"event":"context_before","native":native_context()?,"binary":std::env::current_exe()?}))?;
    r.note(json!({"event":"started","phase":"production.inventory"}))?;
    let start = Instant::now();
    let result = actl_uia::list_windows();
    r.note(json!({"event":"production_inventory","duration_ms":start.elapsed().as_millis(),
        "count":result.as_ref().ok().map(Vec::len),
        "windows":result.as_ref().ok().map(|v|v.iter().map(|w|json!({"hwnd":w.hwnd,"pid":w.pid,"class":w.class})).collect::<Vec<_>>()),
        "error":result.as_ref().err().map(|e|json!({"code":e.code,"evidence":e.evidence}))}))?;
    let auto = r.phase("initialize", UIAutomation::new)?;
    let root = r.phase("get_root", || auto.get_root_element())?;
    let id = r.phase("root.identity", || identity(&root))?;
    r.note(json!({"event":"root_identity","identity":id}))?;
    let raw = query(&auto, &root, "raw_after_production", "metadata", &mut r)?;
    r.note(json!({"event":"query_result","result":raw,"native_after":native_context()?}))?;
    result?;
    Ok(())
}
