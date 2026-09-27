//! 快照语义:UiNode、ref 编号引擎、skeleton 投影、snapshot_id 生成。
//!
//! 纯逻辑(平台无关):actl-uia 产出 DFS 序的节点流,本模块负责产品语义——
//! 哪些元素可交互(06 §3.1)、ref 如何编号(06 §5)、skeleton 如何折叠(06 §3.2 L2)。
//! spike 结论固化:可交互判定不依赖名称(docs/spike-findings.md #5)。

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{CtlError, ErrorCode};

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

/// 快照投影模式(06 §3.2 L2 / §3.4)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// 全量树。
    Full,
    /// 仅可交互元素 + 祖先链 + 单链折叠(Chromium 71% 空名容器的解药)。
    Skeleton,
    /// 内容域:保留 Document 子树(网页/文档正文)与根→Document 路径,
    /// 应用 chrome(工具栏/ribbon)整枝丢弃。`skeleton: true` 时组合——
    /// 折叠只作用于 Document 子树**内部**,chrome 交互元素不参与保留判定
    /// (浏览器形态 skeleton 仅削 ~36% 的根因修复)。
    Content { skeleton: bool },
}

/// 快照构建器:吃 DFS 序节点流,产出带 ref 的输出。
///
/// - ref 只分配给可交互元素,编号从 @e1 起,按 DFS 序(06 §5);
/// - `max_elements` 超限时截断并标记 `truncated`(防失控 UI);
/// - 投影模式见 [`Projection`](06 §3.2 L2);Content 模式下窗口无任何
///   Document → `NOT_FOUND`(内容域不存在,如实报而非静默回退全量)。
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

    pub fn finish(self, view: Projection) -> Result<SnapshotOutput, CtlError> {
        let keep = match view {
            Projection::Full => self.all_keep(),
            Projection::Skeleton => self.skeleton_keep_set(),
            Projection::Content { skeleton } => {
                if !self.nodes.iter().any(|n| n.role == "Document") {
                    return Err(CtlError::new(
                        ErrorCode::NotFound,
                        "no Document (content) region in this window - retry without --view content",
                    ));
                }
                self.content_keep_set(skeleton)
            }
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
        Ok(SnapshotOutput {
            snapshot_id: new_snapshot_id(),
            truncated: self.truncated,
            elements,
        })
    }

    fn all_keep(&self) -> Vec<bool> {
        vec![true; self.nodes.len()]
    }

    /// skeleton 保留集:可交互元素 ∪ 其祖先链,再**单链折叠**——非交互保留节点
    /// 若只剩一个保留子则自身折叠(Chromium/Electron 的深度单子容器是削减大头;
    /// depth 跳变即折叠信号,窗口根豁免以保留窗口身份)。ref 只编号交互元素,
    /// DFS 序不变 → 折叠不影响 @eN 编号(与全量快照互通)。
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
        self.collapse_single_chains(&mut keep);
        keep
    }

    /// content 保留集:in_doc(Document 子树)∪ 其祖先链(根→Document 路径)。
    /// DFS 先序保证 parent 索引恒小于子,一遍即成。组合模式(skeleton=true)
    /// 交互种子只取 in_doc 节点——chrome 里的按钮/地址栏不再强制保留,
    /// 这是浏览器形态削减率的主增量。
    fn content_keep_set(&self, skeleton: bool) -> Vec<bool> {
        let mut in_doc = vec![false; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            in_doc[i] = n.role == "Document" || n.parent.is_some_and(|p| in_doc[p]);
        }
        let mut keep = vec![false; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            if in_doc[i] && (!skeleton || is_interactive_role(&n.role)) {
                keep[i] = true;
                let mut p = n.parent;
                while let Some(pi) = p {
                    if keep[pi] {
                        break;
                    }
                    keep[pi] = true;
                    p = self.nodes[pi].parent;
                }
            }
        }
        if skeleton {
            self.collapse_single_chains(&mut keep);
        }
        keep
    }

    /// 单链折叠:kept_children[i] = i 的保留子数;倒序(深→浅)一遍收敛——
    /// 处理父时子的去留已定。窗口根(i=0)豁免,保留窗口身份。
    fn collapse_single_chains(&self, keep: &mut [bool]) {
        let mut kept_children = vec![0usize; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            if keep[i]
                && let Some(pi) = n.parent
            {
                kept_children[pi] += 1;
            }
        }
        for i in (0..self.nodes.len()).rev() {
            if i > 0
                && keep[i]
                && !is_interactive_role(&self.nodes[i].role)
                && kept_children[i] == 1
            {
                keep[i] = false;
                if let Some(pi) = self.nodes[i].parent {
                    kept_children[pi] -= 1;
                }
            }
        }
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
        let out = b.finish(Projection::Full).expect("full");
        let refs: Vec<&str> = out
            .elements
            .iter()
            .filter_map(|e| e.ref_id.as_deref())
            .collect();
        assert_eq!(refs, vec!["@e1", "@e2"]); // Button、Edit 依次编号,Window/Pane/Text 无 ref
    }

    #[test]
    fn skeleton_keeps_interactives_and_collapses_single_chains() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Pane", Some(0))); // 单保留子链上的祖先:折叠
        b.push(node(2, "Pane", Some(1))); // 同上:折叠
        b.push(node(3, "Text", Some(2))); // 空文本:折叠
        b.push(node(2, "Button", Some(1))); // 交互:保留(2/1 折叠后由 depth 跳变表达)
        b.push(node(1, "Text", Some(0))); // 无交互子孙的分支:折叠
        let out = b.finish(Projection::Skeleton).expect("skeleton");
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        // 根豁免 + 交互元素;单链祖先全部折叠
        assert_eq!(roles, vec!["Window", "Button"]);
    }

    #[test]
    fn skeleton_keeps_shared_ancestors_with_multiple_kept_children() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Pane", Some(0))); // 两个保留子(Button+Edit):不折叠
        b.push(node(2, "Button", Some(1)));
        b.push(node(2, "Edit", Some(1)));
        let out = b.finish(Projection::Skeleton).expect("skeleton");
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        assert_eq!(roles, vec!["Window", "Pane", "Button", "Edit"]);
    }

    /// 浏览器形态的最小树:Window → [ToolBar(按钮×2) + Pane → Document → 链接×2]。
    /// content 投影须整枝丢 ToolBar、保 Document 子树与根→Document 路径。
    #[test]
    fn content_view_drops_chrome_and_keeps_document_subtree() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "ToolBar", Some(0)));
        b.push(node(2, "Button", Some(1))); // chrome 交互元素
        b.push(node(2, "Button", Some(1)));
        b.push(node(1, "Pane", Some(0)));
        b.push(node(2, "Document", Some(4)));
        b.push(node(3, "Hyperlink", Some(5)));
        b.push(node(3, "Text", Some(5)));
        b.push(node(3, "Hyperlink", Some(5)));
        let out = b
            .finish(Projection::Content { skeleton: false })
            .expect("content");
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        assert_eq!(
            roles,
            vec![
                "Window",
                "Pane",
                "Document",
                "Hyperlink",
                "Text",
                "Hyperlink"
            ]
        );
        // ref 只落在 Document 子树内
        let refs: Vec<&str> = out
            .elements
            .iter()
            .filter_map(|e| e.ref_id.as_deref())
            .collect();
        assert_eq!(refs, vec!["@e1", "@e2", "@e3"]);
    }

    /// 组合模式:chrome 交互元素不参与保留判定(纯 skeleton 削不动的根因),
    /// Document 子树内部按 skeleton 折叠(Text 无交互子孙 → 丢)。
    #[test]
    fn content_view_combined_with_skeleton_ignores_chrome_interactives() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "ToolBar", Some(0)));
        b.push(node(2, "Button", Some(1))); // chrome 按钮:skeleton 会保,content+skeleton 必丢
        b.push(node(1, "Pane", Some(0)));
        b.push(node(2, "Document", Some(3)));
        b.push(node(3, "Hyperlink", Some(4)));
        b.push(node(3, "Text", Some(4))); // 文档内非交互:组合模式折叠
        let out = b
            .finish(Projection::Content { skeleton: true })
            .expect("content+skeleton");
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        assert_eq!(roles, vec!["Window", "Document", "Hyperlink"]);
    }

    /// 无 Document(计算器类纯控件应用):NOT_FOUND,不静默回退全量。
    #[test]
    fn content_view_without_document_is_not_found() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Button", Some(0)));
        let err = b
            .finish(Projection::Content { skeleton: false })
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    /// 嵌套 Document(iframe):内层 Document 仍在内容域内。
    #[test]
    fn content_view_keeps_nested_documents() {
        let mut b = SnapshotBuilder::new(100);
        b.push(node(0, "Window", None));
        b.push(node(1, "Document", Some(0)));
        b.push(node(2, "Pane", Some(1)));
        b.push(node(3, "Document", Some(2))); // iframe
        b.push(node(4, "Hyperlink", Some(3)));
        let out = b
            .finish(Projection::Content { skeleton: false })
            .expect("content");
        let roles: Vec<&str> = out.elements.iter().map(|e| e.role.as_ref()).collect();
        assert_eq!(
            roles,
            vec!["Window", "Document", "Pane", "Document", "Hyperlink"]
        );
    }

    #[test]
    fn truncation_is_flagged() {
        let mut b = SnapshotBuilder::new(2);
        assert!(b.push(node(0, "Window", None)));
        assert!(b.push(node(1, "Button", Some(0))));
        assert!(!b.push(node(1, "Button", Some(0))));
        let out = b.finish(Projection::Full).expect("full");
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
        let out = b.finish(Projection::Full).expect("full");
        let json = serde_json::to_value(&out.elements[0]).unwrap();
        assert_eq!(json["name"], "确定");
        assert_eq!(json["ref"], "@e1");
    }
}
