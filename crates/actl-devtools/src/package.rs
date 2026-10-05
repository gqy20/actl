use crate::{Args, Result, io, require};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};
fn collect(base: &Path, path: &Path, items: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(io)?;
    require(!meta.file_type().is_symlink(), "package symlink rejected")?;
    if meta.is_dir() {
        for item in fs::read_dir(path).map_err(io)? {
            collect(base, &item.map_err(io)?.path(), items)?;
        }
    } else {
        require(
            meta.len() <= 64 * 1024 * 1024,
            "package member exceeds 64 MiB",
        )?;
        let name = path
            .strip_prefix(base)
            .map_err(io)?
            .to_string_lossy()
            .replace('\\', "/");
        let data = fs::read(path).map_err(io)?;
        if name.starts_with("skills/")
            && ["md", "yml"].contains(&path.extension().and_then(|v| v.to_str()).unwrap_or(""))
        {
            let text = std::str::from_utf8(&data).map_err(io)?;
            require(
                !text.contains("mailto:") && !text.contains("www."),
                "skill contains external URL",
            )?;
            require(!external_url(text), "skill contains external URL")?;
        }
        items.insert(name, data);
    }
    Ok(())
}

/// skills 不携带外部网络指针的纪律(2026-10-04 前打包事故的 lint)。窄豁免
/// (mcp-remote.md 增补):① `actl://` 是自有 URN(资源模板寻址,非网络);
/// ② http/https 仅放行本机与尖括号占位符(MCP 接入配置的格式示例)——
/// 真实外域仍然全拒,纪律意图不变。
fn external_url(text: &str) -> bool {
    text.match_indices("://").any(|(at, _)| {
        let scheme: String = text[..at]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '-')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if scheme.eq_ignore_ascii_case("actl") {
            return false;
        }
        let host = text[at + 3..]
            .split(['/', ':', '>', '"', '\n', ' '])
            .next()
            .unwrap_or("");
        !(host.is_empty() || host == "localhost" || host == "127.0.0.1" || host.starts_with('<'))
    })
}
pub fn build(root: &Path, binaries: &Path, out: &Path, version: &str) -> Result<Value> {
    require(
        !version.is_empty()
            && version.as_bytes()[0].is_ascii_alphanumeric()
            && version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "invalid release version",
    )?;
    let mut entries = BTreeMap::new();
    for name in ["README.md", "LICENSE", "skills"] {
        collect(root, &root.join(name), &mut entries)?;
    }
    for name in ["actl.exe", "actl-signal.exe", "actl-mcp.exe"] {
        entries.insert(name.into(), fs::read(binaries.join(name)).map_err(io)?);
    }
    for name in [
        "skills/actl/SKILL.md",
        "skills/actl/PACKS.md",
        "skills/actl/references/error-codes.md",
        "skills/actl/workflows/README.md",
        "skills/actl/references/flows.md",
        "skills/actl/references/mcp-remote.md",
        "skills/actl/references/task-records.md",
        "skills/actl/workflows/assets/seed.docx",
        "skills/actl/workflows/assets/seed.xlsx",
    ] {
        require(
            entries.contains_key(name),
            &format!("package missing {name}"),
        )?;
    }
    // Verify relative Markdown links against the actual package inventory.
    for (name, bytes) in &entries {
        if name.starts_with("skills/") && name.ends_with(".md") {
            let text = std::str::from_utf8(bytes).map_err(io)?;
            for tail in text.split("](").skip(1) {
                let target = tail
                    .split(')')
                    .next()
                    .unwrap_or("")
                    .split('#')
                    .next()
                    .unwrap_or("");
                if target.is_empty() {
                    continue;
                }
                let mut parts: Vec<_> = name.split('/').collect();
                parts.pop();
                for part in target.split('/') {
                    match part {
                        "." | "" => {}
                        ".." => {
                            parts.pop();
                        }
                        _ => parts.push(part),
                    }
                }
                let resolved = parts.join("/");
                require(
                    resolved.starts_with("skills/actl/") && entries.contains_key(&resolved),
                    &format!("broken packaged link: {name} -> {target}"),
                )?;
            }
        }
    }
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in &entries {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(io)?;
        writer.write_all(bytes).map_err(io)?;
    }
    let archive = writer.finish().map_err(io)?.into_inner();
    let mut check = zip::ZipArchive::new(Cursor::new(&archive)).map_err(io)?;
    require(check.len() == entries.len(), "archive inventory mismatch")?;
    for (name, expected) in &entries {
        let mut bytes = vec![];
        check
            .by_name(name)
            .map_err(io)?
            .read_to_end(&mut bytes)
            .map_err(io)?;
        require(&bytes == expected, "archive content mismatch")?;
    }
    fs::create_dir_all(out).map_err(io)?;
    let base = format!("actl_{version}_windows_amd64");
    let zipname = format!("{base}.zip");
    let exename = format!("{base}.exe");
    for name in [&zipname, &exename, "checksums.txt"] {
        require(
            !out.join(name).exists(),
            "release output exists; use a new output directory",
        )?;
    }
    let exe = entries
        .get("actl.exe")
        .ok_or_else(|| io("binary missing"))?;
    let hashes = format!(
        "{}  {}\n{}  {}\n",
        actl_uia::evidence::sha256(exe)?,
        exename,
        actl_uia::evidence::sha256(&archive)?,
        zipname
    );
    for (name, bytes) in [
        (&zipname, archive.as_slice()),
        (&exename, exe.as_slice()),
        (&"checksums.txt".to_owned(), hashes.as_bytes()),
    ] {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(out.join(name))
            .map_err(io)?;
        file.write_all(bytes).map_err(io)?;
        file.sync_all().map_err(io)?;
    }
    Ok(
        json!({"archive":out.join(zipname),"binary":out.join(exename),"checksums":out.join("checksums.txt"),"entries":entries.len(),"verified":true}),
    )
}
pub fn run(args: &Args) -> Result<Value> {
    args.check(&["version", "binary-dir", "output"], &[])?;
    build(
        &crate::root(),
        &PathBuf::from(args.get("binary-dir")?),
        &PathBuf::from(args.get("output")?),
        args.get("version")?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_roundtrip_assets_and_refuse_overwrite() {
        let root = std::env::temp_dir().join(crate::id("package-native"));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("actl.exe"), "CLI").unwrap();
        fs::write(root.join("actl-signal.exe"), "display").unwrap();
        fs::write(root.join("actl-mcp.exe"), "mcp").unwrap();
        let out = root.join("dist");
        let r = build(&crate::root(), &root, &out, "test").unwrap();
        assert_eq!(r["verified"], true);
        assert_eq!(fs::read_dir(&out).unwrap().count(), 3);
        assert!(build(&crate::root(), &root, &out, "test").is_err());
        fs::remove_file(root.join("actl-signal.exe")).unwrap();
        assert!(build(&crate::root(), &root, &root.join("bad"), "test").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_url_lint_allows_own_urn_and_local_placeholders() {
        // 窄豁免面:actl:// URN、localhost/127.0.0.1/尖括号占位符;
        // 真实外域(含 https)仍然全拒——纪律意图不变。
        assert!(!external_url("读取资源模板 actl://help/{command} 按需"));
        assert!(!external_url("\"url\":\"http://<device>:7600/mcp\""));
        assert!(!external_url("见 http://localhost:3000 本机"));
        assert!(!external_url("见 https://127.0.0.1:8443 本机"));
        assert!(external_url("见 https://github.com/foo/bar"));
        assert!(external_url("文档在 http://example.com/docs"));
        assert!(external_url("连接 ws://remote.host:1"));
    }
}
