//! Read-only window assertions; missing, ambiguous and failed observations stay distinct.
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};

pub fn verify_window(app: &str, state: &str) -> Result<Value, CtlError> {
    validate(app, state)?;
    let rows = if let Some(raw) = app.strip_prefix("hwnd:") {
        crate::native_window(crate::parse_hwnd(raw)?)?
    } else {
        crate::native_windows()?
    };
    evaluate(&rows, app, state)
}
pub fn validate(app: &str, state: &str) -> Result<(), CtlError> {
    if app.trim().is_empty()
        || !matches!(
            state,
            "exists" | "absent" | "foreground" | "minimized" | "visible"
        )
    {
        return Err(CtlError::protocol(
            "verify-window requires a nonempty app and exists/absent/foreground/minimized/visible state",
        ));
    }
    if let Some(raw) = app.strip_prefix("hwnd:") {
        crate::parse_hwnd(raw)?;
    }
    Ok(())
}
fn evaluate(rows: &[crate::WindowEntry], app: &str, state: &str) -> Result<Value, CtlError> {
    let hwnd = app
        .strip_prefix("hwnd:")
        .map(crate::parse_hwnd)
        .transpose()?;
    let matches: Vec<_> = rows
        .iter()
        .filter(|w| match hwnd {
            Some(id) => w.hwnd == Some(id),
            None => w.title.contains(app),
        })
        .collect();
    if matches.len() > 1 {
        return Err(CtlError::new(
            ErrorCode::Ambiguous,
            "window condition matched multiple windows; narrow --app",
        ));
    }
    let window = matches.first();
    let matched = match state {
        "exists" => window.is_some(),
        "absent" => window.is_none(),
        "foreground" => window.is_some_and(|w| w.foreground),
        "minimized" => window.is_some_and(|w| w.minimized),
        "visible" => window.is_some_and(|w| w.visible),
        _ => return Err(CtlError::protocol("invalid window state")),
    };
    let window=window.map(|w|json!({"hwnd":w.hwnd,"pid":w.pid,"title":w.title,"foreground":w.foreground,"minimized":w.minimized,"visible":w.visible}));
    let data = json!({"action":"verify-window","state":state,"matched":matched,"window":window});
    if matched {
        Ok(data)
    } else {
        Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "window state did not match",
            data,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn absent_is_known_false_and_ambiguity_is_not_a_negative_match() {
        assert!(evaluate(&[], "test", "absent").is_ok());
        assert_eq!(
            evaluate(&[], "test", "exists").unwrap_err().code,
            ErrorCode::AssertionFailed
        );
        let row = || crate::WindowEntry {
            title: "test".into(),
            class: "Test".into(),
            pid: 42,
            foreground: true,
            visible: true,
            minimized: false,
            topmost: false,
            z_order: None,
            rect: None,
            owner_title: None,
            hwnd: Some(123),
        };
        assert!(evaluate(&[row()], "test", "foreground").is_ok());
        assert_eq!(
            evaluate(&[row(), row()], "test", "exists")
                .unwrap_err()
                .code,
            ErrorCode::Ambiguous
        );
        assert!(validate("", "exists").is_err());
        assert!(validate("test", "unknown").is_err());
    }
}
