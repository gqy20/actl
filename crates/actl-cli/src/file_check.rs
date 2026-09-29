//! Fixed read-only file inspector. Never evaluates caller supplied code.
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
const LIMIT: u64 = 4 * 1024 * 1024;

fn command_has_path(line: &str, path: &str) -> bool {
    line.match_indices(path).any(|(index, _)| {
        let before = line[..index].chars().next_back();
        let after = line[index + path.len()..].chars().next();
        before.is_none_or(|c| c.is_whitespace() || c == '"' || c == '=')
            && after.is_none_or(|c| c.is_whitespace() || c == '"')
    })
}

pub fn element_text(
    target: &str,
    expected: &str,
    property: &str,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<Value, CtlError> {
    let result = crate::query::get(target, Some(property), app, near, None)?.data;
    let actual = result["properties"][property]
        .as_str()
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "text channel unavailable"))?;
    let normalize = |text: &str| {
        text.replace("\r\n", "\n")
            .replace('\r', "\n")
            .trim_end_matches('\n')
            .to_owned()
    };
    if normalize(actual) != normalize(expected) {
        return Err(CtlError::new(
            ErrorCode::AssertionFailed,
            "normalized text differs",
        ));
    }
    Ok(json!({"verified":true,"channel":property,"comparison":"normalized_exact"}))
}

pub fn owner(
    app: &str,
    program: &str,
    token: &str,
    instance: Option<&str>,
    expected_hwnd: Option<&str>,
    expected_pid: Option<u32>,
) -> Result<Value, CtlError> {
    if !std::path::Path::new(token).is_absolute() || token.len() < 12 {
        return Err(CtlError::protocol(
            "owner token requires an absolute task path",
        ));
    }
    let windows = actl_uia::native_windows()?;
    let windows: Vec<_> = windows
        .iter()
        .filter(|window| {
            window.visible
                && if let Some(hwnd) = app.strip_prefix("hwnd:") {
                    window.hwnd == actl_uia::parse_hwnd(hwnd).ok()
                } else {
                    window.title.contains(app)
                }
        })
        .collect();
    if windows.len() != 1 {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "owned native window missing or ambiguous",
        ));
    }
    let window = windows[0];
    if expected_hwnd
        .map(actl_uia::parse_hwnd)
        .transpose()?
        .is_some_and(|hwnd| window.hwnd != Some(hwnd))
        || expected_pid.is_some_and(|pid| window.pid != pid)
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "native window identity changed",
        ));
    }
    let data = inspect_office(&window.pid.to_string(), "process")?;
    if instance.is_some_and(|expected| data["start"] != expected) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "process instance changed",
        ));
    }
    let normalize = |text: &str| text.replace('/', "\\").to_lowercase();
    if data["program"]
        .as_str()
        .is_none_or(|path| normalize(path) != normalize(program))
        || data["command_line"]
            .as_str()
            .is_none_or(|line| !command_has_path(&normalize(line), &normalize(token)))
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "process task ownership unconfirmed",
        ));
    }
    Ok(json!({"verified":true,"hwnd":window.hwnd,"pid":window.pid,"process_start":data["start"]}))
}

pub fn closed(hwnd: &str, pid: u32, instance: &str) -> Result<Value, CtlError> {
    settle(|| closed_once(hwnd, pid, instance))
}
fn closed_once(hwnd: &str, pid: u32, instance: &str) -> Result<Value, CtlError> {
    let hwnd = actl_uia::parse_hwnd(hwnd)?;
    let windows = actl_uia::native_windows()?;
    let process = inspect_office(&pid.to_string(), "process")?;
    let absent = process["state"] == "absent";
    if absent && windows.iter().any(|w| w.hwnd == Some(hwnd) && w.pid == pid) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "native window remains while process state is absent",
        ));
    }
    let same = process["start"]
        .as_str()
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "unknown process identity"));
    if !absent && same? == instance {
        return Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "owned process instance remains",
            json!({"window_present":windows.iter().any(|w| w.hwnd == Some(hwnd) && w.pid == pid)}),
        ));
    }
    // A reused PID belongs to a different process; never close it.
    Ok(json!({"verified":true,"original_instance_gone":true}))
}
pub fn clipboard(expected: &str, normalize_lines: bool) -> Result<Value, CtlError> {
    let result = crate::dispatch(&crate::cli::Command::GetClipboard)?.data;
    let actual = result["text"]
        .as_str()
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "clipboard text unavailable"))?;
    let normalize = |text: &str| {
        if normalize_lines {
            text.replace("\r\n", "\n").trim_end_matches('\n').to_owned()
        } else {
            text.to_owned()
        }
    };
    if normalize(actual) != normalize(expected) {
        return Err(CtlError::new(
            ErrorCode::AssertionFailed,
            "clipboard differs",
        ));
    }
    Ok(json!({"verified":true}))
}

pub fn rows_expected(prefix: &str, expected: &str) -> Result<Vec<String>, CtlError> {
    if prefix.strip_suffix("-row-").is_none_or(str::is_empty) {
        return Err(CtlError::protocol(
            "row prefix must name a parent tree followed by -row-",
        ));
    }
    let value = expectation(expected)?;
    serde_json::from_value(value)
        .map_err(|_| CtlError::protocol("rows expectation must be a string array"))
}
pub fn rows(app: &str, prefix: &str, expected: &str) -> Result<Value, CtlError> {
    settle(|| rows_once(app, prefix, expected))
}
fn rows_once(app: &str, prefix: &str, expected: &str) -> Result<Value, CtlError> {
    use clap::Parser;
    let mut expected = rows_expected(prefix, expected)?;
    let command = crate::cli::Cli::try_parse_from(["actl", "snapshot", "--app", app])
        .map_err(|e| CtlError::protocol(e.to_string()))?
        .command;
    let result = crate::dispatch(&command)?.data;
    if result["truncated"] == true {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "row snapshot truncated",
        ));
    }
    let elements = result["elements"]
        .as_array()
        .ok_or_else(|| CtlError::internal("snapshot elements unavailable"))?;
    let parent = prefix
        .strip_suffix("-row-")
        .ok_or_else(|| CtlError::protocol("invalid row prefix"))?;
    if elements
        .iter()
        .filter(|item| item["automation_id"] == parent)
        .count()
        != 1
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "row parent tree missing or ambiguous",
        ));
    }
    let mut actual = elements
        .iter()
        .filter(|item| {
            item["role"] == "TreeItem"
                && item["automation_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(prefix))
        })
        .map(|item| {
            item["name"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "row name unknown"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(CtlError::new(
            ErrorCode::AssertionFailed,
            "result rows differ",
        ));
    }
    Ok(json!({"verified":true,"rows":actual.len()}))
}

pub fn expectation(text: &str) -> Result<Value, CtlError> {
    if text.len() > 16384 {
        return Err(CtlError::protocol("file expectation exceeds 16 KiB"));
    }
    serde_json::from_str(text).map_err(|_| CtlError::protocol("expected must be a JSON value"))
}
fn matches(actual: &Value, expected: &Value, subset: bool) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(b)) if subset => b
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|a| matches(a, v, true))),
        (Value::Array(a), Value::Array(b)) if subset => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| matches(a, b, true))
        }
        _ => actual == expected,
    }
}
pub fn verify(path: &str, kind: &str, expected: &str, subset: bool) -> Result<Value, CtlError> {
    settle(|| verify_once(path, kind, expected, subset))
}
fn settle(mut read: impl FnMut() -> Result<Value, CtlError>) -> Result<Value, CtlError> {
    let start = std::time::Instant::now();
    loop {
        match read() {
            Err(error)
                if error.code == ErrorCode::AssertionFailed
                    && start.elapsed().as_millis() < 5000 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            result => return result,
        }
    }
}
fn verify_once(path: &str, kind: &str, expected: &str, subset: bool) -> Result<Value, CtlError> {
    let expected = expectation(expected)?;
    let actual = if matches!(kind, "text" | "json") {
        let file = std::fs::File::open(path).map_err(|error| {
            CtlError::new(
                if error.kind() == std::io::ErrorKind::NotFound {
                    ErrorCode::AssertionFailed
                } else {
                    ErrorCode::NotActionable
                },
                "cannot read evidence file",
            )
        })?;
        let mut text = String::new();
        file.take(LIMIT + 1)
            .read_to_string(&mut text)
            .map_err(|_| CtlError::new(ErrorCode::NotActionable, "invalid UTF-8 evidence file"))?;
        if text.len() as u64 > LIMIT {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "evidence file exceeds 4 MiB",
            ));
        }
        if kind == "text" {
            Value::String(text.replace("\r\n", "\n"))
        } else {
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| {
                CtlError::new(ErrorCode::NotActionable, "invalid JSON evidence file")
            })?
        }
    } else {
        inspect_office(path, kind)?
    };
    if !matches(&actual, &expected, subset) {
        return Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "file content differs",
            json!({"channel":kind,"comparison":if subset {"subset"} else {"exact"},"path":path}),
        ));
    }
    Ok(json!({"verified":true,"kind":kind,"comparison":if subset {"subset"} else {"exact"}}))
}
fn inspect_office(path: &str, kind: &str) -> Result<Value, CtlError> {
    let root = std::env::var_os("SystemRoot")
        .ok_or_else(|| CtlError::internal("SystemRoot unavailable"))?;
    let program =
        std::path::Path::new(&root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            include_str!("file_inspect.ps1"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| CtlError::internal("file inspector could not start"))?;
    let input = serde_json::to_vec(&json!({"path":path,"kind":kind}))
        .map_err(|e| CtlError::internal(e.to_string()))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CtlError::internal("inspector stdin unavailable"))?;
    if stdin.write_all(&input).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CtlError::internal("inspector request failed"));
    }
    drop(stdin);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CtlError::internal("inspector stdout unavailable"))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|_| CtlError::internal("inspector wait failed"))?
        {
            break status;
        }
        if start.elapsed().as_secs() >= 20 {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "file inspection timed out",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    };
    let output = reader
        .join()
        .map_err(|_| CtlError::internal("inspector reader failed"))?
        .map_err(|_| CtlError::internal("inspector output failed"))?;
    if !status.success() || output.len() as u64 > LIMIT {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "Office evidence unreadable or exceeds limit",
        ));
    }
    serde_json::from_slice(&output).map_err(|_| CtlError::internal("invalid inspector response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_requires_a_complete_path_argument() {
        assert!(command_has_path(
            "app --user-data-dir=\"c:\\task profile\"",
            "c:\\task profile"
        ));
        assert!(!command_has_path("app c:\\task-other", "c:\\task"));
        assert!(!command_has_path("app prefixc:\\task", "c:\\task"));
    }
    #[test]
    fn subset_objects_still_require_exact_array_lengths() {
        assert!(matches(&json!({"a":1,"b":2}), &json!({"a":1}), true));
        assert!(!matches(&json!([{"a":1},{"a":1}]), &json!([{"a":1}]), true));
        assert!(rows_expected("row", "[]").is_err());
        assert!(rows_expected("tree-row-", "[null]").is_err());
    }
}
