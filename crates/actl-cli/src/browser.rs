//! browser —— CDP 通道命令执行体(actl-browser 包装,设计 docs/72)。
//! 会话生命周期与页面读写;与 UIA 通道按会话归属分工,本模块只管 actl 自有浏览器。

use actl_browser::{SelectorKind, ops, session::Paths};
use actl_core::CtlError;

use crate::CommandOutput;

fn paths() -> Paths {
    Paths::default()
}

fn selector_of<'a>(
    css: Option<&'a str>,
    xpath: Option<&'a str>,
) -> Result<(SelectorKind, &'a str), CtlError> {
    match (css, xpath) {
        (Some(v), None) => Ok((SelectorKind::Css, v)),
        (None, Some(v)) => Ok((SelectorKind::Xpath, v)),
        _ => Err(CtlError::protocol(
            "exactly one of --css or --xpath is required",
        )),
    }
}

pub(crate) fn launch_cmd(
    browser_executable: &str,
    url: Option<&str>,
    profile: Option<&str>,
    proxy: Option<&str>,
    headless: bool,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    ops::launch(
        &paths(),
        ops::LaunchSpec {
            browser_executable: browser_executable.to_string(),
            url: url.map(str::to_string),
            profile: profile.map(str::to_string),
            proxy: proxy.map(str::to_string),
            headless,
            timeout_ms: timeout,
        },
    )
    .map(Into::into)
}

pub(crate) fn navigate_cmd(
    session: &str,
    url: &str,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    ops::navigate(&paths(), session, url, timeout).map(Into::into)
}

pub(crate) fn read_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    iframe: bool,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::read(&paths(), session, kind, value, iframe, timeout).map(Into::into)
}

pub(crate) fn click_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    iframe: bool,
    expect_text: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::click(&paths(), session, kind, value, iframe, expect_text, timeout).map(Into::into)
}

pub(crate) fn close_cmd(session: &str, timeout: Option<u64>) -> Result<CommandOutput, CtlError> {
    ops::close(&paths(), session, timeout).map(Into::into)
}
