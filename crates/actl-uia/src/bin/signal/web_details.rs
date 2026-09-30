//! On-demand WebView2 content. This bridge only reports readiness and layout;
//! desktop actions remain genuine native button clicks handled by controls.rs.
use std::{cell::RefCell, time::Instant};
use webview2_com::{Microsoft::Web::WebView2::Win32::*, *};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::{Interface, PCWSTR, PWSTR},
};

pub const HTML: &str = include_str!("../../../assets/signal-details.html");
const UPDATE: u32 = WM_APP + 61;
thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }
struct State {
    hwnd: HWND,
    controller: Option<ICoreWebView2Controller>,
    webview: Option<ICoreWebView2>,
    ready: bool,
    height: i32,
    started: Instant,
    error: Option<String>,
    last_json: String,
    initial_navigation: bool,
}

#[derive(Debug, PartialEq)]
enum Message {
    Ready,
    Height(i32),
}
fn parse_message(source: &str, json: &str) -> Option<Message> {
    if source != "about:blank" || json.len() > 256 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["version"] != 1
        || value
            .as_object()?
            .keys()
            .any(|key| !matches!(key.as_str(), "version" | "kind" | "height"))
    {
        return None;
    }
    match value["kind"].as_str()? {
        "ready" => Some(Message::Ready),
        "height" => value["height"]
            .as_i64()
            .filter(|h| (32..=4096).contains(h))
            .map(|h| Message::Height(h as i32)),
        _ => None,
    }
}
fn fail(hwnd: HWND, error: impl std::fmt::Display) {
    let controller = STATE.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut().filter(|s| s.hwnd == hwnd) {
            state.error = Some(format!(
                "NOT_ACTIONABLE：WebView2 未就绪，使用原生详情。{error}"
            ));
            state.ready = false;
            state.webview = None;
            state.controller.take()
        } else {
            None
        }
    });
    unsafe {
        if let Some(controller) = controller {
            let _ = controller.Close();
        }
        let _ = PostMessageW(Some(hwnd), UPDATE, WPARAM(0), LPARAM(0));
    }
}
fn current(hwnd: HWND) -> bool {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|s| s.hwnd == hwnd && s.error.is_none())
    })
}
pub unsafe fn start(
    hwnd: HWND,
    paths: &actl_core::state::SignalPaths,
) -> windows::core::Result<()> {
    let profile = paths.dir.join("display-webview");
    std::fs::create_dir_all(&profile)
        .map_err(|e| windows::core::Error::new(E_FAIL, e.to_string()))?;
    let profile = super::paint::wide(&profile.to_string_lossy());
    STATE.with(|slot| {
        *slot.borrow_mut() = Some(State {
            hwnd,
            controller: None,
            webview: None,
            ready: false,
            height: 220,
            started: Instant::now(),
            error: None,
            last_json: String::new(),
            initial_navigation: true,
        })
    });
    let handler =
        CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(move |result, env| {
            if !current(hwnd) {
                return Ok(());
            }
            let outcome = (|| {
                result?;
                let env = env.ok_or_else(|| windows::core::Error::from(E_POINTER))?;
                env.CreateCoreWebView2Controller(
                    hwnd,
                    &CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                        move |result, controller| {
                            if !current(hwnd) {
                                if let Some(controller) = controller {
                                    let _ = controller.Close();
                                }
                                return Ok(());
                            }
                            let outcome = (|| {
                                result?;
                                setup(
                                    hwnd,
                                    controller
                                        .ok_or_else(|| windows::core::Error::from(E_POINTER))?,
                                )
                            })();
                            if let Err(error) = outcome {
                                fail(hwnd, error);
                            }
                            Ok(())
                        },
                    )),
                )
            })();
            if let Err(error) = outcome {
                fail(hwnd, error);
            }
            Ok(())
        }));
    let result = CreateCoreWebView2EnvironmentWithOptions(
        PCWSTR::null(),
        PCWSTR(profile.as_ptr()),
        None,
        &handler,
    );
    if let Err(error) = &result {
        fail(hwnd, error);
    }
    result
}
unsafe fn setup(hwnd: HWND, controller: ICoreWebView2Controller) -> windows::core::Result<()> {
    let webview = controller.CoreWebView2()?;
    let settings = webview.Settings()?;
    settings.SetAreDefaultContextMenusEnabled(false)?;
    settings.SetAreDevToolsEnabled(false)?;
    settings.SetAreDefaultScriptDialogsEnabled(false)?;
    settings.SetIsStatusBarEnabled(false)?;
    settings.SetIsWebMessageEnabled(true)?;
    settings.SetAreHostObjectsAllowed(false)?;
    settings.SetIsZoomControlEnabled(false)?;
    controller.SetZoomFactor(1.0)?;
    controller
        .cast::<ICoreWebView2Controller2>()?
        .SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
            A: 255,
            R: 0x22,
            G: 0x24,
            B: 0x26,
        })?;
    controller.SetIsVisible(false)?;
    let mut token = 0;
    controller.add_MoveFocusRequested(
        &MoveFocusRequestedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let mut reason = COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC;
                args.Reason(&mut reason)?;
                let id = if reason == COREWEBVIEW2_MOVE_FOCUS_REASON_PREVIOUS {
                    15
                } else {
                    [10, 11, 13, 12]
                        .into_iter()
                        .find(|id| {
                            GetDlgItem(Some(hwnd), *id).is_ok_and(|c| {
                                IsWindowVisible(c).as_bool()
                                    && windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(
                                        c,
                                    )
                                    .as_bool()
                            })
                        })
                        .unwrap_or(15)
                };
                if let Ok(child) = GetDlgItem(Some(hwnd), id) {
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(child));
                    args.SetHandled(true)?;
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_NavigationStarting(
        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = CoTaskMemPWSTR::from(uri).to_string();
                // NavigateToString emits an internal data URI before committing about:blank.
                // Permit that first load only; the immutable page cannot navigate elsewhere.
                let initial = STATE.with(|slot| {
                    slot.borrow_mut().as_mut().is_some_and(|s| {
                        s.hwnd == hwnd && std::mem::take(&mut s.initial_navigation)
                    })
                });
                if !initial || !uri.starts_with("data:text/html;charset=utf-8;base64,") {
                    args.SetCancel(true)?;
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_NewWindowRequested(
        &NewWindowRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                args.SetHandled(true)?;
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_WebMessageReceived(
        &WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
            if !current(hwnd) {
                return Ok(());
            }
            if let Some(args) = args {
                let (mut source, mut json) = (PWSTR::null(), PWSTR::null());
                args.Source(&mut source)?;
                let source = CoTaskMemPWSTR::from(source).to_string();
                args.WebMessageAsJson(&mut json)?;
                let json = CoTaskMemPWSTR::from(json).to_string();
                if let Some(message) = parse_message(&source, &json) {
                    STATE.with(|slot| {
                        if let Some(state) = slot.borrow_mut().as_mut() {
                            match message {
                                Message::Ready => state.ready = true,
                                Message::Height(h) => state.height = h,
                            }
                        }
                    });
                    let _ = PostMessageW(Some(hwnd), UPDATE, WPARAM(0), LPARAM(0));
                }
            }
            Ok(())
        })),
        &mut token,
    )?;
    webview.add_ProcessFailed(
        &ProcessFailedEventHandler::create(Box::new(move |_, _| {
            fail(hwnd, "页面进程已退出。");
            Ok(())
        })),
        &mut token,
    )?;
    STATE.with(|slot| {
        if let Some(state) = slot.borrow_mut().as_mut() {
            state.controller = Some(controller.clone());
            state.webview = Some(webview.clone());
        }
    });
    let html = super::paint::wide(HTML);
    webview.NavigateToString(PCWSTR(html.as_ptr()))?;
    Ok(())
}
pub fn active(hwnd: HWND) -> bool {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|s| s.hwnd == hwnd && s.ready && s.error.is_none())
    })
}
pub fn height(hwnd: HWND) -> i32 {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|s| s.hwnd == hwnd)
            .map_or(220, |s| s.height)
    })
}
pub fn error(hwnd: HWND) -> Option<String> {
    STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|s| s.hwnd == hwnd)
            .and_then(|s| s.error.clone())
    })
}
pub unsafe fn tick(hwnd: HWND, json: &str) {
    let expired = STATE.with(|slot| {
        slot.borrow().as_ref().is_some_and(|s| {
            s.hwnd == hwnd && !s.ready && s.error.is_none() && s.started.elapsed().as_secs() >= 8
        })
    });
    if expired {
        fail(hwnd, "初始化超时。");
    }
    if !active(hwnd) {
        return;
    }
    let target = STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let state = slot.as_mut()?;
        if state.last_json == json {
            return None;
        }
        state.webview.clone()
    });
    if let Some(webview) = target {
        let text = super::paint::wide(json);
        match webview.PostWebMessageAsJson(PCWSTR(text.as_ptr())) {
            Ok(()) => STATE.with(|slot| {
                if let Some(state) = slot.borrow_mut().as_mut() {
                    state.last_json = json.into();
                }
            }),
            Err(error) => fail(hwnd, error),
        }
    }
}
pub unsafe fn bounds(hwnd: HWND, rect: RECT) {
    let controller = STATE.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|s| s.hwnd == hwnd)
            .and_then(|s| s.controller.clone())
    });
    if let Some(controller) = controller
        && let Err(error) = controller
            .SetBounds(rect)
            .and_then(|_| controller.SetIsVisible(active(hwnd)))
    {
        fail(hwnd, error);
    }
}
pub unsafe fn focus(hwnd: HWND) {
    move_focus(hwnd, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
}
unsafe fn move_focus(hwnd: HWND, reason: COREWEBVIEW2_MOVE_FOCUS_REASON) {
    if active(hwnd) {
        let controller =
            STATE.with(|slot| slot.borrow().as_ref().and_then(|s| s.controller.clone()));
        if let Some(controller) = controller {
            let _ = controller.MoveFocus(reason);
        }
    }
}
/// Leave Chromium's keyboard handling intact; bridge Tab only at native boundaries.
pub unsafe fn keyboard(hwnd: HWND, msg: &MSG) -> Option<bool> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    if !active(hwnd) {
        return None;
    }
    let focus = GetFocus();
    if IsChild(hwnd, focus).as_bool()
        && !(10..=17)
            .chain(100..=107)
            .any(|id| GetDlgItem(Some(hwnd), id).is_ok_and(|c| c == focus))
    {
        if msg.message == WM_KEYDOWN && msg.wParam.0 == VK_ESCAPE.0 as usize {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            return Some(true);
        }
        return Some(false);
    }
    if msg.message == WM_KEYDOWN && msg.wParam.0 == VK_TAB.0 as usize {
        let previous = GetKeyState(VK_SHIFT.0 as i32) < 0;
        let id = GetDlgCtrlID(focus);
        if (!previous && id == 15) || (previous && matches!(id, 10..=13)) {
            move_focus(
                hwnd,
                if previous {
                    COREWEBVIEW2_MOVE_FOCUS_REASON_PREVIOUS
                } else {
                    COREWEBVIEW2_MOVE_FOCUS_REASON_NEXT
                },
            );
            return Some(true);
        }
    }
    None
}
pub struct Apartment(bool);
impl Apartment {
    pub unsafe fn new() -> Self {
        use windows::Win32::System::Com::*;
        Self(CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok())
    }
    pub fn initialized(&self) -> bool {
        self.0
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}
pub unsafe fn close(hwnd: HWND) {
    let old = STATE.with(|slot| {
        if slot.borrow().as_ref().is_some_and(|s| s.hwnd == hwnd) {
            slot.borrow_mut().take()
        } else {
            None
        }
    });
    if let Some(mut state) = old
        && let Some(controller) = state.controller.take()
    {
        let _ = controller.Close();
    }
}
pub fn is_update(message: u32) -> bool {
    message == UPDATE
}

#[cfg(test)]
#[path = "web_details_live.rs"]
mod live;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_rejects_actions_foreign_pages_and_unbounded_layout() {
        assert_eq!(
            parse_message("about:blank", r#"{"version":1,"kind":"ready"}"#),
            Some(Message::Ready)
        );
        assert_eq!(
            parse_message(
                "about:blank",
                r#"{"version":1,"kind":"height","height":200}"#
            ),
            Some(Message::Height(200))
        );
        for json in [
            r#"{"version":1,"kind":"resume"}"#,
            r#"{"version":1,"kind":"height","height":999999}"#,
            r#"{"version":1,"kind":"ready","task":"other"}"#,
            r#"{"version":2,"kind":"ready"}"#,
        ] {
            assert_eq!(parse_message("about:blank", json), None);
        }
        assert_eq!(
            parse_message("https://example.com", r#"{"version":1,"kind":"ready"}"#),
            None
        );
    }
}
