//! interact —— 操作类命令执行体:click/type/press/key-down/up/set-value、
//! 物理指针族(right-click/double-click/hover/drag/scroll)、窗口操作
//! (focus/close/resize)与 launch。协议护栏(replay_guard/require_app/parse_expect)
//! 也在此:注入前的 fail-closed 门。观察/编排类见 `query`。

use actl_core::{CtlError, ErrorCode};
use serde_json::json;

use crate::CommandOutput;

pub(crate) fn click(
    target_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    physical: bool,
    with: &[String],
    expect: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    replay_guard(snapshot, app)?;
    // --expect = 新窗口断言:动作前取 RuntimeId 基线(与 press 相同语义)
    let expect_substr = match expect {
        Some(raw) => Some(parse_expect(raw)?),
        None => None,
    };
    let baseline = if expect_substr.is_some() {
        actl_uia::window_identities()?
    } else {
        Vec::new()
    };

    let data = if physical {
        let action = if with.is_empty() {
            actl_uia::PointerAction::LeftClick
        } else {
            actl_uia::PointerAction::ClickWith {
                modifiers: with.to_vec(),
            }
        };
        let (loc, (px, py), scrolled) = actl_uia::pointer_physical(app, &target, near, action)?;
        json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": loc.resolved_by,
            "pattern": if with.is_empty() { "physical-left" } else { "physical-modified" },
            "modifiers": with,
            "point": [px, py],
            "scrolled_into_view": scrolled,
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "move_and_click" },
            "resolved": { "role": loc.role, "name": loc.name, "automation_id": loc.automation_id },
            "window": loc.window_title,
        })
    } else {
        let (loc, pattern) = actl_uia::click(app, &target, near)?;
        json!({
            "action": "click",
            "target": target.describe(),
            "resolved_by": loc.resolved_by,
            "pattern": pattern,
            "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
            "resolved": {
                "role": loc.role,
                "name": loc.name,
                "automation_id": loc.automation_id,
            },
            "window": loc.window_title,
        })
    };

    // 06 §4:--expect = 执行后隐式 wait(默认 2s),失败 ASSERTION_FAILED
    if let Some(substr) = expect_substr {
        actl_uia::wait_new_window(&substr, timeout.unwrap_or(2000), &baseline)?;
    }
    Ok(data.into())
}

pub(crate) fn type_cmd(
    target_raw: &str,
    text: &str,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
    paste: bool,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "type")?;
    replay_guard(snapshot, Some(app))?;
    let target = actl_core::parse_target(target_raw)?;
    let res = actl_uia::type_text(Some(app), &target, text, paste, near)?;
    let keyboard = if res.stopped_early {
        "partial"
    } else {
        "injected"
    };
    Ok(json!({
        "action": "type",
        "target": target.describe(),
        "chars": res.total,
        "delivered": res.delivered,
        "stopped_early": res.stopped_early,
        "via": if paste { "clipboard-paste" } else { "keyboard" },
        "input_effects": {
            "keyboard": keyboard,
            "clipboard": if paste { "used" } else { "untouched" },
            "pointer": "none",
        },
        "resolved": { "role": res.located.role, "name": res.located.name },
    })
    .into())
}

pub(crate) fn press(
    keys: &str,
    app: Option<&str>,
    expect: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, "press")?;
    let spec = actl_core::parse_key_expr(keys)?;
    // --expect = "本动作导致新窗口":注入前先取窗口基线,断言时排除既有同名窗口
    // (存在性检查会被预先打开的同名窗口恒真欺骗,doc 09 §2)
    let expect_substr = match expect {
        Some(raw) => Some(parse_expect(raw)?),
        None => None,
    };
    let baseline = if expect_substr.is_some() {
        actl_uia::window_identities()?
    } else {
        Vec::new()
    };
    // 键盘作用于全局焦点:裸 SendInput 会打进当下前台(实测坑)。
    // press_keys 内部在 --app 时先置前台并稳定窗(--app 语义,focus+delay 在 uia 层)。
    let focused = actl_uia::press_keys(&spec, Some(app))?;
    let mut expect_window = None;
    if let Some(substr) = expect_substr {
        // 06 §4:--expect = 执行后隐式 wait(默认 2s),失败 ASSERTION_FAILED
        actl_uia::wait_new_window(&substr, timeout.unwrap_or(2000), &baseline)?;
        expect_window = Some(substr);
    }
    Ok(json!({
        "action": "press",
        "keys": keys,
        "focused": focused,
        "expect": expect_window,
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
pub(crate) fn replay_guard(snapshot: Option<&str>, app: Option<&str>) -> Result<(), CtlError> {
    let Some(id) = snapshot else { return Ok(()) };
    let Some(pattern) = app else {
        return Err(CtlError::protocol(
            "--snapshot requires --app (the window to freshness-check)",
        ));
    };
    actl_uia::check_snapshot_freshness_by_pattern(id, pattern)
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
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    replay_guard(snapshot, app)?;
    let res = actl_uia::set_value(app, &target, value, near)?;
    Ok(json!({
        "action": "set-value",
        "target": target.describe(),
        "resolved": { "role": res.role, "name": res.name },
    })
    .into())
}

/// --expect 谓词 v1:仅 `window:<substr>`。
fn parse_expect(raw: &str) -> Result<String, CtlError> {
    raw.strip_prefix("window:")
        .filter(|s| !s.is_empty())
        .map(String::from)
        .ok_or_else(|| {
            CtlError::protocol(format!(
                "invalid --expect {raw:?}: only window:<substr> is supported"
            ))
        })
}

pub(crate) fn launch(args: &[String]) -> Result<CommandOutput, CtlError> {
    let Some(program) = args.first() else {
        return Err(CtlError::protocol("launch requires a program name"));
    };
    // App Paths 解析:裸程序名(Office/商店别名)不在 PATH 时查注册表取完整路径
    // (通用增强,docs/14 实测痛点:launch WINWORD 曾 NotFound)
    // GUI 程序不得继承控制台句柄:Word 等会攥住 stdout 写端,调用方的
    // 管道(head/$())永远等不到 EOF(实测挂死)。stdio 显式分离。
    let detached = |cmd: &mut std::process::Command| {
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    };
    let mut first = std::process::Command::new(program);
    detached(&mut first);
    let resolved = first
        .args(&args[1..])
        .spawn()
        .map(|child| (child, program.clone()))
        .or_else(|first_err| match actl_uia::resolve_app_path(program) {
            Some(full) if full != *program => {
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
    Ok(json!({ "pid": child.id(), "program": program }).into())
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
    kind: PointerKind,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
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
    let (loc, (px, py), scrolled) = actl_uia::pointer_physical(app, &target, near, action)?;
    Ok(json!({
        "action": name,
        "target": target.describe(),
        "resolved_by": loc.resolved_by,
        "point": [px, py],
        "scrolled_into_view": scrolled,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": effects },
        "resolved": { "role": loc.role, "name": loc.name },
        "window": loc.window_title,
    })
    .into())
}

pub(crate) fn drag_cmd(
    from_raw: &str,
    to_raw: &str,
    app: Option<&str>,
    near: Option<&str>,
    to_near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let from = actl_core::parse_target(from_raw)?;
    let to = actl_core::parse_target(to_raw)?;
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
    down: bool,
) -> Result<CommandOutput, CtlError> {
    let app = require_app(app, name)?;
    let spec = actl_core::parse_key_expr(keys)?;
    // 与 press 相同的护栏链:置焦 + 物理前台核对
    actl_uia::press_half(&spec, Some(app))?;
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
) -> Result<CommandOutput, CtlError> {
    let title = actl_uia::close_window_by(window_ref_of(pattern, pid, hwnd)?)?;
    Ok(json!({ "action": "close-window", "window": title }).into())
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

/// screenshot:像素捕获兜底(显式请求;envelope 只带文件路径与元数据,
/// 像素落磁盘——stdout 契约不破,视觉消费是上层 agent 的决定)。
pub(crate) fn screenshot_cmd(
    app: Option<&str>,
    hwnd: Option<&str>,
    rect: Option<&str>,
    element: Option<&str>,
    sapp: Option<&str>,
    snear: Option<&str>,
    out: Option<&str>,
) -> Result<CommandOutput, CtlError> {
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
    Ok(json!({
        "action": "screenshot",
        "path": shot.path,
        "width": shot.width,
        "height": shot.height,
        "bytes": shot.bytes,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    })
    .into())
}
