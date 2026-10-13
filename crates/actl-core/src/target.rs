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
    /// Strict conjunction with an optional unique container scope.
    Uia(Box<crate::selector::Selector>),
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
    /// AutomationId + 1 起序数(`id:RootWebArea#2` = 第 2 个同名 id)——多文档
    /// 页的确定性寻址(iframe/扩展弹层会让同一 AutomationId 出现多次;
    /// 2026-09-27 三连踩后补,与 RoleAt 同机制)
    IdAt(String, u32),
    /// OS 键盘焦点元素(`focused:` 裸 / `focused:Edit` 角色校验)。
    /// 树对 name:/id:/role: 不可见但焦点可达的应用(Electron 懒树,
    /// 如 uTools)的唯一元素寻址通道;焦点唯一,无序数形式。
    /// 属主与角色校验 fail-closed(焦点不在 --app 窗口/角色不符 → NOT_FOUND)。
    Focused(Option<String>),
    /// 屏幕物理坐标(`xy:1420,856`;负值合法——多显示器左侧屏)。
    /// 视觉兜底的"手":坐标来自调用方(screenshot+VLM/人),不经元素定位;
    /// 强制 --app + --physical(窗口边界守卫,06 §7)
    Coord(i32, i32),
    /// 窗口客户区相对坐标(`xyw:300,120`,2026-10-13 Fix B):原点=目标窗口
    /// 客户区左上角,单位=物理像素——截图上看到的像素位置直接可点,agent
    /// 不做缩放/平移换算;同样强制 --app + --physical(边界守卫在换算后的
    /// 屏幕绝对坐标上执行,守卫语义与 xy: 一致)。
    CoordRel(i32, i32),
}

impl Target {
    /// 面向错误消息的人类可读形式
    pub fn describe(&self) -> String {
        match self {
            Target::Uia(selector) => format!("uia:{}", serde_json::json!(selector)),
            Target::Ref(n) => format!("@e{n}"),
            Target::Name(s) => format!("name:{s}"),
            Target::Id(s) => format!("id:{s}"),
            Target::Role(s) => format!("role:{s}"),
            Target::RoleAt(s, n) => format!("role:{s}#{n}"),
            Target::IdAt(s, n) => format!("id:{s}#{n}"),
            Target::Focused(None) => "focused:".into(),
            Target::Focused(Some(role)) => format!("focused:{role}"),
            Target::Coord(x, y) => format!("xy:{x},{y}"),
            Target::CoordRel(x, y) => format!("xyw:{x},{y}"),
        }
    }
}

/// 解析目标字符串;非法形式 → PROTOCOL(exit 2 之外的协议层错误)。
pub fn parse_target(raw: &str) -> Result<Target, CtlError> {
    if let Some(json) = raw.strip_prefix("uia:") {
        return crate::selector::Selector::parse(json).map(|s| Target::Uia(Box::new(s)));
    }
    if let Some(n) = raw.strip_prefix("@e") {
        let n: u32 = n.parse().map_err(|_| bad(raw))?;
        if n == 0 {
            return Err(bad(raw)); // ref 从 @e1 起
        }
        return Ok(Target::Ref(n));
    }
    // 裸 e<N> 与 @e<N> 同义(2026-10-12 交互轮):PowerShell 把裸 @eN 当
    // splat 语法吞掉(agent 侧两次踩坑),e<N> 是免引号等价形态;此前的
    // 未知形态报错意味着无回归面。
    if let Some(n) = raw
        .strip_prefix('e')
        .filter(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
    {
        let n: u32 = n.parse().map_err(|_| bad(raw))?;
        if n == 0 {
            return Err(bad(raw));
        }
        return Ok(Target::Ref(n));
    }
    // xy:x,y(物理屏幕坐标;负 x 合法——虚拟桌面左侧屏)
    if let Some(rest) = raw.strip_prefix("xy:") {
        if let Some((xs, ys)) = rest.split_once(',')
            && let (Ok(x), Ok(y)) = (xs.trim().parse::<i32>(), ys.trim().parse::<i32>())
        {
            return Ok(Target::Coord(x, y));
        }
        return Err(bad(raw));
    }
    // xyw:x,y(窗口客户区相对坐标,Fix B):换算归执行侧,agent 零换算;
    // 负值非法(客户区相对坐标恒在客户区内,负数是调用方算错的信号)
    if let Some(rest) = raw.strip_prefix("xyw:") {
        if let Some((xs, ys)) = rest.split_once(',')
            && let (Ok(x), Ok(y)) = (xs.trim().parse::<i32>(), ys.trim().parse::<i32>())
            && x >= 0
            && y >= 0
        {
            return Ok(Target::CoordRel(x, y));
        }
        return Err(bad(raw));
    }
    // focused:/focused:<Role>:OS 键盘焦点元素;焦点唯一,序数/空白非法
    if let Some(rest) = raw.strip_prefix("focused:") {
        if rest.contains('#') || rest.contains(char::is_whitespace) {
            return Err(bad(raw));
        }
        return Ok(Target::Focused(
            (!rest.is_empty()).then(|| rest.to_string()),
        ));
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
            // <Type|Id>#N → 序数形式(fallback 链 L3;#0 非法)
            if (prefix == "role:" || prefix == "id:")
                && let Some((key, idx)) = rest.rsplit_once('#')
            {
                let n: u32 = idx.parse().map_err(|_| bad(raw))?;
                if key.is_empty() || n < 1 {
                    return Err(bad(raw));
                }
                return Ok(if prefix == "role:" {
                    Target::RoleAt(key.to_string(), n)
                } else {
                    Target::IdAt(key.to_string(), n)
                });
            }
            return Ok(build(rest.to_string()));
        }
    }
    Err(bad(raw))
}

fn bad(raw: &str) -> CtlError {
    CtlError::protocol(format!(
        "invalid target {raw:?}: expected @eN, name:<substr>, id:<AutomationId>, role:<Type>, focused:[<Type>] or xy:<x>,<y>"
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
        // 裸 e<N> 别名(PS splat 安全):与 @e<N> 同义;e0 非法同 @e0;
        // 非 e 开头既有形态不受影响
        assert_eq!(parse_target("e3").unwrap(), Target::Ref(3));
        assert_eq!(parse_target("e391").unwrap(), Target::Ref(391));
        assert!(parse_target("e0").is_err());
        assert!(parse_target("e").is_err());
        assert!(parse_target("e3x").is_err());
        assert_eq!(parse_target("name:e3").unwrap(), Target::Name("e3".into()));
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
            parse_target("id:RootWebArea#2").unwrap(),
            Target::IdAt("RootWebArea".into(), 2)
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
        // xyw:窗口客户区相对坐标(Fix B):零换算寻址;负值 PROTOCOL 拒绝
        assert_eq!(
            parse_target("xyw:300,120").unwrap(),
            Target::CoordRel(300, 120)
        );
        assert_eq!(parse_target("xyw:0,0").unwrap(), Target::CoordRel(0, 0));
        assert!(parse_target("xyw:-5,10").is_err());
        assert!(parse_target("xyw:300").is_err());
        assert_eq!(Target::CoordRel(300, 120).describe(), "xyw:300,120");
        assert_eq!(parse_target("xy:0,0").unwrap(), Target::Coord(0, 0));
        // focused:裸(不校验角色)与角色校验形态
        assert_eq!(parse_target("focused:").unwrap(), Target::Focused(None));
        assert_eq!(
            parse_target("focused:Edit").unwrap(),
            Target::Focused(Some("Edit".into()))
        );
    }

    #[test]
    fn focused_rejects_ordinals_and_whitespace() {
        for bad in [
            "focused:Edit#2",
            "focused: Edit",
            "focused:Edit ",
            "focused:a b",
        ] {
            let err = parse_target(bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::Protocol, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn coord_edges_rejected() {
        for bad in ["xy:100", "xy:100,", "xy:,200", "xy:a,b", "xy:", "xy:1,2,3"] {
            let err = parse_target(bad).unwrap_err();
            assert_eq!(err.code, ErrorCode::Protocol, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn ordinal_edges() {
        for bad in [
            "role:Button#0",
            "role:#2",
            "role:Button#",
            "role:Button#x",
            "id:Foo#0",
            "id:#2",
            "id:Foo#",
            "id:Foo#x",
        ] {
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
