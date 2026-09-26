//! 命令目标(@eN ref / 语义选择器)的解析,纯逻辑。
//!
//! 06 §3.1 规则③:位置参数只放目标。合法形式四种:
//! `@eN`(snapshot 重放序号)/ `name:子串` / `id:AutomationId 精确` / `role:Type 精确`。
//! ref 的漂移风险与 fallback 定位链的根治关系见 06 §5、spike-findings。

use crate::CtlError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// snapshot 中的第 N 个可交互元素(DFS 序重放)
    Ref(u32),
    /// Name 子串匹配(首个命中)
    Name(String),
    /// AutomationId 精确匹配
    Id(String),
    /// ControlType 精确匹配(如 Button)
    Role(String),
}

impl Target {
    /// 面向错误消息的人类可读形式
    pub fn describe(&self) -> String {
        match self {
            Target::Ref(n) => format!("@e{n}"),
            Target::Name(s) => format!("name:{s}"),
            Target::Id(s) => format!("id:{s}"),
            Target::Role(s) => format!("role:{s}"),
        }
    }
}

/// 解析目标字符串;非法形式 → PROTOCOL(exit 2 之外的协议层错误)。
pub fn parse_target(raw: &str) -> Result<Target, CtlError> {
    if let Some(n) = raw.strip_prefix("@e") {
        let n: u32 = n.parse().map_err(|_| bad(raw))?;
        if n == 0 {
            return Err(bad(raw)); // ref 从 @e1 起
        }
        return Ok(Target::Ref(n));
    }
    for (prefix, build) in [
        (
            "name:",
            (|s: String| Target::Name(s)) as fn(String) -> Target,
        ),
        ("id:", |s: String| Target::Id(s)),
        ("role:", |s: String| Target::Role(s)),
    ] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            if rest.is_empty() {
                return Err(bad(raw));
            }
            return Ok(build(rest.to_string()));
        }
    }
    Err(bad(raw))
}

fn bad(raw: &str) -> CtlError {
    CtlError::protocol(format!(
        "invalid target {raw:?}: expected @eN, name:<substr>, id:<AutomationId> or role:<Type>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn parses_all_four_forms() {
        assert_eq!(parse_target("@e3").unwrap(), Target::Ref(3));
        assert_eq!(
            parse_target("name:确定").unwrap(),
            Target::Name("确定".into())
        );
        assert_eq!(
            parse_target("id:num1Button").unwrap(),
            Target::Id("num1Button".into())
        );
        assert_eq!(
            parse_target("role:Button").unwrap(),
            Target::Role("Button".into())
        );
    }

    #[test]
    fn rejects_bare_words_and_edge_cases() {
        for bad in [
            "button", "@e", "@e0", "@ex", "name:", "id:", "role:", "@e-1",
        ] {
            let err = parse_target(bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::Protocol, "{bad:?} should be PROTOCOL");
        }
    }

    #[test]
    fn describe_round_trips_for_messages() {
        assert_eq!(parse_target("@e7").unwrap().describe(), "@e7");
    }
}
