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

/// 清场全集:通用 + 左右手变体,共 11 个。
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

/// 对全部修饰键发送 UP(幂等):清除任何残留的按下状态。
pub fn release_all_modifiers() {
    let ups: Vec<INPUT> = all_modifier_vks()
        .iter()
        .map(|vk| vk_event(*vk, true))
        .collect();
    if !ups.is_empty() {
        unsafe {
            SendInput(&ups, std::mem::size_of::<INPUT>() as i32);
        }
    }
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

/// 注入一个按键规格:先清场幽灵修饰键 → mods DOWN → 逐键 → mods UP → 再清场。
pub fn send_key_spec(spec: &KeySpec) -> Result<(), CtlError> {
    // 开场清场:清除历史注入可能残留的修饰键按下状态
    release_all_modifiers();
    let mut inputs: Vec<INPUT> = Vec::new();
    for m in &spec.modifiers {
        inputs.push(vk_event(modifier_vk(m), false));
    }
    for key in &spec.keys {
        match key {
            Key::Char(c) => {
                // 双通道规则(IME 实测教训,docs/spike-findings.md):
                // - 带修饰键(ctrl+s 等)→ 字符经 VkKeyScanW 映射为 VK 事件:
                //   VK 通道才吃修饰键状态;且 IME 不拦截 ctrl+字母这类组合键。
                // - 无修饰键(纯文本)→ UNICODE 包:绕过 IME(否则空格被当作
                //   组合上屏键吞掉、半角标点被全角化),WM_CHAR 直达。
                let vk_channel = !spec.modifiers.is_empty();
                if vk_channel {
                    if let Some((vk, needs_shift)) = char_vk(*c) {
                        let shift_held = spec.modifiers.iter().any(|m| m == "shift");
                        if needs_shift && !shift_held {
                            inputs.push(vk_event(VK_SHIFT, false));
                        }
                        inputs.push(vk_event(vk, false));
                        inputs.push(vk_event(vk, true));
                        if needs_shift && !shift_held {
                            inputs.push(vk_event(VK_SHIFT, true));
                        }
                        continue;
                    }
                }
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    if *unit == 0 {
                        continue;
                    }
                    inputs.push(uni_event(*unit, false));
                    inputs.push(uni_event(*unit, true));
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

    // 结尾清场:把全部修饰键 UP 一遍,杜绝任何 UP 丢失导致的幽灵修饰键
    // (实测事故:一次残留会让后续所有注入被解释为 ctrl+key 而被目标丢弃)
    for vk in all_modifier_vks() {
        inputs.push(vk_event(vk, true));
    }

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(CtlError::internal(format!(
            "SendInput delivered {sent}/{} events",
            inputs.len()
        )))
    }
}
