//! Validate structured targets before dispatch or any flow step. Text payloads are not targets.
use crate::cli::Command;
use actl_core::{CtlError, Target, parse_target};

fn check(raw: &str, near: Option<&str>) -> Result<(), CtlError> {
    if raw.starts_with("uia:") {
        parse_target(raw)?;
        if near.is_some() {
            return Err(CtlError::protocol(
                "uia selectors do not support --near; use within and unique conditions",
            ));
        }
    }
    // The legacy anchor path cannot safely interpret structured queries.
    if near.is_some_and(|s| s.starts_with("uia:")) {
        return Err(CtlError::protocol(
            "uia selectors cannot be used as --near anchors; use within",
        ));
    }
    Ok(())
}

pub(crate) fn validate(command: &Command) -> Result<(), CtlError> {
    if let Command::Click {
        expect: Some(expect),
        ..
    }
    | Command::Press {
        expect: Some(expect),
        ..
    } = command
        && let Some(target) = expect.strip_prefix("element:")
    {
        check(target, None)?;
    }
    match command {
        Command::Snapshot {
            within: None,
            depth,
            limit,
            ..
        } if depth.is_some() || limit.is_some() => {
            return Err(CtlError::protocol(
                "snapshot --depth/--limit require --within",
            ));
        }
        Command::Snapshot {
            within: Some(within),
            app,
            hwnd,
            depth,
            limit,
            ..
        } => {
            if app.as_deref().is_none_or(|s| s.trim().is_empty()) && hwnd.is_none() {
                return Err(CtlError::protocol(
                    "snapshot --within requires --app or --hwnd",
                ));
            }
            actl_uia::scoped_snapshot::validate(
                within,
                depth.unwrap_or(actl_uia::MAX_DEPTH),
                limit.unwrap_or(actl_uia::MAX_ELEMENTS),
            )?;
        }
        Command::Preview {
            target,
            candidate,
            app,
            snapshot,
            message,
            duration,
            ..
        } => {
            if app.trim().is_empty() {
                return Err(CtlError::protocol("preview requires a nonempty --app"));
            }
            let target = parse_target(target)?;
            actl_uia::preview::validate(&target, snapshot.as_deref(), message, *duration)?;
            actl_uia::preview::validate_candidates(&target, candidate, snapshot.as_deref())?;
        }
        Command::Click { target, near, .. }
        | Command::Get { target, near, .. }
        | Command::Type { target, near, .. }
        | Command::SetValue { target, near, .. }
        | Command::RightClick { target, near, .. }
        | Command::DoubleClick { target, near, .. }
        | Command::TripleClick { target, near, .. }
        | Command::Hover { target, near, .. }
        | Command::Scroll { target, near, .. }
        | Command::Select { target, near, .. }
        | Command::Toggle { target, near, .. } => check(target, near.as_deref())?,
        Command::Drag {
            target,
            to,
            near,
            to_near,
            ..
        } => {
            check(target, near.as_deref())?;
            check(to, to_near.as_deref())?;
        }
        Command::Extract { target, .. } => check(target, None)?,
        Command::Press {
            target: Some(target),
            ..
        } => check(target, None)?,
        Command::Screenshot {
            element: Some(target),
            snear,
            ..
        } => check(target, snear.as_deref())?,
        Command::Verify {
            exists,
            value,
            checked,
            near,
            ..
        } => {
            for target in exists.iter().chain(value.iter()).chain(checked.iter()) {
                check(target, near.as_deref())?;
            }
        }
        Command::Wait { element, gone, .. } => {
            for target in element.iter().chain(gone.iter()) {
                check(target, None)?;
            }
        }
        Command::Find {
            target: Some(target),
            ..
        } if !matches!(parse_target(target)?, Target::Uia(_)) => {
            return Err(CtlError::protocol("find --target requires a uia: selector"));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn every_target_position_is_preflighted_without_parsing_payloads() {
        for args in [
            vec!["click", "uia:{}"],
            vec!["get", "uia:{}"],
            vec!["type", "uia:{}", "text"],
            vec!["set-value", "uia:{}", "text"],
            vec!["right-click", "uia:{}"],
            vec!["double-click", "uia:{}"],
            vec!["triple-click", "uia:{}"],
            vec!["hover", "uia:{}"],
            vec!["scroll", "uia:{}", "--down", "1"],
            vec!["select", "uia:{}", "choice"],
            vec!["toggle", "uia:{}"],
            vec!["extract", "uia:{}"],
            vec!["drag", "id:x", "uia:{}"],
            vec!["drag", "uia:{}", "id:x"],
            vec!["press", "enter", "--target", "uia:{}"],
            vec!["screenshot", "--element", "uia:{}"],
            vec!["verify", "--exists", "uia:{}"],
            vec!["verify", "--value", "uia:{}", "text"],
            vec!["verify", "--checked", "uia:{}"],
            vec!["wait", "--element", "uia:{}"],
            vec!["wait", "--gone", "uia:{}"],
            vec!["find", "--target", "uia:{}"],
            vec!["click", "id:x", "--expect", "element:uia:{}"],
            vec!["press", "enter", "--expect", "element:uia:{}"],
        ] {
            let command = Cli::try_parse_from(std::iter::once("actl").chain(args.iter().copied()))
                .unwrap()
                .command;
            assert!(validate(&command).is_err(), "{args:?}");
        }
        for args in [
            vec!["type", "id:x", "uia:{}"],
            vec!["set-value", "id:x", "uia:{}"],
            vec!["verify", "--value", "id:x", "uia:{}"],
        ] {
            let command = Cli::try_parse_from(std::iter::once("actl").chain(args))
                .unwrap()
                .command;
            assert!(validate(&command).is_ok());
        }
    }

    #[test]
    fn strict_queries_cannot_use_distance_fallback() {
        let q = r#"uia:{"match":{"role":"Button"}}"#;
        assert!(check(q, Some("name:OK")).is_err());
        assert!(check("id:x", Some(q)).is_err());
        assert!(check(q, None).is_ok());
    }
}
