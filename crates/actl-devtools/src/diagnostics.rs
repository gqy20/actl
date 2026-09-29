use crate::{
    Args, Result,
    capture::{Context, process, read_json},
    checks, io, require, root,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
pub fn context(args: &Args, label: &str) -> Result<Context> {
    let binary = PathBuf::from(
        args.value(
            "actl",
            root()
                .join("target/validation/release/actl.exe")
                .to_string_lossy()
                .as_ref(),
        ),
    );
    let dir = PathBuf::from(
        args.value(
            "output",
            root()
                .join("target")
                .join(crate::id(label))
                .to_string_lossy()
                .as_ref(),
        ),
    );
    Context::new(binary, dir)
}
fn identity(c: &mut Context, hwnd: &str) -> Result<Value> {
    let data = c.call(&["list-windows", "--native", "--hwnd", hwnd])?;
    let windows = data["windows"]
        .as_array()
        .ok_or_else(|| io("native windows missing"))?;
    require(windows.len() == 1, "exact window is unavailable")?;
    let w = &windows[0];
    require(
        checks::observable_window(w),
        "window hidden/minimized; observation will not restore it",
    )?;
    let pid = w["pid"].as_u64().ok_or_else(|| io("process ID missing"))? as u32;
    let p = actl_uia::evidence::process(pid, true)?;
    require(p["start"].is_string(), "process identity unavailable")?;
    Ok(json!({"hwnd":w["hwnd"],"pid":pid,"class":w["class"],"process":p}))
}
fn bytes(c: &Context) -> Result<u64> {
    let last = c.calls.last().ok_or_else(|| io("missing capture"))?;
    std::fs::metadata(
        last["stdout"]
            .as_str()
            .ok_or_else(|| io("missing capture path"))?,
    )
    .map(|m| m.len())
    .map_err(io)
}
pub fn run(kind: &str, args: &Args) -> Result<Value> {
    args.check(
        &[
            "actl",
            "output",
            "hwnd",
            "within",
            "depth",
            "limit",
            "samples",
            "winapp",
            "automation-id",
            "timeout",
        ],
        &[],
    )?;
    let mut c = context(args, kind)?;
    let mut report = json!({"kind":kind,"status":"blocked","cleanup":"no_target_resources_created","atomic":false,"unit":"UTF-8 bytes, not tokenizer tokens"});
    let outcome = (|| -> Result<Value> {
        let state = c.call(&["execution-status"])?;
        report["execution"] = state;
        if kind == "phase2-readonly" {
            let n = args.number("samples", 100, 1, 1000)?;
            let mut times = vec![];
            for _ in 0..n {
                c.call(&["status"])?;
                times.push(
                    c.calls
                        .last()
                        .and_then(|v| v["elapsed_ms"].as_f64())
                        .ok_or_else(|| io("missing timing"))?,
                );
            }
            report["timings"] = json!({"samples":n,"p50":checks::percentile(&times,0.5)?,"p95":checks::percentile(&times,0.95)?,"p99":checks::percentile(&times,0.99)?});
            if !args.values.contains_key("hwnd") {
                return Ok(json!({"status":"measured","scope":"CLI cold calls only"}));
            }
        }
        let hwnd = args.get("hwnd")?;
        let app = format!("hwnd:{hwnd}");
        let first_identity = identity(&mut c, hwnd)?;
        report["identity"] = first_identity.clone();
        let full = c.call(&["snapshot", "--app", &app])?;
        let full_bytes = bytes(&c)?;
        let result = match kind {
            "app-observe" => {
                let mut roles = BTreeMap::new();
                for e in full["elements"]
                    .as_array()
                    .ok_or_else(|| io("missing elements"))?
                {
                    *roles
                        .entry(e["role"].as_str().unwrap_or("unknown").to_owned())
                        .or_insert(0usize) += 1;
                }
                json!({"status":"observed","roles":roles,"snapshot":full})
            }
            "phase2-readonly" => {
                let skeleton = c.call(&["snapshot", "--app", &app, "--skeleton"])?;
                let after = c.call(&["snapshot", "--app", &app])?;
                checks::projection(&full, &skeleton, &after)
            }
            "local-observation" => {
                let within = args.get("within")?;
                let depth = args.number("depth", 40, 0, 40)?.to_string();
                let limit = args.number("limit", 5000, 1, 5000)?.to_string();
                let command = [
                    "snapshot", "--app", &app, "--within", within, "--depth", &depth, "--limit",
                    &limit,
                ];
                let local = c.raw(&command)?;
                require(local["ok"] == true, "local observation failed")?;
                let local_bytes = bytes(&c)?;
                // Replay one local ref with its own snapshot, never the full-window ordinal.
                if let Some(reference) = local["data"]["elements"]
                    .as_array()
                    .and_then(|e| e.iter().find_map(|e| e["ref"].as_str()))
                {
                    let snapshot = local["snapshot_id"]
                        .as_str()
                        .ok_or_else(|| io("local snapshot identity missing"))?;
                    c.call(&[
                        "get",
                        reference,
                        "--snapshot",
                        snapshot,
                        "--property",
                        "name",
                        "--app",
                        &app,
                    ])?;
                }
                let repeat = c.call(&command)?;
                let after = c.call(&["snapshot", "--app", &app])?;
                checks::local(
                    &full,
                    &local["data"],
                    &repeat,
                    &after,
                    full_bytes,
                    local_bytes,
                )
            }
            "uia-composite-observe" => {
                require(full["truncated"] == false, "snapshot incomplete")?;
                let elements = full["elements"]
                    .as_array()
                    .ok_or_else(|| io("missing elements"))?;
                let id = args.get("automation-id")?;
                let matches: Vec<_> = elements
                    .iter()
                    .filter(|e| e["automation_id"] == id)
                    .collect();
                require(matches.len() == 1, "AutomationId not unique")?;
                let node = matches[0];
                let parent = elements.first().ok_or_else(|| io("empty snapshot"))?;
                require(
                    parent["name"].as_str().is_some_and(|s| !s.is_empty()),
                    "root name missing",
                )?;
                let mut query = json!({"match":{"id":id,"role":node["role"]},"within":{"role":parent["role"],"name":parent["name"]}});
                let selector = format!("uia:{query}");
                let found = c.call(&["find", "--target", &selector, "--app", &app])?;
                require(
                    found["elements"][0]["automation_id"] == id,
                    "wrong scoped element",
                )?;
                let read = c.call(&[
                    "get",
                    &selector,
                    "--property",
                    "automation_id",
                    "--app",
                    &app,
                ])?;
                require(
                    read["properties"]["automation_id"] == id,
                    "wrong scoped property",
                )?;
                query["scope"] = json!("children");
                let direct = format!("uia:{query}");
                if node["depth"] == 1 {
                    c.call(&["find", "--target", &direct, "--app", &app])?;
                } else {
                    c.expected(&["find", "--target", &direct, "--app", &app], "NOT_FOUND")?;
                }
                let wrong = format!(
                    "uia:{}",
                    json!({"match":{"id":id,"name":crate::id("missing")}})
                );
                c.expected(&["find", "--target", &wrong, "--app", &app], "NOT_FOUND")?;
                json!({"status":"passed"})
            }
            "uia-compare" => {
                let winapp = PathBuf::from(args.get("winapp")?)
                    .canonicalize()
                    .map_err(io)?;
                let depth = args.number("depth", 5, 1, 12)?.to_string();
                let timeout = Duration::from_millis(args.number("timeout", 10000, 100, 30000)?);
                let external_args = vec![
                    "ui".into(),
                    "inspect".into(),
                    "-w".into(),
                    hwnd.into(),
                    "--depth".into(),
                    depth,
                    "--json".into(),
                ];
                let prefix = c.dir.join("winapp");
                let record = process(
                    &winapp,
                    &external_args,
                    &c.dir,
                    &prefix,
                    &BTreeMap::new(),
                    timeout,
                )?;
                report["external_capture"] = record.clone();
                require(
                    record["exit_code"] == 0 && record["status"] != "timeout",
                    "external UIA tool failed",
                )?;
                let external = read_json(&prefix.with_extension("stdout.json"))?;
                require(
                    !external.is_null() && external["ok"] != false,
                    "external observation failed",
                )?;
                require(
                    external["windows"].as_array().is_some_and(|w| {
                        w.len() == 1
                            && w[0]["hwnd"].as_i64().map(|h| h.to_string()).as_deref() == Some(hwnd)
                            && w[0]["elements"].is_array()
                    }),
                    "external tool returned a different window or schema",
                )?;
                report["external_sha256"] = json!(actl_uia::evidence::sha256(
                    &std::fs::read(&winapp).map_err(io)?
                )?);
                let after = c.call(&["snapshot", "--app", &app])?;
                require(
                    full["truncated"] == false
                        && after["truncated"] == false
                        && full["elements"] == after["elements"],
                    "actl observation changed during comparison",
                )?;
                json!({"status":"collected","actl":full,"winapp":external,"comparison":"candidate correspondence only; raw/control views and depths differ"})
            }
            _ => return Err(io("unknown diagnostic")),
        };
        require(
            identity(&mut c, hwnd)? == first_identity,
            "window/process identity changed",
        )?;
        Ok(result)
    })();
    match outcome {
        Ok(result) => {
            report["result"] = result;
            report["status"] = report["result"]["status"].clone();
            c.report("report.json", &mut report)?;
            Ok(report)
        }
        Err(e) => {
            report["error"] = json!({"code":e.code,"message":e.message});
            c.report("report.json", &mut report)?;
            Err(e)
        }
    }
}
