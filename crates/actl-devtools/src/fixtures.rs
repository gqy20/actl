mod browser;
mod notepad;
mod system;
use crate::{Args, Result, capture::Context, checks, io, require};
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};
fn elements(data: &Value) -> Result<&Vec<Value>> {
    data["elements"]
        .as_array()
        .ok_or_else(|| io("missing elements"))
}
fn precondition(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(actl_core::CtlError::with_evidence(
            actl_core::ErrorCode::NotActionable,
            message,
            json!({"stage":"fixture_preflight"}),
        ))
    }
}
fn windows(c: &mut Context) -> Result<Vec<Value>> {
    c.call(&["list-windows", "--native"])?["windows"]
        .as_array()
        .cloned()
        .ok_or_else(|| io("missing windows"))
}
fn owner(
    c: &mut Context,
    marker: &str,
    program: Option<&str>,
    token: Option<&str>,
    class: Option<&str>,
) -> Result<Value> {
    let list = windows(c)?;
    // class 过滤:UWP 应用一标题多窗(框架 ApplicationFrameWindow + 应用
    // CoreWindow + 迟到的 TitleBarWindow),归属唯一性按标题匹配会撞车
    // (2026-10-01 金任务1第2轮实证)——限定框架窗类才能绑定到可操作顶层窗。
    let candidates: Vec<_> = list
        .iter()
        .filter(|w| {
            w["title"].as_str().is_some_and(|t| t.contains(marker))
                && class.is_none_or(|k| w["class"].as_str() == Some(k))
        })
        .collect();
    require(candidates.len() == 1, "task window not unique")?;
    let window = candidates[0];
    let pid = window["pid"].as_u64().ok_or_else(|| io("PID missing"))? as u32;
    let metadata = actl_uia::evidence::process(pid, true)?;
    if let (Some(program), Some(token)) = (program, token) {
        c.call(&[
            "verify-owner",
            "--app",
            &format!("hwnd:{}", window["hwnd"]),
            "--program",
            program,
            "--token",
            token,
        ])?;
    }
    Ok(
        json!({"hwnd":window["hwnd"],"pid":pid,"class":window["class"],"marker":marker,"start":metadata["start"],"program":metadata["program"],"created_unix_ms":metadata["created_unix_ms"]}),
    )
}
fn cleanup(c: &mut Context, owner: &Value, browser_tab: bool, browser: bool) -> Result<String> {
    require(!c.uncertain, "retain uncertain scene")?;
    require(!c.stopped()?, "retain stopped scene")?;
    let hwnd = owner["hwnd"].to_string();
    let app = format!("hwnd:{hwnd}");
    let list = c.call(&["list-windows", "--native", "--hwnd", &hwnd])?;
    let pid = owner["pid"]
        .as_u64()
        .ok_or_else(|| io("owner PID missing"))? as u32;
    let process = actl_uia::evidence::process(pid, false)?;
    let list = list["windows"]
        .as_array()
        .ok_or_else(|| io("native window list unavailable"))?;
    if list.is_empty() {
        require(
            process["state"] == "absent" || process["start"] != owner["start"],
            "window absent but process remains",
        )?;
        return Ok("already_closed".into());
    }
    require(
        list.len() == 1 && checks::matching_identity(owner, &list[0], &process["start"]),
        "window/process identity changed",
    )?;
    if browser_tab && browser {
        let snapshot = c.raw(&["snapshot", "--app", &app])?;
        require(
            snapshot["ok"] == true && snapshot["data"]["truncated"] == false,
            "cleanup snapshot incomplete",
        )?;
        let marker = owner["marker"]
            .as_str()
            .ok_or_else(|| io("marker missing"))?;
        let close = checks::owned_tab(elements(&snapshot["data"])?, marker, browser_tab)?;
        if browser_tab {
            let snapshot_id = snapshot["snapshot_id"]
                .as_str()
                .ok_or_else(|| io("snapshot ID missing"))?;
            c.call(&["click", &close, "--snapshot", snapshot_id, "--app", &app])?;
            let remain = c.call(&["list-windows", "--native", "--hwnd", &hwnd])?;
            if remain["windows"].as_array().is_some_and(|w| !w.is_empty()) {
                let read = c.call(&["snapshot", "--app", &app])?;
                require(
                    read["truncated"] == false
                        && !elements(&read)?.iter().any(|e| {
                            e["role"] == "TabItem"
                                && e["name"].as_str().is_some_and(|s| s.contains(marker))
                        }),
                    "owned tab remains or observation incomplete",
                )?;
            }
            return Ok("owned_tab_closed".into());
        }
    }
    // UWP 进程(计算器)在窗口关闭后驻留——直接 system-close(跳过普通 WM_CLOSE,
    // 避免窗口被第一次关闭后 system-close 找不到窗口);验证放宽为 verify-window
    // absent(进程长尾退出不阻塞下一轮)。非 UWP 用 verify-closed 即可。
    let is_uwp = owner["program"].as_str().is_some_and(|p| {
        let lower = p.to_ascii_lowercase();
        // UWP 分支只给 ApplicationFrameHost 承载的应用(计算器等):窗口关闭后进程驻留。
        // Store 记事本虽从 WindowsApps 别名启动,但行为是普通应用——未保存时弹
        // 保存对话框,必须走下方处理「不保存」的普通分支(2026-10-01 金任务2实证:
        // 走 UWP 分支窗口关不掉,清理 retained_for_review 堵塞下一轮)。
        lower.contains("applicationframehost") || lower.contains("calculatorapp")
    });
    if is_uwp {
        c.call(&["close-window", "--hwnd", &hwnd, "--system-close"])?;
        c.call(&[
            "verify-window",
            "--app",
            &format!("hwnd:{hwnd}"),
            "--state",
            "absent",
        ])?;
    } else {
        // 多标签应用只清理本任务标签(AGENTS):Store 记事本按标签关闭——选中
        // 标签(selection-item)→ ctrl+w → 未保存弹**窗内弹层**点「不保存」
        // Button;最后一个标签关闭时窗口随之关闭(单标签 = 关标签即关窗)。
        // 整窗 WM_CLOSE 路线在「恢复上次会话」下残留标签逐轮累积(2026-10-01
        // 实证 17 个);「不保存」必须按 Button ref 点——name 选择器同时命中
        // 弹层 ContentBlock Text(AMBIGUOUS)。
        let marker = owner["marker"]
            .as_str()
            .ok_or_else(|| io("marker missing"))?;
        close_task_tabs(c, &app, marker)?;
        // 窗口已随标签清空消失;进程可能拖尾退出(Store 打包应用常态,实测
        // 超过 verify-closed 内置 5s 沉降窗、数十秒内自退)。先核窗口消失,
        // verify-closed 有界串行重试(每次内部沉降 5s,×3 ≈ 15s);仅剩进程
        // 拖尾时如实记录状态、不判负——下一轮 preflight 看窗口不看进程,
        // 与 UWP 分支「进程长尾退出不阻塞下一轮」同族裁决。
        c.call(&["verify-window", "--app", &app, "--state", "absent"])?;
        let instance = owner["start"]
            .as_str()
            .ok_or_else(|| io("process start missing"))?
            .to_owned();
        let pid_text = pid.to_string();
        for _ in 0..3 {
            let v = c.raw(&[
                "verify-closed",
                "--hwnd",
                &hwnd,
                "--pid",
                &pid_text,
                "--instance",
                &instance,
            ])?;
            if v["ok"] == true {
                return Ok("closed_verified".into());
            }
        }
        return Ok("window_closed_process_tailed".into());
    }
    Ok("closed_verified".into())
}

/// 按标签关闭本任务标签;未保存内容经窗内弹层「不保存」丢弃。
/// 第二遍清扫夹具同族残留(notepad-s*):历史失败轮的标签经「恢复上次
/// 会话」持久化跨进程存活,整窗关闭不清会话——同族前缀是夹具世系的可证
/// 归属;非本族标签(用户真实内容)不碰。快照失败须区分:NOT_FOUND=窗口
/// 已关(成功路径),STALE_REF/observation_failed=提供方忙窗(沉降重试)。
fn close_task_tabs(c: &mut Context, app: &str, marker: &str) -> Result<()> {
    let gone = |v: &Value| v["ok"] != true && v["error"]["code"] == "NOT_FOUND";
    let busy = |v: &Value| {
        v["ok"] != true
            && v["error"]["code"] == "STALE_REF"
            && v["error"]["evidence"]["reason"] == "observation_failed"
    };
    let tabs_matching = |snapshot: &Value, pred: &dyn Fn(&str) -> bool| -> Option<String> {
        if snapshot["ok"] != true {
            return None;
        }
        elements(&snapshot["data"])
            .ok()?
            .iter()
            .find(|e| e["role"] == "TabItem" && e["name"].as_str().is_some_and(pred))
            .and_then(|e| e["ref"].as_str().map(str::to_owned))
    };
    let observe = |c: &mut Context| -> Result<Value> {
        for _ in 0..8 {
            let snap = c.raw(&["snapshot", "--app", app])?;
            if gone(&snap) || snap["ok"] == true {
                return Ok(snap);
            }
            if busy(&snap) {
                std::thread::sleep(std::time::Duration::from_millis(800));
                continue;
            }
            let code: actl_core::ErrorCode = serde_json::from_value(snap["error"]["code"].clone())
                .map_err(|_| io("unknown CLI error code"))?;
            return Err(actl_core::CtlError::with_evidence(
                code,
                format!(
                    "snapshot: {}",
                    snap["error"]["message"]
                        .as_str()
                        .unwrap_or("cleanup observation failed")
                ),
                snap["error"]["evidence"].clone(),
            ));
        }
        Err(io("cleanup snapshot stayed busy"))
    };
    // 第一遍严格本任务 marker;第二遍同族前缀(notepad-s = 夹具世系)
    for (pred, label) in [
        (
            &(|n: &str| n.contains(marker)) as &dyn Fn(&str) -> bool,
            "task",
        ),
        (
            &(|n: &str| n.contains("notepad-s") && !n.contains(marker)),
            "lineage",
        ),
    ] {
        for _ in 0..24 {
            let snapshot = observe(c)?;
            if gone(&snapshot) {
                return Ok(());
            }
            let Some(tab_ref) = tabs_matching(&snapshot, pred) else {
                break; // 本遍无匹配标签
            };
            let sid = snapshot["snapshot_id"]
                .as_str()
                .ok_or_else(|| io("snapshot missing"))?;
            c.call(&["click", &tab_ref, "--snapshot", sid, "--app", app])
                .map_err(|e| {
                    actl_core::CtlError::with_evidence(
                        e.code,
                        format!("{label} tab close: {}", e.message),
                        e.evidence.unwrap_or_else(|| json!({})),
                    )
                })?;
            c.call(&["press", "ctrl+w", "--app", app])?;
            // 弹层可能迟于 ctrl+w 渲染:有界轮询,「不保存」Button 出现或标签已消失即止
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let popup = observe(c)?;
                if gone(&popup) {
                    break;
                }
                let button = elements(&popup["data"])?.iter().find(|e| {
                    e["role"] == "Button"
                        && e["name"].as_str().is_some_and(|n| n.contains("不保存"))
                });
                if let Some(button) = button {
                    let psid = popup["snapshot_id"]
                        .as_str()
                        .ok_or_else(|| io("snapshot missing"))?;
                    let bref = button["ref"]
                        .as_str()
                        .ok_or_else(|| io("button ref missing"))?;
                    c.call(&["click", bref, "--snapshot", psid, "--app", app])?;
                    break;
                }
                // 标签已消失(未修改标签静默关闭)则停止等待弹层
                if tabs_matching(&popup, pred).is_none() {
                    break;
                }
            }
        }
    }
    let final_snap = observe(c)?;
    require(
        gone(&final_snap)
            || (tabs_matching(&final_snap, &|n| n.contains(marker)).is_none()
                && tabs_matching(&final_snap, &|n| n.contains("notepad-s")).is_none()),
        "task or lineage tabs remain after bounded close attempts",
    )?;
    Ok(())
}
pub fn run(case: &str, args: &Args) -> Result<Value> {
    args.check(&["actl", "output", "hwnd", "runs"], &["physical"])?;
    let mut c = crate::diagnostics::context(args, case)?;
    let mut report = json!({"case":case,"passed":false,"cleanup":"no_owned_window"});
    let mut owned = None;
    let result = (|| -> Result<()> {
        if case == "flow-evaluation" {
            return flow_evaluation(&mut c);
        }
        precondition(
            !c.stopped()?,
            "desktop stopped or needs repair; retain state",
        )?;
        match case {
            "golden-task-2" | "input-target" => notepad::run(&mut c, case, &mut owned),
            "edge-deep-tree" | "edge-content-view" | "chrome-content-view" => {
                browser::run(&mut c, case, &mut owned, &mut report)
            }
            "golden-task-1" | "win32-table-extract" | "virtualized-list" => {
                system::run(&mut c, case, args, &mut owned, &mut report)
            }
            "signal-display" => signal(&mut c, args),
            "display-readiness" => readiness(&mut c, args),
            _ => Err(actl_core::CtlError::protocol("unknown native fixture")),
        }
    })();
    let mut failure = result.err();
    if let Some(owner) = owned {
        report["owned"] = owner.clone();
        // AGENTS:"fixture 无论成败统一进入收尾判定并记录清理状态"——
        // 无论成败都尝试清理;失败时清理失败不覆盖原测试失败原因。
        let cleanup_result = cleanup(
            &mut c,
            &owner,
            // 记事本不再走标签路径——该路径不处理保存对话框,未保存窗口关不掉,
            // 堵塞下一轮 preflight(2026-10-01 金任务2十轮全败的根因)。
            false,
            case.contains("content-view") || case == "edge-deep-tree" || case == "virtualized-list",
        );
        match cleanup_result {
            Ok(state) => report["cleanup"] = json!(state),
            Err(e) => {
                report["cleanup"] = json!("retained_for_review");
                if failure.is_none() {
                    failure = Some(e);
                }
            }
        }
    }
    report["passed"] = json!(failure.is_none());
    if let Some(e) = &failure {
        report["error"] = json!({"code":e.code,"message":e.message});
        if e.evidence
            .as_ref()
            .is_some_and(|e| e["stage"] == "fixture_preflight")
        {
            report["status"] = json!("blocked");
        }
    }
    c.report("result.json", &mut report)?;
    match failure {
        Some(_) if report["status"] == "blocked" => Ok(report),
        Some(e) => Err(e),
        None => Ok(report),
    }
}
fn signal(c: &mut Context, args: &Args) -> Result<()> {
    require(
        args.flags.iter().any(|f| f == "physical"),
        "signal fixture requires explicit --physical",
    )?;
    let app = format!("hwnd:{}", args.get("hwnd")?);
    c.call(&["verify", "--exists", "id:num7Button", "--app", &app])?;
    c.call(&["click", "id:num7Button", "--app", &app, "--physical"])?;
    let file = actl_core::state::SignalPaths::default()
        .dir
        .join("state.json");
    let physical = crate::capture::read_json(&file)?;
    require(
        physical["action"]["effects"]["pointer"] == true
            && physical["action"]["phase"] == "delivered"
            && physical["action"]["target"]["point"]
                .as_array()
                .is_some_and(|p| p.len() == 2),
        "physical feedback missing",
    )?;
    c.call(&["click", "id:num2Button", "--app", &app])?;
    let semantic = crate::capture::read_json(&file)?;
    require(
        semantic["action"]["effects"]["pointer"] == false
            && semantic["action"]["target"]["bounds"]
                .as_array()
                .is_some_and(|p| p.len() == 4),
        "semantic feedback invalid",
    )?;
    require(
        !windows(c)?.iter().any(|w| {
            w["class"]
                .as_str()
                .is_some_and(|s| s.starts_with("actl_signal_"))
        }),
        "signal window leaked into targets",
    )?;
    Ok(())
}
fn readiness(c: &mut Context, args: &Args) -> Result<()> {
    let hwnd = args.get("hwnd")?;
    // Explicitly scoped caller window; never hide the display or clear stop state.
    let state = c.call(&["display-status"])?;
    require(state["ready"] == true, "display not ready")?;
    let first = c.call(&["desktop-prepare", "--hwnd", hwnd])?;
    require(
        first["results"]
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["status"] == "minimized"),
        "window not minimized",
    )?;
    let token = first["session"]
        .as_str()
        .ok_or_else(|| io("session missing"))?;
    let restored = c.call(&["desktop-restore", "--session", token])?;
    require(
        restored["results"]
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["status"] == "restored"),
        "window not restored",
    )?;
    let second = c.call(&["desktop-prepare", "--hwnd", hwnd])?;
    c.call(&["restore-window", "--hwnd", hwnd])?;
    let token = second["session"]
        .as_str()
        .ok_or_else(|| io("session missing"))?;
    let skipped = c.call(&["desktop-restore", "--session", token])?;
    require(
        skipped["results"]
            .as_array()
            .is_some_and(|r| r.len() == 1 && r[0]["status"] == "skipped"),
        "changed placement was restored",
    )?;
    Ok(())
}
fn flow_evaluation(c: &mut Context) -> Result<()> {
    c.env
        .insert("LOCALAPPDATA".into(), c.dir.to_string_lossy().into());
    c.env.remove("ACTL_TASK_CONFIG");
    c.env.remove("ACTL_TASK_ID");
    let path = c.dir.join("inspect.yml");
    fs::copy(crate::root().join("fixtures/flows/inspect.yml"), &path).map_err(io)?;
    let path = path.to_string_lossy().into_owned();
    c.call(&["flow-validate", "--file", &path])?;
    let run = c.call(&["flow-run", "--file", &path, "--id", "checkpoint"])?;
    require(run["run"]["status"] == "paused", "checkpoint did not pause")?;
    fs::write(&path, "invalid after start").map_err(io)?;
    let done = c.call(&["flow-resume", "--id", "checkpoint"])?;
    require(
        done["run"]["status"] == "completed" && done["run"]["steps"][0]["attempts"] == 1,
        "frozen plan or no-replay failed",
    )?;
    // Live pause/death/exclusion are also covered by CLI integration tests; this command
    // runs against the explicitly selected binary, including actual process interruption.
    fs::write(&path,"version: 1\nname: Wait\nsteps:\n - id: wait\n   title: Wait\n   run: {command: wait, args: [--window, ACTL-ABSENT-694201, --timeout, '3000']}\n - id: next\n   title: Next\n   run: {command: status}\n").map_err(io)?;
    for id in ["pause", "crash"] {
        let mut cmd = std::process::Command::new(&c.binary);
        cmd.args(["flow-run", "--file", &path, "--id", id])
            .envs(&c.env)
            .env_remove("ACTL_TASK_ID")
            .env_remove("ACTL_TASK_CONFIG")
            .stdout(std::process::Stdio::from(
                fs::File::create(c.dir.join(format!("{id}.json"))).map_err(io)?,
            ))
            .stderr(std::process::Stdio::from(
                fs::File::create(c.dir.join(format!("{id}.stderr"))).map_err(io)?,
            ));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(io)?;
        let attempt = (|| -> Result<()> {
            let deadline = Instant::now();
            loop {
                let state = c.dir.join(format!("actl/flows/{id}/state.json"));
                if let Ok(state) = crate::capture::read_json(&state)
                    && state["steps"][0]["status"] == "running"
                {
                    break;
                }
                require(
                    deadline.elapsed() < Duration::from_secs(10),
                    "runner did not enter step",
                )?;
                std::thread::sleep(Duration::from_millis(20));
            }
            if id == "pause" {
                c.expected(&["flow-resume", "--id", id], "NOT_ACTIONABLE")?;
                c.call(&["flow-pause", "--id", id])?;
            } else {
                child.kill().map_err(io)?;
            }
            let deadline = Instant::now();
            while child.try_wait().map_err(io)?.is_none() {
                require(
                    deadline.elapsed() < Duration::from_secs(10),
                    "fixture CLI did not finish",
                )?;
                std::thread::sleep(Duration::from_millis(20));
            }
            let state = c.call(&["flow-status", "--id", id])?;
            if id == "pause" {
                require(
                    state["run"]["status"] == "paused" && state["run"]["steps"][1]["attempts"] == 0,
                    "pause boundary failed",
                )?;
            } else {
                require(
                    state["interrupted"] == true && state["runner_active"] == false,
                    "process death unrecognized",
                )?;
                let resumed = c.call(&["flow-resume", "--id", id])?;
                require(
                    resumed["run"]["status"] == "paused"
                        && resumed["run"]["steps"][0]["attempts"] == 2,
                    "read-only recovery failed",
                )?;
            }
            Ok(())
        })();
        if child.try_wait().map_err(io)?.is_none() {
            let _ = child.kill();
            let _ = child.wait();
        }
        attempt?;
    }
    fs::write(
        &path,
        "version: 1\nname: Invalid\nsteps:\n - id: bad\n   title: Bad\n   run: {command: wait}\n",
    )
    .map_err(io)?;
    c.expected(&["flow-validate", "--file", &path], "PROTOCOL")?;
    Ok(())
}
