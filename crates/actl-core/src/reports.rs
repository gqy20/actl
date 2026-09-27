//! Caller-authored claims and explicit evidence references, never an automatic truth verdict.
use crate::{
    CtlError,
    history::{FeedbackSource, TaskReport, valid_id},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StatementKind {
    Fact,
    Inference,
    UserFeedback,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    LocalFile,
    ExternalReference,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub id: String,
    pub kind: EvidenceKind,
    pub reference: String,
}

pub fn validate(report: &TaskReport) -> Result<(), CtlError> {
    let mut ids = std::collections::HashSet::new();
    for artifact in &report.artifacts {
        if !valid_id(&artifact.id)
            || !ids.insert(&artifact.id)
            || artifact.reference.is_empty()
            || artifact.reference.len() > 4096
        {
            return Err(CtlError::protocol(
                "artifact requires unique ID and bounded reference",
            ));
        }
        if artifact.kind == EvidenceKind::LocalFile
            && (!Path::new(&artifact.reference).is_absolute()
                || artifact.reference.starts_with("\\\\")
                || artifact.reference.starts_with("//"))
        {
            return Err(CtlError::protocol(
                "local evidence requires an absolute non-UNC path; use external_reference otherwise",
            ));
        }
    }
    for item in &report.feedback {
        if item.evidence_ids.iter().any(|id| !ids.contains(id)) {
            return Err(CtlError::protocol(
                "feedback references unknown artifact ID",
            ));
        }
        match item.kind {
            Some(StatementKind::Fact) if !item.verified || item.evidence_ids.is_empty() => {
                return Err(CtlError::protocol(
                    "facts require caller verification and evidence IDs",
                ));
            }
            Some(StatementKind::Inference) if item.verified => {
                return Err(CtlError::protocol("inference cannot be marked verified"));
            }
            Some(StatementKind::UserFeedback) if !matches!(item.source, FeedbackSource::User) => {
                return Err(CtlError::protocol("user_feedback requires source=user"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn availability(artifact: &EvidenceRef) -> &'static str {
    if artifact.kind == EvidenceKind::ExternalReference {
        return "unchecked";
    }
    match std::fs::metadata(&artifact.reference) {
        Ok(m) if m.is_file() => {
            if std::fs::File::open(&artifact.reference).is_ok() {
                "available"
            } else {
                "unreadable"
            }
        }
        Ok(_) => "not_a_file",
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "missing",
        Err(_) => "unreadable",
    }
}
pub fn read(root: &Path, task: &str, limit: usize) -> Result<Value, CtlError> {
    let dir = root.join("tasks");
    if !dir.exists() {
        return Ok(json!([]));
    }
    let mut files = vec![];
    for entry in std::fs::read_dir(dir).map_err(|e| CtlError::internal(e.to_string()))? {
        let entry = entry.map_err(|e| CtlError::internal(e.to_string()))?;
        if entry.file_type().is_ok_and(|t| t.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{task}-"))
        {
            files.push((
                entry.metadata().and_then(|m| m.modified()).ok(),
                entry.path(),
            ));
        }
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    let mut result = vec![];
    for (_, path) in files {
        if result.len() == limit {
            break;
        }
        let mut bytes = vec![];
        std::fs::File::open(&path)
            .and_then(|f| f.take(131073).read_to_end(&mut bytes))
            .map_err(|e| CtlError::internal(e.to_string()))?;
        if bytes.len() > 131072 {
            return Err(CtlError::protocol("archived report exceeds read limit"));
        }
        let mut doc: Value =
            serde_json::from_slice(&bytes).map_err(|e| CtlError::protocol(e.to_string()))?;
        if doc["report"]["task_id"].as_str() != Some(task) {
            continue;
        }
        let report: TaskReport = serde_json::from_value(doc["report"].clone())
            .map_err(|e| CtlError::protocol(e.to_string()))?;
        validate(&report)?;
        doc["evidence_status"] = json!(report.artifacts.iter().map(|a| json!({"id":a.id,"availability":availability(a),"checked_ms":crate::state::unix_ms()})).collect::<Vec<_>>());
        doc["archive_path"] = json!(path);
        result.push(doc);
    }
    Ok(json!(result))
}
