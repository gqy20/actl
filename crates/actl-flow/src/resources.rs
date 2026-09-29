//! Bounded, declarative resources prepared once before a new run. No shell evaluation.
use actl_core::CtlError;
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Resources {
    #[serde(default)]
    directories: Vec<String>,
    #[serde(default)]
    files: Vec<File>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    path: String,
    text: Option<String>,
    copy: Option<String>,
}
pub struct Prepared {
    pub text: String,
    pub root: PathBuf,
    enabled: bool,
    directories: Vec<PathBuf>,
    files: Vec<(PathBuf, Vec<u8>)>,
}

fn error(message: impl Into<String>) -> CtlError {
    CtlError::protocol(message)
}
fn io(e: impl std::fmt::Display) -> CtlError {
    CtlError::internal(format!("workflow resources: {e}"))
}

fn relative(value: &str) -> Result<PathBuf, CtlError> {
    if value.is_empty() || value.len() > 240 || value.contains('\\') {
        return Err(error(
            "resource path must be a nonempty relative slash-separated path",
        ));
    }
    for part in value.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err(error("resource path has an unsafe or reserved component"));
        }
    }
    Ok(PathBuf::from(value))
}

fn expand(value: &str, id: &str, root: &Path, environment: bool) -> Result<String, CtlError> {
    let mut rest = value;
    let mut result = String::new();
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let end = rest
            .find('}')
            .ok_or_else(|| error("unterminated resource token"))?;
        let key = &rest[..end];
        match key {
            "run.id" => result.push_str(id),
            "run.dir" if environment => result.push_str(&root.to_string_lossy().replace('\\', "/")),
            // Firefox 系应用(如 Zotero)的 -datadir 校验拒绝正斜杠绝对路径,
            // 需要保留原生分隔符的变体;其余路径继续统一正斜杠以便跨平台记录。
            "run.dir_native" if environment => result.push_str(&root.to_string_lossy()),
            _ if environment && key.starts_with("env:") => {
                let name = &key[4..];
                if ![
                    "LOCALAPPDATA",
                    "APPDATA",
                    "ProgramFiles",
                    "ProgramFiles(x86)",
                    "USERPROFILE",
                ]
                .contains(&name)
                {
                    return Err(error("unsupported resource environment token"));
                }
                result.push_str(
                    &std::env::var(name)
                        .map_err(|_| error(format!("environment variable unavailable: {name}")))?
                        .replace('\\', "/"),
                );
            }
            _ => return Err(error(format!("unsupported resource token: {key}"))),
        }
        rest = &rest[end + 1..];
    }
    result.push_str(rest);
    if result.len() > 262_144 || result.contains('\0') {
        return Err(error("resource value exceeds bounds"));
    }
    Ok(result)
}

/// Read/validate assets and expand only input defaults and resource paths/text.
/// Does not create directories, files, or a workflow record.
pub fn prepare(text: &str, template: &Path, root: &Path, id: &str) -> Result<Prepared, CtlError> {
    if !actl_core::history::valid_id(id) || !root.is_absolute() {
        return Err(error("invalid resource run context"));
    }
    let mut raw = super::decode_yaml(text)?;
    let resource = raw.as_object_mut().and_then(|v| v.remove("resources"));
    let enabled = resource.is_some();
    let resources: Resources = resource
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| error(format!("invalid resources: {e}")))?
        .unwrap_or_default();
    if resources.files.len() + resources.directories.len() > 128 {
        return Err(error("too many workflow resources"));
    }
    if enabled && let Some(inputs) = raw.get_mut("inputs").and_then(|v| v.as_object_mut()) {
        for input in inputs.values_mut() {
            if let Some(value) = input.get("default").and_then(|v| v.as_str()) {
                input["default"] = expand(value, id, root, true)?.into();
            }
        }
    }
    let mut prepared = Prepared {
        text: serde_json::to_string(&raw).map_err(io)?,
        root: root.into(),
        enabled,
        directories: vec![],
        files: vec![],
    };
    let mut names = BTreeSet::new();
    for dir in resources.directories {
        let path = relative(&expand(&dir, id, root, false)?)?;
        if !names.insert(path.to_string_lossy().to_lowercase()) {
            return Err(error("duplicate resource path"));
        }
        prepared.directories.push(path);
    }
    let mut total = 0;
    for file in resources.files {
        let path = relative(&expand(&file.path, id, root, false)?)?;
        if !names.insert(path.to_string_lossy().to_lowercase()) {
            return Err(error("duplicate resource path"));
        }
        let bytes = match (file.text, file.copy) {
            (Some(text), None) => expand(&text, id, root, false)?.into_bytes(),
            (None, Some(copy)) => {
                let base = template
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .canonicalize()
                    .map_err(io)?;
                let source = base.join(relative(&copy)?).canonicalize().map_err(io)?;
                if !source.starts_with(&base) || !source.is_file() {
                    return Err(error("resource copy must remain inside template directory"));
                }
                if fs::metadata(&source).map_err(io)?.len() > 1_048_576 {
                    return Err(error("resource file exceeds 1 MiB"));
                }
                let mut bytes = Vec::new();
                fs::File::open(source)
                    .map_err(io)?
                    .take(1_048_577)
                    .read_to_end(&mut bytes)
                    .map_err(io)?;
                bytes
            }
            _ => return Err(error("resource file requires exactly one of text or copy")),
        };
        total += bytes.len();
        if bytes.len() > 1_048_576 || total > 4_194_304 {
            return Err(error("resource data exceeds size limit"));
        }
        prepared.files.push((path, bytes));
    }
    // A file cannot be an ancestor of another file or declared directory.
    for (file, _) in &prepared.files {
        let prefix = format!(
            "{}/",
            file.to_string_lossy().replace('\\', "/").to_lowercase()
        );
        if names
            .iter()
            .any(|name| name.replace('\\', "/").starts_with(&prefix))
        {
            return Err(error("resource file conflicts with child path"));
        }
    }
    Ok(prepared)
}

impl Prepared {
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({"root":self.root,"directories":self.directories,"files":self.files.iter().map(|(p,b)| serde_json::json!({"path":p,"bytes":b.len()})).collect::<Vec<_>>()})
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    /// Called only after full command preflight, under the global runner lock.
    /// A failed/partial initialization is retained; never overwrite or auto-retry it.
    pub fn create(&self) -> Result<(), CtlError> {
        if !self.enabled {
            return Ok(());
        }
        let parent = self
            .root
            .parent()
            .ok_or_else(|| error("resource parent missing"))?;
        fs::create_dir_all(parent).map_err(io)?;
        fs::create_dir(&self.root).map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                error("resource run directory already exists; inspect it and use a new run ID")
            } else {
                io(e)
            }
        })?;
        for dir in &self.directories {
            fs::create_dir_all(self.root.join(dir)).map_err(io)?;
        }
        for (path, bytes) in &self.files {
            let target = self.root.join(path);
            fs::create_dir_all(
                target
                    .parent()
                    .ok_or_else(|| error("resource file parent missing"))?,
            )
            .map_err(io)?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)
                .map_err(io)?;
            output.write_all(bytes).map_err(io)?;
            output.sync_all().map_err(io)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_dir_native_preserves_windows_separators() {
        let root = Path::new(r"C:\task\dir");
        assert_eq!(
            expand("${run.dir}/data", "t", root, true).unwrap(),
            "C:/task/dir/data"
        );
        // 占位符外的后缀是字面量:需要原生分隔符时后缀也写反斜杠。
        assert_eq!(
            expand("${run.dir_native}\\data", "t", root, true).unwrap(),
            "C:\\task\\dir\\data",
        );
        // 资源路径与文本不支持环境占位符,保持 fail-closed。
        assert!(expand("${run.dir_native}/data", "t", root, false).is_err());
    }
}
