use super::*;

#[test]
fn role_only_queries_have_a_native_seed() {
    assert_eq!(super::super::role_id("document"), Some(50030));
    assert_eq!(super::super::role_id("Edit"), Some(50004));
    assert_eq!(super::super::role_id("unknown-role"), None);
    let q = query(r#"{"match":{"role":"Document"}}"#);
    assert_eq!(
        resolve(&provider(vec![node("doc", "Document", vec![])]), &q)
            .unwrap()
            .item
            .role,
        "Document"
    );
}

#[derive(Clone)]
struct Provider {
    hits: Vec<Fake>,
    item: Fake,
    error: Option<ErrorCode>,
}
impl Tree for Provider {
    fn exact_candidates(
        &self,
        _: &Filter,
        _: Scope,
        _: bool,
    ) -> Result<Option<Vec<Self>>, CtlError> {
        if let Some(code) = self.error {
            return Err(CtlError::new(code, "provider failed"));
        }
        Ok(Some(
            self.hits
                .iter()
                .map(|item| Self {
                    item: item.clone(),
                    hits: vec![],
                    error: None,
                })
                .collect(),
        ))
    }
    fn children(&self) -> Result<Vec<Self>, CtlError> {
        panic!("provider query must not fall back to DFS")
    }
    fn matches(&self, filter: &Filter) -> Result<bool, CtlError> {
        if self.item.broken {
            Err(CtlError::internal("property unavailable"))
        } else {
            self.item.matches(filter)
        }
    }
    fn evidence(&self) -> Result<serde_json::Value, CtlError> {
        self.item.evidence()
    }
}
fn provider(hits: Vec<Fake>) -> Provider {
    Provider {
        hits,
        item: node("root", "Window", vec![]),
        error: None,
    }
}

#[test]
fn complete_candidates_are_rechecked_and_duplicates_rejected() {
    let q = query(r#"{"match":{"id":"same","role":"Button"}}"#);
    let p = provider(vec![
        node("same", "Pane", vec![]),
        node("same", "Button", vec![]),
    ]);
    assert_eq!(resolve(&p, &q).unwrap().item.role, "Button");
    let p = provider(vec![node("same", "Button", vec![]); 2]);
    assert_eq!(resolve(&p, &q).err().unwrap().code, ErrorCode::Ambiguous);
    assert_eq!(
        resolve(&provider(vec![]), &q).err().unwrap().code,
        ErrorCode::NotFound
    );
}

#[test]
fn provider_and_late_property_failures_cannot_prove_uniqueness() {
    let q = query(r#"{"match":{"id":"same"}}"#);
    for code in [
        ErrorCode::Internal,
        ErrorCode::StaleRef,
        ErrorCode::PermDenied,
    ] {
        let mut p = provider(vec![node("same", "Button", vec![])]);
        p.error = Some(code);
        assert_eq!(resolve(&p, &q).err().unwrap().code, code);
    }
    let mut bad = node("same", "Button", vec![]);
    bad.broken = true;
    let p = provider(vec![node("same", "Button", vec![]), bad]);
    assert_eq!(resolve(&p, &q).err().unwrap().code, ErrorCode::Internal);
    let p = provider(vec![
        node("same", "Button", vec![]);
        crate::MAX_ELEMENTS + 1
    ]);
    let error = resolve(&p, &q).err().unwrap();
    assert_eq!(error.code, ErrorCode::NotActionable);
    assert_eq!(
        serde_json::to_value(actl_core::ErrorEnvelope::new("find", &error, 0)).unwrap(),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../tests/golden/selector_candidate_budget.json"
        ))
        .unwrap()
    );
}
