//! 探索模式决策协议(docs/42 §5/§9.2):候选评分、熵门、词法闸、状态指纹、
//! 决策卡构建与应答解析。纯逻辑、确定性(整数评分、稳定排序、截断阶梯)——
//! 4 KiB/卡与 Top-K≤8 是协议硬上限,由构建器保证,不是调用方约定。
//!
//! token 纪律(docs/42 §3"前提三件真做"):卡自包含但静态头极小(任务一句话/
//! 应用锚/目标描述),候选只带判别性字段;全量证据落盘由引擎负责,卡内不携带。

use crate::diff::SnapshotDiff;
use crate::snapshot::{ElementOut, is_interactive_role};
use crate::{CtlError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 决策卡序列化硬上限(docs/42 §9.2 delta 2)。
pub const CARD_MAX_BYTES: usize = 4096;
/// 卡内候选上限(Top-K 截断;docs/42 §9.2)。
pub const CARD_MAX_CANDIDATES: usize = 8;
/// 熵门领先裕度:头部候选领先次名不足此值即视为平局,外抛决策卡。
pub const GATE_MARGIN: u32 = 2;

/// 词法闸单向门(docs/42 §4.3):命中词即视为不可逆动作,永不自动执行,
/// 只能进决策卡由应答者裁决。子串匹配、大小写不敏感;过度匹配是安全方向
/// (多问一次,不会误执行)。
pub const DANGEROUS_PATTERNS: &[&str] = &[
    "delete",
    "remove",
    "uninstall",
    "send",
    "pay",
    "purchase",
    "format",
    "删除",
    "移除",
    "卸载",
    "发送",
    "支付",
    "付款",
    "购买",
    "格式化",
    "清空",
    "注销",
];

/// 词法闸判定:name/automation_id 任一命中危险词(ASCII 大小写不敏感)。
pub fn dangerous(name: Option<&str>, automation_id: Option<&str>) -> bool {
    let hit = |text: Option<&str>| {
        text.is_some_and(|t| {
            let lowered = t.to_lowercase();
            DANGEROUS_PATTERNS
                .iter()
                .any(|p| lowered.contains(&p.to_lowercase()))
        })
    };
    hit(name) || hit(automation_id)
}

/// 候选的稳定目标选择器:automation_id 优先,回退 name;两者皆空的交互元素
/// 无法用语义选择器定位,不进入前沿(坐标兜底属 --physical 范畴,探索模式不启用)。
pub fn selector_of(element: &ElementOut) -> Option<String> {
    if !is_interactive_role(&element.role) {
        return None;
    }
    if let Some(id) = element.automation_id.as_deref().filter(|s| !s.is_empty()) {
        return Some(format!("id:{id}"));
    }
    element
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|n| format!("name:{n}"))
}

/// 角色先验:可点击导航类高于容器/输入类。Edit 不参与(输入需要文本内容,
/// 探索 v1 只做导航动作);Document/DataGrid 等容器为零分。
fn role_prior(role: &str) -> u32 {
    match role {
        "Button" | "MenuItem" | "Hyperlink" | "ListItem" => 2,
        "TabItem" | "TreeItem" | "CheckBox" | "RadioButton" | "ToggleButton" | "ComboBox" => 1,
        _ => 0,
    }
}

/// 确定性候选评分(整数):词表命中强度 × 身份稳定性 × 角色先验 × 新鲜度。
///
/// - name 关键词命中 +3/词(≤2 词封顶),automation_id 命中 +2/词(≤2 封顶)
///   ——词表来自任务编译,命中即任务相关性;
/// - automation_id 非空 +1(稳定身份,动作可回溯);
/// - 本帧新增元素 +2(新鲜 affordance 优先,避免在旧元素上打转);
/// - 角色先验 0..=2。
pub fn score_candidate(element: &ElementOut, keywords: &[String], added_this_frame: bool) -> u32 {
    let mut score = role_prior(&element.role);
    if element
        .automation_id
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        score += 1;
    }
    let name = element.name.as_deref().unwrap_or_default().to_lowercase();
    let id = element
        .automation_id
        .as_deref()
        .unwrap_or_default()
        .to_lowercase();
    let mut name_hits = 0;
    let mut id_hits = 0;
    for keyword in keywords {
        let k = keyword.to_lowercase();
        if !k.is_empty() {
            name_hits += name.contains(&k) as u32;
            id_hits += id.contains(&k) as u32;
        }
    }
    score += 3 * name_hits.min(2) + 2 * id_hits.min(2);
    if added_this_frame {
        score += 2;
    }
    score
}

/// 观察帧中通过评分产生的前沿候选。
#[derive(Debug, Clone, PartialEq)]
pub struct ScoredCandidate {
    pub selector: String,
    pub role: String,
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub score: u32,
    pub dangerous: bool,
}

/// 从观察帧构建前沿候选:交互元素 + 可定位选择器,按 (分数降序, 选择器字典序)
/// 稳定排序——同分不随机,卡内容与回退序完全确定。
///
/// `priors`(docs/42 §4.5 统计剪枝的 v1 形态):选择器 → 分数调整,由调用方从
/// pack quirks/历史 tried 效果注入(writeback 的 suggested_priors 即其数据源);
/// 负值压低已知有风险/无效果的候选,正值提高已知有效候选。调整后分数下限 0。
pub fn frontier(
    elements: &[ElementOut],
    keywords: &[String],
    added_selectors: &std::collections::BTreeSet<String>,
    priors: &std::collections::BTreeMap<String, i32>,
) -> Vec<ScoredCandidate> {
    let mut candidates: Vec<ScoredCandidate> = elements
        .iter()
        .filter_map(|e| {
            let selector = selector_of(e)?;
            let added_this_frame = added_selectors.contains(&selector);
            let base = score_candidate(e, keywords, added_this_frame);
            let adjusted = (i64::from(base)
                + i64::from(priors.get(&selector).copied().unwrap_or(0)))
            .clamp(0, i64::from(u32::MAX)) as u32;
            Some(ScoredCandidate {
                dangerous: dangerous(e.name.as_deref(), e.automation_id.as_deref()),
                selector,
                role: e.role.clone(),
                name: e.name.clone(),
                automation_id: e.automation_id.clone(),
                score: adjusted,
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.selector.cmp(&b.selector))
    });
    candidates
}

/// 熵门(docs/42 §3 降决策次数):头部非危险候选领先次名 ≥ GATE_MARGIN 时
/// 自动执行,否则外抛决策卡。危险候选(词法闸)永不自动执行——全前沿皆危险
/// 时同样外抛,由应答者裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome {
    /// 自动执行候选(前沿内索引)。
    AutoExecute(usize),
    /// 平局/全危险/无安全候选:外抛决策卡。
    Externalize,
}

pub fn gate(candidates: &[ScoredCandidate]) -> GateOutcome {
    let mut ranked: Vec<usize> = (0..candidates.len())
        .filter(|&i| !candidates[i].dangerous)
        .collect();
    ranked.sort_by(|&a, &b| {
        candidates[b]
            .score
            .cmp(&candidates[a].score)
            .then_with(|| candidates[a].selector.cmp(&candidates[b].selector))
    });
    let Some(&top) = ranked.first() else {
        return GateOutcome::Externalize;
    };
    match ranked.get(1) {
        None => GateOutcome::AutoExecute(top),
        Some(&second) if candidates[top].score >= candidates[second].score + GATE_MARGIN => {
            GateOutcome::AutoExecute(top)
        }
        Some(_) => GateOutcome::Externalize,
    }
}

/// 观察状态指纹:可识别元素身份(automation_id 优先,回退小写 name)+ 角色多重集
/// 的 FNV-1a。已访问指纹去重用(docs/42 §3 状态指纹去重砍步数与观察体积)。
/// 身份排序后哈希——同一状态元素乱序(虚拟化重排)指纹不变。
pub fn state_fingerprint(elements: &[ElementOut]) -> String {
    let mut keys: Vec<String> = elements
        .iter()
        .map(|e| {
            let identity = e
                .automation_id
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|id| format!("i:{id}"))
                .or_else(|| {
                    e.name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|n| format!("n:{}", n.to_lowercase()))
                })
                .unwrap_or_else(|| "-".into());
            format!("{identity}|{}", e.role)
        })
        .collect();
    keys.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for key in &keys {
        for byte in key.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// 卡内候选:判别性字段的确定性压缩(名字/ID ≤64 字符)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardCandidate {
    pub id: String,
    pub selector: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    /// 该选择器历史动作的效果标注(reveals | no_effect),来自引擎学习。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    pub score: u32,
    pub dangerous: bool,
}

/// 差分摘要:计数恒在,样本名按需截断。卡不复发全树(docs/42 §3)。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardDiff {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_names: Vec<String>,
}

/// 决策卡(docs/42 §9.2 delta 2):自包含、硬上限 4 KiB、Top-K≤8。
/// 应答者对 actl 是同一种协议——人、启发式脚本或 LLM agent 都只做选择题。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionCard {
    pub version: u32,
    pub card_id: String,
    /// 任务一句话(≤160 字符)。
    pub task: String,
    /// 应用锚(≤80 字符)。
    pub app: String,
    /// 目标谓词描述(≤200 字符)。
    pub goal: String,
    /// 已执行动作步数。
    pub step: u32,
    pub diff: CardDiff,
    pub candidates: Vec<CardCandidate>,
}

/// 决策卡构建输入。
pub struct CardInput<'a> {
    pub card_id: &'a str,
    pub task: &'a str,
    pub app: &'a str,
    pub goal: &'a str,
    pub step: u32,
    pub diff: &'a SnapshotDiff,
    pub candidates: &'a [ScoredCandidate],
    /// 选择器 → 历史效果标注。
    pub effects: &'a BTreeMap<String, String>,
}

fn clip(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// 差分摘要样本:新增/消失元素名各 ≤4 条、每条 ≤32 字符(无名者用 role)。
fn diff_summary(diff: &SnapshotDiff) -> CardDiff {
    let sample = |entries: &[crate::diff::DiffEntry]| -> Vec<String> {
        entries
            .iter()
            .map(|e| {
                let text = e
                    .name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("<{}>", e.role));
                clip(&text, 32)
            })
            .take(4)
            .collect()
    };
    CardDiff {
        added: diff.added.len(),
        removed: diff.removed.len(),
        changed: diff.changed.len(),
        added_names: sample(&diff.added),
        removed_names: sample(&diff.removed),
    }
}

/// 构建决策卡。确定性压缩阶梯:超预算时依次丢弃样本名 → 缩短名字/ID →
/// 截短候选列表(Top-4 → Top-2);全部阶梯后仍超限返回 PROTOCOL
/// (防御路径:各字段上限之和理论最大 ~2.7 KiB,正常不可达)。
pub fn build_card(input: &CardInput) -> Result<DecisionCard, CtlError> {
    struct Stage {
        keep_samples: bool,
        name_chars: usize,
        top_k: usize,
    }
    let stages = [
        Stage {
            keep_samples: true,
            name_chars: 64,
            top_k: CARD_MAX_CANDIDATES,
        },
        Stage {
            keep_samples: false,
            name_chars: 64,
            top_k: CARD_MAX_CANDIDATES,
        },
        Stage {
            keep_samples: false,
            name_chars: 24,
            top_k: CARD_MAX_CANDIDATES,
        },
        Stage {
            keep_samples: false,
            name_chars: 24,
            top_k: 4,
        },
        Stage {
            keep_samples: false,
            name_chars: 24,
            top_k: 2,
        },
    ];
    for stage in &stages {
        let mut diff = diff_summary(input.diff);
        if !stage.keep_samples {
            diff.added_names.clear();
            diff.removed_names.clear();
        }
        let candidates: Vec<CardCandidate> = input
            .candidates
            .iter()
            .take(stage.top_k)
            .enumerate()
            .map(|(index, c)| CardCandidate {
                id: format!("c{}", index + 1),
                selector: clip(&c.selector, 96),
                role: clip(&c.role, 24),
                name: c.name.as_deref().map(|n| clip(n, stage.name_chars)),
                automation_id: c
                    .automation_id
                    .as_deref()
                    .map(|id| clip(id, stage.name_chars)),
                effect: input.effects.get(&c.selector).cloned(),
                score: c.score,
                dangerous: c.dangerous,
            })
            .collect();
        let card = DecisionCard {
            version: 1,
            card_id: input.card_id.to_owned(),
            task: clip(input.task, 160),
            app: clip(input.app, 80),
            goal: clip(input.goal, 200),
            step: input.step,
            diff,
            candidates,
        };
        if card_bytes(&card) <= CARD_MAX_BYTES {
            return Ok(card);
        }
    }
    Err(CtlError::new(
        ErrorCode::Protocol,
        "decision card cannot fit the 4 KiB budget",
    ))
}

/// 决策卡序列化体积(上限验收与 token 审计共用)。
pub fn card_bytes(card: &DecisionCard) -> usize {
    serde_json::to_vec(card)
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
}

/// 解析应答(docs/42 §9.2 delta 3):单个候选 id,必须属于卡内候选 id 集,
/// 否则 PROTOCOL。选择后的回退序由引擎按卡内分数序自动安排,应答协议保持
/// 单选最小面。
pub fn parse_answer(answer: &str, card: &DecisionCard) -> Result<String, CtlError> {
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err(CtlError::protocol("decision answer is empty"));
    }
    if trimmed.contains(',') {
        return Err(CtlError::protocol(
            "decision answer expects one choice id (fallback order is engine-side)",
        ));
    }
    let valid: Vec<&str> = card.candidates.iter().map(|c| c.id.as_str()).collect();
    if valid.contains(&trimmed) {
        Ok(trimmed.to_owned())
    } else {
        Err(CtlError::protocol(format!(
            "decision answer must be one of: {}",
            valid.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{ChangedEntry, DiffEntry};

    fn el(role: &str, name: Option<&str>, id: Option<&str>) -> ElementOut {
        ElementOut {
            ref_id: None,
            role: role.to_string(),
            name: name.map(str::to_string),
            automation_id: id.map(str::to_string),
            depth: 1,
        }
    }

    #[test]
    fn dangerous_matches_cjk_and_ascii_case_insensitively() {
        assert!(dangerous(Some("删除所选文件"), None));
        assert!(dangerous(Some("Delete All"), None));
        assert!(dangerous(None, Some("btnUninstall")));
        assert!(!dangerous(Some("保存"), None));
        assert!(!dangerous(None, None));
        // 过度匹配是安全方向:sender 也会命中 send。
        assert!(dangerous(Some("sender address"), None));
    }

    #[test]
    fn scoring_is_deterministic_and_capped() {
        let keywords: Vec<String> = vec!["保存".into(), "导出".into(), "save".into()];
        let element = el("Button", Some("保存并导出"), Some("btnSaveExport"));
        // name 双词命中 3*2=6(中文关键词命中不了 ASCII id)+ id "save" 命中 2
        // + id 非空 1 + Button 2 = 11;第三词已封顶不加分。
        assert_eq!(score_candidate(&element, &keywords, false), 11);
        let fresh = el("Button", Some("保存"), None);
        // name 单词 3 + Button 2 + 新鲜 2 = 7
        assert_eq!(score_candidate(&fresh, &keywords, true), 7);
        // 词表不命中:只剩角色先验
        assert_eq!(
            score_candidate(&el("Edit", None, None), &keywords, false),
            0
        );
    }

    #[test]
    fn gate_uses_margin_and_never_autos_danger() {
        let mk = |name: &str, score: u32| ScoredCandidate {
            selector: format!("name:{name}"),
            role: "Button".into(),
            name: Some(name.into()),
            automation_id: None,
            score,
            dangerous: dangerous(Some(name), None),
        };
        // 领先 2:自动
        let cs = vec![mk("保存", 6), mk("另存", 4)];
        assert_eq!(gate(&cs), GateOutcome::AutoExecute(0));
        // 领先 1:平局外抛
        let cs = vec![mk("保存", 6), mk("另存", 5)];
        assert_eq!(gate(&cs), GateOutcome::Externalize);
        // 头部危险但存在安全候选:裁决安全组,不因危险邻居而外抛
        let cs = vec![mk("删除", 9), mk("保存", 4)];
        assert_eq!(gate(&cs), GateOutcome::AutoExecute(1));
        // 全危险:外抛
        let cs = vec![mk("删除", 9), mk("卸载", 8)];
        assert_eq!(gate(&cs), GateOutcome::Externalize);
        // 唯一安全候选:自动
        let cs = vec![mk("保存", 0)];
        assert_eq!(gate(&cs), GateOutcome::AutoExecute(0));
        // 安全组内平局(危险头部不改变裁决组):外抛
        let cs = vec![mk("删除", 9), mk("保存", 5), mk("另存", 5)];
        assert_eq!(gate(&cs), GateOutcome::Externalize);
    }

    #[test]
    fn gate_and_frontier_agree_on_stable_ordering() {
        let elements = vec![el("Button", Some("b"), None), el("Button", Some("a"), None)];
        let candidates = frontier(&elements, &[], &Default::default(), &Default::default());
        // 同分按 selector 字典序:a 在前(确定性,不随机)
        assert_eq!(candidates[0].selector, "name:a");
        // 等分即平局:外抛
        assert_eq!(gate(&candidates), GateOutcome::Externalize);
    }

    #[test]
    fn priors_demote_risky_candidates_deterministically() {
        // 两个同分候选;负先验压低其一 → 熵门转为自动执行安全侧
        let elements = vec![
            el("Button", Some("置顶"), Some("btnTop")),
            el("Button", Some("保存"), Some("btnSave")),
        ];
        let mut priors = std::collections::BTreeMap::new();
        priors.insert("id:btnTop".to_string(), -4);
        let candidates = frontier(&elements, &[], &Default::default(), &priors);
        // 同基础分(Button+id=3),btnTop 被压到 0,btnSave 3 → 领先 3 ≥ 2
        assert_eq!(candidates[0].selector, "id:btnSave");
        assert_eq!(candidates[0].score, 3);
        assert_eq!(candidates[1].score, 0);
        assert_eq!(gate(&candidates), GateOutcome::AutoExecute(0));
        // 正先验提升已验证候选(下限 0 不产生负分)
        let mut priors = std::collections::BTreeMap::new();
        priors.insert("id:btnSave".to_string(), 2);
        let candidates = frontier(&elements, &[], &Default::default(), &priors);
        assert_eq!(candidates[0].score, 5);
    }

    #[test]
    fn fingerprint_is_reorder_invariant_and_change_sensitive() {
        let a = vec![
            el("Button", Some("OK"), Some("btnOk")),
            el("Edit", Some("q"), None),
        ];
        let reordered = vec![
            el("Edit", Some("q"), None),
            el("Button", Some("OK"), Some("btnOk")),
        ];
        assert_eq!(
            state_fingerprint(&a),
            state_fingerprint(&reordered),
            "虚拟化重排不改变指纹"
        );
        let changed = vec![
            el("Button", Some("确定"), Some("btnOther")),
            el("Edit", Some("q"), None),
        ];
        assert_ne!(state_fingerprint(&a), state_fingerprint(&changed));
        // 同 automation_id 仅标签变化:指纹不变——指纹刻画身份形状,标签变化由差分层上报
        let relabel = vec![
            el("Button", Some("确定"), Some("btnOk")),
            el("Edit", Some("q"), None),
        ];
        assert_eq!(state_fingerprint(&a), state_fingerprint(&relabel));
    }

    fn sample_diff(count: usize) -> SnapshotDiff {
        SnapshotDiff {
            added: (0..count)
                .map(|i| DiffEntry {
                    role: "Button".into(),
                    name: Some(format!("新增条目{i}")),
                    automation_id: None,
                    depth: 2,
                })
                .collect(),
            removed: vec![],
            changed: vec![ChangedEntry {
                paired_by: "automation_id_role",
                role: "Edit".into(),
                name_from: Some("a".into()),
                name_to: Some("b".into()),
                automation_id_from: None,
                automation_id_to: None,
            }],
            pairing: crate::diff::PairingStats {
                paired_by_automation_id_role: 1,
                paired_by_name_role: 0,
                added: count,
                removed: 0,
                changed: 1,
            },
        }
    }

    fn scored(count: usize, name_len: usize) -> Vec<ScoredCandidate> {
        (0..count)
            .map(|i| ScoredCandidate {
                selector: format!("id:button_{i}"),
                role: "Button".into(),
                name: Some("名".repeat(name_len)),
                automation_id: Some(format!("button_{i}")),
                score: 20 - i as u32,
                dangerous: false,
            })
            .collect()
    }

    #[test]
    fn card_fits_budget_and_deterministic_order() {
        let diff = sample_diff(12);
        let candidates = scored(12, 64);
        let card = build_card(&CardInput {
            card_id: "d1",
            task: &"把记事本文档另存为 UTF-8 编码的文本文件到桌面".repeat(8),
            app: "记事本",
            goal: "verify --exists name:完成 --app 记事本",
            step: 5,
            diff: &diff,
            candidates: &candidates,
            effects: &Default::default(),
        })
        .expect("card builds");
        assert!(card_bytes(&card) <= CARD_MAX_BYTES, "4 KiB 硬上限");
        assert!(card.candidates.len() <= CARD_MAX_CANDIDATES, "Top-K≤8");
        assert_eq!(card.candidates.len(), 8);
        // 分数降序
        assert!(card.candidates.windows(2).all(|w| w[0].score >= w[1].score));
        // 差分样本封顶 4 条
        assert_eq!(card.diff.added, 12);
        assert_eq!(card.diff.added_names.len(), 4);
        // 重建一致(确定性)
        let again = build_card(&CardInput {
            card_id: "d1",
            task: &"把记事本文档另存为 UTF-8 编码的文本文件到桌面".repeat(8),
            app: "记事本",
            goal: "verify --exists name:完成 --app 记事本",
            step: 5,
            diff: &diff,
            candidates: &candidates,
            effects: &Default::default(),
        })
        .expect("card builds");
        assert_eq!(card, again);
    }

    #[test]
    fn card_carries_effect_labels_and_truncates_names() {
        let diff = sample_diff(1);
        let candidates = scored(2, 200);
        let mut effects = BTreeMap::new();
        effects.insert("id:button_1".to_string(), "no_effect".to_string());
        let card = build_card(&CardInput {
            card_id: "d2",
            task: "任务",
            app: "应用",
            goal: "goal",
            step: 1,
            diff: &diff,
            candidates: &candidates,
            effects: &effects,
        })
        .expect("card builds");
        assert_eq!(card.candidates[1].effect.as_deref(), Some("no_effect"));
        assert!(card.candidates[0].name.as_deref().unwrap().chars().count() <= 64);
    }

    #[test]
    fn answer_parsing_validates_membership() {
        let card = build_card(&CardInput {
            card_id: "d1",
            task: "t",
            app: "a",
            goal: "g",
            step: 0,
            diff: &sample_diff(0),
            candidates: &scored(3, 4),
            effects: &Default::default(),
        })
        .unwrap();
        assert_eq!(parse_answer(" c2 ", &card).unwrap(), "c2");
        assert!(parse_answer("c9", &card).is_err());
        assert!(parse_answer("", &card).is_err());
        assert!(parse_answer("c1,c2", &card).is_err());
    }

    #[test]
    fn selector_requires_identity_and_interactivity() {
        assert_eq!(
            selector_of(&el("Button", Some("保存"), Some("btnSave"))),
            Some("id:btnSave".into())
        );
        assert_eq!(
            selector_of(&el("Button", Some("保存"), None)),
            Some("name:保存".into())
        );
        assert_eq!(selector_of(&el("Button", None, None)), None);
        assert_eq!(selector_of(&el("Text", Some("标签"), None)), None);
    }
}
