//! collect —— 虚拟化容器有界收集驱动(06 §3.4 `collect`;ROADMAP 2026-10-07
//! 登记形态裁定:引擎有界动词,非 flow 循环步骤)。
//!
//! 机制 = 滚动 → 每轮新鲜容器观察 → 差分去重累积 → 收敛(纯逻辑在
//! actl-core collect)。**完成判据 = 纯经验(空连两轮零新增)**;每轮滚动
//! = 1 × LargeIncrement(页级;真机实测 SmallIncrement≈1 条、物化窗≈1 页
//! 有余量,页级步进 600 条实测无跳格)。
//!
//! 读取类纪律:仅语义 ScrollPattern,零输入注入;物理滚轮兜底按
//! ADR-007 留给调用方显式组合(scroll --physical)。
//!
//! **设计过程中的三处真机否证(2026-10-08 Explorer 600 文件 fixture,
//! 如实入档)**:
//! ① ItemContainerPattern 枚举模式(propertyId=0+VT_EMPTY)只返回当前
//!   物化窗(17/600)后即报"耗尽"——枚举耗尽不证明全量,假完整,不采用;
//! ② 起始位置依赖:从底部起步会把"起始即末尾"误判成整表完成——先语义
//!   复位到顶(SetScrollPercent 垂直归零)消除;
//! ③ 滚动位置信号(percent+view≥100)提前一页误报(触发后小步仍可推进、
//!   尾条缺 14)——位置信号只当启发不当判据,完成权威只有空连两轮。
//!
//! 条数/轮次预算有界,截断如实上报(complete=false + reason),不谎称
//! 全量。收集项不携带 @eN ref——虚拟化条目跨轮不保 ref 稳定,调用方重
//! 定向走 find/语义选择器。

use actl_core::CtlError;
use actl_core::collect::{CollectItem, CollectState, RoundOutcome};
use actl_core::target::Target;

use uiautomation::UIElement;
use uiautomation::patterns::UIScrollPattern;
use uiautomation::types::ScrollAmount;

use crate::locate::{self, Located};

/// 有界收集的预算束(CLI 校验后传入)。
pub struct CollectOptions {
    /// 观察轮次预算(含首帧;1..=200,默认 50——每轮一页,覆盖 ~50 页列表)。
    pub max_rounds: usize,
    /// 总条数预算(1..=5000)。
    pub max_items: usize,
    /// 每轮页滚后的沉降等待(语义滚动后树重物化的宽限)。
    pub settle_ms: u64,
    /// 可选角色过滤(小写子串,CLI 侧预折;find --role 同语义)。预算只
    /// 计过滤后条目——装饰元素不吃掉收集预算。
    pub role_filter: Option<String>,
}

pub struct CollectReport {
    pub resolved_by: &'static str,
    pub container_role: String,
    pub container_name: Option<String>,
    pub container_automation_id: Option<String>,
    pub items: Vec<CollectItem>,
    pub rounds: usize,
    /// 每轮新增数(证据面:settle 调参/跳格检测/截断归因)。
    pub new_per_round: Vec<usize>,
    pub complete: bool,
    pub truncation_reason: Option<&'static str>,
    /// 完成判据:"empty-streak"(空连两轮,纯经验收敛)
    pub converged_by: Option<&'static str>,
}

/// 收集入口:定位容器 → 滚动收敛。
pub fn collect(
    app: Option<&str>,
    target: &Target,
    opts: &CollectOptions,
) -> Result<CollectReport, CtlError> {
    let loc = locate::locate(app, target, None)?;
    scroll_converge(&loc, opts)
}

/// 条目角色过滤(小写子串;None 全收)。
fn role_matches(opts: &CollectOptions, role: &str) -> bool {
    opts.role_filter
        .as_deref()
        .is_none_or(|f| role.to_lowercase().contains(f))
}

/// 到顶判据:percent ≤ 0.5(仅复位路径的启发;不作完成判据——percent
/// 信号在 Explorer 实证提前一页误报)。
fn at_scroll_top(scroll: &UIScrollPattern) -> Option<bool> {
    Some(scroll.get_vertical_scroll_percent().ok()? <= 0.5)
}

/// 语义复位到顶(收集语义=整表,不受用户遗留滚动位置影响——从底部起步
/// 会把"起始即末尾"误判成整表完成)。垂直归零(水平 -1 = UIA 的
/// NoScroll);SetScrollPercent 不支持则大步上滚兜底。全程零注入。
fn reset_to_top(scroll: &UIScrollPattern) -> bool {
    if scroll.set_scroll_percent(-1.0, 0.0).is_ok() {
        return true;
    }
    for _ in 0..8 {
        if scroll
            .scroll(ScrollAmount::NoAmount, ScrollAmount::LargeDecrement)
            .is_err()
        {
            return false;
        }
        match at_scroll_top(scroll) {
            Some(true) => return true,
            None | Some(false) => continue,
        }
    }
    false
}

/// 滚动收敛主循环。轮次口径 = 观察轮数(含首帧)。
fn scroll_converge(loc: &Located, opts: &CollectOptions) -> Result<CollectReport, CtlError> {
    let mut state = CollectState::new(opts.max_rounds, opts.max_items);
    // 定位时刻的容器 RuntimeId = 终检基准(容器重建检测)
    let baseline_rid = {
        let _watchdog = crate::watchdog::enter();
        loc.element.get_runtime_id().ok().filter(|r| !r.is_empty())
    };
    let mut truncation_reason: Option<&'static str> = None;
    let mut converged_by: Option<&'static str> = None;
    let scroll = {
        let _watchdog = crate::watchdog::enter();
        loc.element.get_pattern::<UIScrollPattern>().ok()
    };
    let scroll = match scroll {
        None => {
            // 无模式:当前可见条目即所得(诚实部分结果,不空手)
            let initial = observe_items(&loc.element, opts)?;
            state.observe(initial);
            return Ok(report(
                loc,
                state,
                false,
                None,
                Some("scroll_pattern_missing"),
            ));
        }
        Some(scroll) => scroll,
    };
    // 复位到顶(收集语义=整表,不受用户遗留滚动位置影响;复位失败=起始
    // 位置之上的内容收不到,整表完成不可声称,如实截断向下收集)
    let at_top = matches!(at_scroll_top(&scroll), Some(true));
    let reset_ok = at_top || reset_to_top(&scroll);
    if !reset_ok {
        truncation_reason = Some("scroll_reset_failed");
    }
    // 复位后的首帧
    let initial = observe_items(&loc.element, opts)?;
    if let RoundOutcome::BudgetExhausted = state.observe(initial) {
        // max_rounds=1 边界:只看了首帧即预算尽,如实截断
        return Ok(report(loc, state, false, None, Some("max_rounds")));
    }
    // 收敛权威 = 纯经验(空连两轮):provider 的滚动位置信号(Vertical-
    // ScrollPercent+ViewSize≥100)在 Explorer 实证**提前一页误报**(信号
    // 触发后小步仍可推进)——位置信号只当启发不当判据,不作完成依据。
    while reset_ok && truncation_reason.is_none() && state.rounds() < opts.max_rounds {
        actl_core::wait_control::check()?;
        // 语义页滚(向末尾 1 × LargeIncrement);失败如实截断,不吞错
        let scrolled = {
            let _watchdog = crate::watchdog::enter();
            scroll
                .scroll(ScrollAmount::NoAmount, ScrollAmount::LargeIncrement)
                .is_ok()
        };
        if !scrolled {
            truncation_reason = Some("scroll_call_failed");
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(opts.settle_ms));
        let round = observe_items(&loc.element, opts)?;
        match state.observe(round) {
            RoundOutcome::Continue => {}
            RoundOutcome::Converged => {
                converged_by = Some("empty-streak");
                break;
            }
            RoundOutcome::BudgetExhausted => {
                truncation_reason = Some(if state.items().len() >= opts.max_items {
                    "max_items"
                } else {
                    "max_rounds"
                });
                break;
            }
        }
    }
    if truncation_reason.is_none() && converged_by.is_none() {
        truncation_reason = Some("max_rounds");
    }
    // 容器身份终检:完成结论要成立,收尾时容器必须仍是定位时刻那个实例
    let mut complete = truncation_reason.is_none();
    if complete {
        let current = {
            let _watchdog = crate::watchdog::enter();
            loc.element.get_runtime_id().ok().filter(|r| !r.is_empty())
        };
        if current.is_none() || current != baseline_rid {
            complete = false;
            truncation_reason = Some("container_rebuilt");
        }
    }
    Ok(report(
        loc,
        state,
        complete,
        converged_by,
        truncation_reason,
    ))
}

/// 容器子树一轮新鲜观察(观察走物化默认 + provider 拒答自动回落;角色
/// 过滤内联——预算只计过滤后条目)。
fn observe_items(
    container: &UIElement,
    opts: &CollectOptions,
) -> Result<Vec<CollectItem>, CtlError> {
    let observed = crate::observation::tree(container)?;
    let mut out = Vec::new();
    for entry in &observed.entries {
        let role = format!("{:?}", crate::observation::role_of(&entry.element)?);
        if !role_matches(opts, &role) {
            continue;
        }
        out.push(CollectItem {
            role,
            name: Some(crate::observation::name_of(&entry.element)?),
            automation_id: Some(crate::observation::automation_id_of(&entry.element)?),
            depth: entry.depth,
        });
    }
    Ok(out)
}

fn report(
    loc: &Located,
    state: CollectState,
    complete: bool,
    converged_by: Option<&'static str>,
    reason: Option<&'static str>,
) -> CollectReport {
    CollectReport {
        resolved_by: loc.resolved_by,
        container_role: loc.role.clone(),
        container_name: loc.name.clone(),
        container_automation_id: loc.automation_id.clone(),
        items: state.items().to_vec(),
        rounds: state.rounds(),
        new_per_round: state.new_per_round().to_vec(),
        complete,
        truncation_reason: reason,
        converged_by,
    }
}
