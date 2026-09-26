//! 键盘注入:自建 SendInput 管线(带 scan code 的 VK + UNICODE 事件)。
//!
//! 为什么不用 uiautomation crate 的 Keyboard:其 VK 注入 wScan=0(上游缺陷),
//! WinUI 应用(新记事本/计算器)直接丢弃 —— 实测记录见 docs/spike-findings.md。
//! 事件形态:修饰键与命名键走 VK+scan code;字符走 KEYEVENTF_UNICODE(绕 IME,
//! 与修饰键 VK 可自由组合)。本模块是 unsafe 豁免点之一(AGENTS.md §5.5)。

use actl_core::keys::{Key, KeySpec};
use actl_core::{CtlError, ErrorCode};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_BACK, VK_DELETE,
    VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_HOME, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SPACE, VK_TAB, VK_UP,
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

fn modifier_vk(name: &str) -> VIRTUAL_KEY {
    match name {
        "ctrl" => VK_LCONTROL,
        "shift" => VK_LSHIFT,
        "alt" => VK_LMENU,
        _ => VK_LWIN,
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

/// 注入一个按键规格:mods DOWN → 逐键 → mods UP(单次 SendInput 批量,系统保序)。
pub fn send_key_spec(spec: &KeySpec) -> Result<(), CtlError> {
    let mut inputs: Vec<INPUT> = Vec::new();
    for m in &spec.modifiers {
        inputs.push(vk_event(modifier_vk(m), false));
    }
    for key in &spec.keys {
        match key {
            Key::Char(c) => {
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
