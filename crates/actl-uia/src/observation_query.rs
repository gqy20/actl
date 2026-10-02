//! Separate native queries from array conversion without reading extra properties.
use crate::observation_diagnostics::{Counts, Stage, observe_query};
use actl_core::{CtlError, ErrorCode};
use serde::Serialize;
use uiautomation::{
    UIElement,
    core::{UICacheRequest, UICondition},
    types::TreeScope,
};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    DesktopChildren,
    ElementChildren,
}

pub(crate) fn children(
    root: &UIElement,
    condition: &UICondition,
    cache: &UICacheRequest,
    scope: Scope,
) -> Result<Vec<UIElement>, CtlError> {
    enumerate(
        crate::observation::cached_hwnd(root),
        scope,
        || {
            // SAFETY: owned COM interfaces remain on the calling MTA; direct children only.
            unsafe {
                root.as_ref()
                    .FindAllBuildCache(TreeScope::Children.into(), condition, cache)
            }
            .map_err(|e| crate::read_channel::failure("children", e.into()))
        },
        |array| {
            // SAFETY: the successful native query returned an owned, non-null array.
            unsafe { array.Length() }
                .map_err(|e| crate::read_channel::failure("children", e.into()))
        },
        |array, index| {
            // SAFETY: index is within the array's reported length; ownership transfers to UIElement.
            unsafe { array.GetElement(index) }
                .map(UIElement::from)
                .map_err(|e| crate::read_channel::failure("children", e.into()))
        },
    )
}

fn enumerate<A, T>(
    hwnd: Option<isize>,
    scope: Scope,
    query: impl FnOnce() -> Result<A, CtlError>,
    length: impl FnOnce(&A) -> Result<i32, CtlError>,
    mut element: impl FnMut(&A, i32) -> Result<T, CtlError>,
) -> Result<Vec<T>, CtlError> {
    observe_query(
        Stage::Children,
        hwnd,
        scope,
        || {
            let array = observe_query(
                Stage::FindAllBuildCache,
                hwnd,
                scope,
                || {
                    let _trace = actl_core::trace::scope("observation.find_all_build_cache");
                    query()
                },
                |_| Counts::default(),
            )?;
            let length = observe_query(
                Stage::ArrayLength,
                hwnd,
                scope,
                || {
                    let _trace = actl_core::trace::scope("observation.array_length");
                    let raw = length(&array)?;
                    u32::try_from(raw).map_err(|_| {
                        CtlError::with_evidence(
                            ErrorCode::Internal,
                            "invalid UIA array length",
                            serde_json::json!({"reason":"observation_failed","channel":"children"}),
                        )
                    })
                },
                |result| Counts {
                    result_count: result.as_ref().ok().copied(),
                    ..Counts::default()
                },
            )?;
            let processed = std::cell::Cell::new(0);
            let failed = std::cell::Cell::new(None);
            observe_query(
                Stage::ArrayElements,
                hwnd,
                scope,
                || {
                    let _trace = actl_core::trace::scope("observation.array_elements");
                    let mut values = Vec::new();
                    for index in 0..length {
                        match element(&array, index as i32) {
                            Ok(value) => {
                                values.push(value);
                                processed.set(index + 1);
                            }
                            Err(error) => {
                                failed.set(Some(index));
                                return Err(error);
                            }
                        }
                    }
                    Ok(values)
                },
                |_| Counts {
                    result_count: Some(length),
                    processed_count: Some(processed.get()),
                    failed_index: failed.get(),
                },
            )
        },
        |result| Counts {
            result_count: result
                .as_ref()
                .ok()
                .and_then(|v| u32::try_from(v.len()).ok()),
            ..Counts::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use actl_core::{
        ErrorCode,
        history::read_timeline,
        signal_session::SignalSession,
        state::{Phase, SignalPaths},
    };
    use serde_json::{Value, json};

    fn recorded(
        run: impl FnOnce() -> Result<Vec<i32>, CtlError>,
    ) -> (Result<Vec<i32>, CtlError>, Vec<Value>) {
        let paths =
            SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()));
        let mut session = SignalSession::start_with_task(
            paths.clone(),
            "list-windows",
            None,
            Some("query-tests".into()),
        );
        let result = run();
        session.finish(
            if result.is_ok() {
                Phase::Done
            } else {
                Phase::Error
            },
            None,
        );
        let timeline = read_timeline(&paths.dir, Some("query-tests"), 500).unwrap();
        let events = timeline["events"].as_array().unwrap().clone();
        drop(session);
        std::fs::remove_dir_all(paths.dir).unwrap();
        (result, events)
    }
    fn fail() -> CtlError {
        crate::read_channel::failure(
            "children",
            uiautomation::errors::Error::new(-2146233083, "PRIVATE ERROR"),
        )
    }
    fn finished(events: &[Value]) -> Vec<Value> {
        events.iter().filter(|e| e["event"] == "observation_finished").map(|e| {
            let d=&e["data"];
            json!({"stage":d["stage"],"query_scope":d["query_scope"],"result_count":d["result_count"],"processed_count":d["processed_count"],"failed_index":d["failed_index"],"outcome":d["outcome"]})
        }).collect()
    }
    #[test]
    fn successful_query_keeps_native_and_conversion_phases() {
        let (result, events) = recorded(|| {
            enumerate(
                None,
                Scope::DesktopChildren,
                || Ok(()),
                |_| Ok(2),
                |_, i| Ok(i),
            )
        });
        assert_eq!(result.unwrap(), [0, 1]);
        let expected: Value =
            serde_json::from_str(include_str!("../tests/golden/observation_query.json")).unwrap();
        assert_eq!(json!(finished(&events)), expected);
        assert_eq!(
            events
                .iter()
                .filter(|e| e["event"] == "observation_started")
                .count(),
            4
        );
        assert!(!json!(events).to_string().contains("PRIVATE"));
    }
    #[test]
    fn query_failure_preserves_native_code_and_never_reads_array() {
        let (result, events) = recorded(|| {
            enumerate::<(), i32>(
                None,
                Scope::DesktopChildren,
                || Err(fail()),
                |_| panic!("no array"),
                |_, _| panic!("no array"),
            )
        });
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert_eq!(
            error.evidence.unwrap()["observation_stage"],
            "find_all_build_cache"
        );
        assert_eq!(finished(&events).len(), 2);
        assert!(!json!(events).to_string().contains("PRIVATE"));
    }
    #[test]
    fn length_failure_never_converts_elements() {
        let (result, _) = recorded(|| {
            enumerate::<(), i32>(
                None,
                Scope::ElementChildren,
                || Ok(()),
                |_| Err(fail()),
                |_, _| panic!("no length"),
            )
        });
        assert_eq!(
            result.unwrap_err().evidence.unwrap()["observation_stage"],
            "array_length"
        );
    }
    #[test]
    fn negative_length_is_not_a_successful_empty_tree() {
        let (result, _) = recorded(|| {
            enumerate::<(), i32>(
                None,
                Scope::DesktopChildren,
                || Ok(()),
                |_| Ok(-1),
                |_, _| panic!("invalid length"),
            )
        });
        assert_eq!(result.unwrap_err().code, ErrorCode::Internal);
    }
    #[test]
    fn zero_length_is_known_empty_and_never_reads_elements() {
        let (result, events) = recorded(|| {
            enumerate::<(), i32>(
                None,
                Scope::ElementChildren,
                || Ok(()),
                |_| Ok(0),
                |_, _| panic!("empty array"),
            )
        });
        assert!(result.unwrap().is_empty());
        let phases = finished(&events);
        assert_eq!(phases[2]["result_count"], 0);
        assert_eq!(phases[2]["processed_count"], 0);
    }
    #[test]
    fn conversion_failure_preserves_index_without_partial_success() {
        let (result, events) = recorded(|| {
            enumerate(
                None,
                Scope::ElementChildren,
                || Ok(()),
                |_| Ok(3),
                |_, i| if i == 1 { Err(fail()) } else { Ok(i) },
            )
        });
        assert_eq!(
            result.unwrap_err().evidence.unwrap()["observation_stage"],
            "array_elements"
        );
        let phases = finished(&events);
        assert_eq!(phases[2]["result_count"], 3);
        assert_eq!(phases[2]["processed_count"], 1);
        assert_eq!(phases[2]["failed_index"], 1);
        assert_eq!(phases[2]["outcome"], "failed");
    }
}
