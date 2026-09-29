use super::*;
pub(super) fn run(c: &mut Context, case: &str, owned: &mut Option<Value>) -> Result<()> {
    precondition(
        !windows(c)?
            .iter()
            .any(|w| w["class"].as_str().is_some_and(|s| s.contains("Notepad"))),
        "existing Notepad window; untouched",
    )?;
    let marker = crate::id("notepad");
    let file = c.dir.join(format!("{marker}.txt"));
    fs::write(&file, "").map_err(io)?;
    let started = actl_core::state::unix_ms();
    c.call(&["launch", "notepad.exe", &file.to_string_lossy()])?;
    c.call(&["wait", "--window", &marker, "--timeout", "12000"])?;
    let own = owner(c, &marker, None, None)?;
    require(
        own["created_unix_ms"]
            .as_u64()
            .is_some_and(|n| n >= started)
            && own["program"]
                .as_str()
                .is_some_and(|p| p.to_ascii_lowercase().ends_with("notepad.exe")),
        "Notepad instance not owned by this launch",
    )?;
    let app = format!("hwnd:{}", own["hwnd"]);
    *owned = Some(own);
    let snapshot = c.raw(&["snapshot", "--app", &app])?;
    let docs: Vec<_> = elements(&snapshot["data"])?
        .iter()
        .filter(|e| e["role"] == "Document")
        .collect();
    require(docs.len() == 1, "document not unique")?;
    let reference = docs[0]["ref"]
        .as_str()
        .ok_or_else(|| io("document ref missing"))?;
    let sid = snapshot["snapshot_id"]
        .as_str()
        .ok_or_else(|| io("snapshot ID missing"))?;
    let content = if case == "input-target" {
        "ACTL input 42"
    } else {
        "golden task 2\n第二行中文"
    };
    c.call(&[
        "type",
        reference,
        content,
        "--paste",
        "--verify-value",
        content,
        "--snapshot",
        sid,
        "--app",
        &app,
    ])?;
    c.call(&[
        "press",
        "ctrl+s",
        "--target",
        "role:Document",
        "--app",
        &app,
    ])?;
    c.call(&[
        "verify-file",
        "--path",
        &file.to_string_lossy(),
        "--kind",
        "text",
        "--expected",
        &json!(content).to_string(),
        "--normalize-lines",
    ])?;
    if case == "input-target" {
        let snapshot = c.raw(&["snapshot", "--app", &app])?;
        let button = elements(&snapshot["data"])?
            .iter()
            .find(|e| e["role"] == "Button")
            .ok_or_else(|| io("second focus target missing"))?;
        c.expected(
            &[
                "press",
                "enter",
                "--target",
                button["ref"]
                    .as_str()
                    .ok_or_else(|| io("button ref missing"))?,
                "--snapshot",
                snapshot["snapshot_id"]
                    .as_str()
                    .ok_or_else(|| io("snapshot missing"))?,
                "--app",
                &app,
            ],
            "NOT_ACTIONABLE",
        )?;
        c.expected(
            &[
                "type",
                "role:Document",
                "",
                "--verify-value",
                "incorrect",
                "--app",
                &app,
            ],
            "ASSERTION_FAILED",
        )?;
        let snapshot = c.raw(&["snapshot", "--app", &app])?;
        let doc = elements(&snapshot["data"])?
            .iter()
            .find(|e| e["role"] == "Document")
            .ok_or_else(|| io("document missing"))?;
        let reference = doc["ref"].as_str().ok_or_else(|| io("ref missing"))?;
        let sid = snapshot["snapshot_id"]
            .as_str()
            .ok_or_else(|| io("snapshot missing"))?;
        let file = actl_core::state::SignalPaths::default()
            .dir
            .join("snapshots")
            .join(format!("{sid}.json"));
        let mut record = crate::capture::read_json(&file)?;
        let index = reference
            .trim_start_matches("@e")
            .parse::<usize>()
            .map_err(io)?
            .checked_sub(1)
            .ok_or_else(|| io("bad ref"))?;
        require(
            record["elements"]
                .as_array()
                .is_some_and(|v| index < v.len()),
            "snapshot record mismatch",
        )?;
        record["elements"][index]["runtime_id"] = json!([-2147483647, 123456789]);
        crate::save(&file, &record)?;
        c.expected(
            &["get", reference, "--snapshot", sid, "--app", &app],
            "STALE_REF",
        )?;
        c.call(&["verify", "--value", "role:Document", content, "--app", &app])?;
    } else {
        // Save-As remains a distinct assertion, with an owned process and exact filename field.
        let dest = c.dir.join(format!("{marker}-saved.txt"));
        c.call(&[
            "press",
            "ctrl+shift+s",
            "--target",
            "role:Document",
            "--app",
            &app,
        ])?;
        c.call(&["wait", "--window", "另存为", "--timeout", "8000"])?;
        let own = owned.as_ref().ok_or_else(|| io("owner missing"))?;
        let dialogs: Vec<_> = windows(c)?
            .into_iter()
            .filter(|w| {
                w["pid"] == own["pid"]
                    && w["title"] == "另存为"
                    && w["owner_title"]
                        .as_str()
                        .is_some_and(|t| t.contains(&marker))
            })
            .collect();
        require(dialogs.len() == 1, "save dialog ownership unclear")?;
        let dialog = format!("hwnd:{}", dialogs[0]["hwnd"]);
        c.call(&[
            "set-value",
            "id:1001",
            &dest.to_string_lossy(),
            "--app",
            &dialog,
        ])?;
        c.call(&["press", "enter", "--target", "id:1001", "--app", &dialog])?;
        c.call(&[
            "verify-file",
            "--path",
            &dest.to_string_lossy(),
            "--kind",
            "text",
            "--expected",
            &json!(content).to_string(),
            "--normalize-lines",
        ])?;
    }
    Ok(())
}
