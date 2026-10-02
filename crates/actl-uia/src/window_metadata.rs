//! Window properties share one cache refresh; unnamed entries stay unobserved.
use actl_core::CtlError;
use uiautomation::{
    UIAutomation, UIElement,
    core::UICacheRequest,
    types::{TreeScope, UIProperty},
};

pub(crate) struct Metadata {
    pub title: String,
    pub class: String,
    pub pid: u32,
    pub hwnd: Option<isize>,
}

trait Properties: Clone {
    fn name(&self) -> Result<String, CtlError>;
    fn class(&self) -> Result<String, CtlError>;
    fn pid(&self) -> Result<u32, CtlError>;
    fn hwnd(&self) -> Result<Option<isize>, CtlError>;
    fn refresh(&self) -> Result<Self, CtlError>;
}

fn read(properties: &impl Properties) -> Result<Option<Metadata>, CtlError> {
    let title = properties.name()?;
    if title.is_empty() {
        return Ok(None);
    }
    let properties = properties.refresh()?;
    Ok(Some(Metadata {
        title,
        hwnd: properties.hwnd()?,
        class: properties.class()?,
        pid: properties.pid()?,
    }))
}

pub(crate) struct Query {
    cache: UICacheRequest,
}
impl Query {
    pub fn new(auto: &UIAutomation) -> Result<Self, CtlError> {
        let cache = auto.create_cache_request().map_err(crate::internal)?;
        cache
            .set_tree_scope(TreeScope::Element)
            .map_err(crate::internal)?;
        cache
            .set_tree_filter(auto.create_true_condition().map_err(crate::internal)?)
            .map_err(crate::internal)?;
        for property in [
            UIProperty::ClassName,
            UIProperty::ProcessId,
            UIProperty::NativeWindowHandle,
        ] {
            cache.add_property(property).map_err(crate::internal)?;
        }
        Ok(Self { cache })
    }
    pub fn read(&self, element: &UIElement) -> Result<Option<Metadata>, CtlError> {
        read(&Provider {
            element: element.clone(),
            cache: &self.cache,
        })
    }
}

#[derive(Clone)]
struct Provider<'a> {
    element: UIElement,
    cache: &'a UICacheRequest,
}
impl Properties for Provider<'_> {
    fn name(&self) -> Result<String, CtlError> {
        self.element
            .get_name()
            .map_err(|e| crate::read_channel::failure("window.name", e))
    }
    fn class(&self) -> Result<String, CtlError> {
        self.element
            .get_cached_classname()
            .map_err(|e| crate::read_channel::failure("window.class", e))
    }
    fn pid(&self) -> Result<u32, CtlError> {
        // Match the existing wrapper's CurrentProcessId signed-to-unsigned conversion.
        self.element
            .get_cached_process_id()
            .map(|n| n as u32)
            .map_err(|e| crate::read_channel::failure("window.pid", e))
    }
    fn hwnd(&self) -> Result<Option<isize>, CtlError> {
        let handle: windows::Win32::Foundation::HWND = self
            .element
            .get_cached_native_window_handle()
            .map_err(|e| crate::read_channel::failure("window.hwnd", e))?
            .into();
        Ok((!handle.0.is_null()).then_some(handle.0 as isize))
    }
    fn refresh(&self) -> Result<Self, CtlError> {
        let element = self
            .element
            .build_updated_cache(self.cache)
            .map_err(|e| crate::read_channel::failure("window.cache", e))?;
        Ok(Self {
            element,
            cache: self.cache,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actl_core::ErrorCode;
    use std::{cell::Cell, rc::Rc};
    #[derive(Clone)]
    struct Fake {
        calls: Rc<Cell<u32>>,
        cached: bool,
        title: &'static str,
        refresh_fails: bool,
        field_fails: bool,
        hwnd: Option<isize>,
    }
    impl Fake {
        fn count(&self) {
            if !self.cached {
                self.calls.set(self.calls.get() + 1);
            }
        }
        fn new() -> Self {
            Self {
                calls: Rc::new(Cell::new(0)),
                cached: false,
                title: "target",
                refresh_fails: false,
                field_fails: false,
                hwnd: Some(17),
            }
        }
    }
    impl Properties for Fake {
        fn name(&self) -> Result<String, CtlError> {
            self.count();
            Ok(self.title.into())
        }
        fn class(&self) -> Result<String, CtlError> {
            self.count();
            if self.cached && self.field_fails {
                Err(CtlError::internal("cached property unavailable"))
            } else {
                Ok("test".into())
            }
        }
        fn pid(&self) -> Result<u32, CtlError> {
            self.count();
            Ok(42)
        }
        fn hwnd(&self) -> Result<Option<isize>, CtlError> {
            self.count();
            Ok(self.hwnd)
        }
        fn refresh(&self) -> Result<Self, CtlError> {
            self.calls.set(self.calls.get() + 1);
            if self.refresh_fails {
                return Err(CtlError::new(ErrorCode::Timeout, "refresh timed out"));
            }
            let mut cached = self.clone();
            cached.cached = true;
            Ok(cached)
        }
    }
    #[test]
    fn named_window_uses_two_provider_round_trips() {
        let source = Fake::new();
        let metadata = read(&source).unwrap().unwrap();
        assert_eq!(
            (
                metadata.title.as_str(),
                metadata.class.as_str(),
                metadata.pid,
                metadata.hwnd
            ),
            ("target", "test", 42, Some(17))
        );
        assert_eq!(source.calls.get(), 2);
    }
    #[test]
    fn refresh_failure_is_not_replaced_by_live_reads() {
        let mut source = Fake::new();
        source.refresh_fails = true;
        assert_eq!(read(&source).err().unwrap().code, ErrorCode::Timeout);
        assert_eq!(source.calls.get(), 2);
    }
    #[test]
    fn cached_field_failure_is_not_replaced_by_live_reads() {
        let mut source = Fake::new();
        source.field_fails = true;
        assert_eq!(read(&source).err().unwrap().code, ErrorCode::Internal);
        assert_eq!(source.calls.get(), 2);
    }
    #[test]
    fn unnamed_window_does_not_request_other_properties() {
        let mut source = Fake::new();
        source.title = "";
        source.refresh_fails = true;
        assert!(read(&source).unwrap().is_none());
        assert_eq!(source.calls.get(), 1);
    }
    #[test]
    fn virtual_window_retains_missing_native_handle() {
        let mut source = Fake::new();
        source.hwnd = None;
        assert!(read(&source).unwrap().unwrap().hwnd.is_none());
    }

    #[test]
    #[ignore = "read-only interactive validation; set ACTL_WINDOW_METADATA_HWND/OUTPUT"]
    fn cached_properties_match_live_provider() -> Result<(), Box<dyn std::error::Error>> {
        use actl_core::ErrorCode;
        use std::io::Write;
        let raw: isize = std::env::var("ACTL_WINDOW_METADATA_HWND")?.parse()?;
        let mut output = std::fs::File::create_new(std::env::var("ACTL_WINDOW_METADATA_OUTPUT")?)?;
        let result = (|| -> Result<serde_json::Value, CtlError> {
            let before = crate::native_window(raw)?;
            if before.len() != 1 || !before[0].visible || before[0].minimized {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "target unavailable; no activation",
                ));
            }
            let process_before = crate::evidence::process(before[0].pid, false)?;
            let auto = UIAutomation::new().map_err(crate::internal)?;
            let root = auto
                .element_from_handle(uiautomation::types::Handle::from(raw))
                .map_err(crate::internal)?;
            let runtime_before = root.get_runtime_id().map_err(crate::internal)?;
            let query = Query::new(&auto)?;
            let started = std::time::Instant::now();
            let cached = query
                .read(&root)?
                .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "unnamed target"))?;
            let cache_ms = started.elapsed().as_millis();
            let current_name = root.get_name().map_err(crate::internal)?;
            let current_class = root.get_classname().map_err(crate::internal)?;
            let current_pid = root.get_process_id().map_err(crate::internal)?;
            let current_hwnd: windows::Win32::Foundation::HWND = root
                .get_native_window_handle()
                .map_err(crate::internal)?
                .into();
            let current_hwnd = (!current_hwnd.0.is_null()).then_some(current_hwnd.0 as isize);
            let children = crate::observation::ChildrenQuery::new(
                &auto,
                &[UIProperty::NativeWindowHandle, UIProperty::ControlType],
            )?
            .children(&root)?;
            for child in &children {
                let cached_role = child.get_cached_control_type().map_err(crate::internal)?;
                let current_role = child.get_control_type().map_err(crate::internal)?;
                let cached_handle: windows::Win32::Foundation::HWND = child
                    .get_cached_native_window_handle()
                    .map_err(crate::internal)?
                    .into();
                let current_handle: windows::Win32::Foundation::HWND = child
                    .get_native_window_handle()
                    .map_err(crate::internal)?
                    .into();
                if cached_role != current_role || cached_handle != current_handle {
                    return Err(CtlError::new(
                        ErrorCode::StaleRef,
                        "child properties changed during comparison",
                    ));
                }
            }
            let after = crate::native_window(raw)?;
            if after.len() != 1
                || before[0].pid != after[0].pid
                || before[0].class != after[0].class
                || process_before["start"].is_null()
                || process_before != crate::evidence::process(after[0].pid, false)?
                || runtime_before != root.get_runtime_id().map_err(crate::internal)?
            {
                return Err(CtlError::new(
                    ErrorCode::StaleRef,
                    "window identity changed during comparison",
                ));
            }
            if cached.title != current_name
                || cached.class != current_class
                || cached.pid != current_pid
                || cached.hwnd != current_hwnd
            {
                return Err(CtlError::new(
                    ErrorCode::StaleRef,
                    "window properties changed during comparison",
                ));
            }
            Ok(
                serde_json::json!({"hwnd":raw,"pid":cached.pid,"window_identity_stable":true,
                "metadata_equal":true,"child_properties_equal":true,"children":children.len(),
                "cached_metadata_ms":cache_ms,"process":process_before,
                "input_injected":false,"cleanup":"no_resources_created"}),
            )
        })();
        output.write_all(&serde_json::to_vec_pretty(&serde_json::json!({"data":result.as_ref().ok(),
            "error":result.as_ref().err().map(|e|serde_json::json!({"code":e.code,"evidence":e.evidence,"message":e.message}))}))?)?;
        result?;
        Ok(())
    }
}
