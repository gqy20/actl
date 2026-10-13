//! Fixed read-only file inspector. Never evaluates caller supplied code.
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::io::Read;
const LIMIT: u64 = 4 * 1024 * 1024;

pub fn url_query(
    target: &str,
    expected: &str,
    key: &str,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<Value, CtlError> {
    if key.is_empty() {
        return Err(CtlError::protocol("URL query key must not be empty"));
    }
    let value = crate::query::get(target, Some("value"), app, near, None)?.data;
    let url = value["properties"]["value"]
        .as_str()
        .ok_or_else(|| CtlError::new(ErrorCode::NotActionable, "URL value unavailable"))?;
    if decoded_query(url, key)? != expected {
        return Err(CtlError::new(
            ErrorCode::AssertionFailed,
            "URL query value differs",
        ));
    }
    Ok(json!({"verified":true,"channel":"value","comparison":"url_query_exact"}))
}
fn decoded_query(url: &str, key: &str) -> Result<String, CtlError> {
    fn decode(text: &str) -> Result<String, CtlError> {
        let mut bytes = Vec::new();
        let mut source = text.bytes();
        while let Some(byte) = source.next() {
            if byte == b'%' {
                let a = source.next().and_then(|b| (b as char).to_digit(16));
                let b = source.next().and_then(|b| (b as char).to_digit(16));
                match (a, b) {
                    (Some(a), Some(b)) => bytes.push((a * 16 + b) as u8),
                    _ => {
                        return Err(CtlError::new(
                            ErrorCode::NotActionable,
                            "invalid URL escape",
                        ));
                    }
                }
            } else {
                bytes.push(byte);
            }
        }
        String::from_utf8(bytes)
            .map_err(|_| CtlError::new(ErrorCode::NotActionable, "invalid URL UTF-8"))
    }
    let query = url
        .split('#')
        .next()
        .unwrap_or("")
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or("");
    let mut found = None;
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if decode(name)? == key {
            if found.is_some() {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "duplicate URL query key",
                ));
            }
            found = Some(decode(value)?);
        }
    }
    found.ok_or_else(|| CtlError::new(ErrorCode::AssertionFailed, "URL query key missing"))
}

#[cfg(test)]
mod url_tests {
    use super::*;
    #[test]
    fn exact_decoding_and_ambiguous_refusal() {
        assert_eq!(
            decoded_query(
                "uccd://cloud/path?keyword=%E4%B8%AD%20x%26y#ignored",
                "keyword"
            )
            .unwrap(),
            "中 x&y"
        );
        assert_eq!(decoded_query("x:?keyword=a+b", "keyword").unwrap(), "a+b");
        for url in [
            "x:?keyword=%FF",
            "x:?keyword=%",
            "x:?keyword=a&keyword=b",
            "x:#?keyword=a",
        ] {
            assert!(decoded_query(url, "keyword").is_err());
        }
    }
}

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
    // 元素重渲染期读到旧值表现为内容不符(断言失败),与 verify-file 家族同款 5 秒沉降。
    settle(|| element_text_once(target, expected, property, app, near))
}
fn element_text_once(
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

/// 进程树归属判定(纯函数,可测):一份进程证据是否同时满足 program 与令牌。
fn lineage_match(data: &serde_json::Value, program: &str, token: &str) -> bool {
    let normalize = |text: &str| text.replace('/', "\\").to_lowercase();
    data["program"]
        .as_str()
        .is_some_and(|path| normalize(path) == normalize(program))
        && data["command_line"]
            .as_str()
            .is_some_and(|line| command_has_path(&normalize(line), &normalize(token)))
}

/// 窗口属主进程的命令行不含任务令牌时,沿父链(有界 4 级)找携带令牌的祖先:
/// 保存/下载/打印对话框常跑在 contentproc 等子进程,令牌只在主进程命令行
/// (zotero 保存对话框实证)。返回祖先证据供成功载荷说明。
fn lineage_token_owner(start_pid: u32, program: &str, token: &str) -> Option<serde_json::Value> {
    let mut pid = start_pid;
    for _ in 0..4 {
        let parent = actl_uia::evidence::parent_pid(pid)?;
        let data = inspect_office(&parent.to_string(), "process").ok()?;
        if data["state"].as_str() == Some("absent") {
            return None;
        }
        if lineage_match(&data, program, token) {
            return Some(
                json!({"pid": parent, "program": data["program"], "start": data["start"]}),
            );
        }
        pid = parent;
    }
    None
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
    // 直接归属失败时按进程树回退:子进程对话框的令牌在祖先命令行。
    let direct = lineage_match(&data, program, token);
    let via_parent = if direct {
        None
    } else {
        let lineage = lineage_token_owner(window.pid, program, token);
        if lineage.is_none() {
            return Err(CtlError::with_evidence(
                ErrorCode::NotActionable,
                "process task ownership unconfirmed",
                json!({"direct":false,"lineage_searched":true}),
            ));
        }
        lineage
    };
    // hwnd_anchor:即绑即用的窗口锚字符串(2026-10-11,flow 锚点标题插值
    // 登记项的降格收口):HWND 不随标题变(SPA/脏标记改标题正是引入 hwnd
    // 锚的原因),outputs.pointer 指到 /hwnd_anchor 即可把动态标题窗口
    // 锚进后续任意 --app 参数槽——静态计划无须标题插值。
    let hwnd_anchor = window
        .hwnd
        .map(|h| format!("hwnd:{h}"))
        .map(serde_json::Value::String);
    Ok(
        json!({"verified":true,"hwnd":window.hwnd,"hwnd_anchor":hwnd_anchor,"pid":window.pid,"process_start":data["start"],"via_parent":via_parent}),
    )
}

pub fn closed(hwnd: &str, pid: u32, instance: &str) -> Result<Value, CtlError> {
    settle(|| closed_once(hwnd, pid, instance))
}
fn closed_once(hwnd: &str, pid: u32, instance: &str) -> Result<Value, CtlError> {
    let hwnd = actl_uia::parse_hwnd(hwnd)?;
    let process = inspect_office(&pid.to_string(), "process-identity")?;
    // Query windows after the slower process read, not before a possible exit.
    let windows = actl_uia::native_windows()?;
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
    // 复制送达与剪贴板可读之间有竞态:空剪贴板表现为 NOT_ACTIONABLE、旧内容为
    // 断言失败,两者都在 5 秒内重读(剪贴板只读,重试无副作用)。
    let start = std::time::Instant::now();
    loop {
        match clipboard_once(expected, normalize_lines) {
            Err(error)
                if matches!(
                    error.code,
                    ErrorCode::AssertionFailed | ErrorCode::NotActionable
                ) && start.elapsed().as_millis() < 5000 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(
                    actl_core::timing::Timing::load().poll_ms,
                ))
            }
            result => return result,
        }
    }
}
fn clipboard_once(expected: &str, normalize_lines: bool) -> Result<Value, CtlError> {
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
    // 行树缺失(渲染未完成)与内容不符同为可沉降瞬态;歧义立即失败。
    let start = std::time::Instant::now();
    loop {
        match rows_once(app, prefix, expected) {
            Err(error)
                if matches!(error.code, ErrorCode::AssertionFailed | ErrorCode::NotFound)
                    && start.elapsed().as_millis() < 5000 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(
                    actl_core::timing::Timing::load().poll_ms,
                ))
            }
            result => return result,
        }
    }
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
        == 0
    {
        // 缺失与歧义分码:缺失可能是"尚未渲染"(NOT_FOUND,可重试观察);
        // 歧义是既成事实(NOT_ACTIONABLE,重试无意义)。
        return Err(CtlError::new(
            ErrorCode::NotFound,
            "row parent tree not present",
        ));
    }
    if elements
        .iter()
        .filter(|item| item["automation_id"] == parent)
        .count()
        > 1
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "row parent tree ambiguous",
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
pub fn verify(
    path: &str,
    kind: &str,
    expected: &str,
    subset: bool,
    discover: Option<&str>,
) -> Result<Value, CtlError> {
    match discover {
        None => settle(|| verify_once(path, kind, expected, subset)),
        // 发现形态(2026-10-12 Zotero 导出轮):保存对话框按类型过滤器补扩展名
        // (输入 .csl.json 落盘 .csl.json.bib),按预设精确名断言会静默落空——
        // 目录内按通配取 mtime 最新者断言,信封带 discovered 实名。发现也进
        // settle 窗:导出异步落盘时晚到的文件同样被等到。
        Some(pattern) => {
            let mut hit = String::new();
            let result = settle(|| {
                let found = newest_match(path, pattern)?;
                let outcome = verify_once(&found, kind, expected, subset);
                hit = found;
                outcome
            });
            let mut value = result?;
            if let Some(object) = value.as_object_mut() {
                object.insert("discovered".into(), json!(hit));
            }
            Ok(value)
        }
    }
}
use crate::file_discover::newest_match;
fn settle(mut read: impl FnMut() -> Result<Value, CtlError>) -> Result<Value, CtlError> {
    let start = std::time::Instant::now();
    loop {
        match read() {
            Err(error)
                if error.code == ErrorCode::AssertionFailed
                    && start.elapsed().as_millis() < 5000 =>
            {
                std::thread::sleep(std::time::Duration::from_millis(
                    actl_core::timing::Timing::load().poll_ms,
                ))
            }
            result => return result,
        }
    }
}
/// IO 失败的可区分分类（docs/ROADMAP P1 补：共享冲突 vs 权限 vs 不存在——
/// 内容不符已有 expected 语义，此项只细分打不开的原因）。NotFound 保持
/// ASSERTION_FAILED（文件可能稍后出现，settle 窗内重试）；其余 NOT_ACTIONABLE，
/// evidence 携带 io_kind/os_code 供失败处置协议直接读证。
pub(crate) fn io_failure(error: &std::io::Error) -> CtlError {
    let os_code = error.raw_os_error();
    let io_kind = match os_code {
        // ERROR_SHARING_VIOLATION：文件被 Excel/Word 等持有句柄
        Some(32) => "sharing_violation",
        // ERROR_ACCESS_DENIED / ERROR_SHARING_VIOLATION 之外按 std 分类
        _ => match error.kind() {
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::InvalidFilename => "invalid_filename",
            _ => "other",
        },
    };
    CtlError::with_evidence(
        ErrorCode::NotActionable,
        "cannot read evidence file",
        json!({"io_kind": io_kind, "os_code": os_code, "raw": error.to_string()}),
    )
}

fn verify_once(path: &str, kind: &str, expected: &str, subset: bool) -> Result<Value, CtlError> {
    let expected = expectation(expected)?;
    let actual = if matches!(kind, "text" | "json") {
        let file = std::fs::File::open(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "cannot read evidence file",
                    json!({"io_kind": "not_found", "os_code": error.raw_os_error()}),
                )
            } else {
                io_failure(&error)
            }
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
    } else if kind == "meta" {
        // 文件元数据断言(docs/81 提速轮):size/mtime 读取不受 Office 排他锁影响,
        // 是"保存确实落盘"的唯一打开期断言通道。可选关系键 size_min/mtime_min
        // (unix 毫秒)供单调性断言;其余键走通用子集/全等比较。
        let meta = std::fs::metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "cannot read evidence file",
                    json!({"io_kind": "not_found", "os_code": error.raw_os_error()}),
                )
            } else {
                io_failure(&error)
            }
        })?;
        let mtime_unix_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        json!({"size": meta.len(), "mtime_unix_ms": mtime_unix_ms})
    } else {
        inspect_office(path, kind)?
    };
    if kind == "meta" {
        if let Some(min) = expected.get("size_min").and_then(Value::as_u64) {
            let size = actual.get("size").and_then(Value::as_u64).unwrap_or(0);
            if size < min {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "file size below minimum",
                    json!({"size": size, "size_min": min, "path": path}),
                ));
            }
        }
        if let Some(min) = expected.get("mtime_min").and_then(Value::as_u64) {
            match actual.get("mtime_unix_ms").and_then(Value::as_u64) {
                Some(mtime) if mtime >= min => {}
                other => {
                    return Err(CtlError::with_evidence(
                        ErrorCode::AssertionFailed,
                        "file mtime below minimum",
                        json!({"mtime_unix_ms": other, "mtime_min": min, "path": path}),
                    ));
                }
            }
        }
    }
    // 关系键(size_min/mtime_min)只走上面的专用断言,不参与通用比较;
    // 剔除后空对象 + subset = 纯关系断言形态。
    let generic_expected = if kind == "meta" {
        let mut filtered = expected.clone();
        if let Some(object) = filtered.as_object_mut() {
            object.remove("size_min");
            object.remove("mtime_min");
        }
        filtered
    } else {
        expected.clone()
    };
    if !matches(&actual, &generic_expected, subset) {
        return Err(CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "file content differs",
            json!({"channel":kind,"comparison":if subset {"subset"} else {"exact"},"path":path}),
        ));
    }
    Ok(json!({"verified":true,"kind":kind,"comparison":if subset {"subset"} else {"exact"}}))
}
fn inspect_office(path: &str, kind: &str) -> Result<Value, CtlError> {
    match kind {
        "process" | "process-identity" => actl_uia::evidence::process(
            path.parse()
                .map_err(|_| CtlError::protocol("invalid process ID"))?,
            kind == "process",
        ),
        _ => actl_uia::evidence::office(path, kind),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meta_kind_asserts_size_and_mtime_with_relational_keys() {
        // 元数据断言:关系键专用通道 + 通用比较剔除关系键(空对象+subset=纯关系形态)
        let dir = std::env::temp_dir().join(format!("meta-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("evidence.bin");
        std::fs::write(&file, vec![0u8; 120]).unwrap();
        let path = file.to_str().unwrap();
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        // 纯关系断言:只给 size_min
        assert!(verify_once(path, "meta", r#"{"size_min":100}"#, true).is_ok());
        assert!(verify_once(path, "meta", r#"{"size_min":121}"#, true).is_err());
        // 关系 + 精确键混合:size 走通用比较,mtime_min 走关系
        assert!(verify_once(path, "meta", r#"{"size":120,"mtime_min":1}"#, true).is_ok());
        assert!(verify_once(path, "meta", r#"{"size":119,"mtime_min":1}"#, true).is_err());
        // mtime_min 用"未来的时间"必然失败(时钟单调)
        assert!(
            verify_once(
                path,
                "meta",
                &format!(r#"{{"mtime_min":{}}}"#, before + 600_000),
                true
            )
            .is_err()
        );
        // 不存在:ASSERTION_FAILED(settle 可重试语义),与 text/json 一致
        let missing = dir.join("missing.bin");
        let err =
            verify_once(missing.to_str().unwrap(), "meta", r#"{"size_min":1}"#, true).unwrap_err();
        assert_eq!(err.code, ErrorCode::AssertionFailed);
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn io_failure_classifies_sharing_violation_permission_and_other() {
        // 共享冲突(os 32,Excel/Word 持句柄)、权限拒绝、其他——分类入 evidence
        let shared = std::io::Error::from_raw_os_error(32);
        let e = io_failure(&shared);
        assert_eq!(e.code, ErrorCode::NotActionable);
        assert_eq!(e.evidence.clone().unwrap()["io_kind"], "sharing_violation");

        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let e = io_failure(&denied);
        assert_eq!(e.evidence.clone().unwrap()["io_kind"], "permission_denied");

        let broken = std::io::Error::from_raw_os_error(1234);
        let e = io_failure(&broken);
        let ev = e.evidence.clone().unwrap();
        assert_eq!(ev["io_kind"], "other");
        assert_eq!(ev["os_code"], 1234);
    }
    #[test]
    fn discover_mode_verifies_newest_and_carries_discovered_in_envelope() {
        // 发现形态端到端:目录内取最新匹配者断言内容,信封带 discovered 实名
        let dir = std::env::temp_dir().join(format!("discover-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.csl.json.bib"), "payload").unwrap();
        let value = verify(
            dir.to_str().unwrap(),
            "text",
            "\"payload\"",
            false,
            Some("*.csl.json*"),
        )
        .unwrap();
        assert_eq!(value["verified"], true);
        assert_eq!(value["kind"], "text");
        assert!(
            value["discovered"]
                .as_str()
                .unwrap()
                .ends_with("x.csl.json.bib")
        );
        // 内容不符照常 ASSERTION_FAILED(发现的文件也要过内容断言)
        assert!(
            verify(
                dir.to_str().unwrap(),
                "text",
                "\"other\"",
                false,
                Some("*.csl.json*"),
            )
            .is_err()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn lineage_match_requires_both_program_and_token() {
        // 进程树归属纯函数:program 与令牌须同时满足(路径分隔与大小写归一)
        let data = json!({
            "program": "C:/App/bin/main.EXE",
            "command_line": "\"C:\\App\\bin\\main.EXE\" --task C:\\work\\token-file.txt"
        });
        assert!(lineage_match(
            &data,
            "c:\\app\\bin\\main.exe",
            "c:/work/token-file.txt"
        ));
        assert!(!lineage_match(
            &data,
            "c:\\app\\bin\\other.exe",
            "c:/work/token-file.txt"
        ));
        assert!(!lineage_match(
            &data,
            "c:\\app\\bin\\main.exe",
            "c:/work/absent.txt"
        ));
        assert!(!lineage_match(&json!({"program": "x"}), "x", "y"));
    }
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
