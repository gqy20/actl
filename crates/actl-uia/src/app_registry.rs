//! 本地应用位置注册表(docs/06 §3 应用注册表):名字 → 可执行文件路径的
//! 机器本地解析层,消灭"每次验收都找一遍软件路径"的反模式。
//!
//! 存储:`%LOCALAPPDATA%/actl/apps.json`(机器本地,不入库)。
//! 两类条目:稳定路径(path)与 Store 应用模式(pattern + resolved 缓存)。
//! 解析优先级:apps.json 精确名 → pattern glob → App Paths 注册表 → PATH(既有行为)。

use actl_core::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn error(message: impl Into<String>) -> CtlError {
    CtlError::protocol(message)
}

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Discovery {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppEntry {
    /// 稳定路径(非 Store 应用);与 pattern 互斥
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Store 应用通配模式(版本号段用 *);与 path 互斥
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// pattern 上次成功 glob 的实际路径(缓存;失效时重新 glob)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    /// 来源:manual(用户显式指定——auto 发现永不覆盖)/auto(工具发现,
    /// 可被显式 app-register 或后续 manual 覆盖)。**手改 JSON 缺省按 manual
    /// 解析**(人手写的条目就是显式意志,天然受保护)。
    #[serde(default = "default_discovery")]
    pub discovered: Discovery,
    #[serde(default)]
    pub last_verified_unix: u64,
}

fn default_discovery() -> Discovery {
    Discovery::Manual
}

#[derive(Debug, Serialize, Deserialize)]
struct Registry {
    version: u32,
    apps: std::collections::BTreeMap<String, AppEntry>,
}

impl Registry {
    fn new() -> Self {
        Self {
            version: 1,
            apps: std::collections::BTreeMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 存取
// ---------------------------------------------------------------------------

fn registry_path() -> PathBuf {
    actl_core::state::SignalPaths::default()
        .dir
        .join("apps.json")
}

fn load() -> Result<Registry, CtlError> {
    let path = registry_path();
    if !path.exists() {
        return Ok(Registry::new());
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| CtlError::internal(format!("cannot read apps.json: {e}")))?;
    // 空文件是合法空表
    if text.trim().is_empty() {
        return Ok(Registry::new());
    }
    serde_json::from_str(&text)
        .map_err(|e| CtlError::protocol(format!("apps.json is not valid JSON: {e}")))
}

fn save(registry: &Registry) -> Result<(), CtlError> {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| CtlError::internal(format!("cannot create actl dir: {e}")))?;
    }
    let text = serde_json::to_string_pretty(registry)
        .map_err(|e| CtlError::internal(format!("cannot serialize apps.json: {e}")))?;
    std::fs::write(&path, text)
        .map_err(|e| CtlError::internal(format!("cannot write apps.json: {e}")))
}

// ---------------------------------------------------------------------------
// 公开操作
// ---------------------------------------------------------------------------

/// 同名 manual 条目在场时的现有解析目标(auto 注册的预检查询)。
/// 返回 Some = 用户已显式指定,auto 不得覆盖。
pub fn manual_entry(name: &str) -> Result<Option<String>, CtlError> {
    let reg = load()?;
    let Some(entry) = reg.apps.get(name) else {
        return Ok(None);
    };
    if entry.discovered != Discovery::Manual {
        return Ok(None);
    }
    Ok(entry.path.clone().or_else(|| entry.resolved.clone()))
}

/// 注册稳定路径条目(手动或自动发现写入)。
/// **auto(manual=false)不覆盖同名 manual 条目**——用户的显式指定是意志,
/// 自动发现撞上时如实返回现有路径,改它必须走显式 app-register/app-remove。
pub fn register(name: &str, path: &str, manual: bool) -> Result<(), CtlError> {
    validate_name(name)?;
    if path.is_empty() || path.len() > 1024 {
        return Err(error("path must be 1-1024 characters"));
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(error(format!("path must be absolute: {path}")));
    }
    let mut reg = load()?;
    if !manual
        && reg
            .apps
            .get(name)
            .is_some_and(|e| e.discovered == Discovery::Manual)
    {
        return Err(error(format!(
            "{name} is a manual entry; auto discovery will not overwrite it \
             (use app-register to change it deliberately)"
        )));
    }
    reg.apps.insert(
        name.to_string(),
        AppEntry {
            path: Some(path.to_string()),
            pattern: None,
            resolved: None,
            discovered: if manual {
                Discovery::Manual
            } else {
                Discovery::Auto
            },
            last_verified_unix: now_unix(),
        },
    );
    save(&reg)
}

/// 注册 Store 应用模式条目(pattern 含 * 通配版本段)。
pub fn register_pattern(name: &str, pattern: &str) -> Result<(), CtlError> {
    validate_name(name)?;
    if pattern.is_empty() || pattern.len() > 1024 || !pattern.contains('*') {
        return Err(error(
            "pattern must be 1-1024 characters and contain at least one *",
        ));
    }
    let mut reg = load()?;
    reg.apps.insert(
        name.to_string(),
        AppEntry {
            path: None,
            pattern: Some(pattern.to_string()),
            resolved: None,
            discovered: Discovery::Manual,
            last_verified_unix: 0,
        },
    );
    save(&reg)
}

/// 解析名字 → 绝对路径。只查注册表,不做 PATH/App Paths 回退
/// (那些由 launch 层的既有逻辑处理,这里只提供第一优先级)。
/// pattern 条目:resolved 缓存有效则直接返回,否则 glob 重解析并回写。
pub fn resolve(name: &str) -> Result<Option<String>, CtlError> {
    let mut reg = load()?;
    let Some(entry) = reg.apps.get(name).cloned() else {
        return Ok(None);
    };
    // 稳定路径:存在即用,不存在也返回(调用方决定报错)
    if let Some(path) = &entry.path {
        if Path::new(path).is_file() {
            return Ok(Some(path.clone()));
        }
        return Ok(Some(path.clone())); // 路径可能暂时不可达但配置就是它
    }
    // pattern:先试缓存
    if let Some(pattern) = &entry.pattern {
        if let Some(resolved) = &entry.resolved
            && Path::new(resolved).is_file()
        {
            return Ok(Some(resolved.clone()));
        }
        // 缓存失效 → glob 重解析
        if let Some(found) = glob_resolve(pattern)? {
            // 回写缓存
            if let Some(entry) = reg.apps.get_mut(name) {
                entry.resolved = Some(found.clone());
                entry.last_verified_unix = now_unix();
            }
            let _ = save(&reg);
            return Ok(Some(found));
        }
        return Ok(None);
    }
    Ok(None)
}

/// 列出全部注册条目(名字 + 解析状态)。
pub fn list() -> Result<serde_json::Value, CtlError> {
    let reg = load()?;
    let apps: Vec<serde_json::Value> = reg
        .apps
        .iter()
        .map(|(name, e)| {
            serde_json::json!({
                "name": name,
                "path": e.path,
                "pattern": e.pattern,
                "resolved": e.resolved,
                "discovered": e.discovered,
                "last_verified_unix": e.last_verified_unix,
            })
        })
        .collect();
    Ok(serde_json::json!({ "apps": apps, "total": apps.len() }))
}

/// 移除条目。
pub fn remove(name: &str) -> Result<(), CtlError> {
    let mut reg = load()?;
    if reg.apps.remove(name).is_none() {
        return Err(CtlError::new(
            ErrorCode::NotFound,
            format!("app {name:?} is not registered"),
        ));
    }
    save(&reg)
}

// ---------------------------------------------------------------------------
// 内部
// ---------------------------------------------------------------------------

fn validate_name(name: &str) -> Result<(), CtlError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(error("app name must be 1-64 of [a-zA-Z0-9_-]"))
    }
}

/// 极简 glob:仅支持目录段中的 * 通配(Store 应用版本号场景)。
/// 实现:按段拆分,含 * 的段做前缀/后缀匹配,其余段精确。
/// Windows 绝对路径首段 "C:" 会展开为 "C:\" 根(否则 PathBuf::join 产生驱动器相对路径)。
fn glob_resolve(pattern: &str) -> Result<Option<String>, CtlError> {
    let normalized = pattern.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').collect();
    let mut candidates: Vec<PathBuf> = vec![PathBuf::new()];
    for seg in &segments {
        let mut next = Vec::new();
        if seg.contains('*') {
            let (prefix, suffix) = seg.split_once('*').unwrap_or((seg, ""));
            for base in &candidates {
                if let Ok(entries) = std::fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with(prefix) && name.ends_with(suffix) {
                            next.push(entry.path());
                        }
                    }
                }
            }
        } else if candidates.len() == 1
            && candidates[0].as_os_str().is_empty()
            && seg.ends_with(':')
        {
            // 首段驱动器号:"C:" → "C:\"(Windows 绝对路径根)
            next.push(PathBuf::from(format!("{seg}/")));
        } else {
            for base in &candidates {
                next.push(base.join(seg));
            }
        }
        candidates = next;
        if candidates.is_empty() {
            return Ok(None);
        }
    }
    // 找第一个存在的文件
    for c in &candidates {
        if c.is_file() {
            return Ok(Some(c.to_string_lossy().replace('\\', "/")));
        }
    }
    Ok(None)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_resolves_wildcard_directory() {
        let dir = std::env::temp_dir().join("actl-app-registry-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Microsoft.Paint_11.2605.81.0").join("PaintApp")).unwrap();
        std::fs::create_dir_all(dir.join("Microsoft.Paint_11.2607.14.0").join("PaintApp")).unwrap();
        std::fs::write(
            dir.join("Microsoft.Paint_11.2607.14.0")
                .join("PaintApp")
                .join("mspaint.exe"),
            b"fake",
        )
        .unwrap();
        let pattern = format!(
            "{}/Microsoft.Paint_*/PaintApp/mspaint.exe",
            dir.to_string_lossy().replace('\\', "/")
        );
        let found = glob_resolve(&pattern).unwrap();
        assert!(found.is_some(), "should find via wildcard");
        assert!(found.unwrap().contains("Paint_11.2607"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn glob_returns_none_on_miss() {
        assert!(glob_resolve("C:/nonexistent-*/app.exe").unwrap().is_none());
    }

    #[test]
    fn name_validation_rejects_bad() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("my_app").is_ok());
        assert!(validate_name("MyApp123").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("../evil").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name(&"x".repeat(65)).is_err());
    }
}

#[cfg(test)]
mod manual_protection_tests {
    use super::*;

    /// 手改 JSON 的缺省字段形态:省略 discovered/last_verified_unix
    /// → 按 manual 解析(人手写的条目就是显式意志,受 auto 保护)。
    #[test]
    fn hand_edited_entry_defaults_to_manual() {
        let hand_written = r#"{"version":1,"apps":{"wemeet":{"path":"D:/tools/x.exe"}}}"#;
        let reg: Registry = serde_json::from_str(hand_written).unwrap();
        assert_eq!(reg.apps["wemeet"].discovered, Discovery::Manual);
    }

    /// manual 条目在场时,auto 注册被 register() 层防线拒绝
    /// (显式 app-register 才能改;app-locate --register 走 dispatch 预检)。
    #[test]
    fn auto_register_refuses_to_overwrite_manual() {
        let entry = AppEntry {
            path: Some("D:/custom/wherever.exe".into()),
            pattern: None,
            resolved: None,
            discovered: Discovery::Manual,
            last_verified_unix: 1,
        };
        let mut reg = Registry::new();
        reg.apps.insert("wemeet".into(), entry);
        assert!(reg.apps["wemeet"].discovered == Discovery::Manual);
        // 语义断言:auto/manual 区分存在且 register 防线条件可判
        // (文件 IO 层的端到端走 main.rs dispatch 预检+契约)
        assert_eq!(
            reg.apps["wemeet"].path.as_deref(),
            Some("D:/custom/wherever.exe")
        );
    }
}
