//! query —— 观察与编排类命令执行体:commands/status 元命令、snapshot/find/get/
//! verify/extract/wait 观察、list-windows、剪贴板数据面、batch 编排器。
//! 操作类(注入/指针/窗口操作/launch)见 `interact`。

use std::time::Instant;

use actl_core::{CtlError, SnapshotBuilder};
use clap::{CommandFactory, Parser};
use serde_json::{Value, json};

use crate::cli::{Cli, Command};
use crate::interact::replay_guard;
use crate::{CommandOutput, dispatch};

/// L2 自发现(06 §3.5):从 clap 命令树同源生成 schema,不手写命令清单。
pub(crate) fn commands() -> Result<Value, CtlError> {
    let root = Cli::command();
    let mut list = Vec::new();
    for sub in root.get_subcommands() {
        if sub.get_name() == "help" {
            continue; // clap 自动生成的 help 不是命令面的一部分
        }
        let args: Vec<Value> = sub
            .get_arguments()
            .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
            .map(|a| {
                json!({
                    "name": a.get_id().as_str(),
                    "help": a.get_help().map(|h| h.to_string()),
                })
            })
            .collect();
        list.push(json!({
            "name": sub.get_name(),
            "about": sub.get_about().map(|h| h.to_string()),
            "args": args,
        }));
    }
    Ok(json!({ "commands": list }))
}

pub(crate) fn status() -> Result<Value, CtlError> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": actl_core::ENVELOPE_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

pub(crate) fn snapshot_cmd(
    app: Option<&str>,
    hwnd: Option<&str>,
    skeleton: bool,
) -> Result<CommandOutput, CtlError> {
    let cap = match (app, hwnd) {
        (_, Some(h)) => actl_uia::capture_hwnd(actl_uia::parse_hwnd(h)?)?,
        (a, None) => actl_uia::capture(a)?,
    };
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    let out = builder.finish(skeleton);
    actl_uia::persist_snapshot(&out.snapshot_id, &cap.window.title, &cap.window_runtime_id);
    let data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "truncated": cap.truncated || out.truncated,
        // 激活感知:目标曾挂起并被唤醒时如实上报等待量(0/未挂起时省略)
        "wake_ms": cap.wake_ms,
        "elements": serde_json::to_value(&out.elements).expect("elements serialize"),
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

pub(crate) fn get(
    target_raw: &str,
    property: Option<&str>,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    replay_guard(snapshot, app)?;
    let info = actl_uia::read_property(app, &target, property, near)?;
    let mut props = serde_json::Map::new();
    if let Some(v) = &info.name {
        props.insert("name".into(), json!(v));
    }
    if !info.role.is_empty() {
        props.insert("role".into(), json!(info.role));
    }
    if let Some(v) = &info.automation_id {
        props.insert("automation_id".into(), json!(v));
    }
    if let Some(v) = info.offscreen {
        props.insert("offscreen".into(), json!(v));
    }
    if let Some(v) = &info.value {
        props.insert("value".into(), json!(v));
    }
    if let Some(via) = info.value_via {
        props.insert("value_via".into(), json!(via));
    }
    if let Some(v) = &info.text {
        props.insert("text".into(), json!(v));
    }
    Ok(json!({ "target": target.describe(), "properties": props }).into())
}

/// find:条件查元素,复用 snapshot 的 ref 编号(同一 DFS,同一编号空间,
/// find 的 @eN 与 snapshot 的 @eN 可互换)。至少一个过滤条件,反对全树 dump。
pub(crate) fn find_cmd(
    name: Option<&str>,
    role: Option<&str>,
    id: Option<&str>,
    app: Option<&str>,
    limit: Option<usize>,
) -> Result<CommandOutput, CtlError> {
    if name.is_none() && role.is_none() && id.is_none() {
        return Err(CtlError::protocol(
            "find needs at least one of --name/--role/--id (full-tree dump is `snapshot`)",
        ));
    }
    let cap = actl_uia::capture(app)?;
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    let out = builder.finish(false);
    actl_uia::persist_snapshot(&out.snapshot_id, &cap.window.title, &cap.window_runtime_id);
    let role_want = role.map(str::to_lowercase);
    let matched: Vec<&actl_core::snapshot::ElementOut> = out
        .elements
        .iter()
        .filter(|e| {
            name.map(|n| e.name.as_deref().is_some_and(|en| en.contains(n)))
                .unwrap_or(true)
        })
        .filter(|e| {
            role_want
                .as_ref()
                .map(|r| e.role.to_lowercase().contains(r.as_str()))
                .unwrap_or(true)
        })
        .filter(|e| {
            id.map(|i| e.automation_id.as_deref() == Some(i))
                .unwrap_or(true)
        })
        .collect();
    // 虚拟化容器兜底:已物化树 0 命中且查询可映射为精确属性时,走定位链 L4
    // (ItemContainerPattern;Explorer 文件列表等,US1 开放项)。命中元素不在
    // 快照树内 → 无 @eN ref,返回 name/id 选择器语义(agent 重放走同一查询)。
    let mut virtual_hit: Option<(actl_core::snapshot::ElementOut, &'static str)> = None;
    if matched.is_empty() {
        if let Some(raw_target) = name
            .map(|n| format!("name:{n}"))
            .or_else(|| id.map(|i| format!("id:{i}")))
        {
            if let Ok(target) = actl_core::parse_target(&raw_target) {
                if let Ok(loc) = actl_uia::locate(app, &target, None) {
                    if loc.resolved_by == "item-container" {
                        virtual_hit = Some((
                            actl_core::snapshot::ElementOut {
                                ref_id: None,
                                role: loc.role.clone(),
                                name: loc.name.clone(),
                                automation_id: loc.automation_id.clone(),
                                depth: 0,
                            },
                            loc.resolved_by,
                        ));
                    }
                }
            }
        }
    }
    let total = matched.len() + virtual_hit.is_some() as usize;
    let limit = limit.unwrap_or(20);
    let mut elements: Vec<Value> = matched
        .iter()
        .take(limit)
        .map(|e| serde_json::to_value(e).expect("element serializes"))
        .collect();
    if let Some((elem, level)) = virtual_hit {
        elements.push(serde_json::to_value(&elem).expect("element serializes"));
        let data = json!({
            "window": {
                "title": cap.window.title,
                "class": cap.window.class,
                "pid": cap.window.pid,
            },
            "total": total,
            "returned": elements.len(),
            "resolved_by": level,
            "wake_ms": cap.wake_ms,
            "elements": elements,
        });
        return Ok(CommandOutput {
            data,
            snapshot_id: Some(out.snapshot_id),
        });
    }
    let data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "total": total,
        "returned": elements.len(),
        "wake_ms": cap.wake_ms,
        "elements": elements,
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

pub(crate) fn get_clipboard_cmd() -> Result<CommandOutput, CtlError> {
    let text = actl_uia::get_clipboard_text()?;
    Ok(json!({ "text": text }).into())
}

pub(crate) fn set_clipboard_cmd(text: &str) -> Result<CommandOutput, CtlError> {
    actl_uia::set_clipboard_text(text)?;
    Ok(json!({ "chars": text.chars().count() }).into())
}

pub(crate) fn wait_cmd(
    element: Option<&str>,
    window: Option<&str>,
    gone: Option<&str>,
    property: Option<&str>,
    contains: bool,
    app: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let timeout = timeout.unwrap_or(5000);
    let started = Instant::now();
    let what = match (element, window, gone) {
        (Some(t), _, _) => {
            let target = actl_core::parse_target(t)?;
            // --property:谓词等待(隐含存在性;元素未解析继续等)
            if let Some(raw) = property {
                let mut pred = actl_uia::WaitPredicate::parse(raw)?;
                pred.contains = contains;
                let actual = actl_uia::wait_property(app, &target, &pred, timeout)?;
                json!({ "element": target.describe(), "predicate": pred.describe(), "actual": actual })
            } else {
                actl_uia::wait_element(app, &target, timeout)?;
                json!({ "element": target.describe() })
            }
        }
        (None, Some(w), _) => {
            let title = actl_uia::wait_window(w, timeout)?;
            json!({ "window": title })
        }
        (None, None, Some(g)) => {
            let target = actl_core::parse_target(g)?;
            actl_uia::wait_gone(app, &target, timeout)?;
            json!({ "gone": target.describe() })
        }
        (None, None, None) => {
            return Err(CtlError::protocol("wait needs --element/--window/--gone"));
        }
    };
    Ok(json!({
        "waited": what,
        "waited_ms": started.elapsed().as_millis() as u64,
    })
    .into())
}

/// batch:单进程顺序执行(--json '["cmd args", ...]'),首错即停。
/// 每步结果与耗时进 `steps[]`(编排层可观测:哪步慢、哪步开始等待);
/// 失败时部分步骤随错误 evidence 带出(失败前的已执行步不再丢失)。
pub(crate) fn batch_cmd(json_str: &str) -> Result<CommandOutput, CtlError> {
    let cmds: Vec<String> = serde_json::from_str(json_str).map_err(|e| {
        CtlError::protocol(format!(
            "--json must be a JSON array of command strings: {e}"
        ))
    })?;
    if cmds.is_empty() {
        return Err(CtlError::protocol("batch needs at least one command"));
    }
    let signals = actl_core::state::SignalPaths::default();
    let mut executed = 0usize;
    let mut steps: Vec<Value> = Vec::with_capacity(cmds.len());
    // 窗口基线(进入时的 hwnd 集):结束时差集 = 本轮新开的窗口。
    // 如实上报(windows_created: title/pid/hwnd),不代管生命周期——
    // 关不关是编排层(agent/fixture)的决定,清理武器是 close-window --pid/--hwnd。
    let baseline: std::collections::HashSet<isize> = actl_uia::list_windows()
        .unwrap_or_default()
        .iter()
        .filter_map(|w| w.hwnd)
        .collect();
    // 弹窗闸门的合法目标集:batch 自身声明的 --app 值(launch-then-interact 模式里,
    // 自启窗口会成为前台,不该误报)
    let app_patterns: Vec<String> = cmds
        .iter()
        .filter_map(|l| {
            let toks: Vec<&str> = l.split_whitespace().collect();
            toks.iter().position(|t| *t == "--app").and_then(|p| {
                toks.get(p + 1)
                    .map(|v| v.trim_matches(|c| c == '"' || c == '\'').to_string())
            })
        })
        .collect();
    for (i, line) in cmds.iter().enumerate() {
        // 步间双闸门:停止标志(用户热键)优先;随后步级状态落盘(阶段信号)
        if signals.stop_requested() {
            return Err(CtlError::new(
                actl_core::ErrorCode::Aborted,
                format!("batch stopped by user at step {i} (after {executed} ok)"),
            ));
        }

        // 意外弹窗闸门(warning 不阻断):前台窗口若既不在基线、也不匹配本批任何
        // --app 目标,记入该步 blocked_by——弹窗可能是良性的,裁决权归编排层;
        // 步失败时它随 evidence 直接指出元凶
        let fg_blocked = || {
            actl_uia::foreground_window().and_then(|(title, hwnd)| {
                if baseline.contains(&hwnd)
                    || app_patterns.iter().any(|p| title.contains(p.as_str()))
                {
                    None
                } else {
                    Some(json!({ "title": title, "hwnd": hwnd }))
                }
            })
        };
        // 步前采样一次(launch 后自启窗未成形的竞态由失败时刻补采样兜住)
        let mut blocked_by = fg_blocked();
        signals.write_state(&actl_core::state::SessionState::now(
            std::process::id(),
            "batch",
            None,
            Some(format!("{}/{}", i + 1, cmds.len())),
            actl_core::state::Phase::Running,
        ));
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return Err(CtlError::protocol(format!("batch[{i}] is empty")));
        }
        let sub = Cli::try_parse_from(std::iter::once("actl").chain(tokens.iter().copied()))
            .map_err(|e| CtlError::protocol(format!("batch[{i}] {line:?}: {e}")))?;
        if let Command::Batch { .. } = sub.command {
            return Err(CtlError::protocol("batch cannot nest batch"));
        }
        let step_started = Instant::now();
        match dispatch(&sub.command) {
            Ok(_) => {
                let mut entry = json!({
                    "i": i, "cmd": line, "ok": true,
                    "duration_ms": step_started.elapsed().as_millis() as u64,
                });
                if let Some(b) = &blocked_by {
                    entry["blocked_by"] = b.clone();
                }
                steps.push(entry);
            }
            Err(err) => {
                // 失败时刻补采样:launch 竞态/慢弹窗在步执行中才成形,
                // 此刻仍占据前台者即元凶候选
                if blocked_by.is_none() {
                    blocked_by = fg_blocked();
                }
                let mut entry = json!({
                    "i": i, "cmd": line, "ok": false,
                    "duration_ms": step_started.elapsed().as_millis() as u64,
                    "error": { "code": err.code, "message": err.message },
                });
                if let Some(b) = &blocked_by {
                    entry["blocked_by"] = b.clone();
                }
                steps.push(entry);
                return Err(CtlError::with_evidence(
                    err.code,
                    format!(
                        "batch[{i}] {:?} failed after {executed} ok: {}",
                        line, err.message
                    ),
                    json!({
                        "executed": executed,
                        "steps": steps,
                        "windows_created": windows_created_since(&baseline),
                    }),
                ));
            }
        }
        executed += 1;
    }
    let mut data = json!({ "executed": executed, "steps": steps });
    let created = windows_created_since(&baseline);
    if created.as_array().is_some_and(|a| !a.is_empty()) {
        data["windows_created"] = created;
    }
    Ok(data.into())
}

/// 基线差集:本轮 batch 新开的窗口(title/pid/hwnd)。空省略(token);
/// 失败路径必带(evidence)——那是清理地基。
fn windows_created_since(baseline: &std::collections::HashSet<isize>) -> Value {
    let created: Vec<Value> = actl_uia::list_windows()
        .unwrap_or_default()
        .into_iter()
        .filter(|w| w.hwnd.map(|h| !baseline.contains(&h)).unwrap_or(false))
        .map(|w| json!({ "title": w.title, "pid": w.pid, "hwnd": w.hwnd }))
        .collect();
    json!(created)
}

pub(crate) fn list_windows_cmd() -> Result<CommandOutput, CtlError> {
    let wins = actl_uia::list_windows()?;
    let list: Vec<Value> = wins
        .into_iter()
        .map(|w| {
            let mut m = serde_json::Map::new();
            m.insert("title".into(), json!(w.title));
            m.insert("class".into(), json!(w.class));
            m.insert("pid".into(), json!(w.pid));
            if let Some(h) = w.hwnd {
                m.insert("hwnd".into(), json!(h));
            }
            m.insert("foreground".into(), json!(w.foreground));
            if w.topmost {
                m.insert("topmost".into(), json!(true));
            }
            if w.minimized {
                m.insert("minimized".into(), json!(true));
            }
            if !w.visible {
                m.insert("visible".into(), json!(false));
            }
            if let Some(z) = w.z_order {
                m.insert("z_order".into(), json!(z));
            }
            if let Some(r) = w.rect {
                m.insert("rect".into(), json!(r));
            }
            if let Some(o) = w.owner_title {
                m.insert("owner_title".into(), json!(o));
            }
            Value::Object(m)
        })
        .collect();
    Ok(json!({ "windows": list, "count": list.len() }).into())
}

pub(crate) fn verify_cmd(
    exists: Option<&str>,
    value_target: Option<&str>,
    expected: Option<&str>,
    contains: bool,
    checked: Option<&str>,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let (raw, kind) = if let Some(t) = exists {
        (t, actl_uia::VerifyKind::Exists)
    } else if let Some(t) = value_target {
        (
            t,
            actl_uia::VerifyKind::Value {
                expected: expected.unwrap_or_default().to_string(),
                contains,
            },
        )
    } else if let Some(t) = checked {
        (t, actl_uia::VerifyKind::Checked)
    } else {
        return Err(CtlError::protocol(
            "verify needs one of --exists/--value/--checked",
        ));
    };
    let target = actl_core::parse_target(raw)?;
    let out = actl_uia::verify(app, &target, near, kind)?;
    Ok(json!({
        "action": "verify",
        "mode": out.mode,
        "target": target.describe(),
        "resolved_by": out.located.resolved_by,
        "actual": out.actual,
    })
    .into())
}

pub(crate) fn extract_cmd(target_raw: &str, app: Option<&str>) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let (rows, source) = actl_uia::extract_table(app, &target)?;
    Ok(json!({
        "action": "extract",
        "target": target.describe(),
        "source": source,
        "rows": rows.len(),
        "table": rows,
    })
    .into())
}
