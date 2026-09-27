//! 按键序列解析:actl 人文语法 `ctrl+s` → 结构化 KeySpec。
//!
//! 纯逻辑(core);执行(VK/UNICODE 注入)在 actl-uia 的 kbd 模块。
//! 历史注记:最初实现翻译为 uiautomation crate 的键表达式,但实测其 VK 注入不带
//! scan code(wScan=0),WinUI 应用直接丢弃 —— press 的执行已改为自建注入管线。

use crate::{CtlError, ErrorCode};

const MODIFIERS: &[&str] = &["ctrl", "shift", "alt", "win"];

/// 命名键白名单(单字符键走 Key::Char)。
pub const NAMED_KEYS: &[&str] = &[
    "enter",
    "esc",
    "tab",
    "space",
    "del",
    "delete",
    "backspace",
    "back",
    "up",
    "down",
    "left",
    "right",
    "home",
    "end",
    "pgup",
    "pgdn",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpec {
    /// 修饰键,已按 ctrl→shift→alt→win 归一顺序去重
    pub modifiers: Vec<String>,
    pub keys: Vec<Key>,
}

/// 解析 `ctrl+shift+t` / `enter` / `a`;非法 → PROTOCOL。
pub fn parse_key_expr(expr: &str) -> Result<KeySpec, CtlError> {
    let mut mods: Vec<String> = Vec::new();
    let mut keys: Vec<Key> = Vec::new();
    for part in expr.split('+') {
        let p = part.trim().to_lowercase();
        if p.is_empty() {
            return Err(bad(expr));
        }
        if MODIFIERS.contains(&p.as_str()) {
            if !mods.iter().any(|m| m == &p) {
                mods.push(p);
            }
            continue;
        }
        let mut chars = p.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            keys.push(Key::Char(c));
            continue;
        }
        let named = match p.as_str() {
            "delete" => "del",
            "back" => "backspace",
            other => other,
        };
        // f1..f12:数字段长度 1-2(曾因 p.len()==2 拒绝 f10-f12,回归见 tests)
        let is_fkey = (2..=3).contains(&p.len())
            && p.starts_with('f')
            && p[1..].chars().all(|c| c.is_ascii_digit())
            && (1..=12).contains(&p[1..].parse::<u8>().unwrap_or(0));
        if NAMED_KEYS.contains(&named) || is_fkey {
            keys.push(Key::Named(named.to_string()));
            continue;
        }
        return Err(bad(expr)); // 未知命名键
    }
    // 裸修饰键(win/shift)= 按下并释放该键本身:Win 弹开始菜单/呼出隐藏任务栏的
    // 入口(用户实测工作流);key-down shift 先例同语义。修饰键本身即按键。
    if keys.is_empty() && mods.is_empty() {
        return Err(bad(expr));
    }
    // 归一修饰键顺序
    let modifiers = MODIFIERS
        .iter()
        .filter(|m| mods.iter().any(|x| x == *m))
        .map(|m| m.to_string())
        .collect();
    Ok(KeySpec { modifiers, keys })
}

fn bad(expr: &str) -> CtlError {
    CtlError::new(
        ErrorCode::Protocol,
        format!("invalid key expression {expr:?}: use e.g. ctrl+s, ctrl+shift+t, enter, f5"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_combos() {
        let spec = parse_key_expr("ctrl+s").unwrap();
        assert_eq!(spec.modifiers, vec!["ctrl"]);
        assert_eq!(spec.keys, vec![Key::Char('s')]);

        let spec = parse_key_expr("ctrl+shift+t").unwrap();
        assert_eq!(spec.modifiers, vec!["ctrl", "shift"]);

        let spec = parse_key_expr("alt+F4").unwrap();
        assert_eq!(spec.keys, vec![Key::Named("f4".into())]);

        assert_eq!(
            parse_key_expr("pgdn").unwrap().keys,
            vec![Key::Named("pgdn".into())]
        );
        assert_eq!(parse_key_expr("a").unwrap().keys, vec![Key::Char('a')]);
    }

    #[test]
    fn modifier_order_is_normalized_and_deduped() {
        let spec = parse_key_expr("shift+ctrl+a").unwrap();
        assert_eq!(spec.modifiers, vec!["ctrl", "shift"]);
        let spec = parse_key_expr("ctrl+ctrl+a").unwrap();
        assert_eq!(spec.modifiers, vec!["ctrl"]);
    }

    #[test]
    fn aliases_normalize() {
        assert_eq!(
            parse_key_expr("delete").unwrap().keys,
            vec![Key::Named("del".into())]
        );
        assert_eq!(
            parse_key_expr("back").unwrap().keys,
            vec![Key::Named("backspace".into())]
        );
    }

    #[test]
    fn rejects_garbage() {
        // 注:"f" 是合法单字符(Char('f')),"f01" 归一为 f1,均不该在此列
        for bad in ["", "foo+bar", "ctrl+", "+a", "notakey", "f0", "f13", "f99"] {
            assert_eq!(
                parse_key_expr(bad).unwrap_err().code,
                ErrorCode::Protocol,
                "{bad:?} should be PROTOCOL"
            );
        }
    }

    #[test]
    fn fkeys_f1_through_f12_all_parse() {
        // 回归:旧条件 p.len()==2 把 f10/f11/f12 挡在解析层,后端却支持 1..=12
        for name in [
            "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
        ] {
            let spec = parse_key_expr(name).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(spec.keys, vec![Key::Named(name.into())], "{name}");
            assert!(spec.modifiers.is_empty());
        }
    }
}

#[cfg(test)]
mod bare_modifier_tests {
    use super::*;

    #[test]
    fn bare_modifier_is_a_keyable_press() {
        // press win = 弹开始菜单(呼出自动隐藏任务栏的工作流);key-down shift 同语义
        let spec = parse_key_expr("win").unwrap();
        assert_eq!(spec.modifiers, vec!["win"]);
        assert!(spec.keys.is_empty());
        assert!(parse_key_expr("shift").is_ok());
        assert!(parse_key_expr("").is_err());
    }
}
