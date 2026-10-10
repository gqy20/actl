//! collect —— 虚拟化容器有界收集的纯逻辑(06 §3.4 `collect`)。
//!
//! 机制 = 滚动→观察→差分→收敛(ROADMAP 2026-10-07 登记形态):每轮观察的
//! 元素清单进来,按 (role, name, automation_id) 精确键去重累积;连续 K 轮
//! 零新增即收敛;轮次/条数预算耗尽如实上报截断,不谎称完整。
//! 顺序 = 首见序(虚拟化列表的视觉序,不按字典序重排)。

/// 单个收集项(与 diff 的 DiffEntry 同形,无 ref——虚拟化条目跨轮不保
/// ref 稳定,调用方重定向走 find/选择器,不回放)。去重键 = (role, name,
/// automation_id) 精确三元组(状态机内联构造;名称不折叠——差分的宽松
/// 配对归 diff,收集键从严,避免误合)。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CollectItem {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_id: Option<String>,
    pub depth: u32,
}

/// 收敛判定:连续 2 轮零新增即"再滚也出不来新条目"。
const CONVERGENCE_EMPTY_ROUNDS: usize = 2;

/// 收集状态机(观察注入式,驱动侧见 actl-uia collect)。
#[derive(Debug)]
pub struct CollectState {
    items: Vec<CollectItem>,
    seen: std::collections::HashSet<(String, Option<String>, Option<String>)>,
    rounds: usize,
    /// 连续零新增轮计数
    empty_streak: usize,
    /// 每轮新增数(证据面:settle 调参/截断归因)
    new_per_round: Vec<usize>,
    converged: bool,
    max_items: usize,
    max_rounds: usize,
}

/// 一轮观察后的裁决。
#[derive(Debug, PartialEq, Eq)]
pub enum RoundOutcome {
    /// 继续下一轮(本轮新增 N 条,已入账)。
    Continue,
    /// 收敛:连续零新增,容器末尾已到。
    Converged,
    /// 预算耗尽(轮次或条数),如实截断。
    BudgetExhausted,
}

impl CollectState {
    pub fn new(max_rounds: usize, max_items: usize) -> Self {
        Self {
            items: Vec::new(),
            seen: std::collections::HashSet::new(),
            rounds: 0,
            empty_streak: 0,
            new_per_round: Vec::new(),
            converged: false,
            max_items,
            max_rounds,
        }
    }

    pub fn items(&self) -> &[CollectItem] {
        &self.items
    }

    pub fn rounds(&self) -> usize {
        self.rounds
    }

    pub fn new_per_round(&self) -> &[usize] {
        &self.new_per_round
    }

    pub fn converged(&self) -> bool {
        self.converged
    }

    /// 入账一轮观察(首见去重);返回裁决。条数预算在**入账时**截断
    /// (保留已观察到的,不丢弃),截断如实走 BudgetExhausted。
    pub fn observe(&mut self, round: Vec<CollectItem>) -> RoundOutcome {
        self.rounds += 1;
        let mut added = 0usize;
        for item in round {
            if self.items.len() >= self.max_items {
                self.new_per_round.push(added);
                return RoundOutcome::BudgetExhausted;
            }
            if self.seen.insert((
                item.role.clone(),
                item.name.clone(),
                item.automation_id.clone(),
            )) {
                self.items.push(item);
                added += 1;
            }
        }
        self.new_per_round.push(added);
        if added == 0 {
            self.empty_streak += 1;
        } else {
            self.empty_streak = 0;
        }
        if self.empty_streak >= CONVERGENCE_EMPTY_ROUNDS {
            self.converged = true;
            RoundOutcome::Converged
        } else if self.rounds >= self.max_rounds {
            RoundOutcome::BudgetExhausted
        } else {
            RoundOutcome::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(role: &str, name: &str, depth: u32) -> CollectItem {
        CollectItem {
            role: role.into(),
            name: Some(name.into()),
            automation_id: None,
            depth,
        }
    }

    #[test]
    fn dedups_rematerialized_items_across_rounds() {
        // 虚拟化核心行为:滚回来/重物化的同一条目重复出现
        let mut s = CollectState::new(10, 100);
        assert_eq!(
            s.observe(vec![item("ListItem", "a", 3), item("ListItem", "b", 3)]),
            RoundOutcome::Continue
        );
        assert_eq!(
            s.observe(vec![
                item("ListItem", "a", 3), // 重复
                item("ListItem", "c", 3),
            ]),
            RoundOutcome::Continue
        );
        assert_eq!(s.items().len(), 3);
        assert_eq!(s.new_per_round(), &[2, 1]);
        // 首见序稳定,不重排
        let names: Vec<&str> = s
            .items()
            .iter()
            .map(|i| i.name.as_deref().unwrap())
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn converges_after_two_empty_rounds() {
        let mut s = CollectState::new(10, 100);
        s.observe(vec![item("ListItem", "a", 3)]);
        assert_eq!(s.observe(vec![]), RoundOutcome::Continue);
        assert_eq!(s.observe(vec![]), RoundOutcome::Converged);
        assert!(s.converged());
        assert_eq!(s.rounds(), 3);
    }

    #[test]
    fn rounds_budget_reports_truncation_honestly() {
        let mut s = CollectState::new(3, 100);
        s.observe(vec![item("ListItem", "a", 3)]);
        s.observe(vec![item("ListItem", "b", 3)]);
        assert_eq!(
            s.observe(vec![item("ListItem", "c", 3)]),
            RoundOutcome::BudgetExhausted
        );
        assert!(!s.converged());
    }

    #[test]
    fn items_budget_keeps_observed_prefix() {
        let mut s = CollectState::new(10, 2);
        s.observe(vec![item("ListItem", "a", 3), item("ListItem", "b", 3)]);
        assert_eq!(
            s.observe(vec![item("ListItem", "c", 3)]),
            RoundOutcome::BudgetExhausted
        );
        assert_eq!(s.items().len(), 2);
    }

    #[test]
    fn empty_round_streak_resets_on_new_item() {
        // 一轮零新增后又有新增:streak 归零,不提前收敛
        let mut s = CollectState::new(10, 100);
        s.observe(vec![item("ListItem", "a", 3)]);
        s.observe(vec![]); // streak=1
        s.observe(vec![item("ListItem", "b", 3)]); // streak=0
        s.observe(vec![]); // streak=1
        assert_eq!(s.observe(vec![]), RoundOutcome::Converged); // streak=2
        assert_eq!(s.items().len(), 2);
    }

    #[test]
    fn key_is_exact_not_folded() {
        // 大小写/空白差异是不同条目(从严键,不误合;差分的宽松配对归 diff)
        let mut s = CollectState::new(10, 100);
        s.observe(vec![item("ListItem", "File.txt", 3)]);
        s.observe(vec![item("ListItem", "file.txt", 3)]);
        assert_eq!(s.items().len(), 2);
    }
}
