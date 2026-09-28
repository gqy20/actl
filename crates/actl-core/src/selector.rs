//! Strict, persistent UIA queries. No fuzzy fallback or implicit ordinal selection.
use crate::CtlError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
}

impl Filter {
    fn validate(&self) -> Result<(), CtlError> {
        let fields = [&self.role, &self.id, &self.name, &self.name_contains];
        if fields.iter().all(|v| v.is_none()) {
            return Err(CtlError::protocol(
                "uia filter requires at least one condition",
            ));
        }
        if self.name.is_some() && self.name_contains.is_some() {
            return Err(CtlError::protocol(
                "uia name and name_contains are mutually exclusive",
            ));
        }
        for value in fields.into_iter().flatten() {
            if value.is_empty() || value.len() > 4096 || value.contains('\0') {
                return Err(CtlError::protocol(
                    "uia conditions require 1-4096 UTF-8 bytes without NUL",
                ));
            }
        }
        Ok(())
    }

    pub fn matches(&self, role: Option<&str>, id: Option<&str>, name: Option<&str>) -> bool {
        self.role
            .as_ref()
            .is_none_or(|v| role.is_some_and(|r| r.eq_ignore_ascii_case(v)))
            && self.id.as_ref().is_none_or(|v| id == Some(v.as_str()))
            && self.name.as_ref().is_none_or(|v| name == Some(v.as_str()))
            && self
                .name_contains
                .as_ref()
                .is_none_or(|v| name.is_some_and(|n| n.contains(v)))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Children,
    #[default]
    Descendants,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    #[serde(rename = "match")]
    pub filter: Filter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within: Option<Filter>,
    #[serde(default)]
    pub scope: Scope,
}

impl Selector {
    pub fn parse(raw: &str) -> Result<Self, CtlError> {
        if raw.len() > 16_384 {
            return Err(CtlError::protocol("uia selector exceeds 16 KiB"));
        }
        let selector: Self = serde_json::from_str(raw)
            .map_err(|e| CtlError::protocol(format!("invalid uia selector: {e}")))?;
        selector.filter.validate()?;
        if let Some(within) = &selector.within {
            within.validate()?;
        }
        Ok(selector)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_conditions_and_literal_punctuation() {
        let s =
            Selector::parse(r##"{"match":{"role":"button","id":"x#2","name":"Save > \"copy\""}}"##)
                .unwrap();
        assert!(
            s.filter
                .matches(Some("Button"), Some("x#2"), Some("Save > \"copy\""))
        );
        assert!(
            !s.filter
                .matches(Some("Edit"), Some("x#2"), Some("Save > \"copy\""))
        );
        assert!(
            !s.filter
                .matches(Some("Button"), None, Some("Save > \"copy\""))
        );
        assert!(
            !s.filter
                .matches(Some("Button"), Some("x#2"), Some("save > \"copy\""))
        );
    }

    #[test]
    fn rejects_ambiguous_or_unbounded_syntax() {
        for raw in [
            r#"{}"#,
            r#"{"match":{}}"#,
            r#"{"match":{"name":""}}"#,
            r#"{"match":{"name":"x","name_contains":"x"}}"#,
            r#"{"match":{"id":"a","id":"b"}}"#,
            r#"{"match":{"id":"a"},"match":{"id":"b"}}"#,
            r#"{"match":{"role":"Button","typo":1}}"#,
            r#"{"match":{"id":"x"},"within":{}}"#,
            r#"{"match":{"id":"x"},"within":{"within":{"id":"y"}}}"#,
            r#"{"match":{"id":"x"},"scope":"siblings"}"#,
            r#"{"match":{"id":"x"},"nth":1}"#,
            r#"{"match":{"id":"\u0000"}}"#,
        ] {
            assert!(Selector::parse(raw).is_err(), "{raw}");
        }
        assert!(Selector::parse(&" ".repeat(16_385)).is_err());
    }

    #[test]
    fn contains_is_explicit_and_case_sensitive() {
        let s = Selector::parse(
            r#"{"match":{"name_contains":"Save"},"within":{"role":"Pane"},"scope":"children"}"#,
        )
        .unwrap();
        assert!(s.filter.matches(None, None, Some("Save copy")));
        assert!(!s.filter.matches(None, None, Some("save copy")));
        assert_eq!(s.scope, Scope::Children);
    }

    #[test]
    fn target_roundtrip_keeps_the_same_semantics() {
        let raw = r#"uia:{"match":{"name":"保存","role":"Button"},"within":{"id":"panel"}}"#;
        let target = crate::parse_target(raw).unwrap();
        assert_eq!(crate::parse_target(&target.describe()).unwrap(), target);
    }
}
