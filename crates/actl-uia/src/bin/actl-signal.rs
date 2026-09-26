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
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, CreateBitmap, CreateCompatibleDC,
    CreateDIBSection, CreateSolidBrush, DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect,
    GetStockObject, HGDIOBJ, InvalidateRect, NULL_PEN,
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
    AppendMenuW, CS_HREDRAW, CS_VREDRAW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, GWLP_USERDATA, GetMessageW, GetSystemMetrics, HTCAPTION,
    HWND_BROADCAST, IDC_ARROW, KillTimer, LWA_ALPHA, LoadCursorW, MF_SEPARATOR, MF_STRING, MSG,
    PostQuitMessage, RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow,
    SetLayeredWindowAttributes, SetTimer, SystemParametersInfoW, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_DESTROY,
    WM_EXITSIZEMOVE, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_NCHITTEST,
    WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, ICONINFO};
use windows::core::w;

static HOTKEY_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
const WIDTH: i32 = 420;
const HEIGHT: i32 = 96;
/// 条宽自适应区间(内容短时收窄,右侧不留白;长命令到此封顶后省略号截断)
const MIN_W: i32 = 280;
const MAX_W: i32 = 460;

fn dpi_scale() -> i32 {
    static DPI: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let dpi = *DPI.get_or_init(|| unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() });
    dpi as i32 / 96
}

fn scale(v: i32) -> i32 {
    v * dpi_scale()
}
const TIMER_ID: usize = 1;
const TIMER_MS: u32 = 250;
/// 动画帧率 25fps(40ms);只失效状态点小区域,CPU 占位可忽略
const ANIM_TIMER_ID: usize = 2;
const ANIM_MS: u32 = 40;
const HOTKEY_ID: i32 = 1;
const STALE_MS: u64 = 10_000;
static ANIM_FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// 最近一次轮询快照(数据 timer 写,WM_PAINT 读——绘制不再触发文件 IO)
static SNAPSHOT: std::sync::Mutex<Option<Snapshot>> = std::sync::Mutex::new(None);

/// 渲染模式(颜色 + 语义;颜色之外由文字承载,docs/11 §11)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
struct Snapshot {
    mode: Mode,
    line1: String,
    line2: String,
    /// Running/注入中的已运行秒数(右侧实时跳动——"一直在干活"的最直接感知)
    elapsed_s: Option<u64>,
}

fn state_line(s: &SessionState) -> String {
    let step = s
        .step
        .as_deref()
        .map(|x| format!(" {x}"))
        .unwrap_or_default();
    let app = s
        .app
        .as_deref()
        .map(|a| format!(" · {a}"))
        .unwrap_or_default();
    format!("{}{}{}", s.command, app, step)
}

/// 耗时格式:59s 内裸秒,之后 1m03s
fn fmt_elapsed(s: u64) -> String {
    if s < 60 {
        format!("{s}s")
    } else {
        format!("{}m{:02}s", s / 60, s % 60)
    }
}

fn poll() -> Snapshot {
    let paths = SignalPaths::default();
    if paths.stop_requested() {
        return Snapshot {
            mode: Mode::Stopped,
            line1: "停止请求".into(),
            line2: "点击解除".into(),
            elapsed_s: None,
        };
    }
    let lock_held = input_lock_held();
    let st = paths.read_state();
    let age = st
        .as_ref()
        .map(|s| actl_core::state::unix_ms().saturating_sub(s.ts_ms))
        .unwrap_or(u64::MAX);
    // Running 态的已运行秒数(年龄即耗时——state.json 的 ts_ms 是命令起点)
    let elapsed = st
        .as_ref()
        .filter(|s| s.phase == Phase::Running && age <= STALE_MS)
        .map(|_| age / 1000);
    if lock_held {
        return Snapshot {
            mode: Mode::Active,
            line1: st
                .as_ref()
                .map(state_line)
                .unwrap_or_else(|| "未知命令".into()),
            line2: "注入中".into(),
            elapsed_s: elapsed,
        };
    }
    match st {
        Some(s) if age <= STALE_MS && s.phase == Phase::Running => Snapshot {
            mode: Mode::Busy,
            line1: state_line(&s),
            line2: "执行中".into(),
            elapsed_s: elapsed,
        },
        Some(s) if age <= STALE_MS => Snapshot {
            mode: Mode::Idle,
            line1: state_line(&s),
            line2: match s.phase {
                Phase::Done => "✓ 完成".to_string(),
                Phase::Error => "✗ 失败".to_string(),
                Phase::Stopped => "已停止".to_string(),
                Phase::Running => unreachable!(),
            },
            elapsed_s: None,
        },
        Some(_) => Snapshot {
            mode: Mode::Stale,
            line1: "actl".into(),
            line2: "失联".into(),
            elapsed_s: None,
        },
        None => Snapshot {
            mode: Mode::Idle,
            line1: "actl".into(),
            line2: "空闲".into(),
            elapsed_s: None,
        },
    }
}

/// 提示行(热键可用性自适应;退出入口在右键菜单,不占提示位)
fn hint_text() -> &'static str {
    if HOTKEY_OK.load(std::sync::atomic::Ordering::Relaxed) {
        "Ctrl+Alt+F12 停止 · 点击解除"
    } else {
        "点击本条=停止 · 再点=解除"
    }
}

// ─── 动画几何(纯函数,单测锁定) ────────────────────────────────────────

/// 波纹帧几何:phase ∈ [0,1) → (半径倍数, 向底色淡出度)。
/// ease-out 扩散(快出慢收)+ 线性淡出;两条波纹相位差半周期(Codex 式持续律动)。
fn ripple(phase: f32) -> (f32, f32) {
    let t = phase.clamp(0.0, 1.0);
    let spread = 1.0 + 1.3 * (1.0 - (1.0 - t) * (1.0 - t));
    (spread, t)
}

/// 空闲呼吸:半径倍数 0.88..1.12,周期 2.4s(60 帧 @25fps)
fn breath(frame: u32) -> f32 {
    let phase = (frame % 60) as f32 / 60.0;
    1.0 + 0.12 * (phase * std::f32::consts::TAU).sin()
}

/// 通道级颜色插值(向底色淡出的波纹环;COLORREF/BGR 布局通用)
fn mix(fg: u32, bg: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |shift: u32| -> u32 {
        let f = (fg >> shift) & 0xFF;
        let b = (bg >> shift) & 0xFF;
        ((f as f32 + (b as f32 - f as f32) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
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

/// Source Code Pro 三级字阶(进程生命周期内只建一次)。
/// DEFAULT_CHARSET 让中文走系统 font-linking(否则乱码方框)。
/// 主行 SEMIBOLD(用户反馈细字发虚)、状态行 NORMAL、提示行 NORMAL。
unsafe fn font(height: i32) -> windows::Win32::Graphics::Gdi::HFONT {
    static BIG: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
    static MID: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
    static DIM: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
    use windows::Win32::Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, FIXED_PITCH,
        FW_NORMAL, FW_SEMIBOLD, OUT_DEFAULT_PRECIS,
    };
    let (slot, weight) = match height {
        17 => (&BIG, FW_SEMIBOLD.0 as i32),
        13 => (&MID, FW_NORMAL.0 as i32),
        _ => (&DIM, FW_NORMAL.0 as i32),
    };
    let h = *slot.get_or_init(|| {
        CreateFontW(
            -scale(height), // 逻辑高度 → 物理像素(负值=char height)
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FIXED_PITCH.0 as u32,
            w!("Source Code Pro"),
        )
        .0 as isize
    });
    windows::Win32::Graphics::Gdi::HFONT(h as *mut _)
}

/// 状态行行心(y,未缩放):圆点与波纹的几何中心
const ROW2_CY: i32 = 56;
const DOT_D: i32 = 11;
/// 波纹最大半径倍数(ripple 的 spread 上界 2.3)+ 余量 → 动画失效区
fn dot_anim_rect() -> RECT {
    let cx = scale(18) + scale(DOT_D) / 2;
    let cy = scale(ROW2_CY);
    let r = (scale(DOT_D) as f32 / 2.0 * 2.4) as i32 + scale(2);
    RECT {
        left: cx - r,
        top: cy - r,
        right: cx + r,
        bottom: cy + r,
    }
}

/// HUD 布局:恒定深底 + 左侧状态灯(执行/注入中双波纹律动,空闲慢呼吸)+
/// 状态行文字着色 + 右侧实时耗时(执行中)。宽度由数据 timer 自适应内容。
unsafe fn draw(hwnd: HWND, snap: &Snapshot) {
    use windows::Win32::Graphics::Gdi::{SelectObject, SetBkColor, SetTextColor};

    let mut ps = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // BGR 恒定深底
    const BG: u32 = 0x001E_1E1E;
    // (状态点色, 状态行文字色)——市场通行信号色(GitHub/VS Code/CI 状态灯):
    // 绿=正常执行中 · 琥珀=占用物理输入(需留意) · 红=停止 · 灰=空闲 · 暗=失联
    let (dot, state_fg) = match snap.mode {
        Mode::Stopped => (0x0049_51F8u32, 0x0049_51F8u32), // 红 #F85149
        Mode::Active => (0x0041_B3E3u32, 0x0041_B3E3u32),  // 琥珀 #E3B341
        Mode::Busy => (0x0050_B93Fu32, 0x0050_B93Fu32),    // 绿 #3FB950
        Mode::Idle => (0x009E_948Bu32, 0x009E_948Bu32),    // 灰 #8B949E
        Mode::Stale => (0x0081_766Eu32, 0x0081_766Eu32),   // 暗 #6E7681
    };

    let fill = |r: &RECT, color: u32| {
        let b = CreateSolidBrush(COLORREF(color));
        FillRect(hdc, r, b);
        let _ = DeleteObject(HGDIOBJ(b.0));
    };
    fill(&rc, BG);

    let left = scale(18);
    let right = rc.right - scale(14);
    let line = |r: &mut RECT, s: &str, h: i32, color: u32, right_align: bool| {
        let old_f = SelectObject(hdc, HGDIOBJ(font(h).0));
        let old_c = SetTextColor(hdc, COLORREF(color));
        SetBkColor(hdc, COLORREF(BG));
        let mut buf = wide(s);
        let mut fmt = DT_SINGLELINE | DT_END_ELLIPSIS | DT_VCENTER;
        if right_align {
            fmt |= DT_RIGHT;
        } else {
            fmt |= DT_LEFT;
        }
        let _ = DrawTextW(hdc, &mut buf, r as *mut RECT, fmt);
        SetTextColor(hdc, old_c);
        SelectObject(hdc, old_f);
    };
    let mut r1 = RECT {
        left,
        top: scale(12),
        right,
        bottom: scale(38),
    };
    line(&mut r1, &snap.line1, 17, 0x00F3_EDE6, false);

    // 状态行:左侧状态灯(动画见下)+ 文字;右侧实时耗时(执行/注入中)
    let dot_d = scale(DOT_D);
    let mut r2 = RECT {
        left: left + dot_d + scale(9),
        top: scale(46),
        right,
        bottom: scale(66),
    };
    line(&mut r2, &snap.line2, 13, state_fg, false);
    if let Some(s) = snap.elapsed_s {
        let mut re = RECT {
            left,
            top: scale(46),
            right,
            bottom: scale(66),
        };
        line(&mut re, &fmt_elapsed(s), 13, state_fg, true);
    }

    // 状态灯:执行/注入 = 双波纹持续律动(Codex 式"一直在干活");
    // 空闲 = 慢呼吸;停止/失联 = 静止(红色本就是终止态,不制造动感)
    let circle = |cx: i32, cy: i32, radius: i32, color: u32| {
        let b = CreateSolidBrush(COLORREF(color));
        let old_br = SelectObject(hdc, HGDIOBJ(b.0));
        let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
        let _ = Ellipse(hdc, cx - radius, cy - radius, cx + radius, cy + radius);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_br);
        let _ = DeleteObject(HGDIOBJ(b.0));
    };
    let cx = left + dot_d / 2;
    let cy = scale(ROW2_CY);
    let frame = ANIM_FRAME.load(std::sync::atomic::Ordering::Relaxed);
    match snap.mode {
        Mode::Active | Mode::Busy => {
            // 双波纹相位差 0.5,周期 1.6s(40 帧 @25fps);波纹色向底色淡出
            for k in 0..2u32 {
                let phase = ((frame + k * 20) % 40) as f32 / 40.0;
                let (spread, fade) = ripple(phase);
                let r = (dot_d as f32 / 2.0 * spread) as i32;
                circle(cx, cy, r, mix(dot, BG, fade * 0.85));
            }
            circle(cx, cy, dot_d / 2, dot);
        }
        Mode::Idle => {
            let s = breath(frame);
            let r = (dot_d as f32 / 2.0 * s) as i32;
            circle(cx, cy, r, dot);
        }
        _ => circle(cx, cy, dot_d / 2, dot),
    }

    let mut r3 = RECT {
        left,
        top: scale(74),
        right,
        bottom: scale(90),
    };
    line(&mut r3, hint_text(), 11, 0x006E_6E6E, false);
    let _ = EndPaint(hwnd, &ps);
}

/// 条宽自适应:量三行内容宽,收窄/放宽到 [MIN_W, MAX_W],锚定右缘。
/// 变化超阈值才动窗(避免逐帧抖动);数据 timer 每次轮询后调用。
unsafe fn fit_width(hwnd: HWND, snap: &Snapshot) {
    use windows::Win32::Graphics::Gdi::{DT_CALCRECT, GetDC, ReleaseDC, SelectObject as SO};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
    };

    let hdc = GetDC(Some(hwnd));
    let measure = |s: &str, h: i32| -> i32 {
        let old = SO(hdc, HGDIOBJ(font(h).0));
        let mut r = RECT::default();
        let mut buf = wide(s);
        let _ = DrawTextW(hdc, &mut buf, &mut r, DT_SINGLELINE | DT_CALCRECT);
        let _ = SO(hdc, old);
        r.right
    };
    let status = match snap.elapsed_s {
        Some(s) => format!("{}  {}", snap.line2, fmt_elapsed(s)),
        None => snap.line2.clone(),
    };
    let dot_pad = scale(DOT_D) + scale(9);
    let content = measure(&snap.line1, 17)
        .max(measure(&status, 13) + dot_pad)
        .max(measure(hint_text(), 11));
    let _ = ReleaseDC(Some(hwnd), hdc);
    let want = (content + scale(18) + scale(14)).clamp(scale(MIN_W), scale(MAX_W));

    let mut rc = RECT::default();
    if GetWindowRect(hwnd, &mut rc).is_err() {
        return;
    }
    let cur = rc.right - rc.left;
    if (want - cur).abs() > scale(6) {
        // 锚定右缘:条宽变化时左缘移动,视觉重心稳定(默认挂在屏幕右上)
        let _ = SetWindowPos(
            hwnd,
            None,
            rc.right - want,
            rc.top,
            want,
            rc.bottom - rc.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// 品牌图标(运行时绘制,无 .ico 资源文件):深底圆角方 + 中央绿点——
/// 与状态条的状态灯同一视觉母题。32×32 ARGB DIB → CreateIconIndirect。
unsafe fn make_brand_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
    const SZ: i32 = 32;
    let radius = 7.0f32;
    let dot_r = 8.0f32;
    let mut px = vec![0u32; (SZ * SZ) as usize];
    for y in 0..SZ {
        for x in 0..SZ {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = fx.clamp(radius, SZ as f32 - radius);
            let cy = fy.clamp(radius, SZ as f32 - radius);
            let in_round = (fx - cx) * (fx - cx) + (fy - cy) * (fy - cy) <= radius * radius;
            if !in_round {
                continue;
            }
            let dcx = fx - SZ as f32 / 2.0;
            let dcy = fy - SZ as f32 / 2.0;
            // 0xAARRGGBB(DIB 小端序即 BGRA 字节)
            px[(y * SZ + x) as usize] = if dcx * dcx + dcy * dcy <= dot_r * dot_r {
                0xFF3F_B950 // 绿 #3FB950
            } else {
                0xFF1E_1E1E // 深底 #1E1E1E
            };
        }
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: SZ,
            biHeight: -SZ, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let hdc = GetDC(None);
    let mem = CreateCompatibleDC(Some(hdc));
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let color =
        CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
    if !color.is_invalid() && !bits.is_null() {
        std::ptr::copy_nonoverlapping(px.as_ptr() as *const u8, bits as *mut u8, px.len() * 4);
    }
    let _ = windows::Win32::Graphics::Gdi::DeleteDC(mem);
    let _ = ReleaseDC(None, hdc);
    // 全不透明掩码(1bpp 全零)
    let mask = CreateBitmap(SZ, SZ, 1, 1, None);
    let ii = ICONINFO {
        fIcon: true.into(),
        hbmMask: mask,
        hbmColor: color,
        ..Default::default()
    };
    let icon = CreateIconIndirect(&ii).unwrap_or_default();
    let _ = HWND(std::ptr::null_mut()); // 占位使用避免未用导入
    icon
}

/// 右键菜单(窗口与托盘共用):清除停止 / 显示隐藏 / 退出。
unsafe fn context_menu(hwnd: HWND) {
    use windows::Win32::Foundation::POINT;
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    let _ = AppendMenuW(menu, MF_STRING, 1, w!("清除停止标志"));
    let _ = AppendMenuW(menu, MF_STRING, 2, w!("显示 / 隐藏状态条"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
    let _ = AppendMenuW(menu, MF_STRING, 3, w!("退出 actl-signal"));
    let mut pt = POINT::default();
    let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
    // 经典套路:菜单要能被窗外点击关闭,先 SetForegroundWindow
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        None,
        hwnd,
        None,
    )
    .0;
    let _ = DestroyMenu(menu);
    match cmd {
        1 => {
            SignalPaths::default().clear_stop();
            let _ = InvalidateRect(Some(hwnd), None, true);
        }
        2 => toggle_visible(hwnd),
        3 => {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn toggle_visible(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SW_HIDE, SW_SHOW, ShowWindow};
    if IsWindowVisible(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_HIDE);
    } else {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
}

/// 拖动后的位置落盘(下次启动沿用;简单 kv,无需 serde)。
unsafe fn save_position(hwnd: HWND) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let mut rc = RECT::default();
    if GetWindowRect(hwnd, &mut rc).is_ok() {
        let dir = SignalPaths::default().dir;
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("signal-pos.txt"),
            format!("{},{}", rc.left, rc.top),
        );
    }
}

/// 启动时读回上次位置;不存在或落在虚拟桌面之外(拔屏/DPI 变)则回默认(工作区右上)。
unsafe fn load_position(w: i32, h: i32) -> (i32, i32) {
    use windows::Win32::Foundation::RECT;
    let mut work = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut work as *mut RECT as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    let default = (work.right - w - 16, work.top + 16);
    let Some((x, y)) = std::fs::read_to_string(SignalPaths::default().dir.join("signal-pos.txt"))
        .ok()
        .and_then(|t| {
            let (xs, ys) = t.split_once(',')?;
            Some((
                xs.trim().parse::<i32>().ok()?,
                ys.trim().parse::<i32>().ok()?,
            ))
        })
    else {
        return default;
    };
    // 钳制进虚拟桌面(标题永远可达)
    let (vx, vy, vw, vh) = (
        GetSystemMetrics(SM_XVIRTUALSCREEN),
        GetSystemMetrics(SM_YVIRTUALSCREEN),
        GetSystemMetrics(SM_CXVIRTUALSCREEN),
        GetSystemMetrics(SM_CYVIRTUALSCREEN),
    );
    let x = x.clamp(vx, vx + vw - w);
    let y = y.clamp(vy, vy + vh - h);
    (x, y)
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TIMER if wparam.0 == TIMER_ID => {
            // 数据 tick:轮询 → 缓存(供 WM_PAINT 零 IO 绘制)→ 宽度自适应 →
            // 变化才全量重绘。内容键含 elapsed_s:执行中每秒自然刷新一次
            // (右侧耗时跳动),非执行态仍保持变化检测零重绘。
            let snap = poll();
            let key = format!(
                "{:?}|{}|{}|{:?}",
                std::mem::discriminant(&snap.mode),
                snap.line1,
                snap.line2,
                snap.elapsed_s
            );
            let prev =
                windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if prev != 0 {
                drop(Box::from_raw(prev as *mut String));
            }
            let changed = prev == 0 || unsafe { &*(prev as *const String) } != &key;
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                Box::into_raw(Box::new(key)) as isize,
            );
            *SNAPSHOT.lock().unwrap() = Some(snap.clone());
            if changed {
                unsafe { fit_width(hwnd, &snap) };
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == ANIM_TIMER_ID => {
            // 动画 tick:仅 Active/Busy(波纹)/Idle(呼吸)推进帧并失效状态灯小区,
            // 停止/失联静止(终止态不制造动感)
            let animating = matches!(
                SNAPSHOT.lock().unwrap().as_ref().map(|s| s.mode),
                Some(Mode::Active | Mode::Busy | Mode::Idle)
            );
            if animating {
                ANIM_FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let r = dot_anim_rect();
                let _ = InvalidateRect(Some(hwnd), Some(&r), false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let guard = SNAPSHOT.lock().unwrap();
            if let Some(snap) = guard.as_ref() {
                draw(hwnd, snap);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            SignalPaths::default().clear_stop();
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            context_menu(hwnd);
            LRESULT(0)
        }
        // 整条可拖动(HTCAPTION 语义):Windows 自带移动逻辑,NOACTIVATE 不抢焦点;
        // 单击与拖动由系统的拖拽阈值区分,轻点仍走 WM_LBUTTONDOWN(清除停止)
        WM_NCHITTEST => {
            let _ = DefWindowProcW(hwnd, msg, wparam, lparam);
            LRESULT(HTCAPTION as isize)
        }
        WM_EXITSIZEMOVE => {
            save_position(hwnd);
            LRESULT(0)
        }
        WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => {
            SignalPaths::default().request_stop();
            let _ = InvalidateRect(Some(hwnd), None, true);
            LRESULT(0)
        }
        WM_APP => {
            // 托盘回调:LOWORD(lparam) 是鼠标消息——此前不区分(悬停也清停止,bug)
            let mouse = (lparam.0 & 0xFFFF) as u32;
            match mouse {
                WM_LBUTTONUP => {
                    SignalPaths::default().clear_stop();
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
                WM_LBUTTONDBLCLK => toggle_visible(hwnd),
                WM_RBUTTONUP => context_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            let _ = KillTimer(Some(hwnd), ANIM_TIMER_ID);
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
        // 必须第一个调用:声明 Per-Monitor V2 感知,否则 200% 缩放屏上
        // 整窗被 DWM 位图拉伸,Source Code Pro 全糊(实测)
        let _ = windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        let hinstance = GetModuleHandleW(None).expect("hinstance");
        let class_name = w!("actl_signal_window");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            hIcon: make_brand_icon(),
            // 不设类光标 = 悬浮时系统兜底显示"忙"圈(实测坑);显式箭头
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
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
        let _ = &work; // 默认位置已并入 load_position(带虚拟桌面钳制)
        let (x, y) = load_position(scale(WIDTH), scale(HEIGHT));

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
            scale(WIDTH),
            scale(HEIGHT),
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
        nid.hIcon = make_brand_icon();
        let tip = wide("actl 信号(空闲)");
        nid.szTip[..tip.len() - 1].copy_from_slice(&tip[..tip.len() - 1]);
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);

        // 热键被占不致命:降级运行(提示行自适应),绝不 panic 掉整个信号条
        HOTKEY_OK.store(
            RegisterHotKey(
                Some(hwnd),
                HOTKEY_ID,
                MOD_CONTROL | MOD_ALT,
                VK_F12.0 as u32,
            )
            .is_ok(),
            std::sync::atomic::Ordering::Relaxed,
        );

        // 首帧数据先入缓存再进消息循环(CreateWindowExW 期间可能已触发 WM_PAINT)
        *SNAPSHOT.lock().unwrap() = Some(poll());
        SetTimer(Some(hwnd), TIMER_ID, TIMER_MS, None);
        SetTimer(Some(hwnd), ANIM_TIMER_ID, ANIM_MS, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
        let _ = HWND_BROADCAST;
        let _ = GWLP_USERDATA;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_icon_is_created() {
        unsafe {
            let icon = make_brand_icon();
            assert!(
                !icon.is_invalid(),
                "CreateIconIndirect must produce a valid HICON"
            );
        }
    }

    #[test]
    fn ripple_expands_monotonically_and_fades() {
        let mut last_spread = 0.0f32;
        for i in 0..=20 {
            let (spread, fade) = ripple(i as f32 / 20.0);
            assert!((1.0..=2.31).contains(&spread), "spread {spread} in range");
            assert!((0.0..=1.0).contains(&fade), "fade {fade} in range");
            assert!(spread >= last_spread, "ease-out spread is monotonic");
            last_spread = spread;
        }
    }

    #[test]
    fn breath_stays_within_band_and_loops() {
        for f in 0..120u32 {
            let s = breath(f);
            assert!((0.87..=1.13).contains(&s), "breath {s} in band");
        }
        // 周期 60 帧:首尾相位一致
        assert!((breath(0) - breath(60)).abs() < 1e-6);
    }

    #[test]
    fn elapsed_formats_seconds_then_minutes() {
        assert_eq!(fmt_elapsed(0), "0s");
        assert_eq!(fmt_elapsed(59), "59s");
        assert_eq!(fmt_elapsed(60), "1m00s");
        assert_eq!(fmt_elapsed(63), "1m03s");
        assert_eq!(fmt_elapsed(125), "2m05s");
    }

    #[test]
    fn mix_fades_toward_background_per_channel() {
        const BG: u32 = 0x001E_1E1E;
        let fg = 0x0050_B93Fu32; // 绿 #3FB950(BGR)
        assert_eq!(mix(fg, BG, 0.0), fg);
        assert_eq!(mix(fg, BG, 1.0), BG);
        let mid = mix(fg, BG, 0.5);
        // 每通道都应位于两端之间
        for shift in [16u32, 8, 0] {
            let f = (fg >> shift) & 0xFF;
            let b = (BG >> shift) & 0xFF;
            let m = (mid >> shift) & 0xFF;
            assert!(m >= f.min(b) && m <= f.max(b), "channel midway");
        }
    }
}
