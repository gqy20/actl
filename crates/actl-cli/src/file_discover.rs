//! verify-file 目录发现(2026-10-12 Zotero 导出轮):保存对话框按类型过滤器
//! 补扩展名(输入 .csl.json 落盘 .csl.json.bib),按预设精确名断言会静默落空
//! ——目录内按锚定通配取 mtime 最新者交 verify 断言。独立模块(file-lines)。
use actl_core::{CtlError, ErrorCode};
use serde_json::json;

/// 目录内按锚定通配(仅 `*`)取 mtime 最新的文件;无匹配报 ASSERTION_FAILED
/// (参与 settle 窗重试——文件可能稍后出现),目录不存在同分类。
pub(crate) fn newest_match(dir: &str, pattern: &str) -> Result<String, CtlError> {
    let entries = std::fs::read_dir(dir).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CtlError::with_evidence(
                ErrorCode::AssertionFailed,
                "cannot read evidence directory",
                json!({"io_kind": "not_found", "dir": dir, "pattern": pattern}),
            )
        } else {
            super::file_check::io_failure(&error)
        }
    })?;
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !wildcard_match(pattern, &name) || !entry.path().is_file() {
            continue;
        }
        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        let newer = match (&best, mtime) {
            (None, _) => true,
            (Some((current, _)), Some(m)) => m > *current,
            (Some(_), None) => false,
        };
        if newer {
            best = Some((
                mtime.unwrap_or(std::time::UNIX_EPOCH),
                entry.path().to_string_lossy().into_owned(),
            ));
        }
    }
    best.map(|(_, path)| path).ok_or_else(|| {
        CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            "no file matches discovery pattern",
            json!({"io_kind": "no_match", "dir": dir, "pattern": pattern}),
        )
    })
}

/// 锚定通配(仅 `*`):`*` 匹配任意序列(含点号),其余字符字面比较,
/// Windows 文件系统大小写不敏感。经典贪心双指针,无回溯爆炸。
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut mark = 0usize;
    while ni < name.len() {
        if pi < pattern.len() && (pattern[pi] == '*' || pattern[pi] == name[ni]) {
            if pattern[pi] == '*' {
                star = Some(pi);
                mark = ni;
                pi += 1;
            } else {
                pi += 1;
                ni += 1;
            }
        } else if let Some(star_at) = star {
            pi = star_at + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_match_is_anchored_case_insensitive_and_star_crosses_dots() {
        // 锚定通配:`*` 跨点号(过滤器补扩展名的关键形态 .csl.json → .csl.json.bib)、
        // 大小写不敏感(Windows FS)、必须全长匹配
        assert!(wildcard_match("*.csl.json*", "zt-export.csl.json.bib"));
        assert!(wildcard_match("zt-export*", "ZT-Export.CSL.JSON.BIB"));
        assert!(wildcard_match("*.json", "a.b.json"));
        assert!(wildcard_match("*", "anything.txt"));
        assert!(wildcard_match("exact.txt", "exact.txt"));
        assert!(!wildcard_match("*.json", "a.json.bak"));
        assert!(!wildcard_match("json*", "a.json"));
        assert!(!wildcard_match("*.json", "a.txt"));
    }

    #[test]
    fn newest_match_picks_latest_mtime_and_reports_no_match() {
        let dir = std::env::temp_dir().join(format!("discover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("a.out.json");
        let new = dir.join("a.out.json.bib");
        std::fs::write(&old, b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(60));
        std::fs::write(&new, b"new").unwrap();
        let hit = newest_match(dir.to_str().unwrap(), "*.out.json*").unwrap();
        assert!(hit.ends_with("a.out.json.bib"), "{hit}");
        // 非匹配文件不入选;无匹配 → ASSERTION_FAILED + io_kind=no_match(settle 可重试)
        let err = newest_match(dir.to_str().unwrap(), "*.zzz").unwrap_err();
        assert_eq!(err.code, ErrorCode::AssertionFailed);
        assert_eq!(err.evidence.unwrap()["io_kind"], "no_match");
        // 目录不存在 → ASSERTION_FAILED(not_found),同"文件可能稍后出现"语义
        let err = newest_match(dir.join("missing-dir").to_str().unwrap(), "*.json").unwrap_err();
        assert_eq!(err.code, ErrorCode::AssertionFailed);
        assert_eq!(err.evidence.unwrap()["io_kind"], "not_found");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
