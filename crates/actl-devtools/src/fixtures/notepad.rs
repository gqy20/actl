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
    // Store 版记事本无 system32\notepad.exe;按执行别名 → system32 → 裸名解析
    // (与 timing fixture 的 notepad_program 同族,2026-09-30 实录)。
    let notepad = if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let alias = std::path::PathBuf::from(&local)
            .join("Microsoft")
            .join("WindowsApps")
            .join("notepad.exe");
        if alias.is_file() {
            alias.to_string_lossy().into_owned()
        } else {
            "notepad.exe".into()
        }
    } else {
        "notepad.exe".into()
    };
    c.call(&["launch", &notepad, &file.to_string_lossy()])?;
    c.call(&["wait", "--window", &marker, "--timeout", "12000"])?;
    let own = owner(c, &marker, None, None, None)?;
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
    if case == "composite-locate" {
        return composite_locate(c, &app, &marker);
    }
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
        // verify-file --kind text 内置 CRLF→LF(docs/06 §782),无需旗标。
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
        // 对话框内 id:1001 同时命中地址 ToolBar 与文件名 Edit——快照后按
        // role:Edit 锁定输入框,用 ref 定位(AGENTS:跨调用用 ref 须绑定 snapshot)。
        // 文件名走键盘通道:set-value 写 Edit 的 ValuePattern 不更新 ComboBox
        // 内部提交值,Enter 会按默认旧名保存并弹「确认另存为」(2026-10-01 实证);
        // 先清空再 type,回读校验与 Enter 保存均正确。
        let filename_edit = |snap: &Value| -> Result<String> {
            let edit = elements(&snap["data"])?
                .iter()
                .find(|e| {
                    e["role"] == "Edit" && e["name"].as_str().is_some_and(|n| n.contains("文件名"))
                })
                .ok_or_else(|| io("filename edit missing"))?;
            edit["ref"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| io("filename ref missing"))
        };
        // 文件名录入三步(清空 → 键盘输入 → 回车确认)及其**前置快照**都可能
        // 撞上对话框提供方忙窗(UIA_E_TIMEOUT;2026-10-01 十轮统计实证三步
        // 与快照各中过)。错误契约本身指示"重取快照后重试"——阶段化推进:
        // 哪步失败就从哪步续,续前重取快照重新观察;注入前观测失败无已注入
        // 内容,不存在双写。对话框已消失(NOT_FOUND)时录入结果交由随后的
        // verify-file 裁决(Enter 可能已生效),不盲判失败。
        let dest_text = dest.to_string_lossy().into_owned();
        let transient = |v: &Value| {
            v["error"]["code"] == "STALE_REF"
                && v["error"]["evidence"]["reason"] == "observation_failed"
        };
        #[derive(PartialEq)]
        enum Stage {
            Clear,
            Type,
            Enter,
        }
        enum Observed {
            Ready((String, String)),
            Gone,
            Busy,
        }
        let observe = |c: &mut Context| -> Result<Observed> {
            let snap = c.raw(&["snapshot", "--app", &dialog])?;
            if snap["ok"] == true {
                let snap_id = snap["snapshot_id"]
                    .as_str()
                    .ok_or_else(|| io("snapshot missing"))?
                    .to_owned();
                let edit_ref = filename_edit(&snap)?;
                return Ok(Observed::Ready((edit_ref, snap_id)));
            }
            if snap["error"]["code"] == "NOT_FOUND" {
                return Ok(Observed::Gone);
            }
            if transient(&snap) {
                return Ok(Observed::Busy);
            }
            let code: actl_core::ErrorCode = serde_json::from_value(snap["error"]["code"].clone())
                .map_err(|_| io("unknown CLI error code"))?;
            Err(actl_core::CtlError::with_evidence(
                code,
                format!(
                    "snapshot: {}",
                    snap["error"]["message"]
                        .as_str()
                        .unwrap_or("dialog observation failed")
                ),
                snap["error"]["evidence"].clone(),
            ))
        };
        let mut stage = Stage::Clear;
        let mut failure: Option<Value> = None;
        let mut hard_fail: Option<String> = None;
        let mut entered = false;
        // 总预算 8:忙窗可先后消耗多个阶段与快照;瞬态失败后沉降 800ms 再重试。
        for _ in 0..8 {
            let (edit_ref, snap_id) = match observe(c)? {
                Observed::Ready(pair) => pair,
                Observed::Busy => {
                    std::thread::sleep(std::time::Duration::from_millis(800));
                    continue;
                }
                Observed::Gone => break, // 对话框已消失:结果由 verify-file 裁决
            };
            let outcome = match stage {
                Stage::Clear => c.raw(&[
                    "set-value",
                    &edit_ref,
                    "",
                    "--snapshot",
                    &snap_id,
                    "--app",
                    &dialog,
                ])?,
                Stage::Type => c.raw(&[
                    "type",
                    &edit_ref,
                    &dest_text,
                    "--snapshot",
                    &snap_id,
                    "--app",
                    &dialog,
                ])?,
                Stage::Enter => c.raw(&[
                    "press",
                    "enter",
                    "--target",
                    &edit_ref,
                    "--snapshot",
                    &snap_id,
                    "--app",
                    &dialog,
                ])?,
            };
            if outcome["ok"] == true {
                stage = match stage {
                    Stage::Clear => Stage::Type,
                    Stage::Type => Stage::Enter,
                    Stage::Enter => {
                        entered = true;
                        break;
                    }
                };
                continue;
            }
            let is_transient = transient(&outcome);
            failure = Some(outcome);
            if !is_transient {
                let step = match stage {
                    Stage::Clear => "set-value",
                    Stage::Type => "type",
                    Stage::Enter => "press",
                };
                hard_fail = Some(step.into());
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(800));
        }
        if let Some(step) = hard_fail {
            c.uncertain = true;
            let error = failure.ok_or_else(|| io("filename entry result missing"))?;
            return Err(actl_core::CtlError::with_evidence(
                actl_core::ErrorCode::StaleRef,
                format!(
                    "{step}: {}",
                    error["error"]["message"]
                        .as_str()
                        .unwrap_or("filename entry failed")
                ),
                error["error"]["evidence"].clone(),
            ));
        }
        let _ = entered; // entered 或 Gone-ambiguous 均由 verify-file 终裁
        c.call(&[
            "verify-file",
            "--path",
            &dest.to_string_lossy(),
            "--kind",
            "text",
            "--expected",
            &json!(content).to_string(),
            // verify-file --kind text 内置 CRLF→LF(docs/06 §782),无需旗标。
        ])?;
    }
    Ok(())
}

/// UIA 组合定位真机验收(docs/ROADMAP P0):五个验收点一次跑完——
/// ① 复合选择器唯一化(role 限定 Document);② 同名/同 id 异类消歧(另存为
/// 对话框 id:1001 同时命中地址 ToolBar 与文件名 Edit,role+name 复合唯一,
/// 裸 id 期望 AMBIGUOUS);③ 作用域(descendants 找得到嵌套按钮、children
/// 只看直接子级);④ 控件重建后重定位(对话框开关=子树重建,复合选择器
/// 重新命中);⑤ 流程暂停恢复后重新定位(--paused 起跑 + resume,冻结计划
/// 的 uia 选择器在恢复后重新解析)。
fn composite_locate(c: &mut Context, app: &str, marker: &str) -> Result<()> {
    // ① role 限定的复合定位:主窗 Document 唯一
    let found = c.call(&[
        "find",
        "--target",
        r#"uia:{"match":{"role":"Document"}}"#,
        "--app",
        app,
    ])?;
    let hits = elements(&found)?;
    require(
        hits.len() == 1 && hits[0]["role"] == "Document",
        "composite role match not unique",
    )?;

    // ② 打开另存为(树新增对话框子树),同名异类消歧
    c.call(&[
        "press",
        "ctrl+shift+s",
        "--target",
        "role:Document",
        "--app",
        app,
    ])?;
    c.call(&["wait", "--window", "另存为", "--timeout", "8000"])?;
    let pid = owned_pid(c, app)?;
    let own = windows(c)?
        .into_iter()
        .find(|w| w["title"] == "另存为" && w["pid"] == pid)
        .ok_or_else(|| io("save dialog missing"))?;
    let dialog = format!("hwnd:{}", own["hwnd"]);
    c.expected(
        &[
            "find",
            "--target",
            r#"uia:{"match":{"id":"1001"}}"#,
            "--app",
            &dialog,
        ],
        "AMBIGUOUS",
    )?;
    // role+name 复合唯一
    let edit = c.call(&[
        "find",
        "--target",
        r#"uia:{"match":{"role":"Edit","name_contains":"文件名"}}"#,
        "--app",
        &dialog,
    ])?;
    let edit_hits = elements(&edit)?;
    require(
        edit_hits.len() == 1 && edit_hits[0]["role"] == "Edit",
        "role+name not unique",
    )?;

    // ③ 作用域:深嵌的「文件名」Edit(descendants 唯一命中)在 children
    //    (直接子级)不存在——保存按钮是 depth 1 直接子级(实测),不能当靶。
    let deep = c.call(&[
        "find",
        "--target",
        r#"uia:{"match":{"role":"Edit","name_contains":"文件名"},"scope":"descendants"}"#,
        "--app",
        &dialog,
    ])?;
    require(
        !elements(&deep)?.is_empty(),
        "descendants scope missed nested edit",
    )?;
    c.expected(
        &[
            "find",
            "--target",
            r#"uia:{"match":{"role":"Edit","name_contains":"文件名"},"scope":"children"}"#,
            "--app",
            &dialog,
        ],
        "NOT_FOUND",
    )?;

    // ④ 控件重建:关掉对话框(子树消失)→ 重开(重建)→ 复合选择器再次唯一命中
    c.call(&["press", "esc", "--app", &dialog])?;
    c.call(&["verify-window", "--app", "另存为", "--state", "absent"])?;
    c.call(&[
        "press",
        "ctrl+shift+s",
        "--target",
        "role:Document",
        "--app",
        app,
    ])?;
    c.call(&["wait", "--window", "另存为", "--timeout", "8000"])?;
    let pid = owned_pid(c, app)?;
    let own2 = windows(c)?
        .into_iter()
        .find(|w| w["title"] == "另存为" && w["pid"] == pid)
        .ok_or_else(|| io("rebuilt dialog missing"))?;
    let dialog2 = format!("hwnd:{}", own2["hwnd"]);
    let again = c.call(&[
        "find",
        "--target",
        r#"uia:{"match":{"role":"Edit","name_contains":"文件名"}}"#,
        "--app",
        &dialog2,
    ])?;
    require(
        elements(&again)?.len() == 1,
        "relocation after rebuild failed",
    )?;
    c.call(&["press", "esc", "--app", &dialog2])?;
    c.call(&["verify-window", "--app", "另存为", "--state", "absent"])?;

    // ⑤ 流程暂停恢复后重新定位:--paused 冻结计划,resume 后 uia 选择器重新解析
    let flow = c.dir.join("composite-locate.yml");
    fs::write(
        &flow,
        format!(
            "version: 1\nname: composite relocation\nsteps:\n  - id: locate\n    title: 复合定位文档\n    run: {{command: verify, args: [--exists, 'uia:{{\"match\":{{\"role\":\"Document\"}}}}', --app, {app}]}}\n"
        ),
    )
    .map_err(io)?;
    let flow_path = flow.to_string_lossy().into_owned();
    c.call(&["flow-run", "--file", &flow_path, "--id", marker, "--paused"])?;
    let resumed = c.call(&["flow-resume", "--id", marker])?;
    require(
        resumed["run"]["status"] == "completed" && resumed["run"]["steps"][0]["attempts"] == 1,
        "paused resume relocation failed",
    )?;
    Ok(())
}
/// 从 --app hwnd:N 解析属主 PID(对话框归属筛选用)。
fn owned_pid(c: &mut Context, app: &str) -> Result<Value> {
    let hwnd = app
        .strip_prefix("hwnd:")
        .ok_or_else(|| io("app must be hwnd for pid lookup"))?;
    let list = c.call(&["list-windows", "--native", "--hwnd", hwnd])?;
    Ok(list["windows"][0]["pid"].clone())
}
