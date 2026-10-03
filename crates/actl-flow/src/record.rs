//! 录制 trace → 初版 flow YAML 的机械编译器(docs/06「GUI 操作录制 R1」)。
//! 纯函数、零 Windows 依赖——捕获层(actl-uia)产 trace,本层产**可 flow-validate
//! 的草稿**;TODO 项以注释形态存在(不是步骤,文件始终可验证),每步带
//! `# evidence: trace:<行号>` 证据引用——agent 改写轮(docs/79 §6)的锚点。
//! 编译纪律:不猜测目标(反查失败/无可用身份→TODO)、注入事件不编译、
//! 密码字段内容永不进产物、IME 键序列不做 type(重放必错,Quicker 实证)。

use serde_json::Value;

/// 可执行草稿步骤(全部是写步骤:机械 verify + retry: never 恒配——
/// flow schema 要求副作用必带验证,默认 retry=read_only 不可重放写入)。
pub struct DraftStep {
    pub title: String,
    pub command: String,
    pub args: Vec<String>,
    /// 机械验证参数(verify 命令):click→--exists 目标仍在;type→--value 读回
    /// 文本(真后置条件);press→焦点元素 --exists。效果级验证是 R2/agent 改写轮的事。
    pub verify: Vec<String>,
    /// trace 行号(1 起,含会话头行)。
    pub evidence: usize,
}

/// TODO 项:机械编译不可安全落为步骤的事件(注释形态,证据可溯)。
pub struct DraftTodo {
    pub note: String,
    pub evidence: usize,
}

/// 编译产物。
pub struct Draft {
    pub name: String,
    pub events: usize,
    pub steps: Vec<DraftStep>,
    pub todos: Vec<DraftTodo>,
}

/// trace 全量(会话头 + 事件行)→ 草稿。stop_reason 只入渲染头注释。
pub fn compile(trace: &[Value]) -> Draft {
    let name = trace
        .first()
        .and_then(|header| header["id"].as_str())
        .map(|id| id.to_string())
        .unwrap_or_else(|| "record".into());
    let mut steps = Vec::new();
    let mut todos = Vec::new();
    let mut run: Option<TypingRun> = None;

    for (index, event) in trace.iter().enumerate().skip(1) {
        let line = index + 1;
        match event["kind"].as_str().unwrap_or_default() {
            "click" => {
                flush_run(&mut run, &mut steps, &mut todos);
                if event["injected"].as_bool().unwrap_or(false) {
                    continue; // 注入事件如实在 trace,不编译(录的是人的演示)
                }
                compile_click(event, line, &mut steps, &mut todos);
            }
            "key" => {
                if event["injected"].as_bool().unwrap_or(false) {
                    continue;
                }
                if event["masked"].as_bool().unwrap_or(false) {
                    flush_run(&mut run, &mut steps, &mut todos);
                    todos.push(DraftTodo {
                        note:
                            "password field keys not recorded (privacy; target needs manual fill)"
                                .into(),
                        evidence: line,
                    });
                    continue;
                }
                compile_key(event, line, &mut run, &mut steps, &mut todos);
            }
            _ => continue, // 会话头/未知事件不编译
        }
    }
    flush_run(&mut run, &mut steps, &mut todos);
    Draft {
        name,
        events: trace.len().saturating_sub(1),
        steps,
        todos,
    }
}

fn compile_click(
    event: &Value,
    line: usize,
    steps: &mut Vec<DraftStep>,
    todos: &mut Vec<DraftTodo>,
) {
    let button = event["button"].as_str().unwrap_or_default();
    let window_title = event["window"]["title"].as_str().unwrap_or_default();
    let x = event["x"].as_i64().unwrap_or_default();
    let y = event["y"].as_i64().unwrap_or_default();
    if button != "left" {
        todos.push(DraftTodo {
            note: format!(
                "{button} click at ({x},{y}) in {window_title} — non-left buttons are R2"
            ),
            evidence: line,
        });
        return;
    }
    let element = event.get("element").filter(|e| !e.is_null());
    let selector = element.and_then(element_selector);
    match selector {
        Some(selector) => {
            let title = format!("click {selector} in {window_title}");
            steps.push(DraftStep {
                title,
                command: "click".into(),
                args: vec![selector.clone(), "--app".into(), window_title.into()],
                verify: vec![
                    "--exists".into(),
                    selector,
                    "--app".into(),
                    window_title.into(),
                ],
                evidence: line,
            });
        }
        None => {
            let lookup = event["lookup"].as_str().unwrap_or_default();
            let note = if element.is_none() {
                format!("unresolved click at ({x},{y}) in {window_title} — lookup {lookup}")
            } else {
                format!(
                    "click at ({x},{y}) in {window_title} resolved without usable identity (no automation_id/name)"
                )
            };
            todos.push(DraftTodo {
                note,
                evidence: line,
            });
        }
    }
}

fn element_selector(element: &Value) -> Option<String> {
    let automation_id = element["automation_id"].as_str().unwrap_or_default();
    if !automation_id.is_empty() {
        return Some(format!("id:{automation_id}"));
    }
    let name = element["name"].as_str().unwrap_or_default();
    (!name.is_empty()).then(|| format!("name:{name}"))
}

struct TypingRun {
    text: String,
    /// 不可解析键(IME/未知 VK)计数——置位则整段 TODO,不做 type。
    blocked: usize,
    evidence: usize,
    focus_title: Option<String>,
    focus_element: Option<Value>,
}

fn compile_key(
    event: &Value,
    line: usize,
    run: &mut Option<TypingRun>,
    steps: &mut Vec<DraftStep>,
    todos: &mut Vec<DraftTodo>,
) {
    let vk = event["vk"].as_u64().unwrap_or_default();
    let mods: Vec<String> = event["mods"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let has_ctrl_like = mods.iter().any(|m| m == "ctrl" || m == "alt" || m == "win");
    let has_shift = mods.iter().any(|m| m == "shift");

    if vk_modifier(vk).is_some() {
        return; // 修饰键自身的事件是噪音;组合语义由被修饰键的 mods 携带
    }
    if has_ctrl_like {
        // 快捷键:本键事件自带完整组合(如 ctrl+s 的 s 带 mods=["ctrl"])。
        // 验证需要焦点元素身份(--exists);无身份的快捷键机械不可验证 → TODO。
        flush_run(run, steps, todos);
        let focus_title = event["focus"]["title"].as_str().unwrap_or_default();
        let focus_selector = event.get("focus_element").and_then(element_selector);
        match (vk_key_name(vk), focus_selector) {
            (Some(key), Some(focus_selector)) => {
                let expression = format!("{}{key}", mods_prefix(&mods));
                steps.push(DraftStep {
                    title: format!("press {expression} in {focus_title}"),
                    command: "press".into(),
                    args: vec![expression, "--app".into(), focus_title.into()],
                    verify: vec![
                        "--exists".into(),
                        focus_selector,
                        "--app".into(),
                        focus_title.into(),
                    ],
                    evidence: line,
                });
            }
            (None, _) => todos.push(DraftTodo {
                note: format!("hotkey with unresolvable key (vk {vk}) in {focus_title}"),
                evidence: line,
            }),
            (Some(_), None) => todos.push(DraftTodo {
                note: format!(
                    "hotkey without resolvable focus identity in {focus_title} — add a verify by hand"
                ),
                evidence: line,
            }),
        }
        return;
    }
    if let Some(name) = vk_terminator_name(vk) {
        // Enter/Tab/Esc/F 键:终结键恰恰改变焦点/关窗——对旧焦点元素断言存在是
        // 系统性错误猜测(不猜测纪律)→ TODO(笔记含完整表达式,人工/agent 补 verify)。
        flush_run(run, steps, todos);
        let focus_title = event["focus"]["title"].as_str().unwrap_or_default();
        let expression = if has_shift {
            format!("shift+{name}")
        } else {
            name
        };
        todos.push(DraftTodo {
            note: format!(
                "press {expression} in {focus_title} — terminator keys change focus/windows; needs a manual verify"
            ),
            evidence: line,
        });
        return;
    }
    if vk_gesture(vk) {
        // 内容编辑手势(退格/删除/方向):与文本语义纠缠,机械不可安全表达。
        flush_run(run, steps, todos);
        let focus_title = event["focus"]["title"].as_str().unwrap_or_default();
        todos.push(DraftTodo {
            note: format!(
                "editing gesture (vk {vk}) in {focus_title} — merge into typed text by hand"
            ),
            evidence: line,
        });
        return;
    }
    // 可打印候选:并入打字运行(shift 影响大小写;shift+非字母是布局相关符号,不猜)。
    let ch = match vk_char(vk, has_shift) {
        Some(ch) => ch,
        None => {
            if let Some(current) = run.as_mut() {
                current.blocked += 1;
            } else {
                run.replace(TypingRun {
                    text: String::new(),
                    blocked: 1,
                    evidence: line,
                    focus_title: event["focus"]["title"].as_str().map(str::to_string),
                    focus_element: event.get("focus_element").filter(|e| !e.is_null()).cloned(),
                });
            }
            return;
        }
    };
    if let Some(current) = run.as_mut() {
        current.text.push(ch);
    } else {
        run.replace(TypingRun {
            text: ch.to_string(),
            blocked: 0,
            evidence: line,
            focus_title: event["focus"]["title"].as_str().map(str::to_string),
            focus_element: event.get("focus_element").filter(|e| !e.is_null()).cloned(),
        });
    }
}

fn flush_run(run: &mut Option<TypingRun>, steps: &mut Vec<DraftStep>, todos: &mut Vec<DraftTodo>) {
    let Some(current) = run.take() else {
        return;
    };
    if current.text.is_empty() && current.blocked == 0 {
        return;
    }
    let title = current.focus_title.clone().unwrap_or_default();
    if current.blocked > 0 {
        todos.push(DraftTodo {
            note: format!(
                "key sequence with unresolved/IME keys ({} blocked) in {title} — Chinese text needs the --paste/Value channel (R2)",
                current.blocked
            ),
            evidence: current.evidence,
        });
        return;
    }
    let selector = current.focus_element.as_ref().and_then(element_selector);
    match selector {
        Some(selector) => steps.push(DraftStep {
            title: format!("type \"{}\" into {selector}", current.text),
            command: "type".into(),
            args: vec![
                selector.clone(),
                current.text.clone(),
                "--app".into(),
                title.clone(),
            ],
            // 读回核验:打字的真实后置条件——目标控件值 == 所打文本。
            verify: vec![
                "--value".into(),
                selector,
                current.text,
                "--property".into(),
                "value".into(),
                "--app".into(),
                title.clone(),
            ],
            evidence: current.evidence,
        }),
        None => todos.push(DraftTodo {
            note: format!(
                "typed text \"{}\" without resolvable focus identity in {title} — target needs manual selection",
                current.text
            ),
            evidence: current.evidence,
        }),
    }
}

/// 修饰键名(vk → ctrl/shift/alt/win;与捕获侧 vk_modifiers 同表)。
fn vk_modifier(vk: u64) -> Option<&'static str> {
    Some(match vk {
        0x11 | 0xA2 | 0xA3 => "ctrl",
        0x10 | 0xA0 | 0xA1 => "shift",
        0x12 | 0xA4 | 0xA5 => "alt",
        0x5B | 0x5C => "win",
        _ => return None,
    })
}

fn mods_prefix(mods: &[String]) -> String {
    mods.iter()
        .filter(|m| matches!(m.as_str(), "ctrl" | "alt" | "win" | "shift"))
        .map(|m| format!("{m}+"))
        .collect::<String>()
}

/// press 表达式的键名(kbd.rs named_vk 的反表;仅覆盖其同名集)。
fn vk_key_name(vk: u64) -> Option<String> {
    Some(match vk {
        0x0D => "enter".into(),
        0x1B => "esc".into(),
        0x09 => "tab".into(),
        0x20 => "space".into(),
        0x2E => "del".into(),
        0x08 => "backspace".into(),
        0x26 => "up".into(),
        0x28 => "down".into(),
        0x25 => "left".into(),
        0x27 => "right".into(),
        0x24 => "home".into(),
        0x23 => "end".into(),
        0x21 => "pgup".into(),
        0x22 => "pgdn".into(),
        0x30..=0x39 => char::to_string(&char::from_digit(vk as u32 - 0x30, 10)?),
        0x41..=0x5A => char::from(b'a' + vk as u8 - b'A').to_string(),
        0x70..=0x7B => format!("f{}", vk - 0x6F),
        _ => return None,
    })
}

/// 终结键:打字运行的边界,自身编译为独立 press 步。
fn vk_terminator_name(vk: u64) -> Option<String> {
    match vk {
        0x0D => Some("enter".into()),
        0x09 => Some("tab".into()),
        0x1B => Some("esc".into()),
        0x70..=0x7B => Some(format!("f{}", vk - 0x6F)),
        _ => None,
    }
}

/// 内容编辑手势:与文本语义纠缠,机械编译不做表达。
fn vk_gesture(vk: u64) -> bool {
    matches!(
        vk,
        0x08 | 0x2D | 0x2E | 0x25..=0x28 | 0x21 | 0x22 | 0x24 | 0x23
    )
}

/// 打字字符解析:核心 ASCII 布局无关区(字母/数字/空格/小键盘);shift 仅字母可解析。
fn vk_char(vk: u64, shift: bool) -> Option<char> {
    Some(match vk {
        0x41..=0x5A => {
            let letter = (b'a' + (vk as u8 - b'A')) as char;
            if shift {
                letter.to_ascii_uppercase()
            } else {
                letter
            }
        }
        0x30..=0x39 if !shift => char::from_digit(vk as u32 - 0x30, 10)?,
        0x60..=0x69 if !shift => char::from_digit(vk as u32 - 0x60, 10)?,
        0x20 if !shift => ' ',
        _ => return None,
    })
}

/// 渲染 flow-v1 兼容草稿(必须可 flow-validate;TODO 是注释不是步骤)。
pub fn render(draft: &Draft, stop_reason: &str, trace_path: &str) -> String {
    use crate::route::yaml_scalar;
    let mut out = String::new();
    out.push_str(
        "# actl recording draft (mechanical compile, UNVERIFIED) — docs/06 R1 / docs/79\n",
    );
    out.push_str(&format!(
        "# session: {}; events: {}; stop_reason: {}; trace: {}\n",
        draft.name, draft.events, stop_reason, trace_path
    ));
    out.push_str("# TODO items below are comments, not steps (file stays flow-validate clean);\n");
    out.push_str("# rewrite guidance: docs/79 §6 (agent rewrite pass, replay comparison).\n");
    out.push_str("version: 1\n");
    out.push_str(&format!("name: {}\n", yaml_scalar(&draft.name)));
    // steps 键恒在(空也发 [])——非探索计划的 flow-validate 要求 steps 数组存在。
    if draft.steps.is_empty() {
        out.push_str("steps: []\n");
    } else {
        out.push_str("steps:\n");
        for (index, step) in draft.steps.iter().enumerate() {
            out.push_str(&format!("  - id: s{}\n", index + 1));
            out.push_str(&format!("    title: {}\n", yaml_scalar(&step.title)));
            out.push_str("    run:\n");
            out.push_str(&format!("      command: {}\n", yaml_scalar(&step.command)));
            let args: Vec<String> = step.args.iter().map(|a| yaml_scalar(a)).collect();
            out.push_str(&format!("      args: [{}]\n", args.join(", ")));
            // 机械验证 + retry: never(flow schema:副作用必带验证,默认 read_only
            // 不可重放写入)——效果级验证留给 agent 改写轮。
            out.push_str("    verify:\n");
            out.push_str("      command: verify\n");
            let verify: Vec<String> = step.verify.iter().map(|a| yaml_scalar(a)).collect();
            out.push_str(&format!("      args: [{}]\n", verify.join(", ")));
            out.push_str("    retry: never\n");
            out.push_str(&format!("    # evidence: trace:{}\n", step.evidence));
        }
    }
    if !draft.todos.is_empty() {
        out.push_str("# TODO (not steps; evidence in trace):\n");
        for todo in &draft.todos {
            let note = todo.note.replace('\n', " ");
            out.push_str(&format!("# - [trace:{}] {}\n", todo.evidence, note));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn header() -> Value {
        json!({"kind": "session", "id": "rec-test", "started_ms": 1, "max_seconds": 600})
    }

    fn click(x: i64, y: i64, id: &str, name: &str, title: &str) -> Value {
        json!({
            "t": 2, "kind": "click", "button": "left", "x": x, "y": y,
            "window": {"hwnd": 1, "title": title, "pid": 2, "rect": [0,0,10,10], "dpi": 96},
            "element": {"automation_id": id, "name": name, "control_type": "Button", "runtime_id": [1], "is_password": false},
            "lookup": "ok",
        })
    }

    fn key(vk: u64, mods: &[&str], title: &str, element: Option<Value>) -> Value {
        json!({
            "t": 3, "kind": "key", "vk": vk, "mods": mods, "ime": vk == 0xE5,
            "focus": {"hwnd": 5, "title": title},
            "focus_element": element,
        })
    }

    fn edit_element(id: &str, name: &str) -> Value {
        json!({"automation_id": id, "name": name, "control_type": "Edit", "runtime_id": [2], "is_password": false})
    }

    #[test]
    fn empty_trace_renders_minimal_valid_flow() {
        let draft = compile(&[header()]);
        assert!(draft.steps.is_empty());
        assert!(draft.todos.is_empty());
        let text = render(&draft, "requested", "records/rec-test/trace.jsonl");
        assert!(text.contains("version: 1"));
        let mut options = serde_saphyr::Options::default();
        options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
        let parsed: serde_json::Value =
            serde_saphyr::from_str_with_options(&text, options).expect("draft parses");
        assert_eq!(parsed["name"], "rec-test");
        assert_eq!(parsed["steps"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn resolved_click_prefers_automation_id() {
        let draft = compile(&[header(), click(10, 20, "num7Button", "七", "计算器")]);
        assert_eq!(draft.steps.len(), 1);
        let step = &draft.steps[0];
        assert_eq!(step.command, "click");
        assert_eq!(step.args, vec!["id:num7Button", "--app", "计算器"]);
        assert_eq!(
            step.verify,
            vec!["--exists", "id:num7Button", "--app", "计算器"]
        );
        assert_eq!(step.evidence, 2);
    }

    #[test]
    fn click_with_name_only_uses_name_selector() {
        let mut event = click(1, 2, "", "保存", "记事本");
        event["element"]["automation_id"] = json!("");
        let draft = compile(&[header(), event]);
        assert_eq!(draft.steps[0].args[0], "name:保存");
    }

    #[test]
    fn unresolved_and_non_left_clicks_become_todos() {
        let mut unresolved = click(3, 4, "x", "y", "t");
        unresolved["element"] = Value::Null;
        unresolved["lookup"] = json!("failed:E_FAIL");
        let mut right = click(5, 6, "b", "n", "t");
        right["button"] = json!("right");
        let draft = compile(&[header(), unresolved, right]);
        assert!(draft.steps.is_empty());
        assert_eq!(draft.todos.len(), 2);
        assert!(draft.todos[0].note.contains("unresolved click"));
        assert!(draft.todos[1].note.contains("right click"));
        assert_eq!(draft.todos[0].evidence, 2);
        assert_eq!(draft.todos[1].evidence, 3);
    }

    #[test]
    fn injected_events_are_not_compiled() {
        let mut injected_click = click(1, 2, "id", "name", "t");
        injected_click["injected"] = json!(true);
        let injected_key =
            json!({"t": 3, "kind": "key", "vk": 0x53, "mods": ["ctrl"], "injected": true});
        let draft = compile(&[header(), injected_click, injected_key]);
        assert!(draft.steps.is_empty());
        assert!(draft.todos.is_empty());
    }

    #[test]
    fn hotkey_compiles_to_press_with_focus_verify() {
        let draft = compile(&[
            header(),
            key(
                0x53,
                &["ctrl"],
                "记事本",
                Some(edit_element("edit", "文本框")),
            ), // ctrl+s
        ]);
        assert_eq!(draft.steps.len(), 1);
        assert_eq!(draft.steps[0].command, "press");
        assert_eq!(draft.steps[0].args, vec!["ctrl+s", "--app", "记事本"]);
        assert_eq!(
            draft.steps[0].verify,
            vec!["--exists", "id:edit", "--app", "记事本"]
        );
    }

    #[test]
    fn hotkey_without_focus_identity_becomes_todo() {
        let draft = compile(&[
            header(),
            key(0x53, &["ctrl"], "记事本", None), // ctrl+s,无焦点元素
        ]);
        assert!(draft.steps.is_empty());
        assert!(
            draft.todos[0]
                .note
                .contains("without resolvable focus identity")
        );
    }

    #[test]
    fn ascii_typing_compiles_to_type_with_focus_selector() {
        let draft = compile(&[
            header(),
            key(
                0x48,
                &[],
                "另存为",
                Some(edit_element("FileNameControl", "文件名")),
            ), // h
            key(0x49, &[], "另存为", None), // i (同一焦点,元素沿用首个事件)
        ]);
        assert_eq!(draft.steps.len(), 1);
        let step = &draft.steps[0];
        assert_eq!(step.command, "type");
        assert_eq!(
            step.args,
            vec!["id:FileNameControl", "hi", "--app", "另存为"]
        );
        // 读回核验:值断言 = 打字的真实后置条件。
        assert_eq!(
            step.verify,
            vec![
                "--value",
                "id:FileNameControl",
                "hi",
                "--property",
                "value",
                "--app",
                "另存为"
            ]
        );
        assert_eq!(step.evidence, 2); // 运行首键的 trace 行
    }

    #[test]
    fn shift_letters_uppercase() {
        let draft = compile(&[
            header(),
            key(0x10, &[], "t", None), // shift down(modifier noise)
            key(0x48, &["shift"], "t", Some(edit_element("e", "e"))), // H
            key(0x49, &[], "t", None), // i
        ]);
        assert_eq!(draft.steps[0].args[1], "Hi");
    }

    #[test]
    fn enter_splits_typing_but_becomes_todo() {
        // Enter 是终结键:机械验证会系统性错(提交后焦点/窗口改变)→ TODO,打字段照常编译。
        let draft = compile(&[
            header(),
            key(0x48, &[], "t", Some(edit_element("e", "e"))), // h
            key(0x0D, &[], "t", None),                         // enter
            key(0x53, &[], "t", Some(edit_element("e", "e"))), // s
        ]);
        assert_eq!(draft.steps.len(), 2);
        assert_eq!(draft.steps[0].command, "type");
        assert_eq!(draft.steps[0].args[1], "h");
        assert_eq!(draft.steps[1].command, "type");
        assert_eq!(draft.steps[1].args[1], "s");
        assert_eq!(draft.todos.len(), 1);
        assert!(draft.todos[0].note.contains("enter"));
        assert!(draft.todos[0].note.contains("terminator"));
    }

    #[test]
    fn ime_sequence_becomes_todo_not_type() {
        let draft = compile(&[
            header(),
            key(0xE5, &[], "记事本", None), // IME 处理键
            key(0x41, &[], "记事本", None), // 后续提交字母
        ]);
        assert!(draft.steps.is_empty());
        assert_eq!(draft.todos.len(), 1);
        assert!(draft.todos[0].note.contains("IME"));
    }

    #[test]
    fn masked_keys_become_privacy_todo() {
        let masked = json!({"t": 4, "kind": "key", "masked": true});
        let draft = compile(&[header(), masked]);
        assert!(draft.steps.is_empty());
        assert!(draft.todos[0].note.contains("password"));
    }

    #[test]
    fn typing_without_resolvable_target_becomes_todo() {
        let draft = compile(&[
            header(),
            key(0x41, &[], "记事本", None), // 无焦点元素身份
        ]);
        assert!(draft.steps.is_empty());
        assert!(
            draft.todos[0]
                .note
                .contains("without resolvable focus identity")
        );
    }

    #[test]
    fn rendered_draft_round_trips_through_saphyr() {
        let draft = compile(&[
            header(),
            click(10, 20, "num7Button", "七", "计算器"),
            key(
                0x53,
                &["ctrl"],
                "计算器",
                Some(edit_element("results", "显示")),
            ),
            key(0x48, &[], "记事本", Some(edit_element("e", "编辑"))),
            key(0x0D, &[], "记事本", None),
        ]);
        let text = render(&draft, "requested", "records/rec-test/trace.jsonl");
        let mut options = serde_saphyr::Options::default();
        options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
        let parsed: serde_json::Value =
            serde_saphyr::from_str_with_options(&text, options).expect("draft parses");
        assert_eq!(parsed["version"], 1);
        assert_eq!(parsed["name"], "rec-test");
        let steps = parsed["steps"].as_array().expect("steps present");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0]["run"]["command"], "click");
        assert_eq!(steps[0]["run"]["args"][0], "id:num7Button");
        assert_eq!(steps[0]["run"]["args"][2], "计算器");
        assert_eq!(steps[0]["verify"]["args"][0], "--exists");
        assert_eq!(steps[0]["verify"]["args"][1], "id:num7Button");
        assert_eq!(steps[0]["retry"], "never");
        assert_eq!(steps[1]["run"]["args"][0], "ctrl+s");
        assert_eq!(steps[1]["verify"]["args"][1], "id:results");
        assert_eq!(steps[2]["run"]["args"][1], "h");
        assert_eq!(steps[2]["verify"]["args"][0], "--value");
        assert_eq!(steps[2]["verify"]["args"][2], "h");
        // enter 是终结键 → TODO 注释,不是步骤。
        assert!(text.contains("# - [trace:5] press enter"));
        assert_eq!(steps[0]["id"], "s1");
    }
}
