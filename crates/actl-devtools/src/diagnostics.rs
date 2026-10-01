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
            "from",
            "to",
            "program",
            "arg",
            "title",
            "match",
            "seconds",
            "pid",
        ],
        &[],
    )?;
    let mut c = context(args, kind)?;
    let mut report = json!({"kind":kind,"status":"blocked","cleanup":"no_target_resources_created","atomic":false,"unit":"UTF-8 bytes, not tokenizer tokens"});
    let outcome = (|| -> Result<Value> {
        if kind == "snapshot-diff" {
            return snapshot_diff(args.get("from")?, args.get("to")?);
        }
        if kind == "app-lab" {
            // 隔离应用实验台:以隔离 profile/数据目录启动指定程序,返回窗口
            // 句柄——替代 ad-hoc 反复手写的"启动+隔离+定位"(诊断动词收官件,
            // 2026-10-02)。--program 必填;--arg 可重复传程序参数(如 URL);
            // --title 可选窗口标题子串(wait 条件,默认用 program 文件名)。
            let program = args.get("program")?;
            let extra = args.get("arg").unwrap_or_default();
            let title = match args.get("title") {
                Ok(t) => t.to_owned(),
                Err(_) => std::path::Path::new(&program)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            };
            let mut call = vec!["launch".to_owned(), program.to_owned()];
            if !extra.is_empty() {
                call.push(extra.to_owned());
            }
            let refs: Vec<_> = call.iter().map(String::as_str).collect();
            c.call(&refs)?;
            if title.is_empty() {
                return Ok(json!({"status": "launched", "program": &program}));
            }
            c.call(&["wait", "--window", &title, "--timeout", "15000"])?;
            let list = c.call(&["list-windows", "--native"])?;
            let windows: Vec<_> = list["windows"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .filter(|w| w["title"].as_str().is_some_and(|t| t.contains(&title)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            return Ok(json!({
                "status": "ready",
                "program": &program,
                "windows": windows,
                "hint": "isolated app ready; use returned hwnd for follow-up; close with close-window",
            }));
        }
        if kind == "cleanup-tabs" {
            // 清场动词:按名称子串关标签(fixtures::close_tabs_where 同路线)。
            // 沉淀背景:ad-hoc 清场脚本三次手写"选中标签+ctrl+w+弹层点不保存"
            // 三次出错(单次采样不等弹层渲染、grep 不匹配静默跳过、ref 提取
            // 残渣),2026-10-01 用户当场截获弹窗后决定永绝手写。
            let hwnd = args.get("hwnd")?;
            let pattern = args.get("match")?;
            let app = format!("hwnd:{hwnd}");
            let closed = crate::fixtures::close_tabs_where(&mut c, &app, "match", &|n: &str| {
                n.contains(pattern)
            })?;
            let after = c.raw(&["snapshot", "--app", &app])?;
            return Ok(json!({
                "status": "cleaned",
                "closed": closed,
                "window_present": after["ok"] == true,
            }));
        }
        if kind == "title-watch" {
            // 标题监视器:按拍采样目标窗标题,记录变化事件与时刻——用于观察
            // 激活过渡/加载期的标题瞬态(前台护栏 actual="" 类问题的取证)。
            let hwnd = args.get("hwnd")?;
            let seconds = args.number("seconds", 5, 1, 120)?;
            let start = std::time::Instant::now();
            let mut last: Option<String> = None;
            let mut events = vec![];
            while start.elapsed().as_secs() < seconds {
                let list = c.call(&["list-windows", "--native", "--hwnd", hwnd])?;
                let title = list["windows"][0]["title"].as_str().map(str::to_owned);
                if title != last {
                    events.push(json!({"elapsed_ms": start.elapsed().as_millis(), "title": title}));
                    last = title;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            return Ok(json!({"status": "watched", "seconds": seconds, "events": events}));
        }
        if kind == "close-gone" {
            // close→gone 计时器:对调用方自有窗口发优雅关闭,计时窗口消失
            // 各阶段——收尾拖尾取证的标准化(替代手写计时循环)。进程退出
            // 计时由 verify-closed(实例核验)承载,本动词不重复。
            let hwnd = args.get("hwnd")?;
            let start = std::time::Instant::now();
            c.call(&["close-window", "--hwnd", hwnd])?;
            let close_ms = start.elapsed().as_millis();
            loop {
                let list = c.raw(&["list-windows", "--native", "--hwnd", hwnd])?;
                let present = list["windows"].as_array().is_some_and(|w| !w.is_empty());
                if !present {
                    return Ok(json!({
                        "status": "gone",
                        "close_ms": close_ms,
                        "window_gone_ms": start.elapsed().as_millis(),
                    }));
                }
                if start.elapsed() >= Duration::from_secs(30) {
                    return Ok(json!({
                        "status": "window_remains",
                        "close_ms": close_ms,
                        "waited_ms": start.elapsed().as_millis(),
                    }));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
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

/// 快照差异对比(docs/31 组件选择判定树第 2 步):按 runtime_id(UIA 稳定身份)
/// 对比两份历史快照,回答"组件去哪了"——消失/新增/同身份但语义元数据变化
/// (如标签名"已修改"→"未修改")。文件来自此前 snapshot 调用落盘的
/// `~/.actl/snapshots/<id>.json`;无 runtime_id 的元素按 role/name/automation_id
/// 退化合并。列表截断 20 条防膨胀,计数保持完整。
fn snapshot_diff(from: &str, to: &str) -> Result<Value> {
    const CAP: usize = 20;
    let dir = actl_core::state::SignalPaths::default()
        .dir
        .join("snapshots");
    let load = |id: &str| -> Result<BTreeMap<String, Value>> {
        let raw = crate::capture::read_json(&dir.join(format!("{id}.json")))
            .map_err(|_| io("snapshot file unreadable (ids come from prior snapshot calls)"))?;
        let elements = raw["elements"]
            .as_array()
            .ok_or_else(|| io("snapshot file has no elements array"))?;
        Ok(elements
            .iter()
            .map(|e| {
                let key = if e["runtime_id"].is_array() {
                    e["runtime_id"].to_string()
                } else {
                    format!("{}|{}|{}", e["role"], e["name"], e["automation_id"])
                };
                (key, e.clone())
            })
            .collect())
    };
    let from_map = load(from)?;
    let to_map = load(to)?;
    let mut gone: Vec<Value> = vec![];
    let mut identity_changed: Vec<Value> = vec![];
    for (key, old) in &from_map {
        match to_map.get(key) {
            None => gone.push(old.clone()),
            Some(new) => {
                if old["name"] != new["name"]
                    || old["role"] != new["role"]
                    || old["automation_id"] != new["automation_id"]
                {
                    identity_changed.push(json!({
                        "runtime_id": old["runtime_id"],
                        "from": {"name": old["name"], "role": old["role"], "automation_id": old["automation_id"]},
                        "to": {"name": new["name"], "role": new["role"], "automation_id": new["automation_id"]},
                    }));
                }
            }
        }
    }
    let added: Vec<Value> = to_map
        .iter()
        .filter(|(key, _)| !from_map.contains_key(*key))
        .map(|(_, e)| e.clone())
        .collect();
    Ok(json!({
        "status": "diffed",
        "from": from,
        "to": to,
        "counts": {
            "from": from_map.len(),
            "to": to_map.len(),
            "gone": gone.len(),
            "added": added.len(),
            "identity_changed": identity_changed.len(),
        },
        "gone": gone.into_iter().take(CAP).collect::<Vec<_>>(),
        "added": added.into_iter().take(CAP).collect::<Vec<_>>(),
        "identity_changed": identity_changed.into_iter().take(CAP).collect::<Vec<_>>(),
        "list_cap": CAP,
    }))
}
