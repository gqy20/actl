//! 快照语义:UiNode、ref 编号引擎、skeleton 投影、snapshot_id 生成。
//!
//! 纯逻辑(平台无关):actl-uia 产出 DFS 序的节点流,本模块负责产品语义——
//! 哪些元素可交互(06 §3.1)、ref 如何编号(06 §5)、skeleton 如何折叠(06 §3.2 L2)。
//! spike 结论固化:可交互判定不依赖名称(docs/spike-findings.md #5)。

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// DFS 序节点(UIA 后端产出)。`parent` 为同流内的索引,不参与序列化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiNode {
    pub depth: u32,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    #[serde(skip)]
    pub parent: Option<usize>,
}

/// 快照输出元素:可交互元素携带 ref(serde 字段名 `ref`,与 06 §4 一致)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementOut {
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub ref_id: Option<String>,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    pub depth: u32,
}

/// 可交互角色表(06 §3.1 命名规则;全集变更属 spec 提交)。
/// 注意:判定只看 role,不看名称是否为空(spike #5)。
pub const INTERACTIVE_ROLES: &[&str] = &[
    "Button",
    "CheckBox",
    "RadioButton",
    "ComboBox",
    "Edit",
    "ListItem",
    "MenuItem",
    "TabItem",
    "Hyperlink",
    "ToggleButton",
    "DataGrid",
    "Spinner",
    "Slider",
    "Document",
    "Tree",
    "TreeItem",
    "Table",
    "Calendar",
    "Menu",
];

pub fn is_interactive_role(role: &str) -> bool {
    INTERACTIVE_ROLES.contains(&role)
}

/// 进程内唯一的短 id(s 前缀 + 时间戳十六进制 + 原子计数)。
pub fn new_snapshot_id() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("s{nanos:x}{n:x}")
}

/// 快照构建器:吃 DFS 序节点流,产出带 ref 的输出。
///
/// - ref 只分配给可交互元素,编号从 @e1 起,按 DFS 序(06 §5);
/// - `max_elements` 超限时截断并标记 `truncated`(防失控 UI);
/// - skeleton 模式仅保留可交互元素及其祖先链(06 §3.2 L2,Chromium 71% 空名容器的解药)。
pub struct SnapshotBuilder {
    nodes: Vec<UiNode>,
    max_elements: usize,
    truncated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotOutput {
    pub snapshot_id: String,
    pub truncated: bool,
    pub elements: Vec<ElementOut>,
}

impl SnapshotBuilder {
    pub fn new(max_elements: usize) -> Self {
        Self {
            nodes: Vec::new(),
            max_elements,
            truncated: false,
        }
    }

    /// 压入一个 DFS 序节点;返回 false 表示已达上限(调用方应停止遍历)。
    pub fn push(&mut self, node: UiNode) -> bool {
        if self.nodes.len() >= self.max_elements {
            self.truncated = true;
            return false;
        }
        self.nodes.push(node);
        true
    }

    pub fn finish(self, skeleton: bool) -> SnapshotOutput {
        let keep = if skeleton {
            self.skeleton_keep_set()
        } else {
            self.all_keep()
        };
        let mut next_ref = 0;
        let elements = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| keep[*i])
            .map(|(i, n)| {
                let ref_id = if is_interactive_role(&n.role) {
                    next_ref += 1;
                    Some(format!("@e{next_ref}"))
                } else {
                    None
                };
                let _ = i;
                ElementOut {
                    ref_id,
                    role: n.role.clone(),
                    name: n.name.clone(),
                    automation_id: n.automation_id.clone(),
                    depth: n.depth,
                }
            })
            .collect();
        SnapshotOutput {
            snapshot_id: new_snapshot_id(),
            truncated: self.truncated,
            elements,
        }
    }

    fn all_keep(&self) -> Vec<bool> {
        vec![true; self.nodes.len()]
    }

    /// skeleton 保留集:可交互元素 ∪ 其祖先链。
    fn skeleton_keep_set(&self) -> Vec<bool> {
        let mut keep = vec![false; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            if is_interactive_role(&n.role) {
                keep[i] = true;
                let mut p = n.parent;
                while let Some(pi) = p {
                    if keep[pi] {
                        break; // 祖先链已标记,剪枝
                    }
                    keep[pi] = true;
                    p = self.nodes[pi].parent;
                }
            }
        }
        keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(depth: u32, role: &str, parent: Option<usize>) -> UiNode {
        UiNode {
            depth,
            role: role.into(),
            name: None,
            automation_id: None,
            parent,
        }
    }

    #[test]
    fn refs_are_sequential_and_interactive_only() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Pane", Some(0)));
        b.push(node(2, "Button", Some(1)));
        b.push(node(2, "Text", Some(1)));
        b.push(node(2, "Edit", Some(1)));
        let out = b.finish(false);
        let refs: Vec<&str> = out
            .elements
            .iter()
            .filter_map(|e| e.ref_id.as_deref())
            .collect();
        assert_eq!(refs, vec!["@e1", "@e2"]); // Button、Edit 依次编号,Window/Pane/Text 无 ref
    }

    #[test]
    fn skeleton_keeps_interactives_and_ancestors_only() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Pane", Some(0))); // 祖先链:保留
        b.push(node(2, "Pane", Some(1))); // 空容器:折叠
        b.push(node(3, "Text", Some(2))); // 空文本:折叠
        b.push(node(2, "Button", Some(1)));
        b.push(node(1, "Text", Some(0))); // 无交互子孙的分支:折叠
        let out = b.finish(true);
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        assert_eq!(roles, vec!["Window", "Pane", "Button"]);
    }

    #[test]
    fn truncation_is_flagged() {
        let mut b = SnapshotBuilder::new(2);
        assert!(b.push(node(0, "Window", None)));
        assert!(b.push(node(1, "Button", Some(0))));
        assert!(!b.push(node(1, "Button", Some(0))));
        let out = b.finish(false);
        assert!(out.truncated);
        assert_eq!(out.elements.len(), 2);
    }

    #[test]
    fn snapshot_ids_are_unique_within_process() {
        let a = new_snapshot_id();
        let b = new_snapshot_id();
        assert_ne!(a, b);
        assert!(a.starts_with('s'));
    }

    #[test]
    fn chinese_names_survive_round_trip() {
        let n = UiNode {
            depth: 2,
            role: "Button".into(),
            name: Some("确定".into()),
            automation_id: Some("OK".into()),
            parent: None,
        };
        let mut b = SnapshotBuilder::new(10);
        b.push(n);
        let out = b.finish(false);
        let json = serde_json::to_value(&out.elements[0]).unwrap();
        assert_eq!(json["name"], "确定");
        assert_eq!(json["ref"], "@e1");
    }
}
