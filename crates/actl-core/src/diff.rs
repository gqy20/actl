//! 快照差分(探索模式 S1,docs/42 §5/§9):两份快照元素的多级身份配对差分。
//!
//! 配对层级(前者优先,同级组内按出现顺序配对,重复键容忍——虚拟化列表的常态):
//! 1. `automation_id`(非空)+ role——最强身份;
//! 2. `name`(非空,大小写不敏感)+ role——动态 id 变化后的回退
//!    (automation_id 变了但名字没变 → 配对成功并记 identity_changed);
//! 3. 未配对余部 → added / removed。
//!
//! 不变量(验收硬指标):old 侧 = paired + removed,new 侧 = paired + added,
//! 每个元素恰好归入一类——"差分结果与全量比对一致"由构造保证,单测覆盖
//! 虚拟化重排、动态 id、重复键、大小写漂移四类场景。

use crate::snapshot::ElementOut;
use serde::Serialize;
use std::collections::BTreeMap;

/// 差分中一个元素的精简载荷(不带 ref——跨快照 ref 不可比)。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiffEntry {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    pub depth: u32,
}

impl DiffEntry {
    fn of(e: &ElementOut) -> Self {
        Self {
            role: e.role.clone(),
            name: e.name.clone(),
            automation_id: e.automation_id.clone(),
            depth: e.depth,
        }
    }
}

/// 配对成功且语义字段有变化的记录。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangedEntry {
    /// 配对所用层级:automation_id_role | name_role
    pub paired_by: &'static str,
    pub role: String,
    pub name_from: Option<String>,
    pub name_to: Option<String>,
    pub automation_id_from: Option<String>,
    pub automation_id_to: Option<String>,
}

/// 配对统计(报告与验收观测用)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PairingStats {
    pub paired_by_automation_id_role: usize,
    pub paired_by_name_role: usize,
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotDiff {
    pub added: Vec<DiffEntry>,
    pub removed: Vec<DiffEntry>,
    pub changed: Vec<ChangedEntry>,
    pub pairing: PairingStats,
}

/// L1 配对键:非空 automation_id + role。
fn key_automation(e: &ElementOut) -> Option<(String, String)> {
    e.automation_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|id| (id.to_string(), e.role.clone()))
}

/// L2 配对键:非空 name(小写)+ role。
fn key_name(e: &ElementOut) -> Option<(String, String)> {
    e.name
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|n| (n.to_lowercase(), e.role.clone()))
}

/// 单级配对:old_idx/new_idx 按键分组,组内按出现顺序取 min(len) 配对;
/// 返回 (配对列表, 未配对的 old 索引, 未配对的 new 索引)。
fn pair_level(
    old: &[ElementOut],
    new: &[ElementOut],
    key_of: fn(&ElementOut) -> Option<(String, String)>,
) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
    let mut old_groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, e) in old.iter().enumerate() {
        if let Some(k) = key_of(e) {
            old_groups.entry(k).or_default().push(i);
        }
    }
    let mut new_groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (j, e) in new.iter().enumerate() {
        if let Some(k) = key_of(e) {
            new_groups.entry(k).or_default().push(j);
        }
    }
    let mut paired = Vec::new();
    let mut paired_old = vec![false; old.len()];
    let mut paired_new = vec![false; new.len()];
    for (k, oi) in &old_groups {
        if let Some(nj) = new_groups.get(k) {
            for (&a, &b) in oi.iter().zip(nj.iter()) {
                paired.push((a, b));
                paired_old[a] = true;
                paired_new[b] = true;
            }
        }
    }
    let rest_old = (0..old.len()).filter(|i| !paired_old[*i]).collect();
    let rest_new = (0..new.len()).filter(|j| !paired_new[*j]).collect();
    (paired, rest_old, rest_new)
}

/// 两份快照元素的差分。输入应为同一投影口径的输出(不同口径的差分无意义)。
pub fn diff(old: &[ElementOut], new: &[ElementOut]) -> SnapshotDiff {
    // L1: automation_id + role
    let (pairs_l1, rest_old, rest_new) = pair_level(old, new, key_automation);
    // L2: name(lowercase) + role,只在 L1 未配对余部上做
    let old_rest: Vec<ElementOut> = rest_old.iter().map(|&i| old[i].clone()).collect();
    let new_rest: Vec<ElementOut> = rest_new.iter().map(|&j| new[j].clone()).collect();
    let (pairs_l2_rest, _, _) = pair_level(&old_rest, &new_rest, key_name);
    let pairs_l2: Vec<(usize, usize)> = pairs_l2_rest
        .into_iter()
        .map(|(a, b)| (rest_old[a], rest_new[b]))
        .collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut paired_old = vec![false; old.len()];
    let mut paired_new = vec![false; new.len()];

    for &(a, b) in &pairs_l1 {
        paired_old[a] = true;
        paired_new[b] = true;
        let (o, n) = (&old[a], &new[b]);
        // L1 键含 automation_id+role,可变字段只有 name
        if o.name != n.name {
            changed.push(ChangedEntry {
                paired_by: "automation_id_role",
                role: n.role.clone(),
                name_from: o.name.clone(),
                name_to: n.name.clone(),
                automation_id_from: o.automation_id.clone(),
                automation_id_to: n.automation_id.clone(),
            });
        }
    }
    for &(a, b) in &pairs_l2 {
        paired_old[a] = true;
        paired_new[b] = true;
        let (o, n) = (&old[a], &new[b]);
        if o.name != n.name || o.automation_id != n.automation_id {
            changed.push(ChangedEntry {
                paired_by: "name_role",
                role: n.role.clone(),
                name_from: o.name.clone(),
                name_to: n.name.clone(),
                automation_id_from: o.automation_id.clone(),
                automation_id_to: n.automation_id.clone(),
            });
        }
    }
    for (i, e) in old.iter().enumerate() {
        if !paired_old[i] {
            removed.push(DiffEntry::of(e));
        }
    }
    for (j, e) in new.iter().enumerate() {
        if !paired_new[j] {
            added.push(DiffEntry::of(e));
        }
    }

    let stats = PairingStats {
        paired_by_automation_id_role: pairs_l1.len(),
        paired_by_name_role: pairs_l2.len(),
        added: added.len(),
        removed: removed.len(),
        changed: changed.len(),
    };
    SnapshotDiff {
        added,
        removed,
        changed,
        pairing: stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(role: &str, name: Option<&str>, id: Option<&str>, depth: u32) -> ElementOut {
        ElementOut {
            ref_id: None,
            role: role.to_string(),
            name: name.map(str::to_string),
            automation_id: id.map(str::to_string),
            depth,
        }
    }

    #[test]
    fn partition_invariant_holds() {
        let old = vec![
            el("Button", Some("OK"), Some("btnOk"), 2),
            el("Edit", Some("query"), Some("q"), 3),
            el("Text", Some("gone"), None, 3),
        ];
        let new = vec![
            el("Button", Some("确定"), Some("btnOk"), 2), // name 变化
            el("Edit", Some("query"), Some("q"), 3),      // 不变
            el("Button", Some("fresh"), Some("new1"), 3), // 新增
        ];
        let d = diff(&old, &new);
        // 不变量:old = paired + removed;new = paired + added
        let paired = d.pairing.paired_by_automation_id_role + d.pairing.paired_by_name_role;
        assert_eq!(paired + d.removed.len(), old.len());
        assert_eq!(paired + d.added.len(), new.len());
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].paired_by, "automation_id_role");
        assert_eq!(d.changed[0].name_from.as_deref(), Some("OK"));
    }

    #[test]
    fn dynamic_id_falls_back_to_name_role() {
        let old = vec![el("Button", Some("提交"), Some("m_33"), 2)];
        let new = vec![el("Button", Some("提交"), Some("m_87"), 2)];
        let d = diff(&old, &new);
        assert_eq!(d.pairing.paired_by_name_role, 1);
        assert_eq!(d.pairing.paired_by_automation_id_role, 0);
        assert_eq!(d.added.len() + d.removed.len(), 0);
        assert_eq!(d.changed.len(), 1); // automation_id 变化如实上报
        assert_eq!(d.changed[0].automation_id_from.as_deref(), Some("m_33"));
    }

    #[test]
    fn duplicate_keys_pair_in_order_and_remainder_reports() {
        // 虚拟化列表:重复 (name,role),数量变化只报余部
        let old = vec![
            el("ListItem", Some("item"), None, 4),
            el("ListItem", Some("item"), None, 4),
            el("ListItem", Some("item"), None, 4),
        ];
        let new = vec![
            el("ListItem", Some("item"), None, 4),
            el("ListItem", Some("item"), None, 4),
        ];
        let d = diff(&old, &new);
        assert_eq!(d.pairing.paired_by_name_role, 2);
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.added.len(), 0);
    }

    #[test]
    fn case_drift_pairs_and_reports_exact_names() {
        let old = vec![el("Button", Some("Submit"), None, 2)];
        let new = vec![el("Button", Some("SUBMIT"), None, 2)];
        let d = diff(&old, &new);
        assert_eq!(d.pairing.paired_by_name_role, 1);
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].name_from.as_deref(), Some("Submit"));
        assert_eq!(d.changed[0].name_to.as_deref(), Some("SUBMIT"));
    }

    #[test]
    fn automation_id_wins_over_name_level() {
        // 同 name+role 也有 automation_id:必须 L1 命中(不落 L2)
        let old = vec![el("Button", Some("go"), Some("id1"), 1)];
        let new = vec![el("Button", Some("stop"), Some("id1"), 1)];
        let d = diff(&old, &new);
        assert_eq!(d.pairing.paired_by_automation_id_role, 1);
        assert_eq!(d.pairing.paired_by_name_role, 0);
        assert_eq!(d.changed.len(), 1);
    }

    #[test]
    fn empty_names_and_ids_never_pair() {
        let old = vec![el("Text", None, None, 3), el("Text", Some(""), None, 3)];
        let new = vec![el("Text", None, None, 3)];
        let d = diff(&old, &new);
        assert_eq!(
            d.pairing.paired_by_automation_id_role + d.pairing.paired_by_name_role,
            0
        );
        assert_eq!(d.removed.len(), 2);
        assert_eq!(d.added.len(), 1);
    }
}
