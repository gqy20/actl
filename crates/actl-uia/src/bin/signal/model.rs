use actl_core::activity::{ActionPhase, InputEffects};
use actl_core::state::{Phase, SessionState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Preparing,
    Running,
    Done,
    Partial,
    Failed,
    StopRequested,
    Stopped,
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub mode: Mode,
    pub label: String,
    pub effects: InputEffects,
    pub details: String,
}

pub fn action_name(command: &str) -> &str {
    match command {
        "click" | "double-click" | "right-click" | "middle-click" | "triple-click" => "点击",
        "type" | "press" => "输入",
        "paste" => "粘贴",
        "set-clipboard" => "写剪贴板",
        "get-clipboard" => "读剪贴板",
        "set-value" => "填写",
        "select" => "选择",
        "toggle" => "切换",
        "expand" => "展开",
        "hover" => "移动",
        "drag" => "拖动",
        "scroll" => "滚动",
        "screenshot" => "截图",
        "snapshot" | "find" | "get" => "读取",
        "key-down" => "按住键",
        "key-up" => "释放键",
        "focus-window" => "切换窗口",
        "close-window" => "关闭",
        "resize-window" => "调整窗口",
        "minimize-window" => "最小化",
        "restore-window" => "恢复窗口",
        "launch" => "启动应用",
        "display-start" => "显示已就绪",
        "display-status" => "检查显示",
        "history" => "查看记录",
        "task-archive" => "归档报告",
        "desktop-prepare" => "整理桌面",
        "desktop-restore" => "恢复桌面",
        "status" => "检查状态",
        "list-windows" => "查看窗口",
        s if s.starts_with("wait") => "等待条件满足",
        s => s,
    }
}

pub fn selected(states: &[SessionState], now: u64) -> Option<&SessionState> {
    if let Some(s) = states
        .iter()
        .filter(|s| s.phase == Phase::Running)
        .filter(|s| {
            s.action
                .as_ref()
                .is_some_and(|a| a.phase == ActionPhase::Preparing && a.expires_ms > now)
        })
        .min_by_key(|s| s.action.as_ref().map(|a| a.ts_ms))
    {
        return Some(s);
    }
    states.iter().max_by_key(|s| {
        (
            s.phase == Phase::Running && now.saturating_sub(s.ts_ms) < 10_000,
            s.action.as_ref().is_some_and(|a| {
                a.active()
                    && (a.effects.pointer
                        || a.effects.keyboard
                        || a.effects.clipboard != actl_core::activity::ClipboardEffect::Untouched)
            }),
            s.ts_ms,
        )
    })
}
pub fn view(states: &[SessionState], now: u64, stop_ms: Option<u64>, busy: Option<bool>) -> View {
    let selected = selected(states, now);
    let Some(s) = selected else {
        return View {
            mode: if stop_ms.is_some() {
                Mode::StopRequested
            } else if busy != Some(false) {
                Mode::Unknown
            } else {
                Mode::Idle
            },
            label: if stop_ms.is_some() {
                "已请求停止"
            } else if busy == Some(true) {
                "输入占用 · 动作未知"
            } else if busy.is_none() {
                "输入状态未知"
            } else {
                "actl · 空闲"
            }
            .into(),
            effects: Default::default(),
            details: if stop_ms.is_some() {
                "已请求停止\n等待当前操作结束"
            } else {
                "空闲\n暂无操作"
            }
            .into(),
        };
    };
    let stale = s.phase == Phase::Running && now.saturating_sub(s.ts_ms) > 10_000;
    let mut mode = match s.phase {
        Phase::Done => Mode::Done,
        Phase::Error => Mode::Failed,
        Phase::Stopped => Mode::Stopped,
        Phase::Running => {
            if stale {
                Mode::Unknown
            } else {
                Mode::Running
            }
        }
    };
    let action = s.action.as_ref();
    if !stale
        && s.phase == Phase::Running
        && action.is_some_and(|a| a.phase == ActionPhase::Preparing)
    {
        mode = Mode::Preparing;
    }
    if mode == Mode::Done && action.is_some_and(|a| a.phase == ActionPhase::Partial) {
        mode = Mode::Partial;
    }
    if let Some(t) = stop_ms
        && !(s.phase == Phase::Stopped
            && s.ts_ms >= t
            && !states.iter().any(|s| s.phase == Phase::Running))
    {
        mode = Mode::StopRequested;
    }
    let active = action.filter(|a| a.active() && !stale && s.phase == Phase::Running);
    let mut effects = active.map(|a| a.effects).unwrap_or_default();
    // A compact strip must not hide another invocation's input impact.
    for a in states
        .iter()
        .filter(|s| s.phase == Phase::Running && now.saturating_sub(s.ts_ms) <= 10_000)
        .filter_map(|s| s.action.as_ref())
        .filter(|a| a.active())
    {
        effects.keyboard |= a.effects.keyboard;
        effects.pointer |= a.effects.pointer;
        effects.focus |= a.effects.focus;
        if a.effects.clipboard != actl_core::activity::ClipboardEffect::Untouched {
            effects.clipboard = a.effects.clipboard;
        }
    }
    let target = action
        .and_then(|a| a.target.window.as_deref())
        .or(s.app.as_deref())
        .unwrap_or("actl");
    let target = if target.starts_with("hwnd:") {
        "目标窗口"
    } else {
        target
    };
    let command = active.map(|a| a.kind.as_str()).unwrap_or(&s.command);
    let mut text = match mode {
        Mode::Preparing => format!("即将{}", action_name(command)),
        Mode::Done => "调用完成".into(),
        Mode::Failed => "调用失败".into(),
        Mode::Partial => "部分送达".into(),
        Mode::StopRequested => "已请求停止".into(),
        Mode::Stopped => "本次已停止".into(),
        Mode::Unknown => "状态未知".into(),
        _ => action_name(command).into(),
    };
    if !stale
        && !matches!(mode, Mode::StopRequested | Mode::Preparing)
        && let Some(flow) = &s.workflow
    {
        match flow.status.as_str() {
            "paused" => {
                mode = Mode::Idle;
                text = "流程已暂停".into();
            }
            "needs_review" => {
                mode = Mode::Partial;
                text = "需要核对结果".into();
            }
            "completed" => {
                mode = Mode::Done;
                text = "流程已完成".into();
            }
            "failed" => {
                mode = Mode::Failed;
                text = "流程失败".into();
            }
            "stopped" => {
                mode = Mode::Stopped;
                text = "流程已停止".into();
            }
            "running" if active.is_none() && mode == Mode::Running => {
                text = flow.title.clone();
            }
            _ => {}
        }
    }
    let elapsed = s
        .finished_ms
        .unwrap_or(now)
        .saturating_sub(if s.started_ms == 0 {
            s.ts_ms
        } else {
            s.started_ms
        })
        / 1000;
    let live = states
        .iter()
        .filter(|s| s.phase == Phase::Running && now.saturating_sub(s.ts_ms) < 10_000)
        .count();
    let mut impact = Vec::new();
    let effect = action.map(|a| a.effects).unwrap_or_default();
    if effect.keyboard {
        impact.push("键盘");
    }
    if effect.pointer {
        impact.push("鼠标");
    }
    match effect.clipboard {
        actl_core::activity::ClipboardEffect::Read => impact.push("读取剪贴板"),
        actl_core::activity::ClipboardEffect::Write => impact.push("写入剪贴板"),
        _ => {}
    }
    if effect.focus {
        impact.push("焦点");
    }
    let mut details = vec![
        target.to_owned(),
        format!("{} · {text}", action_name(&s.command)),
        format!(
            "耗时 {elapsed}s · {}s 前更新",
            now.saturating_sub(s.ts_ms) / 1000
        ),
    ];
    if !impact.is_empty() {
        details.push(format!("影响：{}", impact.join("、")));
    }
    if let Some(step) = &s.step {
        details.push(format!("步骤 {step}"));
    }
    if let Some(flow) = &s.workflow {
        details.push(flow.title.clone());
        if flow.status == "paused" {
            details.push("恢复前将重新检查目标".into());
        }
    }
    if live > 1 {
        details.push(format!("{live} 项操作进行中"));
    }
    if let Some(error) = &s.error_code {
        let guidance = match error.as_str() {
            "STALE_REF" => "重新读取界面后再试",
            "NOT_FOUND" => "检查目标是否已打开",
            "AMBIGUOUS" => "需要确定操作对象",
            "PERM_DENIED" => "检查窗口焦点与权限",
            "NOT_ACTIONABLE" => "检查窗口与显示状态",
            "TIMEOUT" => "检查应用是否已就绪",
            "ASSERTION_FAILED" => "结果未确认，请先检查",
            "ABORTED" => "已中断，请先检查已完成操作",
            _ => "查看操作记录以了解原因",
        };
        details.push(guidance.into());
        details.push(format!("错误：{error}"));
    }
    if mode == Mode::StopRequested {
        details.push("等待当前操作结束".into());
    }
    View {
        mode,
        effects,
        label: format!(
            "{} · {text}",
            if target.chars().count() > 9 {
                format!("{}…", target.chars().take(8).collect::<String>())
            } else {
                target.into()
            }
        ),
        details: details.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failure_details_offer_a_next_step_without_claiming_a_specific_cause() {
        let mut s = SessionState::now(1, "click", None, None, Phase::Error);
        s.error_code = Some("STALE_REF".into());
        let v = view(&[s], 0, None, Some(false));
        assert!(v.details.contains("重新读取界面"));
        assert!(v.details.contains("STALE_REF"));
        assert_eq!(action_name("wait"), "等待条件满足");
    }
    #[test]
    fn unknown_input_observation_never_looks_idle() {
        assert_eq!(view(&[], 0, None, None).mode, Mode::Unknown);
        assert_eq!(view(&[], 0, None, Some(true)).mode, Mode::Unknown);
    }
    #[test]
    fn concurrent_effects_are_combined() {
        use actl_core::activity::*;
        let mut keyboard = SessionState::now(1, "type", None, None, Phase::Running);
        let now = keyboard.ts_ms;
        keyboard.action = Some(ActionState {
            id: 1,
            seq: 1,
            kind: "type".into(),
            phase: ActionPhase::Executing,
            effects: InputEffects {
                keyboard: true,
                ..Default::default()
            },
            target: Default::default(),
            ts_ms: now,
            expires_ms: now + 2000,
        });
        let mut pointer = keyboard.clone();
        pointer.pid = 2;
        pointer.action.as_mut().unwrap().effects = InputEffects {
            pointer: true,
            ..Default::default()
        };
        let shown = view(&[keyboard, pointer], now, None, Some(true));
        assert!(shown.effects.keyboard && shown.effects.pointer);
    }
    #[test]
    fn terminal_state_does_not_expire() {
        let mut s = SessionState::now(42, "click", None, None, Phase::Done);
        s.ts_ms = 1;
        assert_eq!(view(&[s], 100_000, None, Some(false)).mode, Mode::Done);
    }
    #[test]
    fn long_running_without_heartbeat_is_unknown_not_success() {
        let mut s = SessionState::now(42, "wait", None, None, Phase::Running);
        s.ts_ms = 1;
        assert_eq!(view(&[s], 100_000, None, Some(false)).mode, Mode::Unknown);
    }
    #[test]
    fn stop_request_does_not_confirm_stop() {
        let s = SessionState::now(42, "click", None, None, Phase::Running);
        assert_eq!(
            view(std::slice::from_ref(&s), s.ts_ms, Some(s.ts_ms), Some(true)).mode,
            Mode::StopRequested
        );
    }
}
