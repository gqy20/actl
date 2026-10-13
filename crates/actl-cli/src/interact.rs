//! interact —— 操作类命令执行体:click/type/press/key-down/up/set-value、
//! 物理指针族(right-click/double-click/hover/drag/scroll)、窗口操作
//! (focus/close/resize)与 launch。协议护栏(replay_guard/require_app/parse_expect)
//! 也在此:注入前的 fail-closed 门。观察/编排类见 `query`。

use actl_core::timing;
use actl_core::{CtlError, ErrorCode};
use serde_json::json;

use crate::CommandOutput;

#[allow(clippy::too_many_arguments)] // 参数面与 clap 命令定义一一对应
pub(crate) fn click(
    target_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    physical: bool,
    with: &[String],
    no_activate: bool,
    expect: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let _replay = replay_guard(snapshot, app)?;
    // --expect = window: 新窗口断言(动作前取 RuntimeId 基线,与 press 相同语义)
    //          | element: 后置条件断言(无基线需求,见 Expect 注释)
    let expect_parsed = match expect {
        Some(raw) => Some(parse_expect(raw)?),
        None => None,
    };
    let baseline = if matches!(expect_parsed, Some(Expect::Window(_))) {
        actl_uia::window_identities()?
    } else {
        Vec::new()
    };

    let data = if let actl_core::target::Target::Coord(x, y)
    | actl_core::target::Target::CoordRel(x, y) = target
    {
        // xy:/xyw: 坐标目标:无元素可校验 → ①物理授权必须显式 --physical;
        // ②强制 --app(窗口边界守卫的锚,防窗口漂移后点进窗外);
        // xyw: 的客户区原点换算归执行侧(2026-10-13 Fix B,agent 零换算)
        if !physical {
            return Err(CtlError::protocol(
                "xy:/xyw: targets are physical-only (no UIA element to verify); add --physical",
            ));
        }
        let app = app.ok_or_else(|| {
            CtlError::protocol(format!(
                "{} requires --app <title-substr>: the coordinate bounds guard needs a window anchor (fail-closed)",
                target.describe()
            ))
        })?;
        let action = if with.is_empty() {
            actl_uia::PointerAction::LeftClick
        } else {
            actl_uia::PointerAction::ClickWith {
                modifiers: with.to_vec(),
            }
        };
        let relative = matches!(target, actl_core::target::Target::CoordRel(..));
        let out = if relative {
            actl_uia::pointer_at_window(app, x, y, action, no_activate)?
        } else {
            actl_uia::pointer_at_physical(app, x, y, action, no_activate)?
        };
        let mut entry = json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": if relative { "coord-window-relative" } else { "coord" },
            "pattern": if with.is_empty() { "physical-left" } else { "physical-modified" },
            "modifiers": with,
            "point": [out.point.0, out.point.1],
            "guard": "window-bounds",
            "input_effects": { "keyboard": if with.is_empty() { "none" } else { "injected" }, "clipboard": "untouched", "pointer": "move_and_click", "activation": if no_activate { "suppressed" } else { "normal" } },
            "window": out.window_title,
        });
        if let Some((ox, oy)) = out.origin {
            entry["origin"] = json!([ox, oy]);
        }
        entry
    } else if physical {
        let action = if with.is_empty() {
            actl_uia::PointerAction::LeftClick
        } else {
            actl_uia::PointerAction::ClickWith {
                modifiers: with.to_vec(),
            }
        };
        let (loc, (px, py), scrolled) =
            actl_uia::pointer_physical(app, &target, near, action, no_activate)?;
        json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": loc.resolved_by,
            "pattern": if with.is_empty() { "physical-left" } else { "physical-modified" },
            "modifiers": with,
            "point": [px, py],
            "scrolled_into_view": scrolled,
            "input_effects": { "keyboard": if with.is_empty() { "none" } else { "injected" }, "clipboard": "untouched", "pointer": "move_and_click", "activation": if no_activate { "suppressed" } else { "normal" } },
            "resolved": { "role": loc.role, "name": loc.name, "automation_id": loc.automation_id },
            "window": loc.window_title,
        })
    } else {
        let (info, pattern) = actl_uia::click(app, &target, near)?;
        json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": info.resolved_by,
            "pattern": pattern,
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
            "resolved": {
                "role": info.role,
                "name": info.name,
                "automation_id": info.automation_id,
            },
            "window": info.window_title,
        })
    };

    // 06 §4:--expect = 执行后隐式 wait(默认 2s),失败 ASSERTION_FAILED
    if let Some(parsed) = expect_parsed {
        match parsed {
            Expect::Window(substr) => {
                actl_uia::wait_new_window(
                    &substr,
                    timeout.unwrap_or(timing::EXPECT_MS),
                    &baseline,
                )?;
            }
            Expect::Element(target) => {
                expect_element(app, &target, timeout.unwrap_or(timing::EXPECT_MS))?;
            }
        }
    }
    Ok(data.into())
}

/// focus(2026-10-11 spec):UIA SetFocus 建立元素键盘焦点+读回核验。
/// 语义动作(零键注入):与 press --target 的"只断言不建立"互补。
pub(crate) fn focus(
    target_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let _replay = replay_guard(snapshot, app)?;
    let info = actl_uia::focus_element(app, &target, near)?;
    Ok(json!({
        "action": "focus",
        "target": target.describe(),
        "resolved_by": info.resolved_by,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    })
    .into())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn type_cmd(
    target_raw: &str,
    text: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    paste: bool,
    verify_value: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "type")?;
    let _replay = replay_guard(snapshot, Some(app))?;
    let target = actl_core::parse_target(target_raw)?;
    let res = actl_uia::type_text(Some(app), &target, text, paste, near, verify_value)?;
    Ok(type_output(&target, &res, paste, verify_value.is_some()).into())
}

fn type_output(
    target: &actl_core::Target,
    res: &actl_uia::TypeOutcome,
    paste: bool,
    verified: bool,
) -> serde_json::Value {
    let keyboard = if res.stopped_early {
        "partial"
    } else {
        "injected"
    };
    json!({
        "action": "type",
        "target": target.describe(),
        "chars": res.total,
        "delivered": res.delivered,
        "stopped_early": res.stopped_early,
        "value_verified": verified.then_some(true),
        "focus_verified": true,
        "via": if paste { "clipboard-paste" } else { "keyboard" },
        "input_effects": {
            "keyboard": keyboard,
            "clipboard": if paste { "used" } else { "untouched" },
            "pointer": "none",
        },
        "resolved": { "role": res.located.role, "name": res.located.name },
    })
}

pub(crate) fn press(
    keys: &str,
    app: Option<&str>,
    expect: Option<&str>,
    timeout: Option<u64>,
    target_raw: Option<&str>,
    snapshot: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "press")?;
    let _replay = replay_guard(snapshot, Some(app))?;
    let target = target_raw.map(actl_core::parse_target).transpose()?;
    let spec = actl_core::parse_key_expr(keys)?;
    // --expect = window: "本动作导致新窗口"(注入前先取窗口基线,断言时排除
    // 既有同名窗口——存在性检查会被预先打开的同名窗口恒真欺骗,doc 09 §2)
    //          | element: 后置条件断言(见 Expect 注释)
    let expect_parsed = match expect {
        Some(raw) => Some(parse_expect(raw)?),
        None => None,
    };
    let baseline = if matches!(expect_parsed, Some(Expect::Window(_))) {
        actl_uia::window_identities()?
    } else {
        Vec::new()
    };
    // 键盘作用于全局焦点:裸 SendInput 会打进当下前台(实测坑)。
    // press_keys 内部在 --app 时先置前台并稳定窗(--app 语义,focus+delay 在 uia 层)。
    let focused = actl_uia::press_keys_targeted(&spec, Some(app), target.as_ref())?;
    let mut expect_out = None;
    if let Some(parsed) = expect_parsed {
        // 06 §4:--expect = 执行后隐式 wait(默认 2s),失败 ASSERTION_FAILED
        match parsed {
            Expect::Window(substr) => {
                actl_uia::wait_new_window(
                    &substr,
                    timeout.unwrap_or(timing::EXPECT_MS),
                    &baseline,
                )?;
                expect_out = Some(format!("window:{substr}"));
            }
            Expect::Element(t) => {
                expect_element(Some(app), &t, timeout.unwrap_or(timing::EXPECT_MS))?;
                expect_out = Some(format!("element:{}", t.describe()));
            }
        }
    }
    Ok(json!({
        "action": "press",
        "keys": keys,
        "focused": focused,
        "focus_verified": target.is_some(),
        "expect": expect_out,
        "input_effects": {
            "keyboard": "injected",
            "clipboard": "untouched",
            "pointer": "none",
        },
    })
    .into())
}

/// --snapshot <id> 回放校验:仅对 @eN 目标有意义;窗口实例与快照记录
/// 不一致 → STALE_REF(同标题 ≠ 同实例,重开窗口后计数重放会点错)。
pub(crate) fn replay_guard(
    snapshot: Option<&str>,
    app: Option<&str>,
) -> Result<Option<actl_uia::ReplayGuard>, CtlError> {
    let Some(id) = snapshot else { return Ok(None) };
    let Some(pattern) = app else {
        return Err(CtlError::protocol(
            "--snapshot requires --app (the window to freshness-check)",
        ));
    };
    actl_uia::ReplayGuard::enter(id, pattern).map(Some)
}

/// 键盘注入命令的 fail-closed 门:必须显式 --app 绑定目标窗口。
/// 裸注入打进"当下恰好前台"的窗口是事故 #1 的路径之一;护栏随 --app 生效。
fn require_app<'a>(app: Option<&'a str>, cmd: &str) -> Result<&'a str, CtlError> {
    app.filter(|a| !a.trim().is_empty()).ok_or_else(|| {
        CtlError::protocol(format!(
            "{cmd} requires --app <title-substr>: keyboard injection is global and \
                 unguarded without a pinned target window (fail-closed)"
        ))
    })
}

pub(crate) fn set_value_cmd(
    target_raw: &str,
    value: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    verify_value: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let _replay = replay_guard(snapshot, app)?;
    let res = actl_uia::set_value(app, &target, value, near, verify_value)?;
    Ok(json!({
        "action": "set-value",
        "target": target.describe(),
        "value_verified": verify_value.is_some().then_some(true),
        "resolved": { "role": res.role, "name": res.name },
        // 宿主容器信号(docs/83 Phase 2):目标是 ComboBox 子 Edit 时直写
        // 不一定被宿主采纳——值未生效先 press tab(焦点离开提交)或改 select。
        "host_control": res.host_control,
    })
    .into())
}

/// --expect 谓词 v2:`window:<substr>` | `element:<target>`。
/// - window: = 本动作导致**新窗口**出现(基线排除既有同名,防存在性恒真);
/// - element: = 动作后该元素**可解析**的后置条件(复用 wait 元素文法,
///   隐式 wait 默认 2s)。不要求"新出现"——已可解析即后置条件成立,
///   与 window: 的"新出现"语义刻意不对称:mmui 类应用 UIA 假成功的
///   验证点是**物化**(切会话后消息区才出现),不是窗口变更。
enum Expect {
    Window(String),
    Element(actl_core::Target),
}

fn parse_expect(raw: &str) -> Result<Expect, CtlError> {
    if let Some(substr) = raw.strip_prefix("window:").filter(|s| !s.is_empty()) {
        return Ok(Expect::Window(substr.into()));
    }
    if let Some(target_raw) = raw.strip_prefix("element:").filter(|s| !s.is_empty()) {
        return Ok(Expect::Element(actl_core::parse_target(target_raw)?));
    }
    Err(CtlError::protocol(format!(
        "invalid --expect {raw:?}: expected window:<substr> or element:<target>"
    )))
}

/// element: 后置条件检查:失败语义是断言(ASSERTION_FAILED)而非等待超时
/// (TIMEOUT)——底层原因随 evidence 保留,供 agent 区分"UIA 假成功
/// (点击无效,元素从未物化)"与"目标本就错误"。
fn expect_element(
    app: Option<&str>,
    target: &actl_core::Target,
    timeout_ms: u64,
) -> Result<(), CtlError> {
    actl_uia::wait_element(app, target, timeout_ms).map_err(|cause| {
        CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            format!(
                "expected element {} did not resolve after action (waited {timeout_ms}ms)",
                target.describe()
            ),
            serde_json::json!({
                "expect": "element",
                "target": target.describe(),
                "cause": { "code": cause.code, "message": cause.message },
            }),
        )
    })
}

pub(crate) fn launch(
    args: &[String],
    wait_window: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let Some(program) = args.first() else {
        return Err(CtlError::protocol("launch requires a program name"));
    };
    // trailing_var_arg 语义:程序名之后的一切 token 都原样转发给子进程——
    // 选项写在程序名后会静默失效且子进程收到垃圾参数(真机实证 2026-10-05:
    // `launch notepad.exe --wait-window X` 使记事本把选项当文件名打开)。
    // fail-fast 指引选项前置,不静默吞;只拦本命令的独有选项 --wait-window
    // (--timeout 是常见子进程参数,不拦)。
    if args[1..].iter().any(|a| a == "--wait-window") {
        return Err(CtlError::protocol(
            "launch options must precede the program name; tokens after the program are \
             forwarded to the child verbatim (write `launch --wait-window <title> <program> ...`)",
        ));
    }
    // 解析优先级:app-registry 别名 → App Paths 注册表 → 原样(继续走 PATH)
    let program = actl_uia::app_registry::resolve(program)
        .ok()
        .flatten()
        .unwrap_or_else(|| program.clone());
    let candidate = actl_uia::resolve_app_path(&program).unwrap_or_else(|| program.clone());
    let direct = std::path::PathBuf::from(&candidate);
    let resolved_path = if direct.is_file() {
        Some(direct)
    } else {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .flat_map(|dir| [dir.join(&candidate), dir.join(format!("{candidate}.exe"))])
                .find(|p| p.is_file())
        })
    };
    let program = resolved_path
        .ok_or_else(|| {
            CtlError::new(
                ErrorCode::NotFound,
                format!("cannot find launch executable {program:?}"),
            )
        })?
        .to_string_lossy()
        .into_owned();
    let mut action = actl_uia::display_gate::prepare_action("launch", &program)?;
    action.executing();
    // App Paths 解析:裸程序名(Office/商店别名)不在 PATH 时查注册表取完整路径
    // (通用增强,docs/14 实测痛点:launch WINWORD 曾 NotFound)
    // GUI 程序不得继承控制台句柄:null 三件套只换子进程自身 stdio,挡不住
    // 句柄表继承——派生时长命子进程(记事本/Word)仍攥住调用方管道写端,
    // flow-run|head/$()/ReadToEnd 全部等到 GUI 退出才 EOF(2026-10-12
    // handoff-02 实证:引擎 4.4s 结束,管道吊 9 分钟到 notepad 关闭)。
    // 浏览器通道/record-worker 同坑先例(docs/72 process_io):派生前清
    // 自身 stdio 的 INHERIT 标志。
    actl_uia::process_io::clear_stdio_inheritance();
    let detached = |cmd: &mut std::process::Command| {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    };
    let mut first = std::process::Command::new(&program);
    detached(&mut first);
    let resolved = first
        .args(&args[1..])
        .spawn()
        .map(|child| (child, program.clone()))
        .or_else(|first_err| match actl_uia::resolve_app_path(&program) {
            Some(full) if full != program => {
                let full2 = full.clone();
                let mut second = std::process::Command::new(&full);
                detached(&mut second);
                second
                    .args(&args[1..])
                    .spawn()
                    .map(move |child| (child, full2))
                    .map_err(|e| {
                        CtlError::new(
                            ErrorCode::NotFound,
                            format!("cannot launch {program:?} (resolved {full:?}): {e}"),
                        )
                    })
            }
            _ => Err(CtlError::new(
                ErrorCode::NotFound,
                format!("cannot launch {program:?}: {first_err}"),
            )),
        })?;
    let (child, program) = resolved;
    action.delivered();
    let mut result = json!({ "pid": child.id(), "program": program, "window_ready": false });
    if let Some(pattern) = wait_window {
        match actl_uia::wait_window_ready(pattern, timeout.unwrap_or(timing::WAIT_WINDOW_MS)) {
            Ok(window) => {
                result["window_ready"] = true.into();
                result["window"] = window;
            }
            Err(mut error) => {
                let mut evidence = error.evidence.take().unwrap_or_else(|| json!({}));
                evidence["spawned_pid"] = child.id().into();
                evidence["process_started"] = true.into();
                evidence["relaunch"] = false.into();
                error.evidence = Some(evidence);
                return Err(error);
            }
        }
    }
    Ok(result.into())
}

// ─── 物理指针族(M2 第二批) ────────────────────────────────────────────────

pub(crate) enum PointerKind {
    Right,
    Double,
    Hover,
    Middle,
    Triple,
}

pub(crate) fn pointer_cmd(
    name: &str,
    target_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    no_activate: bool,
    kind: PointerKind,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    // --snapshot 与 click 同机制:ref 重放的新鲜度核验(replay 守卫为
    // 线程环境,locate 链内生效)。交互轮实证:ref 漂移时右键想绑快照
    // 被拒(--snapshot 缺席),指针族补齐与 click 的参数对称。
    let _replay = replay_guard(snapshot, app)?;
    let action = match kind {
        PointerKind::Right => actl_uia::PointerAction::RightClick,
        PointerKind::Double => actl_uia::PointerAction::DoubleClick,
        PointerKind::Hover => actl_uia::PointerAction::Hover,
        PointerKind::Middle => actl_uia::PointerAction::MiddleClick,
        PointerKind::Triple => actl_uia::PointerAction::TripleClick,
    };
    let effects = match kind {
        PointerKind::Hover => "move",
        _ => "move_and_click",
    };
    if let actl_core::target::Target::Coord(x, y) | actl_core::target::Target::CoordRel(x, y) =
        target
    {
        // xy:/xyw: 坐标目标:本族命名即物理授权,但 --app 必须显式(边界守卫的锚);
        // xyw: 原点换算归执行侧(Fix B)
        let app = app.ok_or_else(|| {
            CtlError::protocol(format!(
                "{name} {} requires --app <title-substr>: the coordinate bounds guard needs a window anchor (fail-closed)",
                target.describe()
            ))
        })?;
        let relative = matches!(target, actl_core::target::Target::CoordRel(..));
        let out = if relative {
            actl_uia::pointer_at_window(app, x, y, action, no_activate)?
        } else {
            actl_uia::pointer_at_physical(app, x, y, action, no_activate)?
        };
        let mut entry = json!({
            "action": name,
            "target": target.describe(),
            "resolved_by": if relative { "coord-window-relative" } else { "coord" },
            "point": [out.point.0, out.point.1],
            "guard": "window-bounds",
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": effects, "activation": if no_activate { "suppressed" } else { "normal" } },
            "window": out.window_title,
        });
        if let Some((ox, oy)) = out.origin {
            entry["origin"] = json!([ox, oy]);
        }
        return Ok(entry.into());
    }
    let (loc, (px, py), scrolled) =
        actl_uia::pointer_physical(app, &target, near, action, no_activate)?;
    Ok(json!({
        "action": name,
        "target": target.describe(),
        "resolved_by": loc.resolved_by,
        "point": [px, py],
        "scrolled_into_view": scrolled,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": effects, "activation": if no_activate { "suppressed" } else { "normal" } },
        "resolved": { "role": loc.role, "name": loc.name },
        "window": loc.window_title,
    })
    .into())
}

/// "x,y" 路径点解析(绝对物理像素;≤64 个,总插值量由 drag_path 上界把关)。
pub(crate) fn parse_waypoints(raw: &[String]) -> Result<Vec<(i32, i32)>, CtlError> {
    if raw.len() > 64 {
        return Err(CtlError::protocol(format!(
            "at most 64 --path waypoints, got {}",
            raw.len()
        )));
    }
    raw.iter()
        .map(|p| {
            let (x, y) = p.split_once(',').ok_or_else(|| {
                CtlError::protocol(format!("--path waypoint must be X,Y, got {p:?}"))
            })?;
            let x: i32 = x.trim().parse().map_err(|_| {
                CtlError::protocol(format!("--path X must be an integer, got {x:?}"))
            })?;
            let y: i32 = y.trim().parse().map_err(|_| {
                CtlError::protocol(format!("--path Y must be an integer, got {y:?}"))
            })?;
            Ok((x, y))
        })
        .collect()
}

pub(crate) fn drag_cmd(
    from_raw: &str,
    to_raw: &str,
    path: &[String],
    app: Option<&str>,
    near: Option<&str>,
    to_near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let from = actl_core::parse_target(from_raw)?;
    let to = actl_core::parse_target(to_raw)?;
    let waypoints = parse_waypoints(path)?;
    if matches!(from, actl_core::Target::CoordRel(..))
        || matches!(to, actl_core::Target::CoordRel(..))
    {
        return Err(CtlError::protocol(
            "xyw: is click/pointer-family only for now — drag endpoints use element targets or absolute xy: (window-relative drag is a follow-up, see docs/83 §8)",
        ));
    }
    let coord_pair = matches!(
        (&from, &to),
        (
            actl_core::Target::Coord(_, _),
            actl_core::Target::Coord(_, _)
        )
    );
    if !waypoints.is_empty() || coord_pair {
        let drag = actl_uia::drag_path_physical(app, &from, &to, &waypoints)?;
        return Ok(json!({
            "action": "drag",
            "mode": "path",
            "from": drag.from,
            "to": to.describe(),
            "waypoints": waypoints.len(),
            "point": [drag.start.0, drag.start.1],
            "destination": [drag.end.0, drag.end.1],
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "move_drag_click" },
            "window": drag.window,
        })
        .into());
    }
    let (loc, (fx, fy), scrolled) = actl_uia::drag_physical(app, &from, near, &to, to_near)?;
    Ok(json!({
        "action": "drag",
        "from": from.describe(),
        "to": to.describe(),
        "resolved_by": loc.resolved_by,
        "point": [fx, fy],
        "scrolled_into_view": scrolled,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "move_drag_click" },
        "resolved": { "role": loc.role, "name": loc.name },
        "window": loc.window_title,
    })
    .into())
}

pub(crate) fn scroll_cmd(
    target_raw: &str,
    down: Option<u32>,
    up: Option<u32>,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let notches = match (down, up) {
        (Some(n), None) => -(n as i32),
        (None, Some(n)) => n as i32,
        (Some(_), Some(_)) => return Err(CtlError::protocol("use only one of --down/--up")),
        (None, None) => return Err(CtlError::protocol("scroll needs --down N or --up N")),
    };
    let target = actl_core::parse_target(target_raw)?;
    let (loc, via) = actl_uia::scroll_element(app, &target, near, notches)?;
    let pointer = if via == "physical-wheel" {
        "move_and_wheel"
    } else {
        "none"
    };
    Ok(json!({
        "action": "scroll",
        "target": target.describe(),
        "resolved_by": loc.resolved_by,
        "notches": notches,
        "via": via,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": pointer },
    })
    .into())
}

pub(crate) fn key_half_cmd(
    name: &str,
    keys: &str,
    app: Option<&str>,
    duration: Option<u64>,
    down: bool,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, name)?;
    let spec = actl_core::parse_key_expr(keys)?;
    if !down && duration.is_some() {
        return Err(CtlError::protocol("--duration applies to key-down only"));
    }
    // 与 press 相同的护栏链:置焦 + 物理前台核对
    actl_uia::press_half_with_direction(&spec, Some(app), down)?;
    // 长按:按住→分片睡眠(急停即释放)→抬起。两半各自持锁(与跨进程
    // key-down/key-up 对同纪律),持有期靠停止核验中断。
    if let Some(ms) = duration {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
        let mut interrupted = false;
        while std::time::Instant::now() < deadline {
            let slice =
                std::time::Duration::from_millis(100).min(deadline - std::time::Instant::now());
            std::thread::sleep(slice);
            if actl_core::stop::check_current().is_err() {
                interrupted = true;
                break;
            }
        }
        let _ = actl_uia::press_half_with_direction(&spec, Some(app), false);
        if interrupted {
            return Err(CtlError::new(
                actl_core::ErrorCode::Aborted,
                format!(
                    "long press interrupted by a stop request; the key was released after {}ms of the requested {ms}ms",
                    ms.saturating_sub((deadline - std::time::Instant::now()).as_millis() as u64)
                ),
            ));
        }
        return Ok(json!({
            "action": name,
            "keys": keys,
            "held_ms": ms,
            "input_effects": {
                "keyboard": "key-down held + auto key-up",
                "clipboard": "untouched",
                "pointer": "none",
            },
        })
        .into());
    }
    Ok(json!({
        "action": name,
        "keys": keys,
        "input_effects": {
            "keyboard": if down { "key-down held" } else { "key-up released" },
            "clipboard": "untouched",
            "pointer": "none",
        },
    })
    .into())
}

pub(crate) fn resize_window_cmd(
    pattern: &str,
    width: i32,
    height: i32,
) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::resize_window(pattern, width, height)?;
    Ok(json!({
        "action": "resize-window",
        "window": title,
        "size": [width, height],
    })
    .into())
}

pub(crate) fn window_ref_of<'a>(
    pattern: Option<&'a str>,
    pid: Option<u32>,
    hwnd: Option<&'a str>,
) -> Result<actl_uia::WindowRef<'a>, CtlError> {
    // FR3 三形态:标题/PID/HWND 互斥(clap 已强制);全空 = 协议错误
    if let Some(p) = pattern {
        return Ok(actl_uia::WindowRef::Title(p));
    }
    if let Some(pid) = pid {
        return Ok(actl_uia::WindowRef::Pid(pid));
    }
    if let Some(raw) = hwnd {
        return Ok(actl_uia::WindowRef::Hwnd(actl_uia::parse_hwnd(raw)?));
    }
    Err(CtlError::protocol(
        "needs a title substring or --pid or --hwnd (run list-windows to find them)",
    ))
}

pub(crate) fn focus_window_cmd(
    pattern: Option<&str>,
    pid: Option<u32>,
    hwnd: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::focus_window_by(window_ref_of(pattern, pid, hwnd)?)?;
    Ok(json!({ "action": "focus-window", "window": title }).into())
}

pub(crate) fn close_window_cmd(
    pattern: Option<&str>,
    pid: Option<u32>,
    hwnd: Option<&str>,
    system_close: bool,
) -> Result<CommandOutput, CtlError> {
    let out = actl_uia::close_window_by(window_ref_of(pattern, pid, hwnd)?, system_close)?;
    Ok(json!({
        "action": "close-window",
        "window": out.title,
        "pid": out.pid,
        "instance": out.instance,
    })
    .into())
}

/// select:容器内按选项名选中(SelectionItem;上报实际选中的名字与匹配层级)。
pub(crate) fn select_cmd(
    target_raw: &str,
    name: &str,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let (loc, selected, level) = actl_uia::select_option(app, &target, name, near)?;
    Ok(json!({
        "action": "select",
        "target": target.describe(),
        "resolved_by": loc.resolved_by,
        "option": selected,
        "matched_by": level,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    })
    .into())
}

/// toggle:翻转 Toggle 状态(上报翻转后状态,On/Off)。
pub(crate) fn toggle_cmd(
    target_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let (loc, state) = actl_uia::toggle_element(app, &target, near)?;
    Ok(json!({
        "action": "toggle",
        "target": target.describe(),
        "resolved_by": loc.resolved_by,
        "state": state,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    })
    .into())
}

/// paste:ctrl+v 到目标窗口当前选中处(无元素落点的粘贴原语)。
pub(crate) fn paste_cmd(app: &str) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::paste(app)?;
    Ok(json!({
        "action": "paste",
        "window": title,
        "input_effects": { "keyboard": "injected", "clipboard": "consumed", "pointer": "none" },
    })
    .into())
}

/// screenshot 的互斥域参数(rect/wrect/element 三选一,CLI 展开后的容器)。
pub(crate) struct ShotDomain<'a> {
    pub(crate) rect: Option<&'a str>,
    pub(crate) wrect: Option<&'a str>,
    pub(crate) element: Option<&'a str>,
}

/// screenshot:像素捕获兜底(显式请求;envelope 只带文件路径与元数据,
/// 像素落磁盘——stdout 契约不破,视觉消费是上层 agent 的决定)。
pub(crate) fn screenshot_cmd(
    app: Option<&str>,
    hwnd: Option<&str>,
    domain: ShotDomain<'_>,
    sapp: Option<&str>,
    snear: Option<&str>,
    out: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let ShotDomain {
        rect,
        wrect,
        element,
    } = domain;
    let shot = if let Some(r) = rect {
        let parts: Vec<i32> = r
            .split(',')
            .map(|p| p.trim().parse::<i32>())
            .collect::<Result<_, _>>()
            .map_err(|_| {
                CtlError::protocol(format!("invalid --rect {r:?}: expected X,Y,W,H integers"))
            })?;
        if parts.len() != 4 {
            return Err(CtlError::protocol(format!(
                "invalid --rect {r:?}: expected X,Y,W,H"
            )));
        }
        actl_uia::screenshot_rect(parts[0], parts[1], parts[2], parts[3], out)?
    } else if let Some(r) = wrect {
        // 窗口相对区域(Fix B):与 --app/--hwnd 组合(窗口锚必须有);
        // 负偏移 PROTOCOL(窗口相对语义恒在窗内),越界部分截屏幕实况
        let parts: Vec<i32> = r
            .split(',')
            .map(|p| p.trim().parse::<i32>())
            .collect::<Result<_, _>>()
            .map_err(|_| {
                CtlError::protocol(format!("invalid --wrect {r:?}: expected X,Y,W,H integers"))
            })?;
        if parts.len() != 4 {
            return Err(CtlError::protocol(format!(
                "invalid --wrect {r:?}: expected X,Y,W,H"
            )));
        }
        if parts[0] < 0 || parts[1] < 0 {
            return Err(CtlError::protocol(format!(
                "invalid --wrect {r:?}: window-relative offsets must be non-negative (whole-window capture is --app, not negative offsets)"
            )));
        }
        let win = if let Some(h) = hwnd {
            actl_uia::WindowRef::Hwnd(actl_uia::parse_hwnd(h)?)
        } else if let Some(a) = app {
            actl_uia::WindowRef::Title(a)
        } else {
            return Err(CtlError::protocol(
                "--wrect requires a window anchor: pass --app <title-substr> or --hwnd <n>",
            ));
        };
        actl_uia::screenshot_window_rect(win, parts[0], parts[1], parts[2], parts[3], out)?
    } else if let Some(h) = hwnd {
        actl_uia::screenshot_window(actl_uia::WindowRef::Hwnd(actl_uia::parse_hwnd(h)?), out)?
    } else if let Some(app) = app {
        actl_uia::screenshot_window(actl_uia::WindowRef::Title(app), out)?
    } else if let Some(t) = element {
        let target = actl_core::parse_target(t)?;
        actl_uia::screenshot_element(sapp, &target, snear, out)?
    } else {
        return Err(CtlError::protocol(
            "screenshot needs one of --app / --hwnd / --rect / --element",
        ));
    };
    let mut envelope = json!({
        "action": "screenshot",
        "path": shot.path,
        "width": shot.width,
        "height": shot.height,
        "bytes": shot.bytes,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    });
    if let Some([x, y, w, h]) = shot.rect {
        envelope["rect"] = json!({"x": x, "y": y, "width": w, "height": h});
    }
    Ok(envelope.into())
}

/// find-image:确定性模板匹配定位(RPA 图色,非语义视觉)。像素相关在
/// actl-core::template_match(纯逻辑单测),原生胶合(WIC 解码+窗口捕获+
/// 物理坐标换算)在 actl-uia::find_image。只读观察命令(flow read_only)。
pub(crate) fn find_image_cmd(
    template: &str,
    app: Option<&str>,
    threshold: Option<f64>,
    all: bool,
    limit: Option<usize>,
    scales: bool,
) -> Result<CommandOutput, CtlError> {
    actl_uia::find_image::find_image(actl_uia::find_image::FindImageSpec {
        app,
        template,
        threshold,
        all,
        limit,
        scales,
    })
    .map(Into::into)
}

#[cfg(test)]
mod input_contract_tests {
    #[test]
    fn verified_type_success_and_partial_readback_status() {
        let mut outcome = actl_uia::TypeOutcome {
            located: actl_uia::InputResult {
                window_title: "fixture".into(),
                role: "Edit".into(),
                name: Some("Search".into()),
                host_control: None,
            },
            total: 5,
            delivered: 5,
            stopped_early: false,
        };
        let data = super::type_output(&actl_core::Target::Ref(3), &outcome, true, true);
        let envelope =
            serde_json::to_value(actl_core::Envelope::success("type", Some(data), 0)).unwrap();
        let golden: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/type_verified.json")).unwrap();
        assert_eq!(envelope, golden);
        outcome.delivered = 2;
        outcome.stopped_early = true;
        let partial = super::type_output(&actl_core::Target::Ref(3), &outcome, false, false);
        assert_eq!(partial["input_effects"]["keyboard"], "partial");
        assert_eq!(partial["delivered"], 2);
        assert!(partial["value_verified"].is_null());
    }
}
