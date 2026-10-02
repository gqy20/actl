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
    disable_extensions: bool,
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
            disable_extensions,
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
    property: Option<&str>,
    iframe: bool,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::read(&paths(), session, kind, value, property, iframe, timeout).map(Into::into)
}

pub(crate) fn type_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    text: &str,
    iframe: bool,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::type_text(&paths(), session, kind, value, text, iframe, timeout).map(Into::into)
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

pub(crate) fn list_cmd() -> Result<CommandOutput, CtlError> {
    ops::list(&paths()).map(Into::into)
}

/// browser-verify 的 CLI 参数束(避开多参数告警;main.rs 从 clap 变体直构)。
pub(crate) struct VerifyArgs<'a> {
    pub session: Option<&'a str>,
    pub alive: bool,
    pub gone: bool,
    pub css: Option<&'a str>,
    pub xpath: Option<&'a str>,
    pub contains: Option<&'a str>,
    pub none: bool,
    pub newest_alive: bool,
    pub timeout: Option<u64>,
}

pub(crate) fn verify_cmd(args: VerifyArgs<'_>) -> Result<CommandOutput, CtlError> {
    use actl_browser::VerifyMode;
    let VerifyArgs {
        session,
        alive,
        gone,
        css,
        xpath,
        contains,
        none,
        newest_alive,
        timeout,
    } = args;
    let mode = match (session, alive, gone, css, xpath, none, newest_alive) {
        (Some(id), true, false, None, None, false, false) => {
            VerifyMode::SessionAlive(id.to_string())
        }
        (Some(id), false, true, None, None, false, false) => {
            VerifyMode::SessionGone(id.to_string())
        }
        (Some(id), false, false, Some(css), None, false, false) => VerifyMode::Selector {
            session: id.to_string(),
            kind: SelectorKind::Css,
            value: css.to_string(),
            contains: contains.map(str::to_string),
        },
        (Some(id), false, false, None, Some(xpath), false, false) => VerifyMode::Selector {
            session: id.to_string(),
            kind: SelectorKind::Xpath,
            value: xpath.to_string(),
            contains: contains.map(str::to_string),
        },
        (None, false, false, None, None, true, false) => VerifyMode::NoneActive,
        (None, false, false, None, None, false, true) => VerifyMode::NewestAlive,
        _ => {
            return Err(CtlError::protocol(
                "browser-verify requires exactly one mode: --newest-alive | --none | --session+--alive | --session+--gone | --session+--css/--xpath [--contains]",
            ));
        }
    };
    ops::verify(&paths(), mode, timeout).map(Into::into)
}
