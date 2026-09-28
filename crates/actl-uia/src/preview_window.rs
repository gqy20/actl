//! Win32 resources owned only by a synchronous preview invocation.
//! Unsafe: window creation/message dispatch and GDI painting; no input injection.
use actl_core::{CtlError, ErrorCode};
use windows::{
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
    core::{PCWSTR, w},
};

pub(crate) struct Window {
    frame: HWND,
    notice: HWND,
    font: HFONT,
}

pub(crate) enum Choice {
    Previous,
    Next,
    Select,
}

impl Window {
    pub fn check_target(&self, owner: HWND, bounds: [i32; 4]) -> Result<(), CtlError> {
        unsafe {
            if !IsWindow(Some(owner)).as_bool()
                || IsIconic(owner).as_bool()
                || !IsWindowVisible(owner).as_bool()
            {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target window is not visible",
                ));
            }
            let point = POINT {
                x: (i64::from(bounds[0]) + (i64::from(bounds[2]) - i64::from(bounds[0])) / 2)
                    as i32,
                y: (i64::from(bounds[1]) + (i64::from(bounds[3]) - i64::from(bounds[1])) / 2)
                    as i32,
            };
            let hit = GetAncestor(WindowFromPoint(point), GA_ROOT);
            if hit != owner && (self.notice.is_invalid() || hit != self.notice) {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target center is covered or unavailable",
                ));
            }
            Ok(())
        }
    }

    pub fn target_owner(element: &uiautomation::UIElement) -> Result<HWND, CtlError> {
        let auto = uiautomation::UIAutomation::new().map_err(crate::internal)?;
        let walker = auto.get_raw_view_walker().map_err(crate::internal)?;
        let mut node = element.clone();
        for _ in 0..=crate::MAX_DEPTH {
            // Failures must not look like an element without a native handle.
            let handle = node
                .get_native_window_handle()
                .map_err(|e| crate::read_channel::failure("preview.hwnd", e))?;
            if !handle.is_invalid() {
                // UIAutomation Handle and HANDLE are native pointer-sized handles.
                let native: HANDLE = unsafe { std::mem::transmute(handle) };
                let hwnd = HWND(native.0);
                crate::display_support::reject_display(hwnd.0 as isize)?;
                return Ok(unsafe { GetAncestor(hwnd, GA_ROOT) });
            }
            node = walker
                .get_parent(&node)
                .map_err(|e| crate::read_channel::failure("preview.parent", e))?;
        }
        Err(CtlError::new(
            ErrorCode::NotActionable,
            "preview cannot identify target window",
        ))
    }

    pub fn check_initial_target(owner: HWND, bounds: [i32; 4]) -> Result<(), CtlError> {
        let empty = Self {
            frame: HWND::default(),
            notice: HWND::default(),
            font: HFONT::default(),
        };
        empty.check_target(owner, bounds)
    }

    pub fn open(bounds: [i32; 4], caption: &str, message: &str) -> Result<Self, CtlError> {
        Self::open_with(bounds, caption, message, None)
    }

    pub fn open_choice(
        bounds: [i32; 4],
        caption: &str,
        message: &str,
        index: usize,
        count: usize,
    ) -> Result<Self, CtlError> {
        Self::open_with(bounds, caption, message, Some((index, count)))
    }

    fn open_with(
        bounds: [i32; 4],
        caption: &str,
        message: &str,
        selection: Option<(usize, usize)>,
    ) -> Result<Self, CtlError> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(crate::internal)?;
            let class = w!("actl_target_preview");
            // Class may already exist when preview is used more than once in batch.
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hbrBackground: GetSysColorBrush(COLOR_WINDOW),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let mut window = Self {
                frame: HWND::default(),
                notice: HWND::default(),
                font: HFONT::default(),
            };
            let [l, t, r, b] = bounds;
            window.frame = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                class,
                w!(""),
                WS_POPUP,
                l,
                t,
                r - l,
                b - t,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            SetLayeredWindowAttributes(window.frame, COLORREF(0), 255, LWA_COLORKEY)
                .map_err(crate::internal)?;
            let monitor = MonitorFromRect(
                &RECT {
                    left: l,
                    top: t,
                    right: r,
                    bottom: b,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview monitor is unavailable",
                ));
            }
            let work = info.rcWork;
            if r <= work.left || l >= work.right || b <= work.top || t >= work.bottom {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview target is outside the visible work area",
                ));
            }
            let (mut dx, mut dy) = (96, 96);
            let _ = windows::Win32::UI::HiDpi::GetDpiForMonitor(
                monitor,
                windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI,
                &mut dx,
                &mut dy,
            );
            let scale = |v: i32| ((i64::from(v) * i64::from(dx)) / 96).clamp(1, 16000) as i32;
            let mut font = LOGFONTW {
                lfHeight: -scale(16),
                lfWeight: 400,
                ..Default::default()
            };
            for (dest, value) in font.lfFaceName.iter_mut().zip("Segoe UI".encode_utf16()) {
                *dest = value;
            }
            window.font = CreateFontIndirectW(&font);
            if window.font.is_invalid() {
                return Err(CtlError::internal("preview font creation failed"));
            }
            let width = scale(600).min(work.right - work.left);
            let height =
                scale(if selection.is_some() { 200 } else { 120 }).min(work.bottom - work.top);
            let x = l.clamp(work.left, work.right - width);
            let y = if b.saturating_add(height + 8) <= work.bottom {
                b + 8
            } else {
                t.saturating_sub(height).saturating_sub(8).max(work.top)
            };
            let title: Vec<u16> = caption.encode_utf16().chain(Some(0)).collect();
            window.notice = CreateWindowExW(
                WS_EX_TOPMOST
                    | if selection.is_some() {
                        WS_EX_APPWINDOW
                    } else {
                        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW
                    },
                class,
                PCWSTR(title.as_ptr()),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            // A native STATIC child wraps the action description; it accepts no input.
            let instruction = if selection.is_some() {
                "切换并确认目标；仅返回选择，不会执行操作。关闭或超时表示未选择。"
            } else {
                "仅预览，不会执行操作。点击右上角关闭，或等待自动结束。"
            };
            let text: Vec<u16> = format!("{message}\n{instruction}")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let text_control = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE,
                scale(12),
                scale(8),
                (width - scale(24)).max(1),
                (height - scale(if selection.is_some() { 98 } else { 38 })).max(1),
                Some(window.notice),
                None,
                Some(instance.into()),
                None,
            )
            .map_err(crate::internal)?;
            SendMessageW(
                text_control,
                WM_SETFONT,
                Some(WPARAM(window.font.0 as usize)),
                Some(LPARAM(1)),
            );
            if let Some((index, count)) = selection {
                for (id, label, x, enabled) in [
                    (1, "上一个", 12, index > 0),
                    (2, "下一个", 152, index + 1 < count),
                    (3, "选择此目标", 342, true),
                ] {
                    let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                    let button = CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        w!("BUTTON"),
                        PCWSTR(label.as_ptr()),
                        WS_CHILD
                            | WS_VISIBLE
                            | WS_TABSTOP
                            | if enabled {
                                WINDOW_STYLE(0)
                            } else {
                                WS_DISABLED
                            },
                        scale(x),
                        height - scale(76),
                        scale(130),
                        scale(36),
                        Some(window.notice),
                        Some(HMENU(id as *mut _)),
                        Some(instance.into()),
                        None,
                    )
                    .map_err(crate::internal)?;
                    SendMessageW(
                        button,
                        WM_SETFONT,
                        Some(WPARAM(window.font.0 as usize)),
                        Some(LPARAM(1)),
                    );
                }
            }
            let _ = ShowWindow(window.frame, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(window.notice, SW_SHOWNOACTIVATE);
            if !UpdateWindow(window.frame).as_bool() || !UpdateWindow(window.notice).as_bool() {
                return Err(CtlError::internal("preview painting failed"));
            }
            windows::Win32::Graphics::Dwm::DwmFlush().map_err(crate::internal)?;
            if !IsWindowVisible(window.frame).as_bool() || !IsWindowVisible(window.notice).as_bool()
            {
                return Err(CtlError::new(
                    ErrorCode::NotActionable,
                    "preview windows are not visible",
                ));
            }
            Ok(window)
        }
    }

    pub fn pump(&self) -> bool {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            IsWindow(Some(self.frame)).as_bool() && IsWindow(Some(self.notice)).as_bool()
        }
    }

    pub fn choice(&self) -> Option<Choice> {
        match unsafe { SetWindowLongPtrW(self.notice, GWLP_USERDATA, 0) } {
            1 => Some(Choice::Previous),
            2 => Some(Choice::Next),
            3 => Some(Choice::Select),
            _ => None,
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            for hwnd in [self.notice, self.frame] {
                if IsWindow(Some(hwnd)).as_bool() {
                    let _ = DestroyWindow(hwnd);
                }
            }
            if !self.font.is_invalid() {
                let _ = DeleteObject(self.font.into());
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND if wp.0 >> 16 == BN_CLICKED as usize && matches!(wp.0 & 0xffff, 1..=3) => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (wp.0 & 0xffff) as isize);
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(if GetDlgItem(Some(hwnd), 3).is_ok() {
            MA_ACTIVATE
        } else {
            MA_NOACTIVATE
        } as isize),
        WM_PAINT if GetWindowTextLengthW(hwnd) == 0 => {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let pen = CreatePen(PS_SOLID, 3, COLORREF(0x0041_B3E3));
            if !pen.is_invalid() {
                let old_pen = SelectObject(dc, HGDIOBJ(pen.0));
                let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                let _ = Rectangle(dc, 1, 1, rect.right - 1, rect.bottom - 1);
                SelectObject(dc, old_pen);
                SelectObject(dc, old_brush);
                let _ = DeleteObject(HGDIOBJ(pen.0));
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests;
