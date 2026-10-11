//! pack 级 popup_ignores 注入(降噪第二轮 pack 来源):apps/*.yml 顶层声明
//! 并入 plan 与模板级声明并集去重。从 flow.rs 拆出(file-lines 门禁:
//! 注入/extraction/候选解析与流程加载解耦,职责自洽)。

use actl_core::CtlError;
use actl_flow::Plan;

type Result<T> = std::result::Result<T, CtlError>;

/// pack 目录解析(CWD 优先,exe 旁回退):仓库内开发态 skills 在 CWD;安装态
/// (hardlink/junction 链接进 ~/.local/bin 等)CWD 任意——回退到 exe 同目录的
/// skills/actl/apps(发布 zip 的 exe 旁打包布局,docs/06 运行时治理④)。
/// 首个存在的候选生效,避免双目录重复注入;都不存在=跳过(嵌入式/测试环境)。
pub(crate) fn pack_apps_dir_candidates(exe_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let cwd = std::path::PathBuf::from("skills").join("actl").join("apps");
    let exe_adjacent = exe_dir.join("skills").join("actl").join("apps");
    vec![cwd, exe_adjacent]
}

/// pack 级常驻面板声明注入(降噪第二轮 pack 来源,2026-10-11):扫描
/// skills/actl/apps/*.yml 顶层 `popup_ignores: [{app, name_contains}]`
/// (与 plan 级同构——pack 作者列全该应用的锚形态),并入 plan 与模板级
/// 声明并集去重。注入幂等(重复字段跳过),失败不阻断(pack 缺失/损坏
/// 时模板级声明仍然自洽)。来源留 stderr 痕(诊断透明)。
pub(crate) fn inject_pack_popup_ignores(plan: &mut Plan) -> Result<()> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();
    let Some(apps_dir) = pack_apps_dir_candidates(&exe_dir)
        .into_iter()
        .find(|d| d.is_dir())
    else {
        return Ok(()); // pack 目录不存在(嵌入式/测试环境):跳过
    };
    let entries = match std::fs::read_dir(&apps_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // pack 目录不可读:跳过(lint-packs 是守门员)
    };
    let mut added: Vec<actl_flow::PopupIgnore> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "yml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // 行级提取而非完整 YAML 解析:pack 是给人读的松散文档,quirks 文本
        // 的冒号/星号形态会让严格流式解析炸(word/cloudmusic 实证)——注入
        // 只需 popup_ignores 块的 app/name_contains 两字段,状态机最鲁棒。
        // (pack 的语法守门员是 lint-packs,注入失败不阻断。)
        for ignore in extract_pack_ignores(&text) {
            if plan.popup_ignores.contains(&ignore) || added.contains(&ignore) {
                continue;
            }
            eprintln!(
                "[flow] popup_ignores + {}:{} (pack {})",
                ignore.app,
                ignore.name_contains,
                path.file_stem().and_then(|s| s.to_str()).unwrap_or("?")
            );
            added.push(ignore);
        }
    }
    plan.popup_ignores.extend(added);
    Ok(())
}

/// 从 pack 文本提取顶层 `popup_ignores:` 块的 {app, name_contains} 对。
/// 状态机:块内匹配 `  - app: "X"` 开条目、`    name_contains: "Y"` 补全;
/// 遇非缩进行(块结束)停止。值取引号内文本(无引号取整行 trim)。
pub(crate) fn extract_pack_ignores(text: &str) -> Vec<actl_flow::PopupIgnore> {
    let mut out = Vec::new();
    let mut in_block = false;
    let mut pending_app: Option<String> = None;
    for line in text.lines() {
        if !in_block {
            if line == "popup_ignores:" {
                in_block = true;
            }
            continue;
        }
        if !line.starts_with(' ') {
            break; // 块结束(下一个顶层键)
        }
        let value_of = |l: &str| -> String {
            let raw = l.split_once(':').map(|(_, v)| v).unwrap_or("").trim();
            raw.trim_matches('"').to_string()
        };
        if let Some(app) = line.strip_prefix("  - app:") {
            let app = app.trim().trim_matches('"').to_string();
            if app.is_empty() {
                continue;
            }
            pending_app = Some(app);
        } else if line.trim_start().starts_with("name_contains:") && pending_app.is_some() {
            let name = value_of(line);
            if name.is_empty() {
                continue;
            }
            out.push(actl_flow::PopupIgnore {
                app: pending_app.take().unwrap_or_default(),
                name_contains: name,
            });
        }
    }
    out
}

#[cfg(test)]
mod pack_ignore_tests {
    use super::{extract_pack_ignores, pack_apps_dir_candidates};

    #[test]
    fn extracts_quoted_pairs_and_stops_at_block_end() {
        // 标准块:引号值、多锚形态;块后顶层键终止(quirks 里的词不误提取)
        let pack = "name: word\nwindow: \"Word\"\npopup_ignores:\n  - app: \"Word\"\n    name_contains: \"导航\"\n  - app: \".docx\"\n    name_contains: \"导航\"\nroutes:\n  - id: r1\n    note: >-\n      文本里有 popup_ignores: 和 name_contains: 字样(缩进块内)不误提取\n";
        let got = extract_pack_ignores(pack);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].app, "Word");
        assert_eq!(got[0].name_contains, "导航");
        assert_eq!(got[1].app, ".docx");
    }

    #[test]
    fn skips_empty_values_and_missing_block() {
        // 空 name(空串)跳过——与 plan 级最窄形态同规;无块返回空
        let with_empty = "popup_ignores:\n  - app: \"X\"\n    name_contains: \"\"\n";
        assert!(extract_pack_ignores(with_empty).is_empty());
        assert!(extract_pack_ignores("name: word\n").is_empty());
    }

    #[test]
    fn pack_apps_dir_prefers_cwd_then_exe_adjacent() {
        // 安装态(exe 链接进 bin,CWD 任意)注入走 exe 旁回退;CWD 优先序保持
        // 仓库开发态行为不变(见 pack_apps_dir_candidates)。
        let candidates = pack_apps_dir_candidates(std::path::Path::new(r"C:\Users\u\.local\bin"));
        assert_eq!(candidates[0], std::path::PathBuf::from("skills/actl/apps"));
        assert_eq!(
            candidates[1],
            std::path::PathBuf::from(r"C:\Users\u\.local\bin\skills\actl\apps")
        );
    }
}
