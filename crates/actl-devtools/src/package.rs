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
                !repo_or_mail_pointer(text),
                "skill contains a repository pointer or mailto (private-repo leakage guard)",
            )?;
        }
        items.insert(name, data);
    }
    Ok(())
}

/// skills 外链纪律(2026-10-04 打包事故的 lint;2026-10-13 范围收窄,用户
/// 裁定:blanket「不含 URL」禁令过宽)。浏览器自动化技能的**操作目标 URL**
/// (站点/API 端点/ws 端点,含 github.com 发布页这类操作靶标)是可操作事实,
/// 合法内容;禁令收窄为真实风险面——**仓库身份指针**:自有仓库路径
/// (gqy20/actl)与用户名在任何书写形式下都是身份泄露(未开源前发布包
/// 携带它 = 私有仓地址泄露;开源后对第三方用户也是错误锚点),裸写同拒;
/// 任意用户的 `user/repo` 形态不拒(操作 github 公共仓的合法目标)。
/// mailto: 同拒。残余风险:「给人读的」第三方外链不再被此 lint 拦——
/// 归代码评审(gate 是边界检查,语义把关在评审)。
fn repo_or_mail_pointer(text: &str) -> bool {
    ["gqy20/actl", "gqy17", "mailto:"]
        .iter()
        .any(|needle| text.contains(needle))
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
    fn url_lint_blocks_repo_identity_and_allows_operational_targets() {
        // 收窄后的纪律面(2026-10-13):仓库身份指针与 mailto 拒——裸写同拒
        // (私有仓身份泄露的灾难场景);操作目标 URL(含 github 发布页靶标、
        // 公共仓 user/repo 形态)合法。
        assert!(repo_or_mail_pointer("标题含 Releases · gqy20/actl"));
        assert!(repo_or_mail_pointer("仓库在 github.com/gqy20/actl"));
        assert!(repo_or_mail_pointer("clone gqy20/actl 后构建"));
        assert!(repo_or_mail_pointer("联系人 mailto:a@b.c"));
        assert!(repo_or_mail_pointer("路径 C:/Users/gqy17/..."));
        assert!(!repo_or_mail_pointer(
            "https://search.bilibili.com/all?keyword=music"
        ));
        assert!(!repo_or_mail_pointer(
            "api.bilibili.com/x/v2/reply/wbi/main?oid=1"
        ));
        assert!(!repo_or_mail_pointer(
            "chrome-extension://service_worker 在场"
        ));
        assert!(!repo_or_mail_pointer(
            "读取资源模板 actl://help/{command} 按需"
        ));
        assert!(!repo_or_mail_pointer("连接 ws://127.0.0.1:9222/devtools"));
        assert!(!repo_or_mail_pointer("网络可达 github.com(操作靶标)"));
        assert!(!repo_or_mail_pointer(
            "公共仓 fork 自 some-user/public-repo"
        ));
        assert!(!repo_or_mail_pointer(
            "草稿 URL 形如 https://<owner>/<repo>/releases"
        ));
    }
}
