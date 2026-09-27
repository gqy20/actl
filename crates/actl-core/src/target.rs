//! 命令目标(@eN ref / 语义选择器 / 坐标)的解析,纯逻辑。
//!
//! 06 §3.1 规则③:位置参数只放目标。合法形式:
//! `@eN`(snapshot 重放序号)/ `name:子串` / `id:AutomationId 精确` /
//! `role:Type 精确` / `xy:x,y`(屏幕坐标,视觉兜底专用——无元素可校验,
//! 强制 `--app` 窗口边界守卫 + 显式 `--physical`,见 06 §7)。
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
    /// ControlType + 1 起序数(`role:Button#2` = 第 2 个 Button;fallback 链 L3)
    RoleAt(String, u32),
    /// 屏幕物理坐标(`xy:1420,856`;负值合法——多显示器左侧屏)。
    /// 视觉兜底的"手":坐标来自调用方(screenshot+VLM/人),不经元素定位;
    /// 强制 --app + --physical(窗口边界守卫,06 §7)
    Coord(i32, i32),
}

impl Target {
    /// 面向错误消息的人类可读形式
    pub fn describe(&self) -> String {
        match self {
            Target::Ref(n) => format!("@e{n}"),
            Target::Name(s) => format!("name:{s}"),
            Target::Id(s) => format!("id:{s}"),
            Target::Role(s) => format!("role:{s}"),
            Target::RoleAt(s, n) => format!("role:{s}#{n}"),
            Target::Coord(x, y) => format!("xy:{x},{y}"),
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
    // xy:x,y(物理屏幕坐标;负 x 合法——虚拟桌面左侧屏)
    if let Some(rest) = raw.strip_prefix("xy:") {
        if let Some((xs, ys)) = rest.split_once(',') {
            if let (Ok(x), Ok(y)) = (xs.trim().parse::<i32>(), ys.trim().parse::<i32>()) {
                return Ok(Target::Coord(x, y));
            }
        }
        return Err(bad(raw));
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
            // role:Type#N → 序数形式(fallback 链 L3;#0 非法)
            if prefix == "role:" {
                if let Some((role, idx)) = rest.rsplit_once('#') {
                    let n: u32 = idx.parse().map_err(|_| bad(raw))?;
                    if !role.is_empty() && n >= 1 {
                        return Ok(Target::RoleAt(role.to_string(), n));
                    }
                    return Err(bad(raw));
                }
            }
            return Ok(build(rest.to_string()));
        }
    }
    Err(bad(raw))
}

fn bad(raw: &str) -> CtlError {
    CtlError::protocol(format!(
        "invalid target {raw:?}: expected @eN, name:<substr>, id:<AutomationId>, role:<Type> or xy:<x>,<y>"
    ))
}

/// L2 模糊匹配:大小写不敏感 + 空白归一(连续空白折成一格 + 去首尾)。
/// 名称动态变化的界面(尾缀计数、对齐空格)靠它兜住。
pub fn fuzzy_contains(haystack: &str, needle: &str) -> bool {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let h = norm(haystack);
    let n = norm(needle);
    !n.is_empty() && h.contains(&n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn parses_all_forms() {
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
        assert_eq!(
            parse_target("role:Button#2").unwrap(),
            Target::RoleAt("Button".into(), 2)
        );
        assert_eq!(
            parse_target("xy:1420,856").unwrap(),
            Target::Coord(1420, 856)
        );
        // 负坐标合法(虚拟桌面左侧屏)
        assert_eq!(
            parse_target("xy:-100,200").unwrap(),
            Target::Coord(-100, 200)
        );
        assert_eq!(parse_target("xy:0,0").unwrap(), Target::Coord(0, 0));
    }

    #[test]
    fn coord_edges_rejected() {
        for bad in ["xy:100", "xy:100,", "xy:,200", "xy:a,b", "xy:", "xy:1,2,3"] {
            let err = parse_target(bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::Protocol, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn role_ordinal_edges() {
        for bad in ["role:Button#0", "role:#2", "role:Button#", "role:Button#x"] {
            assert!(parse_target(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn fuzzy_matches_case_and_whitespace_insensitive() {
        assert!(fuzzy_contains("保存  设置", "保存 设置"));
        assert!(fuzzy_contains("Open File", "open   file"));
        assert!(!fuzzy_contains("Open", "open file"));
        assert!(!fuzzy_contains("anything", ""));
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
