//! app-locate:软件位置跨通道探测(docs/06 §3 应用注册表)。
//!
//! 通道序(全通道都跑、去重后按优先序返回):
//! 1. app-registry 别名(apps.json,机器本地已注册的解析结果);
//! 2. App Paths 注册表(裸名与 .exe 变体);
//! 3. 常见安装目录(C:\Program Files、C:\Program Files (x86)、
//!    %LOCALAPPDATA%\Programs 的同名子目录与顶层 exe);
//! 4. **开始菜单 .lnk**(ProgramData + AppData 两个根,递归 ≤3 层、
//!    文件名含查询词——中文应用名靠这条;IShellLink 解析 TargetPath。
//!    2026-10-12 腾讯会议教训:非标准盘符安装+App Paths 未注册时,
//!    开始菜单是唯一系统入口,曾因漏此通道误判「未安装」);
//! 5. PATH 环境变量(逐目录,name 与 name.exe)。
//!
//! 只读探测,无副作用;--register 由 CLI 层把首个存在的候选写入 apps.json。
//! 有界纪律:开始菜单 .lnk 预算 ≤800、目录扫描每根 ≤64 条目、.lnk 名 ≤1024 字符。

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn internal(message: impl Into<String>) -> CtlError {
    CtlError::internal(message)
}

/// 探测候选:通道 + 路径。路径一律核过 is_file 才入列。
struct Candidate {
    channel: &'static str,
    path: String,
}

pub fn locate(query: &str) -> Result<Value, CtlError> {
    if query.is_empty() || query.len() > 96 || query.contains('\0') {
        return Err(CtlError::protocol("locate query must be 1-96 bytes"));
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let query_lower = query.to_lowercase();
    let push =
        |channel: &'static str, path: PathBuf, seen: &mut Vec<String>, out: &mut Vec<Candidate>| {
            let path = path.canonicalize().unwrap_or(path);
            let text = path
                .to_string_lossy()
                .into_owned()
                // canonicalize 给非 UNC 路径加 \\?\ 前缀;存储面用普通形态
                // (apps.json/yml 可读,CreateProcess 两种都收)
                .trim_start_matches(r"\\?\")
                .to_string();
            if !path.is_file() || seen.contains(&text) {
                return;
            }
            seen.push(text.clone());
            out.push(Candidate {
                channel,
                path: text,
            });
        };

    // ① app-registry 别名(已注册 = 现成解析,最高优先级)
    if let Ok(Some(resolved)) = crate::app_registry::resolve(query) {
        push(
            "app-registry",
            PathBuf::from(&resolved),
            &mut seen,
            &mut candidates,
        );
    }
    // ② App Paths 注册表(裸名 + .exe 变体)
    for probe in [query.to_string(), format!("{query}.exe")] {
        if let Some(found) = crate::window::resolve_app_path(&probe) {
            push(
                "app-paths",
                PathBuf::from(found),
                &mut seen,
                &mut candidates,
            );
        }
    }
    // ③ 常见安装目录:同名子目录内的 exe + 顶层同名 exe
    let mut roots: Vec<PathBuf> = [
        r"C:\Program Files".to_string(),
        r"C:\Program Files (x86)".to_string(),
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    for root in &roots {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten().take(64) {
                let p = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_lowercase();
                if !file_name.contains(&query_lower) {
                    continue;
                }
                if p.is_dir() {
                    if let Ok(inner) = std::fs::read_dir(&p) {
                        for f in inner.flatten() {
                            let fp = f.path();
                            let fname = f.file_name().to_string_lossy().to_lowercase();
                            if fp.extension().and_then(|x| x.to_str()) == Some("exe")
                                && fname.contains(&query_lower)
                                // 卸载器同样匹配应用名——first 候选绝不能是它
                                && !fname.contains("uninstall")
                                && !fname.contains("un install")
                            {
                                push("program-dirs", fp, &mut seen, &mut candidates);
                            }
                        }
                    }
                } else if p.extension().and_then(|x| x.to_str()) == Some("exe")
                    && !file_name.contains("uninstall")
                {
                    push("program-dirs", p, &mut seen, &mut candidates);
                }
            }
        }
    }
    // ④ 开始菜单 .lnk(两个根递归 ≤3 层;文件名含查询词,中文靠这条)
    let mut menu_roots: Vec<PathBuf> = vec![PathBuf::from(
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs",
    )];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        menu_roots.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    let mut lnk_budget = 800usize;
    let mut targets: Vec<PathBuf> = Vec::new();
    for root in &menu_roots {
        collect_shortcuts(root, 0, &query_lower, &mut targets, &mut lnk_budget);
    }
    for lnk in targets {
        if let Some(target) = resolve_shortcut_target(&lnk)? {
            push(
                "start-menu",
                PathBuf::from(&target),
                &mut seen,
                &mut candidates,
            );
        }
    }
    // ⑤ PATH 环境变量
    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in [query, &format!("{query}.exe")] {
                let p = dir.join(name);
                if p.is_file() {
                    push("path", p, &mut seen, &mut candidates);
                }
            }
        }
    }

    if candidates.is_empty() {
        return Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!("no executable found for {query:?} across all channels"),
            json!({
                "query": query,
                "channels": ["app-registry", "app-paths", "program-dirs", "start-menu", "path"],
                "hint": "custom install locations surface via Start Menu .lnk; portable apps without a Start Menu entry need an explicit app-register",
            }),
        ));
    }
    let first = candidates[0].path.clone();
    let total = candidates.len();
    let list: Vec<Value> = candidates
        .iter()
        .map(|c| json!({"channel": c.channel, "path": c.path}))
        .collect();
    Ok(json!({
        "query": query,
        "candidates": list,
        "total": total,
        "first": first,
    }))
}

/// 开始菜单递归收集(≤3 层,.lnk 预算限量防深树)。
/// 卸载类快捷方式(卸载/uninstall)永不入列——它们同样匹配应用名,
/// 但 first 候选若是卸载器,--register 会把别名指向卸载程序(危险)。
fn collect_shortcuts(
    dir: &Path,
    depth: usize,
    query_lower: &str,
    out: &mut Vec<PathBuf>,
    budget: &mut usize,
) {
    if depth > 3 || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_shortcuts(&p, depth + 1, query_lower, out, budget);
        } else if p.extension().and_then(|x| x.to_str()) == Some("lnk") {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains("卸载")
                || name.contains("uninstall")
                || name.contains("un install")
                || !name.contains(query_lower)
            {
                continue;
            }
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            out.push(p);
        }
    }
}

/// IShellLink 解析 .lnk 的 TargetPath。MTA 公寓纪律同 capture.rs:线程一旦
/// 遗留 STA 公寓,同线程后续 UIAutomation::new()(仅接受 MTA)会以
/// RPC_E_CHANGED_MODE 全体失败(docs/68 根因)——必须配对 CoUninitialize。
fn resolve_shortcut_target(lnk: &Path) -> Result<Option<String>, CtlError> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{Interface, PCWSTR};

    struct Com(bool);
    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let _com = Com(hr.is_ok());
    if !hr.is_ok() && hr.0 != -2147417850i32 {
        // RPC_E_CHANGED_MODE = 已是 MTA(复用),其余初始化失败才致命
        return Err(internal(format!("COM init failed: {}", hr.0)));
    }
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| internal(format!("ShellLink create: {e}")))?;
    let persist: IPersistFile = shell_link
        .cast()
        .map_err(|e| internal(format!("ShellLink cast: {e}")))?;
    let wpath: Vec<u16> = lnk
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe { persist.Load(PCWSTR(wpath.as_ptr()), STGM_READ) }
        .map_err(|e| internal(format!("lnk load {}: {e}", lnk.display())))?;
    // SLGP_UNCPRIORITY(2):优先返回可直接 CreateProcess 的完整路径
    let mut buf = [0u16; 1024];
    let ok = unsafe { shell_link.GetPath(&mut buf, std::ptr::null_mut(), 2) };
    if ok.is_err() {
        // 链接可以指向 UWP/URI(无文件目标)——按无目标如实处理
        return Ok(None);
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    if len == 0 {
        return Ok(None);
    }
    Ok(Some(String::from_utf16_lossy(&buf[..len])))
}
