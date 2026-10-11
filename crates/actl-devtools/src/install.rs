//! `cargo xtask install`——把发布布局安装到用户目录并建立链接(免管理员):
//! 版本目录 + current junction + bin 硬链接×3 + skills junction×2。
//!
//! 布局(2026-10-11 用户授权安装链接):
//! ```text
//! ~/.local/share/actl/<label>/   发布布局本体(exe×3 + README/LICENSE + skills/)
//! ~/.local/share/actl/current    junction → <label>(升级只重指这一个)
//! ~/.local/bin/actl{,-signal,-mcp}.exe   硬链接 → current/(升级重建)
//! ~/.local/bin/skills            junction → current/skills(pack 注入的 exe 旁回退)
//! ~/.agents/skills/actl          junction → current/skills/actl(agent 知识面)
//! ```
//! 链接手段按免管理员约束选择:目录 junction(mklink /J)与同卷硬链接
//! (mklink /H)均不需要特权;文件符号链接需要管理员,不采用。安全约束:
//! 只替换经 reparse 校验的 junction 与已存在的硬链接文件,**拒绝删除任何
//! 真实目录**;目标已是真实目录(如用户自建的 ~/.agents/skills/actl)时
//! 明确报错让人工处理,不静默覆盖。
use actl_core::CtlError;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

use crate::{Args, Result};

const EXES: [&str; 3] = ["actl.exe", "actl-signal.exe", "actl-mcp.exe"];

pub fn run(args: &Args) -> Result<Value> {
    let label = args
        .values
        .get("label")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| CtlError::protocol("install requires --label <version-id>"))?;
    if !label
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(CtlError::protocol("label must be [A-Za-z0-9._-]"));
    }
    let home = PathBuf::from(
        args.values
            .get("home")
            .cloned()
            .or_else(|| std::env::var("USERPROFILE").ok())
            .ok_or_else(|| CtlError::protocol("install requires --home or USERPROFILE"))?,
    );
    // 源:--zip <发布 zip>(release 工件)或 --release(仓库 target/release +
    // 仓库 skills/README/LICENSE 本地组装)。
    let staged = match (
        args.values.get("zip"),
        args.flags.iter().any(|f| f == "release"),
    ) {
        (Some(zip), false) => stage_from_zip(Path::new(zip))?,
        (None, true) => stage_from_repo()?,
        _ => {
            return Err(CtlError::protocol(
                "install requires exactly one source: --zip <path> or --release",
            ));
        }
    };
    install_from(&staged, label, &home)
}

/// 校验并落位:staged 目录须为完整发布布局(exe×3 + skills/actl/apps)。
pub(crate) fn install_from(staged: &Path, label: &str, home: &Path) -> Result<Value> {
    verify_layout(staged)?;
    let base = home.join(".local").join("share").join("actl");
    let bin = home.join(".local").join("bin");
    let agents = home.join(".agents").join("skills");
    let version_dir = base.join(label);
    if version_dir.exists() {
        return Err(CtlError::protocol(format!(
            "{} already exists; remove it (or use a new --label) first",
            version_dir.display()
        )));
    }
    fs_create_all(&base)?;
    fs_create_all(&bin)?;
    fs_create_all(&agents)?;
    // 先就位版本目录,链接失败也不留半链接状态(版本目录可独立留存)。
    fs_rename(staged, &version_dir)?;

    let mut linked = Vec::new();
    // current junction:只替换 reparse 链接,拒绝动真实目录。
    replace_junction(&base.join("current"), &version_dir)?;
    // bin 硬链接×3 + bin/skills junction(exe 旁 skills 回退的落点)。
    for exe in EXES {
        let link = bin.join(exe);
        if link.symlink_metadata().is_ok() && !link.is_dir() {
            std::fs::remove_file(&link).map_err(io)?;
        }
        mklink(&[
            "/H",
            &link.to_string_lossy(),
            &version_dir.join(exe).to_string_lossy(),
        ])?;
        linked.push(link.to_string_lossy().into_owned());
    }
    replace_junction(&bin.join("skills"), &version_dir.join("skills"))?;
    // agent 知识面:已是真实目录则拒绝(不静默合并/覆盖),链接则重指。
    match junction_state(&agents.join("actl")) {
        JunctionState::RealDir => {
            return Err(CtlError::protocol(format!(
                "{} is a real directory; move it away first (install refuses to touch real dirs)",
                agents.join("actl").display()
            )));
        }
        _ => replace_junction(
            &agents.join("actl"),
            &version_dir.join("skills").join("actl"),
        )?,
    }
    Ok(json!({
        "installed": true, "label": label,
        "version_dir": version_dir, "current": base.join("current"),
        "bin": linked, "bin_skills": bin.join("skills"),
        "agents_actl": agents.join("actl"),
        "note": "~/.local/bin must be on PATH; upgrade = rerun with a new --label (current junction and links are re-pointed)",
    }))
}

enum JunctionState {
    Missing,
    Link,
    RealDir,
}
fn junction_state(path: &Path) -> JunctionState {
    match std::fs::symlink_metadata(path) {
        Err(_) => JunctionState::Missing,
        Ok(meta) if meta.file_type().is_symlink() => JunctionState::Link,
        Ok(_) => JunctionState::RealDir,
    }
}

/// 重指 junction:现有链接先摘除(reparse 校验);真实目录拒绝。
fn replace_junction(link: &Path, target: &Path) -> Result<()> {
    match junction_state(link) {
        JunctionState::RealDir => Err(CtlError::protocol(format!(
            "{} is a real directory; install refuses to remove real dirs",
            link.display()
        ))),
        JunctionState::Link => {
            std::fs::remove_dir(link).map_err(io)?;
            Ok(())
        }
        JunctionState::Missing => Ok(()),
    }?;
    mklink(&["/J", &link.to_string_lossy(), &target.to_string_lossy()])
}

/// mklink 转发(cmd 内建;路径含空格由 std 参数转发自动加引)。
fn mklink(args: &[&str]) -> Result<()> {
    let mut full = vec!["/c", "mklink"];
    full.extend_from_slice(args);
    let output = std::process::Command::new("cmd")
        .args(&full)
        .output()
        .map_err(io)?;
    if output.status.success() {
        return Ok(());
    }
    Err(CtlError::internal(format!(
        "mklink {} failed: {}",
        args.first().unwrap_or(&"?"),
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

fn verify_layout(dir: &Path) -> Result<()> {
    for exe in EXES {
        if !dir.join(exe).is_file() {
            return Err(CtlError::protocol(format!(
                "layout missing {exe} (expected release layout: exes + skills/)"
            )));
        }
    }
    if !dir.join("skills").join("actl").join("apps").is_dir() {
        return Err(CtlError::protocol(
            "layout missing skills/actl/apps (pack injection source)",
        ));
    }
    Ok(())
}

/// --release 源:仓库 target/release 三个 exe + README/LICENSE/skills 组装到
/// target/install-stage/<label>(不直接进家目录,保持落位原子性)。
fn stage_from_repo() -> Result<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binaries = root.join("target").join("release");
    let staged = root
        .join("target")
        .join("install-stage")
        .join("release-layout");
    if staged.exists() {
        std::fs::remove_dir_all(&staged).map_err(io)?;
    }
    fs_create_all(&staged)?;
    for exe in EXES {
        std::fs::copy(binaries.join(exe), staged.join(exe)).map_err(io)?;
    }
    for name in ["README.md", "LICENSE", "skills"] {
        copy_recursive(&root.join(name), &staged.join(name))?;
    }
    Ok(staged)
}

/// --zip 源:解包到 target/install-stage/<zip 文件名>。
fn stage_from_zip(zip: &Path) -> Result<PathBuf> {
    let file = std::fs::File::open(zip).map_err(io)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| CtlError::internal(format!("install zip open: {e}")))?;
    let staged = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/install-stage")
        .join(
            zip.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("zip-layout"),
        );
    if staged.exists() {
        std::fs::remove_dir_all(&staged).map_err(io)?;
    }
    fs_create_all(&staged)?;
    archive
        .extract(&staged)
        .map_err(|e| CtlError::internal(format!("install zip extract: {e}")))?;
    Ok(staged)
}

fn copy_recursive(src: &Path, dst: &Path) -> Result<()> {
    if src.is_dir() {
        fs_create_all(dst)?;
        for entry in std::fs::read_dir(src).map_err(io)?.flatten() {
            copy_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            fs_create_all(parent)?;
        }
        std::fs::copy(src, dst).map_err(io)?;
    }
    Ok(())
}

fn fs_create_all(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(io)
}
fn fs_rename(from: &Path, to: &Path) -> Result<()> {
    if to.parent().is_some_and(|p| fs_create_all(p).is_err()) {
        return Err(CtlError::internal("cannot create install parent"));
    }
    // 跨卷(仓库在 E:、家在 C:)rename 报 os error 17——回退复制+删源。
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_recursive(from, to)?;
    std::fs::remove_dir_all(from)
        .map_err(|e| CtlError::internal(format!("install stage cleanup: {e}")))
}
fn io(error: std::io::Error) -> CtlError {
    CtlError::internal(format!("install io: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fabricate_layout(dir: &Path) {
        for exe in EXES {
            std::fs::write(dir.join(exe), b"MZ-fake").unwrap();
        }
        let apps = dir.join("skills").join("actl").join("apps");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(apps.join("word.yml"), b"popup_ignores: []\n").unwrap();
    }

    #[test]
    fn install_roundtrip_links_and_switches_versions() {
        let tmp = std::env::temp_dir().join(format!("actl-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let stage1 = tmp.join("stage1");
        let stage2 = tmp.join("stage2");
        std::fs::create_dir_all(&stage1).unwrap();
        std::fs::create_dir_all(&stage2).unwrap();
        fabricate_layout(&stage1);
        fabricate_layout(&stage2);

        let first = install_from(&stage1, "v9.9.9-a", &home).unwrap();
        assert_eq!(first["installed"], true);
        // 三条硬链接可读且内容正确;junction 解析穿透。
        assert_eq!(
            std::fs::read(home.join(".local/bin/actl.exe")).unwrap(),
            b"MZ-fake"
        );
        assert!(home.join(".local/bin/skills/actl/apps").is_dir());
        assert!(home.join(".agents/skills/actl/apps").is_dir());
        assert!(home.join(".local/share/actl/current").is_dir());

        // 版本切换:重指 current 与全部链接,旧版本目录留存。
        let second = install_from(&stage2, "v9.9.9-b", &home).unwrap();
        assert_eq!(second["installed"], true);
        let current_link = std::fs::read_link(home.join(".local/share/actl/current")).ok();
        assert!(
            current_link.is_some_and(|p| p.ends_with("v9.9.9-b")),
            "current should point at the new label"
        );
        assert!(home.join(".local/share/actl/v9.9.9-a").is_dir());

        // 真实目录保护:预置真实 ~/.agents/skills/actl 时拒绝安装。
        std::fs::remove_dir(home.join(".agents/skills/actl")).unwrap();
        std::fs::create_dir_all(home.join(".agents/skills/actl")).unwrap();
        let stage3 = tmp.join("stage3");
        std::fs::create_dir_all(&stage3).unwrap();
        fabricate_layout(&stage3);
        let refused = install_from(&stage3, "v9.9.9-c", &home);
        assert!(refused.is_err(), "real agents dir must not be touched");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn layout_verification_rejects_incomplete_sources() {
        let tmp = std::env::temp_dir().join(format!("actl-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("actl.exe"), b"x").unwrap();
        assert!(verify_layout(&tmp).is_err());
        fabricate_layout(&tmp);
        assert!(verify_layout(&tmp).is_ok());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
