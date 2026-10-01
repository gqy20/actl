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
    // UWP(计算器):同标题存在框架窗/应用 CoreWindow/标题栏窗多个顶层窗,
    // 归属唯一性限定 ApplicationFrameWindow(可操作的真顶层窗);普通 Win32
    // 应用(msinfo32/explorer)无此问题,不过滤。
    let own = owner(
        c,
        &marker,
        None,
        None,
        (case == "golden-task-1").then_some("ApplicationFrameWindow"),
    )?;
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
    // 旧的地址栏核验(role:ToolBar name 含"地址: <路径>")在本机构建已消失:
    // Win11 资源管理器面包屑是自定义控件,UIA 只暴露 Edit"地址栏"(value 恒空,
    // 2026-10-01 实测)。文件夹归属由窗口标题唯一 marker(owner 已核)+ 下方
    // find 走 item-container 找到实际文件承载,不再依赖面包屑控件。
    let _ = &token;
    *owned = Some(own);
    if case == "golden-task-1" {
        let mut rounds = vec![];
        // UWP 挂起恢复竞态:wait --window 只核标题,内容区可能未渲染——
        // 元素级就绪等待兜底(2026-10-01 跨日十轮第 4 轮失败实证:窗口标题
        // 在但 clearButton 不在,等 8s 后出现;上午同夹具 10/10 过,间歇 ~30%)。
        c.call(&[
            "wait",
            "--element",
            "id:clearButton",
            "--timeout",
            "15000",
            "--app",
            &app,
        ])?;
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
