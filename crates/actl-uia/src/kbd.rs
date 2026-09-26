//! 键盘注入:自建 SendInput 管线(VK + scan code;字符走 KEYEVENTF_UNICODE)。
//!
//! 两个实测决定(证据链见 docs/spike-findings.md):
//! 1. 不用 uiautomation crate 的 Keyboard:其 VK 注入 wScan=0(上游缺陷)。
//! 2. 修饰键必须用**通用** VK_CONTROL/VK_MENU/VK_SHIFT,不能用左右手变体
//!    (VK_LCONTROL 等):WinUI 输入栈会丢弃变体 DOWN,导致 ctrl+s 被目标
//!    当成裸 's'。AutoHotkey 与 pywinauto 均发通用码
//!    (AHK globaldata.cpp: "Control"→VK_CONTROL;pywinauto keyboard: '^'→VK_CONTROL)。
//!
//! 字符走 KEYEVENTF_UNICODE 绕过 IME。本模块是 unsafe 豁免点之一(AGENTS.md §5.5)。

use actl_core::keys::{Key, KeySpec};
use actl_core::{CtlError, ErrorCode};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_BACK,
    VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_LCONTROL, VK_LEFT,
    VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP, VkKeyScanW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

/// Win32 物理前台窗口标题(None = 取不到)。
/// verify-then-inject 护栏的判据:UIA 焦点视图可能与物理前台不一致,以物理层为准。
pub fn foreground_title() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buf);
        if len <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// 物理键盘焦点所在控件,上溯到根窗口后的标题(None = 取不到)。
/// 焦点归属的**物理**判据:UIA focused element 视图在 WinUI 上实测会谎报
/// (物理层 RichEditD2DPT 已聚焦时仍报别窗口编辑框,金任务 2 误拒绝实证)。
pub fn focused_root_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GUITHREADINFO, GetAncestor, GetGUIThreadInfo,
    };
    unsafe {
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if !GetGUIThreadInfo(0, &mut info).is_ok() || info.hwndFocus.0.is_null() {
            return None;
        }
        let root = GetAncestor(info.hwndFocus, GA_ROOT);
        if root.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(root, &mut buf);
        if len <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

const KEYDOWN: KEYBD_EVENT_FLAGS = KEYBD_EVENT_FLAGS(0);

fn vk_event(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYDOWN },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn uni_event(ch: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: ch,
                dwFlags: if up {
                    KEYEVENTF_KEYUP | KEYEVENTF_UNICODE
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 修饰键用**通用** VK(0x10/0x11/0x12),与 AutoHotkey/pywinauto 一致。
/// 左右手变体(VK_LCONTROL 等)的 DOWN 会被 WinUI 输入栈丢弃,实测 ctrl+s
/// 退化成裸 's' 直接打进了文档。
fn modifier_vk(name: &str) -> VIRTUAL_KEY {
    match name {
        "ctrl" => VK_CONTROL,
        "shift" => VK_SHIFT,
        "alt" => VK_MENU,
        _ => VK_LWIN,
    }
}

/// 检测全集:通用 + 左右手变体,共 11 个(held_modifiers 的检测范围)。
fn all_modifier_vks() -> [VIRTUAL_KEY; 11] {
    [
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ]
}

/// 当前物理按下的修饰键名字(用户按住,或历史注入残留——两者不可靠区分,
/// doc 09 §3.4)。GetKeyState 高位 = 按下。
pub fn held_modifiers() -> Vec<&'static str> {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;
    all_modifier_vks()
        .iter()
        .filter(|vk| unsafe { GetKeyState(vk.0 as i32) } as u16 & 0x8000 != 0)
        .map(|vk| match vk.0 {
            0x11 | 0xA2 | 0xA3 => "ctrl",
            0x10 | 0xA0 | 0xA1 => "shift",
            0x12 | 0xA4 | 0xA5 => "alt",
            _ => "win",
        })
        .collect()
}

fn named_vk(name: &str) -> Option<VIRTUAL_KEY> {
    let vk = match name {
        "enter" => VK_RETURN,
        "esc" => VK_ESCAPE,
        "tab" => VK_TAB,
        "space" => VK_SPACE,
        "del" => VK_DELETE,
        "backspace" => VK_BACK,
        "up" => VK_UP,
        "down" => VK_DOWN,
        "left" => VK_LEFT,
        "right" => VK_RIGHT,
        "home" => VK_HOME,
        "end" => VK_END,
        "pgup" => VK_PRIOR,
        "pgdn" => VK_NEXT,
        _ => {
            // f1..f12:连续 VK 段
            let digits = name.strip_prefix('f')?;
            let n: u8 = digits.parse().ok()?;
            if (1..=12).contains(&n) {
                VIRTUAL_KEY(VK_F1.0 + n as u16 - 1)
            } else {
                return None;
            }
        }
    };
    Some(vk)
}

/// 字符 → (VK, 是否需要 shift),VK 通道是唯一能让修饰键生效的通道。
/// VkKeyScanW 是 AHK 发送字符时的同款转换;返回 None(无映射或死键)时
/// 调用方退回 UNICODE 包(纯文本,修饰键无效)。
fn char_vk(c: char) -> Option<(VIRTUAL_KEY, bool)> {
    let r = unsafe { VkKeyScanW(c as u16) };
    if r == -1 {
        return None;
    }
    let hi = (r >> 8) as u8;
    if hi & 0x80 != 0 {
        return None; // 死键(如重音组合前缀),退回 UNICODE
    }
    Some((VIRTUAL_KEY((r & 0xFF) as u16), hi & 1 != 0))
}

/// 纯编译:KeySpec → 事件序列(不注入,可单测)。双通道规则在此落地:
/// - 带修饰键(ctrl+s 等)→ 字符必须经 VkKeyScanW 映射为 VK 事件:VK 通道才
///   吃修饰键状态,且 IME 不拦截 ctrl+字母这类组合键。映射不出(如 CJK、
///   死键)直接 PROTOCOL 报错——静默降级 UNICODE 会把组合键退化成裸字符
///   (实测:ctrl+s 打出孤立 's')。
/// - 无修饰键(纯文本)→ UNICODE 包:绕过 IME(空格不被当上屏确认吞掉、
///   标点不被全角化),WM_CHAR 直达。
fn compile_key_spec(spec: &KeySpec) -> Result<Vec<INPUT>, CtlError> {
    let mut inputs: Vec<INPUT> = Vec::new();
    for m in &spec.modifiers {
        inputs.push(vk_event(modifier_vk(m), false));
    }
    for key in &spec.keys {
        match key {
            Key::Char(c) => {
                if !spec.modifiers.is_empty() {
                    let (vk, needs_shift) = char_vk(*c).ok_or_else(|| {
                        CtlError::protocol(format!(
                            "char {c:?} cannot be mapped to a virtual key for combo use \
                             (layout or dead key); send it as text via `type` instead, \
                             or use a named key"
                        ))
                    })?;
                    let shift_held = spec.modifiers.iter().any(|m| m == "shift");
                    if needs_shift && !shift_held {
                        inputs.push(vk_event(VK_SHIFT, false));
                    }
                    inputs.push(vk_event(vk, false));
                    inputs.push(vk_event(vk, true));
                    if needs_shift && !shift_held {
                        inputs.push(vk_event(VK_SHIFT, true));
                    }
                } else {
                    let mut buf = [0u16; 2];
                    for unit in c.encode_utf16(&mut buf) {
                        if *unit == 0 {
                            continue;
                        }
                        inputs.push(uni_event(*unit, false));
                        inputs.push(uni_event(*unit, true));
                    }
                }
            }
            Key::Named(n) => {
                let vk = named_vk(n).ok_or_else(|| {
                    CtlError::new(ErrorCode::Internal, format!("unmapped named key {n:?}"))
                })?;
                inputs.push(vk_event(vk, false));
                inputs.push(vk_event(vk, true));
            }
        }
    }
    for m in spec.modifiers.iter().rev() {
        inputs.push(vk_event(modifier_vk(m), true));
    }
    Ok(inputs)
}

/// 注入一个按键规格:等修饰键空闲 → 编译 → 单批注入(自拥有自释放)。
pub fn send_key_spec(spec: &KeySpec) -> Result<(), CtlError> {
    // 修饰键治理(doc 09 §3.4,2026-09-26 修订):不再无条件清场 11 键——
    // 那会踩掉用户物理按住的修饰键。改为:①开场检测,有按住(用户或历史
    // 残留,两者不可区分)则等待后有界拒绝;②注入序列内自拥有自释放
    // (mods DOWN → keys → mods UP,同批原子);③部分发送失败如实报错。
    wait_modifiers_clear()?;
    let inputs = compile_key_spec(spec)?;
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(CtlError::internal(format!(
            "SendInput delivered {sent}/{} events; modifier/key state is now unknown \
             — do not retry blindly, re-observe the target first",
            inputs.len()
        )))
    }
}

/// 等待修饰键物理释放(≤1.5s);到时仍按住 → NOT_ACTIONABLE。
/// click_physical 同样调用:用户按住 ctrl 时物理点击会被劫持成 ctrl+click。
/// 用户按住 ctrl 时注入 's' 会变成 ctrl+s 被劫持成快捷键——宁可拒绝。
pub fn wait_modifiers_clear() -> Result<(), CtlError> {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_millis(actl_core::timing::Timing::load().modifier_wait_ms);
    loop {
        let held = held_modifiers();
        if held.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let held = held.join("/");
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                format!(
                    "modifier key(s) physically held: {held}; injection would be \
                     hijacked into shortcuts — release them (or tap the key to clear \
                     a stale state) and retry"
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(
            actl_core::timing::Timing::load().poll_ms,
        ));
    }
}

/// 半事件注入(key-down/key-up 命令):只发编译产物中的 DOWN 或 UP 半边。
/// 用于按住类操作(shift 拖拽、游戏按键)。**必须与配对命令成对使用**:
/// 进程异常退出会留下按住状态——后续注入会被修饰键空置检查拒绝
/// (held_modifiers),用户敲一下该键即可恢复,这是有意的 fail-closed。
pub fn send_key_partial(spec: &KeySpec, down: bool) -> Result<(), CtlError> {
    if down {
        wait_modifiers_clear()?;
    }
    let inputs: Vec<INPUT> = compile_key_spec(spec)?
        .into_iter()
        .filter(|i| {
            let ki = unsafe { i.Anonymous.ki };
            ki.dwFlags.contains(KEYEVENTF_KEYUP) != down
        })
        .collect();
    if inputs.is_empty() {
        return Err(CtlError::protocol("nothing to send for this half"));
    }
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(CtlError::internal(format!(
            "SendInput delivered {sent}/{} events; key state is now unknown",
            inputs.len()
        )))
    }
}
#[cfg(test)]
mod tests {
    //! 编译层回归(doc 09 §8 第一批):只断言事件序列,不注入。
    //! 布局依赖说明:字母的 VkKeyScanW 映射在 en-US / zh-CN 等标准布局下一致。
    use super::*;
    use actl_core::keys::parse_key_expr;

    fn compile(expr: &str) -> Vec<INPUT> {
        let spec = parse_key_expr(expr).unwrap();
        compile_key_spec(&spec).unwrap()
    }

    /// (VK 码, 是否 UP) 序列;UNICODE 事件 wVk=0,不会与 VK 断言混淆。
    fn vk_seq(events: &[INPUT]) -> Vec<(u16, bool)> {
        events
            .iter()
            .map(|i| {
                let ki = unsafe { i.Anonymous.ki };
                (ki.wVk.0, ki.dwFlags.contains(KEYEVENTF_KEYUP))
            })
            .collect()
    }

    fn is_unicode(i: &INPUT) -> bool {
        unsafe { i.Anonymous.ki.dwFlags.contains(KEYEVENTF_UNICODE) }
    }

    #[test]
    fn combo_chars_go_vk_channel() {
        // 回归:UNICODE 包不吃修饰键,ctrl+s 曾退化成裸 's'(战记 #2)
        let ev = compile("ctrl+s");
        assert_eq!(
            ev.len(),
            4,
            "ctrl↓ s↓ s↑ ctrl↑;清场事件属于发送层,不进编译产物"
        );
        assert!(
            ev.iter().all(|e| !is_unicode(e)),
            "组合键成员禁止 UNICODE 通道"
        );
        assert_eq!(
            vk_seq(&ev),
            vec![(0x11, false), (0x53, false), (0x53, true), (0x11, true)]
        );
    }

    #[test]
    fn plain_text_goes_unicode_channel() {
        // 回归:纯文本走 VK 会被 IME 改写(空格吞成上屏、标点全角化,战记 #3)。
        // 键表达式不支持多字符("ab" 非法),这里手工构词——type 命令就是这么做的
        let spec = KeySpec {
            modifiers: vec![],
            keys: vec![Key::Char('a'), Key::Char('b')],
        };
        let ev = compile_key_spec(&spec).unwrap();
        assert_eq!(ev.len(), 4);
        assert!(ev.iter().all(is_unicode), "纯文本固定 UNICODE 通道");
        let units: Vec<u16> = ev.iter().map(|i| unsafe { i.Anonymous.ki.wScan }).collect();
        assert_eq!(
            units,
            vec![b'a' as u16, b'a' as u16, b'b' as u16, b'b' as u16]
        );
    }

    #[test]
    fn unmappable_combo_char_is_rejected() {
        // 静默降级 = 组合键退化裸字符;必须 PROTOCOL 报错(doc 09 §3.1 剩余项)
        let spec = KeySpec {
            modifiers: vec!["ctrl".into()],
            keys: vec![Key::Char('中')],
        };
        let err = match compile_key_spec(&spec) {
            Err(e) => e,
            Ok(_) => panic!("unmappable combo char must be rejected, got events"),
        };
        assert_eq!(err.code, ErrorCode::Protocol);
        assert!(err.message.contains("type"));
    }

    #[test]
    fn combo_with_shift_required_char_adds_shift() {
        // 键表达式大小写不敏感(to_lowercase),"ctrl+A" 会归一成 ctrl+a;
        // 大写语义必须显式写 ctrl+shift+a。shift 包裹路径用无大小写但
        // VkKeyScanW 声明需要 shift 的字符触达:'!' → VK 0x31 + shift
        let ev = compile("ctrl+!");
        let seq = vk_seq(&ev);
        assert_eq!(
            seq,
            vec![
                (0x11, false),       // ctrl ↓
                (VK_SHIFT.0, false), // shift ↓(VkKeyScanW 补位)
                (0x31, false),       // '1' 键 ↓('!' 所在键)
                (0x31, true),        // ↑
                (VK_SHIFT.0, true),
                (0x11, true),
            ]
        );
    }

    #[test]
    fn f10_to_f12_compile_to_vk() {
        // 与 actl-core 解析层回归联动:后端 VK 段 1..=12 必须全可达
        for (name, vk) in [("f1", 0x70u16), ("f10", 0x79), ("f12", 0x7B)] {
            let seq = vk_seq(&compile(name));
            assert_eq!(seq, vec![(vk, false), (vk, true)], "{name}");
        }
    }

    #[test]
    fn named_keys_and_multi_key_sequences() {
        let ev = compile("enter");
        assert_eq!(vk_seq(&ev), vec![(VK_RETURN.0, false), (VK_RETURN.0, true)]);
        // 多键序列:mods 只在整段两端
        let ev = compile("ctrl+a+b");
        let seq = vk_seq(&ev);
        assert_eq!(seq.first().copied(), Some((0x11, false)));
        assert_eq!(seq.last().copied(), Some((0x11, true)));
        assert_eq!(seq.len(), 6);
    }
}
