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
) -> Result<Value> {
    let list = windows(c)?;
    let candidates: Vec<_> = list
        .iter()
        .filter(|w| w["title"].as_str().is_some_and(|t| t.contains(marker)))
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
fn cleanup(c: &mut Context, owner: &Value, tab: bool, browser: bool) -> Result<String> {
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
    if tab || browser {
        let snapshot = c.raw(&["snapshot", "--app", &app])?;
        require(
            snapshot["ok"] == true && snapshot["data"]["truncated"] == false,
            "cleanup snapshot incomplete",
        )?;
        let marker = owner["marker"]
            .as_str()
            .ok_or_else(|| io("marker missing"))?;
        let close = checks::owned_tab(elements(&snapshot["data"])?, marker, tab)?;
        if tab {
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
    c.call(&["close-window", "--hwnd", &hwnd])?;
    c.call(&[
        "verify-closed",
        "--hwnd",
        &hwnd,
        "--pid",
        &pid.to_string(),
        "--instance",
        owner["start"]
            .as_str()
            .ok_or_else(|| io("process start missing"))?,
    ])?;
    Ok("closed_verified".into())
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
        if failure.is_none() {
            match cleanup(
                &mut c,
                &owner,
                matches!(case, "golden-task-2" | "input-target"),
                case.contains("content-view")
                    || case == "edge-deep-tree"
                    || case == "virtualized-list",
            ) {
                Ok(state) => report["cleanup"] = json!(state),
                Err(e) => {
                    report["cleanup"] = json!("retained_for_review");
                    failure = Some(e);
                }
            }
        } else {
            report["cleanup"] = json!("retained_after_failure");
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
