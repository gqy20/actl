//! Resolved execution feedback. Uses only local action facts, never key/text contents.
use actl_core::activity::{ActionTarget, ClipboardEffect, InputEffects};
use actl_core::signal_session::ActionGuard;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;

thread_local! { static KEYBOARD_ANCHOR: std::cell::Cell<Option<(usize,usize)>> = const { std::cell::Cell::new(None) }; }
fn physical_focus() -> Result<(usize, usize), CtlError> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo,
    };
    let mut info = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    unsafe { GetGUIThreadInfo(0, &mut info) }.map_err(crate::internal)?;
    Ok((
        unsafe { GetForegroundWindow() }.0 as usize,
        info.hwndFocus.0 as usize,
    ))
}
pub(crate) struct KeyboardAnchor(Option<(usize, usize)>);
impl KeyboardAnchor {
    pub(crate) fn capture() -> Result<Self, CtlError> {
        let focus = physical_focus()?;
        if focus.0 == 0 || focus.1 == 0 {
            return Err(CtlError::new(
                ErrorCode::NotActionable,
                "no physical keyboard focus",
            ));
        }
        Ok(Self(KEYBOARD_ANCHOR.with(|a| a.replace(Some(focus)))))
    }
}
impl Drop for KeyboardAnchor {
    fn drop(&mut self) {
        KEYBOARD_ANCHOR.with(|a| a.set(self.0));
    }
}
pub(crate) fn check_keyboard_anchor() -> Result<(), CtlError> {
    check_stop()?;
    crate::identity::check_focus_anchor()?;
    if let Some(expected) = KEYBOARD_ANCHOR.with(|a| a.get())
        && physical_focus()? != expected
    {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "physical keyboard focus changed before injection",
        ));
    }
    Ok(())
}

pub(crate) fn window_action(
    kind: &str,
    win: &UIElement,
    focus: bool,
) -> Result<ActionGuard, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let title = win.get_name().unwrap_or_default();
    let before = target(win, &title);
    let mut action = prepare(
        kind,
        InputEffects {
            focus,
            ..Default::default()
        },
        before.clone(),
    )?;
    if target(win, &title).bounds != before.bounds || win.get_name().is_err() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "window changed during action preparation",
        ));
    }
    check_stop()?;
    action.executing();
    Ok(action)
}

pub(crate) fn check_stop() -> Result<(), CtlError> {
    actl_core::stop::check_current()?;
    crate::display_gate::check_active()?;
    crate::handoff::check()
}

pub(crate) fn target(elem: &UIElement, window: &str) -> ActionTarget {
    let bounds = elem.get_bounding_rectangle().ok().and_then(|r| {
        let b = [r.get_left(), r.get_top(), r.get_right(), r.get_bottom()];
        (b[2] > b[0] && b[3] > b[1]).then_some(b)
    });
    ActionTarget {
        window: Some(window.into()),
        bounds,
        ..Default::default()
    }
}

pub(crate) fn keyboard(paste: bool, write: bool) -> InputEffects {
    InputEffects {
        keyboard: true,
        focus: true,
        clipboard: if write {
            ClipboardEffect::Write
        } else if paste {
            ClipboardEffect::Read
        } else {
            ClipboardEffect::Untouched
        },
        ..Default::default()
    }
}

/// Require a matching visible companion and acknowledgement of this resolved action.
/// The caller MUST revalidate the target after this cancellable preparation interval.
pub(crate) fn prepare(
    kind: &str,
    effects: InputEffects,
    target: ActionTarget,
) -> Result<ActionGuard, CtlError> {
    check_stop()?;
    let lease = crate::display_gate::ensure()?;
    crate::handoff::prepare(target.window.as_deref().unwrap_or(kind))?;
    let guard = ActionGuard::begin(kind, effects, target);
    if guard.is_recording() {
        crate::display_gate::wait_presented(&guard, &lease.epoch)?;
    }
    Ok(guard)
}

pub(crate) fn check_foreground(expected: &str) -> Result<(), CtlError> {
    check_stop()?;
    if crate::kbd::foreground_title().as_deref() != Some(expected) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "foreground changed during action preparation; re-observe before retrying",
        ));
    }
    Ok(())
}

/// UIA actions use a rectangle, never a fictitious mouse click.
pub(crate) fn semantic<T>(
    kind: &str,
    elem: &UIElement,
    window: &str,
    execute: impl FnOnce() -> Result<T, CtlError>,
) -> Result<T, CtlError> {
    let _dpi = crate::capture::Pmv2Guard::enter();
    let before = target(elem, window);
    let mut action = prepare(kind, InputEffects::default(), before.clone())?;
    if target(elem, window).bounds != before.bounds || !elem.is_enabled().unwrap_or(false) {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "target changed during action preparation",
        ));
    }
    check_stop()?;
    action.executing();
    let value = execute()?;
    action.delivered();
    Ok(value)
}
