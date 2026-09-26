//! actl-signal:显示信号伴随程序(docs/11 显示层 Phase A/B、docs/13 拉取哲学)。
//!
//! 形态:置顶迷你状态条(WS_EX_NOACTIVATE 不抢焦点)+ 托盘锚点 +
//! Ctrl+Alt+F12 紧急停止热键。数据全部拉取:轮询输入占用锁
//! (actl-input-lock,"正在操控"的权威信号)+ state.json(命令/阶段/占用),
//! 零事件订阅(常驻订阅的事故价目表见 docs/13 §3)。
//! 点击状态条 = 清除停止标志(粘性标志只有显式动作清除);托盘消息 = 提示。
//! 本文件属 actl-uia(仓库唯一 unsafe 豁免点,AGENTS §5.5)。

#![allow(unsafe_code)]
#![allow(unsafe_op_in_unsafe_fn)] // edition 2024:unsafe fn 体内调用需显式块,本文件整体即 unsafe 块(豁免点)

use actl_core::state::{Phase, SessionState, SignalPaths};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DeleteObject, DrawTextW,
    EndPaint, FillRect, HGDIOBJ, InvalidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::OpenMutexW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey, VK_F12,
};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA,
    GetMessageW, HWND_BROADCAST, IDI_APPLICATION, KillTimer, LWA_ALPHA, LoadIconW, MB_OK,
    MB_SETFOREGROUND, MSG, MessageBoxW, PostQuitMessage, RegisterClassW, SPI_GETWORKAREA,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetLayeredWindowAttributes, SetTimer,
    SystemParametersInfoW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY,
    WM_HOTKEY, WM_LBUTTONDOWN, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

const WIDTH: i32 = 420;
const HEIGHT: i32 = 80;
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 250;
const HOTKEY_ID: i32 = 1;
const STALE_MS: u64 = 10_000;

/// 渲染模式(颜色 + 语义;颜色之外由文字承载,docs/11 §11)
enum Mode {
    /// 已请求停止(红):在途动作将在边界退出;点击清除
    Stopped,
    /// 正在操控物理输入(琥珀)
    Active,
    /// 命令执行中,未占用物理输入(浅)
    Busy,
    /// 空闲(深灰)
    Idle,
    /// 状态失联(暗):如实显示,不显示成功
    Stale,
}

struct Snapshot {
    mode: Mode,
    line1: String,
    line2: String,
}

fn state_line(s: &SessionState) -> String {
    let step = s
        .step
        .as_deref()
        .map(|x| format!(" · 步 {x}"))
        .unwrap_or_default();
    let app = s
        .app
        .as_deref()
        .map(|a| format!(" · 目标 {a}"))
        .unwrap_or_default();
    format!("{}{}{}", s.command, app, step)
}

fn poll() -> Snapshot {
    let paths = SignalPaths::default();
    if paths.stop_requested() {
        return Snapshot {
            mode: Mode::Stopped,
            line1: "已请求停止 · 在途动作将在边界退出".into(),
            line2: "点击本条清除停止标志后可恢复".into(),
        };
    }
    let lock_held = input_lock_held();
    let st = paths.read_state();
    let age = st
        .as_ref()
        .map(|s| actl_core::state::unix_ms().saturating_sub(s.ts_ms))
        .unwrap_or(u64::MAX);
    if lock_held {
        return Snapshot {
            mode: Mode::Active,
            line1: st
                .as_ref()
                .map(state_line)
                .unwrap_or_else(|| "未知命令".into()),
            line2: "正在操控物理输入(键盘/鼠标)".into(),
        };
    }
    match st {
        Some(s) if age <= STALE_MS && s.phase == Phase::Running => Snapshot {
            mode: Mode::Busy,
            line1: state_line(&s),
            line2: "命令执行中 · 未占用物理输入".into(),
        },
        Some(s) if age <= STALE_MS => Snapshot {
            mode: Mode::Idle,
            line1: state_line(&s),
            line2: format!(
                "最近命令已结束({})",
                match s.phase {
                    Phase::Done => "成功",
                    Phase::Error => "失败",
                    Phase::Stopped => "被停止",
                    Phase::Running => unreachable!(),
                }
            ),
        },
        Some(_) => Snapshot {
            mode: Mode::Stale,
            line1: "状态失联(超过 10 秒无心跳)".into(),
            line2: "不显示成功或安全;请自行确认桌面状态".into(),
        },
        None => Snapshot {
            mode: Mode::Idle,
            line1: "actl 信号".into(),
            line2: "空闲 · Ctrl+Alt+F12 紧急停止".into(),
        },
    }
}

/// 输入占用锁观测:OpenMutex + Wait(0),不干扰持有者。
/// OpenMutex 失败(锁对象尚不存在)时沿用上次观测,躲开瞬时创建竞态。
fn input_lock_held() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static HELD: AtomicBool = AtomicBool::new(false);
    unsafe {
        match OpenMutexW(
            windows::Win32::System::Threading::SYNCHRONIZATION_ACCESS_RIGHTS(0x0010_0000),
            false,
            w!("actl-input-lock"),
        ) {
            Ok(h) => {
                let held = mutex_is_signaled(h);
                let _ = windows::Win32::Foundation::CloseHandle(h);
                HELD.store(held, Ordering::Relaxed);
                held
            }
            Err(_) => HELD.load(Ordering::Relaxed),
        }
    }
}

unsafe fn mutex_is_signaled(h: windows::Win32::Foundation::HANDLE) -> bool {
    windows::Win32::System::Threading::WaitForSingleObject(h, 0) == WAIT_OBJECT_0
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn draw(hwnd: HWND, snap: &Snapshot) {
    let mut ps = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    // BGR
    let (bg, fg) = match snap.mode {
        Mode::Stopped => (0x0040_40b0u32, 0x00FF_FFFFu32), // 红(BGR)
        Mode::Active => (0x0078_5020u32, 0x0000_0000u32),  // 琥珀
        Mode::Busy => (0x0096_9696u32, 0x0000_0000u32),    // 浅灰
        Mode::Idle => (0x0030_3030u32, 0x00E0_E0E0u32),    // 深灰
        Mode::Stale => (0x0020_2020u32, 0x00B0_B0B0u32),   // 暗
    };
    let brush = CreateSolidBrush(COLORREF(bg));
    FillRect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let set_text_color = windows::Win32::Graphics::Gdi::SetTextColor(hdc, COLORREF(fg));
    let _ = windows::Win32::Graphics::Gdi::SetBkColor(hdc, COLORREF(bg));
    let dt = DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS;
    let margin = 12;
    let mut r1 = RECT {
        left: margin,
        top: 8,
        right: rc.right - margin,
        bottom: 34,
    };
    let mut r2 = RECT {
        left: margin,
        top: 36,
        right: rc.right - margin,
        bottom: 56,
    };
    let mut r3 = RECT {
        left: margin,
        top: 60,
        right: rc.right - margin,
        bottom: 76,
    };
    let show = |r: &mut RECT, s: &str| {
        let mut buf = wide(s);
        let len = buf.len() as i32 - 1;
        let _ = DrawTextW(hdc, &mut buf, r as *mut RECT, dt);
        let _ = len;
    };
    show(&mut r1, &snap.line1);
    show(&mut r2, &snap.line2);
    show(
        &mut r3,
        "Ctrl+Alt+F12 紧急停止 · 点击本条=清除停止 · 关闭窗口=退出",
    );
    let _ = set_text_color;
    let _ = EndPaint(hwnd, &ps);
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER if wparam.0 == TIMER_ID => {
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_PAINT => {
            let snap = poll();
            draw(hwnd, &snap);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            SignalPaths::default().clear_stop();
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let tip = w!("左键点击=清除停止;关闭本窗口即退出(托盘菜单规划中)");
            let _ = MessageBoxW(Some(hwnd), tip, w!("actl-signal"), MB_OK | MB_SETFOREGROUND);
            LRESULT(0)
        }
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            SignalPaths::default().request_stop();
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_APP => {
            // 托盘回调:v1 用左键同义(清除停止)
            SignalPaths::default().clear_stop();
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("hinstance");
        let class_name = w!("actl_signal_window");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            lpszClassName: class_name,
            ..Default::default()
        };
        let atom = RegisterClassW(&wc);
        assert!(atom != 0, "RegisterClassW failed");

        // 工作区右上角(docs/11 §5.3:空闲边缘、避开任务栏)
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let x = work.right - WIDTH - 16;
        let y = work.top + 16;

        let ex = WINDOW_EX_STYLE(
            WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0,
        );
        let hwnd = CreateWindowExW(
            ex,
            class_name,
            w!("actl 信号"),
            WINDOW_STYLE(WS_POPUP.0 | WS_VISIBLE.0),
            x,
            y,
            WIDTH,
            HEIGHT,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .expect("CreateWindowExW");
        // 半透明:存在感但不喧宾夺主
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 235, LWA_ALPHA);

        // 托盘锚点(锚点只做存在提示;控制入口在状态条与热键)
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_APP;
        nid.hIcon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();
        let tip = wide("actl 信号(空闲)");
        nid.szTip[..tip.len() - 1].copy_from_slice(&tip[..tip.len() - 1]);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        assert!(
            RegisterHotKey(
                Some(hwnd),
                HOTKEY_ID,
                MOD_CONTROL | MOD_ALT,
                VK_F12.0 as u32
            )
            .is_ok(),
            "RegisterHotKey(Ctrl+Alt+F12) failed - another program owns it"
        );

        SetTimer(Some(hwnd), TIMER_ID, TIMER_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
        let _ = HWND_BROADCAST;
        let _ = GWLP_USERDATA;
    }
}
