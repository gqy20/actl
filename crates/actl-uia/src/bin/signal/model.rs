use actl_core::activity::{ActionPhase, InputEffects};
use actl_core::state::{Phase, SessionState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Background,
    Desktop,
    Paused,
    Waiting,
    Blocked,
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
    pub control: Control,
    /// 按钮的人类文案,按暂停上下文差异化(重试/核验/检查);
    /// 绘制层的图形不变,文案供图例/详情/控制窗口使用。
    pub control_hint: String,
    pub label: String,
    pub effects: InputEffects,
    pub details: String,
    /// 展开态第二行的三列(主体/进度/备注)。列起点在 paint 层固定;
    /// 无任务时也给定值,展开不出现空行。
    pub second: [String; 3],
    /// 运行中调用的进行时长 mm:ss(等宽数字,槽内字符数稳定)。
    pub elapsed: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Pause { call: String, flow: Option<String> },
    Continue { flow: String },
    Details,
}
impl Control {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pause { .. } => "暂停任务",
            Self::Continue { .. } => "检查并继续",
            Self::Details => "查看状态与原因",
        }
    }
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
            s.action.is_some() || s.handoff_reason.is_some() || s.workflow.is_some(),
            s.ts_ms,
        )
    })
}
/// 暂停请求已发出、当前动作尚未到安全边界时的显示覆写(批次2①③)。
/// 标签不带耗时(防托盘图标每秒重建);耗时与急停教育进详情(任务详情窗口
/// 按自身节拍刷新)。超阈值(~5s)提示急停入口,纠正"不耐烦就急停"的工具倒挂——
/// 但不自动升级,保留用户意图。
pub fn pause_pending_view(
    step: Option<&str>,
    elapsed_s: u64,
    hotkey: bool,
) -> (Mode, String, String) {
    let label = match step {
        Some(id) => format!("正在暂停 · {id} · 可操作"),
        None => "正在暂停 · 可操作".into(),
    };
    let mut details = format!(
        "暂停请求已发出 {elapsed_s}s，等待当前动作在安全边界结束；已送达的部分不会重复注入"
    );
    if elapsed_s >= 5 {
        details.push_str(if hotkey {
            "\n当前动作迟迟未结束；需要立即终止请按 Ctrl+Alt+F12 急停（急停独立于暂停）"
        } else {
            "\n当前动作迟迟未结束；需要立即终止请右键菜单 → 紧急停止（急停独立于暂停）"
        });
    }
    (Mode::Paused, label, details)
}

pub fn view(states: &[SessionState], now: u64, stop_ms: Option<u64>, busy: Option<bool>) -> View {
    let selected = selected(states, now);
    let Some(s) = selected else {
        return View {
            control: Control::Details,
            control_hint: Control::Details.label().into(),
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
                if busy == Some(false) {
                    "已请求停止\n后续操作已阻止"
                } else {
                    "已请求停止\n等待当前操作结束"
                }
            } else {
                "空闲\n暂无操作"
            }
            .into(),
            second: ["actl".into(), "空闲".into(), "右键托盘更多".into()],
            elapsed: None,
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
        // 路径类目标只显示文件名(横条空间宝贵,完整路径是废话;
        // 全路径保留在 details 里供需要时查看)。
        target
            .rsplit(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(target)
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
                text = match flow.reason.as_deref() {
                    Some("checkpoint" | "start_paused") => "等待继续",
                    Some("precondition_failed") => "条件未满足",
                    Some("action_failed") => "操作未完成",
                    _ => "流程已暂停",
                }
                .into();
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
        if let Some(reason) = flow.reason.as_deref() {
            details.push(
                match reason {
                    "checkpoint" => "已到检查点，确认后继续",
                    "start_paused" => "尚未开始，确认后继续",
                    "pause_requested" => "已暂停，继续时重新检查",
                    "precondition_failed" => "检查目标是否已就绪",
                    "action_failed" => "检查操作结果后继续",
                    "verification_failed" => "先核对结果，继续仅重新检查",
                    "stop_requested" => "先解除急停，再继续任务",
                    _ => "核对结果后再决定是否重试",
                }
                .into(),
            );
        }
        if let Some(error) = flow.error_code {
            details.push(format!(
                "步骤错误：{}",
                serde_json::to_value(error)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default()
            ));
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
        details.push(
            if live > 0 || busy != Some(false) {
                "等待当前操作结束"
            } else if states.iter().any(|s| s.phase == Phase::Running) {
                "操作状态未知，后续操作已阻止"
            } else {
                "后续操作已阻止"
            }
            .into(),
        );
    }
    let mut control = Control::Details;
    let mut control_hint: String = Control::Details.label().into();
    let flow = s.workflow.as_ref();
    let valid_flow = flow.filter(|f| actl_core::history::valid_id(&f.run_id));
    let blocked =
        matches!(mode, Mode::StopRequested | Mode::Stopped | Mode::Unknown) || stop_ms.is_some();
    let reason = s
        .handoff_reason
        .as_deref()
        .or_else(|| flow.and_then(|f| f.reason.as_deref()));
    let mut compact = false;
    if !blocked {
        match reason {
            Some("handoff_pending") if s.phase == Phase::Running => {
                mode = Mode::Waiting;
                text = "等待交接 · 可操作".into();
                compact = true;
            }
            Some("human_takeover_or_grant_expired") => {
                mode = Mode::Paused;
                text = "已让行 · 可操作".into();
                compact = true;
            }
            Some("user_paused" | "pause_requested") => {
                mode = Mode::Paused;
                text = "已暂停 · 可操作".into();
                compact = true;
            }
            Some("handoff_pending" | "user_deferred") => {
                mode = Mode::Paused;
                text = "等待继续 · 可操作".into();
                compact = true;
            }
            Some(_) if s.handoff_reason.is_some() => {
                mode = Mode::Blocked;
                text = "无法继续 · 查看原因".into();
                compact = true;
            }
            _ => {}
        }
        if let Some(f) = valid_flow {
            match f.status.as_str() {
                "paused" | "needs_review" => {
                    control = Control::Continue {
                        flow: f.run_id.clone(),
                    };
                    // 继续语义按上下文差异化(2026-10 批次1):让按钮文案说真话——
                    // 失败暂停=点继续会自动重发该步动作;待核对=先核验;人工暂停=检查。
                    control_hint = if f.status == "needs_review" {
                        "核验并继续"
                    } else if f.reason.as_deref() == Some("action_failed") {
                        "已修复，重试继续"
                    } else {
                        "检查并继续"
                    }
                    .into();
                    if !compact {
                        let (new_mode, label): (Mode, String) = match f.reason.as_deref() {
                            Some("precondition_failed") => {
                                (Mode::Blocked, "条件未满足 · 可操作".into())
                            }
                            // 弹层门禁/候选歧义/决策卡:专属短文案替代泛化的
                            // "待核对结果"——这三态的正确动作不是核对结果
                            // (release-review-10 两次 popup_pending 实证错配)。
                            Some(r) if r.starts_with("popup_pending") => {
                                (Mode::Blocked, "前台有弹窗 · 处理后继续".into())
                            }
                            Some(r) if r.starts_with("ambiguous_target") => {
                                (Mode::Partial, "多个候选 · 待指定目标".into())
                            }
                            Some("decision_pending") => (Mode::Partial, "待决策 · 需应答".into()),
                            Some("action_failed") => (
                                Mode::Blocked,
                                match f.step_id.as_deref() {
                                    Some(id) => format!("操作未完成 · {id} · 可操作"),
                                    None => "操作未完成 · 可操作".into(),
                                },
                            ),
                            _ if f.status == "needs_review" => {
                                (Mode::Partial, "待核对结果 · 可操作".into())
                            }
                            _ => (Mode::Paused, "已暂停 · 可操作".into()),
                        };
                        mode = new_mode;
                        text = label;
                        compact = true;
                    }
                    details.push(if f.reason.as_deref() == Some("action_failed") {
                        "点继续将自动重发失败步骤的动作（适合已清除障碍）；若你已手动完成该步，请先在应用中核对结果再继续".into()
                    } else {
                        "继续时先核对上一步；结果不明确不会自动重做".into()
                    });
                }
                "running" if s.phase == Phase::Running => {
                    control = Control::Pause {
                        call: s.call_id.clone(),
                        flow: Some(f.run_id.clone()),
                    }
                }
                _ => {}
            }
        } else if matches!(mode, Mode::Paused | Mode::Blocked) {
            details.push("当前调用已结束，没有自动恢复入口；调用方须核对结果后发起后续任务".into());
        }
        if s.phase == Phase::Running && !stale && !compact {
            let desktop = states.iter().any(|state| {
                state.phase == Phase::Running
                    && now.saturating_sub(state.ts_ms) < 10_000
                    && state.action.as_ref().is_some_and(|a| a.active())
            });
            if desktop || busy == Some(true) {
                mode = Mode::Desktop;
                // 设备字形(2026-10-12 显示层 spec 小项收口):琥珀态主行前缀
                // 设备指示——键盘 U+2328(export 探针实证 DirectWrite 回退链
                // 出字形);鼠标 emoji(U+1F5B1)实锤豆腐块,改文字「鼠标」;
                // 剪贴板/焦点维持详情行不占主行。聚合口径与 details 影响行
                // 一致:多调用并发时不遮蔽任一输入影响。
                let mut glyph = String::new();
                if effects.keyboard {
                    glyph.push_str("\u{2328} ");
                }
                if effects.pointer {
                    glyph.push_str("鼠标 ");
                }
                text = if glyph.is_empty() {
                    "桌面操作 · 暂勿操作".into()
                } else {
                    format!("{glyph}桌面操作 · 暂勿操作")
                };
                compact = true;
            } else if busy == Some(false)
                && matches!(
                    command,
                    "snapshot"
                        | "find"
                        | "get"
                        | "extract"
                        | "verify"
                        | "wait"
                        | "list-windows"
                        | "status"
                        | "get-clipboard"
                        | "screenshot"
                )
            {
                mode = Mode::Background;
                text = "后台处理 · 可操作".into();
                compact = true;
            }
        }
        if valid_flow.is_none()
            && s.phase == Phase::Running
            && !stale
            && (active.is_some() || s.handoff_reason.as_deref() == Some("handoff_pending"))
        {
            control = Control::Pause {
                call: s.call_id.clone(),
                flow: None,
            };
        }
    }
    details.push(control_hint.clone());
    // 进行时长 mm:ss:等宽数字,运行中且未陈旧才显示(上限 99:59 后保持稳定宽度)。
    let elapsed = if s.phase == Phase::Running && !stale {
        let secs = now.saturating_sub(s.started_ms) / 1000;
        Some(format!("{:02}:{:02}", (secs / 60).min(99), secs % 60))
    } else {
        None
    };
    let reason_hint = reason.map(super::ui_text::reason);
    // 展开态第二行三列(主体/进度/备注):主体=流程标题(回退调用命令),
    // 进度=步骤或调用相位,备注=原因提示或进行时长。列起点由 paint 固定。
    let second = if let Some(flow) = valid_flow {
        [
            if flow.title.is_empty() {
                flow.run_id.clone()
            } else {
                flow.title.clone()
            },
            flow.step_id.clone().unwrap_or_else(|| flow.status.clone()),
            reason_hint.or(elapsed.as_deref()).unwrap_or("—").into(),
        ]
    } else {
        [
            s.command.clone(),
            match s.phase {
                Phase::Running => "进行中".into(),
                Phase::Done => "已完成".into(),
                Phase::Stopped => "已停止".into(),
                Phase::Error => "失败".into(),
            },
            elapsed.clone().unwrap_or_else(|| target.to_string()),
        ]
    };
    View {
        mode,
        control,
        control_hint,
        effects,
        label: if compact {
            text
        } else {
            format!(
                "{} · {text}",
                if target.chars().count() > 9 {
                    format!("{}…", target.chars().take(8).collect::<String>())
                } else {
                    target.into()
                }
            )
        },
        details: details.join("\n"),
        second,
        elapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_pending_carries_step_and_escalates_to_stop_hint() {
        // 常规:带步骤 id,标签不含耗时,详情含耗时与不重复注入承诺。
        let (mode, label, details) = pause_pending_view(Some("cell-readback"), 2, true);
        assert_eq!(mode, Mode::Paused);
        assert_eq!(label, "正在暂停 · cell-readback · 可操作");
        assert!(
            details.contains("已发出 2s") && !label.contains('s'),
            "{details}"
        );
        assert!(!details.contains("急停"), "under 5s must not nag");
        // 超阈值:附急停教育(有热键与无热键两种文案),仍为 Paused 不是停止。
        let (_, _, details) = pause_pending_view(None, 6, true);
        assert!(details.contains("Ctrl+Alt+F12"), "{details}");
        let (_, _, details) = pause_pending_view(None, 6, false);
        assert!(
            details.contains("右键菜单") && !details.contains("Ctrl+Alt+F12"),
            "{details}"
        );
    }
    #[test]
    fn idle_stop_does_not_claim_an_operation_is_still_running() {
        let mut state = SessionState::now(1, "commands", None, None, Phase::Done);
        state.ts_ms = 100;
        for states in [vec![], vec![state]] {
            let v = view(&states, 500, Some(200), Some(false));
            assert!(!v.details.contains("等待当前操作结束"));
            assert!(v.details.contains("后续操作已阻止"));
        }
    }
    #[test]
    fn workflow_pause_reason_and_step_error_remain_visible() {
        let mut state = SessionState::now(1, "wait", None, None, Phase::Done);
        state.workflow = Some(serde_json::from_value(serde_json::json!({
            "run_id":"test", "title":"Check result", "status":"paused",
            "reason":"action_failed", "step_id":"check", "error_code":"TIMEOUT", "actions":["resume"]
        })).unwrap());
        let shown = view(&[state.clone()], state.ts_ms, None, Some(false));
        assert!(shown.label.contains("操作未完成"));
        assert!(shown.details.contains("检查操作结果"));
        assert!(shown.details.contains("TIMEOUT"));
        assert_eq!(
            view(&[state.clone()], state.ts_ms, Some(0), Some(false)).mode,
            Mode::StopRequested
        );
    }
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
    fn amber_main_line_carries_device_glyph() {
        // ① 显示层 spec 小项收口(2026-10-12):主行设备字形——键盘 U+2328
        // (export 探针实证可渲染),鼠标 emoji 豆腐改文字;观察调用无前缀。
        use actl_core::activity::*;
        let mut st = SessionState::now(1, "type", None, None, Phase::Running);
        let now = st.ts_ms;
        st.action = Some(ActionState {
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
        let shown = view(&[st.clone()], now, None, None);
        assert!(
            shown.label.contains('\u{2328}'),
            "keyboard glyph on main line: {:?}",
            shown.label
        );
        let mut pt = SessionState::now(2, "click", None, None, Phase::Running);
        pt.action = Some(ActionState {
            id: 2,
            seq: 1,
            kind: "click".into(),
            phase: ActionPhase::Executing,
            effects: InputEffects {
                pointer: true,
                ..Default::default()
            },
            target: Default::default(),
            ts_ms: now,
            expires_ms: now + 2000,
        });
        let shown2 = view(&[pt], now, None, None);
        assert!(
            shown2.label.contains("鼠标"),
            "pointer uses text form (U+1F5B1 tofu): {:?}",
            shown2.label
        );
        let mut obs = SessionState::now(3, "status", None, None, Phase::Running);
        obs.action = None;
        let shown3 = view(&[obs], now, None, None);
        assert!(
            !shown3.label.contains('\u{2328}') && !shown3.label.contains("鼠标 "),
            "observation carries no glyph: {:?}",
            shown3.label
        );
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

#[cfg(test)]
mod cooperation_tests {
    use super::*;
    #[test]
    fn human_takeover_is_not_a_generic_failure() {
        let mut s = SessionState::now(1, "type", None, None, Phase::Error);
        s.handoff_reason = Some("human_takeover_or_grant_expired".into());
        let v = view(&[s], actl_core::state::unix_ms(), None, Some(false));
        assert!(v.label.contains("已让行"));
        assert!(!v.label.contains("失败"));
        assert!(v.details.contains("没有自动恢复入口"));
    }
    #[test]
    fn readonly_work_explicitly_permits_human_input() {
        let s = SessionState::now(1, "snapshot", None, None, Phase::Running);
        assert!(
            view(&[s], actl_core::state::unix_ms(), None, Some(false))
                .label
                .contains("可操作")
        );
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;
    fn paused() -> SessionState {
        let mut s = SessionState::now(1, "flow-run", None, None, Phase::Error);
        s.call_id = "call-a".into();
        s.handoff_reason = Some("human_takeover_or_grant_expired".into());
        s.workflow = Some(
            serde_json::from_value(
                serde_json::json!({"run_id":"flow-a","title":"Write","status":"paused"}),
            )
            .unwrap(),
        );
        s
    }
    #[test]
    fn passive_pause_is_bound_to_its_flow_and_stop_disables_resume() {
        let s = paused();
        let v = view(std::slice::from_ref(&s), s.ts_ms, None, Some(false));
        assert_eq!(
            v.control,
            Control::Continue {
                flow: "flow-a".into()
            }
        );
        assert_eq!(
            view(
                std::slice::from_ref(&s),
                s.ts_ms,
                Some(s.ts_ms),
                Some(false)
            )
            .control,
            Control::Details
        );
    }
    #[test]
    fn diagnostic_completion_does_not_hide_a_paused_task() {
        let s = paused();
        let mut query = SessionState::now(2, "display-status", None, None, Phase::Done);
        query.ts_ms = s.ts_ms + 1;
        let v = view(&[s, query], actl_core::state::unix_ms(), None, Some(false));
        assert_eq!(
            v.control,
            Control::Continue {
                flow: "flow-a".into()
            }
        );
    }
    #[test]
    fn standalone_pause_does_not_offer_fake_resume() {
        let mut s = paused();
        s.workflow = None;
        assert_eq!(
            view(std::slice::from_ref(&s), s.ts_ms, None, Some(false)).control,
            Control::Details
        );
    }
}
