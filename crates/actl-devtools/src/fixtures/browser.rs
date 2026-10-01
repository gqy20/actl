use super::*;
pub(super) fn run(
    c: &mut Context,
    case: &str,
    owned: &mut Option<Value>,
    report: &mut Value,
) -> Result<()> {
    let exe = if case.starts_with("chrome") {
        "chrome.exe"
    } else {
        "msedge.exe"
    };
    let program = actl_uia::resolve_app_path(exe)
        .ok_or_else(|| io("browser is not installed in App Paths"))?;
    let marker = crate::id("browser");
    let profile = c.dir.join("profile");
    let html = c.dir.join("probe.html");
    let content = format!(
        "<!doctype html><html lang='en'><meta charset='utf-8'><title>{marker}</title><body><main><h1>Owned Browser Probe</h1><p>The quick brown fox. 敏捷的棕色狐狸。</p><section><div><article><button>Submit form</button></article></div></section><button onclick=\"this.disabled=true;let i=0;const t=setInterval(()=>{{const li=document.createElement('li');li.textContent='lazy-item-'+(++i);document.querySelector('ol').append(li);if(i===40)clearInterval(t)}},150)\">Start stream</button><button>dup-target</button><button>dup-target</button><ol></ol></main></body></html>"
    );
    fs::write(&html, content).map_err(io)?;
    c.call(&[
        "launch",
        &program,
        "--new-window",
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        if exe == "chrome.exe" {
            "--incognito"
        } else {
            "--inprivate"
        },
        "--force-renderer-accessibility",
        &format!("--user-data-dir={}", profile.to_string_lossy()),
        &html.to_string_lossy(),
    ])?;
    c.call(&["wait", "--window", &marker, "--timeout", "15000"])?;
    let own = owner(
        c,
        &marker,
        Some(&program),
        Some(&profile.to_string_lossy()),
        None,
    )?;
    let app = format!("hwnd:{}", own["hwnd"]);
    *owned = Some(own);
    c.call(&[
        "wait",
        "--element",
        "name:Submit form",
        "--app",
        &app,
        "--timeout",
        "12000",
    ])?;
    let document = format!("uia:{}", json!({"match":{"role":"Document","name":marker}}));
    if case.ends_with("content-view") {
        c.call(&["click", "name:Start stream", "--app", &app])?;
        let settled = c.call(&[
            "wait",
            "--element",
            &document,
            "--stable",
            "--app",
            &app,
            "--timeout",
            "20000",
        ])?;
        require(
            settled["waited_ms"].as_u64().is_some_and(|n| n >= 1000)
                && settled["waited"]["changes"]
                    .as_u64()
                    .is_some_and(|n| n >= 1),
            "no observed content changes",
        )?;
        let error = c.expected(
            &[
                "wait",
                "--element",
                "name:dup-target",
                "--app",
                &app,
                "--timeout",
                "5000",
            ],
            "AMBIGUOUS",
        )?;
        require(
            error["duration_ms"].as_u64().is_some_and(|v| v < 3000),
            "ambiguity was not immediate",
        )?;
    }
    let read = c.call(&["get", &document, "--property", "text", "--app", &app])?;
    let text = read["properties"]["text"]
        .as_str()
        .ok_or_else(|| io("page text unavailable"))?;
    require(
        text.contains("quick brown fox")
            && text.contains("敏捷的棕色狐狸")
            && (!case.ends_with("content-view") || text.contains("lazy-item-40")),
        "page text incomplete",
    )?;
    let mut measures = json!({});
    for view in ["full", "skeleton", "content", "both"] {
        let mut args = vec!["snapshot", "--app", &app];
        if view == "skeleton" || view == "both" {
            args.push("--skeleton");
        }
        if view == "content" || view == "both" {
            args.extend(["--view", "content"]);
        }
        let data = c.call(&args)?;
        require(data["truncated"] == false, "snapshot truncated")?;
        measures[view] = json!({"elements":elements(&data)?.len(),"bytes":fs::metadata(c.calls.last().and_then(|r|r["stdout"].as_str()).ok_or_else(||io("capture missing"))?).map_err(io)?.len()});
    }
    require(
        measures["skeleton"]["elements"].as_u64() < measures["full"]["elements"].as_u64()
            && measures["content"]["bytes"].as_u64() < measures["skeleton"]["bytes"].as_u64(),
        "projection did not reduce output",
    )?;
    report["measurements"] = measures;
    Ok(())
}
