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

// clap 变体字段直转的薄适配层——参数即命令面(与 read_cmd/collect_cmd 同豁免)
#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_cmd(
    browser_executable: &str,
    url: Option<&str>,
    profile: Option<&str>,
    proxy: Option<&str>,
    user_agent: Option<&str>,
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
            user_agent: user_agent.map(str::to_string),
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

// clap 变体字段直转的薄适配层——参数即命令面,不拆束(与 dispatch 对齐)
#[allow(clippy::too_many_arguments)]
pub(crate) fn read_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    property: Option<&str>,
    iframe: bool,
    all: bool,
    deep: bool,
    limit: Option<usize>,
    wait_for: Option<u64>,
    target: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::read(
        &paths(),
        ops::ReadSpec {
            session_id: session,
            kind,
            value,
            property,
            iframe,
            timeout_ms: timeout,
            all,
            limit,
            deep,
            wait_for,
            target,
        },
    )
    .map(Into::into)
}

/// clap 变体字段直转的薄适配层(参数即命令面,与 read_cmd 同豁免)
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    property: Option<&str>,
    deep: bool,
    interval_ms: Option<u64>,
    max_rounds: Option<u64>,
    max_items: Option<u64>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    actl_browser::collect::collect(
        &paths(),
        actl_browser::collect::CollectSpec {
            session_id: session,
            kind,
            value,
            property,
            deep,
            interval_ms,
            max_rounds,
            max_items,
            timeout_ms: timeout,
        },
    )
    .map(Into::into)
}

pub(crate) fn netlog_cmd(
    session: &str,
    filter: Option<&str>,
    window: Option<u64>,
    max_entries: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    actl_browser::deep::netlog(
        &paths(),
        actl_browser::deep::NetlogSpec {
            session_id: session,
            filter,
            window_ms: window,
            max_entries,
        },
    )
    .map(Into::into)
}

// clap 变体字段直转的薄适配层——参数即命令面(与 read_cmd/collect_cmd 同豁免)
#[allow(clippy::too_many_arguments)]
pub(crate) fn type_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    text: &str,
    iframe: bool,
    timeout: Option<u64>,
    allow_popups: bool,
    target: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::type_text(
        &paths(),
        session,
        kind,
        value,
        text,
        iframe,
        timeout,
        allow_popups,
        target,
    )
    .map(Into::into)
}

// clap 变体字段直转的薄适配层(参数即命令面,与 read_cmd 同豁免)
#[allow(clippy::too_many_arguments)]
pub(crate) fn click_cmd(
    session: &str,
    css: Option<&str>,
    xpath: Option<&str>,
    iframe: bool,
    expect_text: Option<&str>,
    timeout: Option<u64>,
    allow_popups: bool,
    target: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let (kind, value) = selector_of(css, xpath)?;
    ops::click(
        &paths(),
        session,
        kind,
        value,
        iframe,
        expect_text,
        timeout,
        allow_popups,
        target,
    )
    .map(Into::into)
}

pub(crate) fn close_cmd(session: &str, timeout: Option<u64>) -> Result<CommandOutput, CtlError> {
    ops::close(&paths(), session, timeout).map(Into::into)
}

pub(crate) fn list_cmd() -> Result<CommandOutput, CtlError> {
    ops::list(&paths()).map(Into::into)
}

/// CDP target 枚举(只读):--target 寻址的发现通道(扩展 popup 写链第一步)。
pub(crate) fn targets_cmd(session: &str) -> Result<CommandOutput, CtlError> {
    actl_browser::deep::targets(&paths(), session).map(Into::into)
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
    pub no_popups: bool,
    pub popup_selector: Option<&'a str>,
    pub challenge: bool,
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
        no_popups,
        popup_selector,
        challenge,
        timeout,
    } = args;
    let mode = match (
        session,
        alive,
        gone,
        css,
        xpath,
        none,
        newest_alive,
        no_popups,
        challenge,
    ) {
        (Some(id), true, false, None, None, false, false, false, false) => {
            VerifyMode::SessionAlive(id.to_string())
        }
        (Some(id), false, true, None, None, false, false, false, false) => {
            VerifyMode::SessionGone(id.to_string())
        }
        (Some(id), false, false, Some(css), None, false, false, false, false) => {
            VerifyMode::Selector {
                session: id.to_string(),
                kind: SelectorKind::Css,
                value: css.to_string(),
                contains: contains.map(str::to_string),
            }
        }
        (Some(id), false, false, None, Some(xpath), false, false, false, false) => {
            VerifyMode::Selector {
                session: id.to_string(),
                kind: SelectorKind::Xpath,
                value: xpath.to_string(),
                contains: contains.map(str::to_string),
            }
        }
        (Some(id), false, false, None, None, false, false, true, false) => VerifyMode::NoPopups {
            session: id.to_string(),
            selector: popup_selector.map(str::to_string),
        },
        (Some(id), false, false, None, None, false, false, false, true) => VerifyMode::Challenge {
            session: id.to_string(),
        },
        (None, false, false, None, None, true, false, false, false) => VerifyMode::NoneActive,
        (None, false, false, None, None, false, true, false, false) => VerifyMode::NewestAlive,
        _ => {
            return Err(CtlError::protocol(
                "browser-verify requires exactly one mode: --newest-alive | --none | --session+--alive | --session+--gone | --session+--css/--xpath [--contains] | --session+--no-popups [--popup-selector] | --session+--challenge",
            ));
        }
    };
    actl_browser::verify::verify(&paths(), mode, timeout).map(Into::into)
}

/// browser-screenshot 的 CLI 参数束。
pub(crate) struct ScreenshotArgs<'a> {
    pub session: &'a str,
    pub css: Option<&'a str>,
    pub xpath: Option<&'a str>,
    pub iframe: bool,
    pub target: Option<&'a str>,
    pub out: Option<&'a str>,
    pub timeout: Option<u64>,
}

pub(crate) fn screenshot_cmd(args: ScreenshotArgs<'_>) -> Result<CommandOutput, CtlError> {
    let selector = match (args.css, args.xpath) {
        (Some(value), None) => Some((SelectorKind::Css, value)),
        (None, Some(value)) => Some((SelectorKind::Xpath, value)),
        (None, None) => None,
        _ => {
            return Err(CtlError::protocol(
                "pass at most one of --css or --xpath (omit both for a full-viewport capture)",
            ));
        }
    };
    actl_browser::shot::screenshot(
        &paths(),
        actl_browser::shot::ShotSpec {
            session_id: args.session,
            selector,
            iframe: args.iframe,
            target: args.target,
            out: args.out,
            timeout_ms: args.timeout,
        },
    )
    .map(Into::into)
}

/// browser-drag 的 CLI 参数束。
pub(crate) struct DragArgs<'a> {
    pub session: &'a str,
    pub from: &'a str,
    pub to: &'a str,
    pub human: bool,
    pub seed: Option<u64>,
    pub target: Option<&'a str>,
    pub timeout: Option<u64>,
}

/// X,Y 解析(视口 CSS px;与 --path waypoint 同族小函数,无坐标守卫——
/// CDP 轨的守卫是元素唯一匹配,不是窗口边界)。
fn parse_viewport_point(raw: &str, label: &str) -> Result<(f64, f64), CtlError> {
    let Some((x, y)) = raw.split_once(',') else {
        return Err(CtlError::protocol(format!(
            "{label} must be X,Y (viewport CSS pixels), got {raw:?}"
        )));
    };
    let parse = |value: &str| -> Result<f64, CtlError> {
        value.trim().parse::<f64>().map_err(|_| {
            CtlError::protocol(format!("{label} coordinate must be numeric, got {value:?}"))
        })
    };
    Ok((parse(x)?, parse(y)?))
}

pub(crate) fn drag_cmd(args: DragArgs<'_>) -> Result<CommandOutput, CtlError> {
    let from = parse_viewport_point(args.from, "--from")?;
    let to = parse_viewport_point(args.to, "--to")?;
    actl_browser::input::drag(
        &paths(),
        actl_browser::input::DragSpec {
            session_id: args.session,
            from,
            to,
            human: args.human,
            seed: args.seed,
            target: args.target,
            timeout_ms: args.timeout,
        },
    )
    .map(Into::into)
}

/// challenge-solve 的 CLI 参数束(v1 CSS-only——真实挑战的选择器面
/// 是 CSS 惯例;XPath 需要时按 spec 扩)。
pub(crate) struct SolveArgs<'a> {
    pub session: &'a str,
    pub track_css: &'a str,
    pub piece_css: &'a str,
    pub handle_css: &'a str,
    pub success_css: &'a str,
    pub template: Option<&'a str>,
    pub threshold: Option<f64>,
    pub attempts: Option<u64>,
    pub human: bool,
    pub seed: Option<u64>,
    pub iframe: bool,
    pub target: Option<&'a str>,
    pub timeout: Option<u64>,
}

pub(crate) fn solve_cmd(args: SolveArgs<'_>) -> Result<CommandOutput, CtlError> {
    actl_browser::challenge::solve(
        &paths(),
        actl_browser::challenge::SolveSpec {
            session_id: args.session,
            track: (SelectorKind::Css, args.track_css),
            piece: (SelectorKind::Css, args.piece_css),
            handle: (SelectorKind::Css, args.handle_css),
            success: Some((SelectorKind::Css, args.success_css)),
            template: args.template,
            threshold: args.threshold,
            attempts: args.attempts,
            human: args.human,
            seed: args.seed,
            target: args.target,
            iframe: args.iframe,
            timeout_ms: args.timeout,
        },
    )
    .map(Into::into)
}
