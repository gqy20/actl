use super::*;
pub(super) fn run(
    c: &mut Context,
    case: &str,
    args: &Args,
    owned: &mut Option<Value>,
    report: &mut Value,
) -> Result<()> {
    let initial = windows(c)?;
    let windir = std::env::var("WINDIR").map_err(io)?;
    let (program, marker, token) = match case {
        "golden-task-1" => {
            require(
                !initial
                    .iter()
                    .any(|w| w["title"] == "计算器" || w["title"] == "Calculator"),
                "existing Calculator; untouched",
            )?;
            ("calc.exe".to_owned(), "计算器".to_owned(), None)
        }
        "win32-table-extract" => {
            require(
                !initial.iter().any(|w| w["title"] == "系统信息"),
                "existing System Information; untouched",
            )?;
            (
                format!("{windir}/System32/msinfo32.exe"),
                "系统信息".into(),
                None,
            )
        }
        _ => {
            let marker = crate::id("vlist");
            let dir = c.dir.join(&marker);
            fs::create_dir(&dir).map_err(io)?;
            for i in 1..=60 {
                fs::write(dir.join(format!("f{i:02}_padding.txt")), format!("pad {i}"))
                    .map_err(io)?;
            }
            (format!("{windir}/explorer.exe"), marker, Some(dir))
        }
    };
    let mut call = vec!["launch".to_owned(), program.clone()];
    if case == "golden-task-1" {
        call[1] = format!("{windir}/explorer.exe");
        call.push("shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into());
    }
    if let Some(dir) = &token {
        call.push(format!("/n,{}", dir.display()));
    }
    let refs: Vec<_> = call.iter().map(String::as_str).collect();
    let launch = c.call(&refs)?;
    c.call(&["wait", "--window", &marker, "--timeout", "20000"])?;
    let own = owner(c, &marker, None, None)?;
    require(
        !initial.iter().any(|w| w["hwnd"] == own["hwnd"]),
        "existing HWND; ownership unproven",
    )?;
    if case == "win32-table-extract" {
        require(
            own["pid"] == launch["pid"],
            "process launch identity differs",
        )?;
    }
    if case == "golden-task-1" {
        // UWP 进程模型:窗口属 ApplicationFrameHost(框架),实际计算器逻辑在
        // CalculatorApp 子进程。归属核验放宽为两条路径任一:WindowsApps 计算器
        // 包路径,或 UWP 框架宿主。HWND 不在初始列表(第 57 行)已保证窗口新建。
        require(
            own["program"].as_str().is_some_and(|p| {
                let lower = p.to_ascii_lowercase();
                (lower.contains("windowscalculator") && lower.ends_with("calculatorapp.exe"))
                    || lower.contains("applicationframehost")
            }),
            "unexpected calculator process",
        )?;
    }
    let app = format!("hwnd:{}", own["hwnd"]);
    if let Some(path) = &token {
        c.call(&[
            "verify",
            "--value",
            "uia:{\"match\":{\"role\":\"ToolBar\",\"name_contains\":\"地址:\"}}",
            &format!("地址: {}", path.display()),
            "--property",
            "name",
            "--app",
            &app,
        ])?;
    }
    *owned = Some(own);
    if case == "golden-task-1" {
        let mut rounds = vec![];
        for round in 0..args.number("runs", 3, 1, 100)? {
            let start = Instant::now();
            c.call(&["click", "id:clearButton", "--app", &app])?;
            for button in [
                "num1Button",
                "num2Button",
                "num3Button",
                "multiplyButton",
                "num4Button",
                "num5Button",
                "num6Button",
                "plusButton",
                "num7Button",
                "num8Button",
                "num9Button",
                "equalButton",
            ] {
                c.call(&["click", &format!("id:{button}"), "--app", &app])?;
            }
            c.call(&[
                "verify",
                "--value",
                "id:CalculatorResults",
                "显示为 56,877",
                "--property",
                "name",
                "--app",
                &app,
            ])?;
            rounds.push(
                json!({"round":round+1,"elapsed_ms":start.elapsed().as_millis(),"passed":true}),
            );
        }
        report["rounds"] = json!(rounds);
    } else if case == "win32-table-extract" {
        c.call(&[
            "wait",
            "--element",
            "role:List",
            "--app",
            &app,
            "--timeout",
            "10000",
        ])?;
        checks::summary_table(&c.call(&["extract", "role:List", "--app", &app])?)?;
    } else {
        let target = format!("zzz_late_{}.txt", crate::id("item"));
        fs::write(
            token
                .ok_or_else(|| io("owned directory missing"))?
                .join(&target),
            "late content",
        )
        .map_err(io)?;
        let found = c.call(&["find", "--name", &target, "--app", &app])?;
        require(
            found["resolved_by"] == "item-container"
                && elements(&found)?
                    .iter()
                    .filter(|e| e["name"] == target)
                    .count()
                    == 1,
            "virtualized item-container route not exercised",
        )?;
        c.call(&[
            "get",
            &format!("name:{target}"),
            "--property",
            "name",
            "--app",
            &app,
        ])?;
        c.call(&[
            "verify",
            "--value",
            &format!("name:{target}"),
            &target,
            "--property",
            "name",
            "--app",
            &app,
        ])?;
    }
    Ok(())
}
