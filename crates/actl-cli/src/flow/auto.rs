//! flow-run `--auto-decide` 的探索决策自动应答循环(2026-10-13 用户授权):
//! run 落 `decision_pending` 时读悬挂卡问 decide 的 provider,confident 应答
//! 逐次回填续跑。**授权红线与手工应答完全一致**——危险候选在请求构造前
//! 剥离(provider 物理不可见),任何弃权形态(dangerous_only/provider_none/
//! low_confidence)停止循环交回人;回填走 `explore::execute` 的同一校验链
//! (parse_answer 成员校验/预算/急停/暂停),模型只生成应答,不执行不授权。
//! 终止保证:引擎 decisions 预算封顶(cap=预算+1 防御,撞上即 INTERNAL
//! 如实报 bug);停止/暂停在每次回填边界让行。

use actl_core::CtlError;
use actl_flow::engine::{Run, Status};
use actl_flow::store::Store;
use serde_json::json;

use super::Adapter;

/// 自动应答循环:仅在 run 停在 decision_pending 时运转;调用方已保证
/// explore 计划在手(静态流程+auto 在 flow-run 预检即 PROTOCOL)。
pub(super) fn auto_decide(
    plan: &actl_flow::Plan,
    store: &Store,
    run: &mut Run,
    host: &mut Adapter<'_>,
) -> Result<(), CtlError> {
    let Some(spec) = &plan.explore else {
        // 调用方保证 explore 计划;防御分支,编译器无法表达。
        return Err(CtlError::internal(
            "auto-decide requires an exploration plan",
        ));
    };
    let cap = spec.budgets.decisions.saturating_add(1);
    for _ in 0..cap {
        if run.status != Status::NeedsReview || run.reason.as_deref() != Some("decision_pending") {
            return Ok(());
        }
        // 用户停止 → ABORTED 如实传播;暂停让行:run 留在 decision_pending,
        // 人接手,恢复仍走手工 flow-resume --decision。
        actl_core::stop::check_current()?;
        if store.pause_requested() {
            return Ok(());
        }
        let card = actl_flow::explore::load(store)
            .ok()
            .flatten()
            .and_then(|state| state.pending.map(|pending| pending.card))
            .ok_or_else(|| CtlError::internal("auto-decide: pending decision card missing"))?;
        let decision = crate::decide::answer_pending_auto(&card)?;
        let Some(choice) = decision.choice.clone() else {
            // abstain(dangerous_only/provider_none/low_confidence):
            // 裁决权交回人——run 保持 decision_pending,不自动续跑。
            return Ok(());
        };
        // 逐次应答进显示端事件流(auto_decide_answer),编排层可见性对称。
        actl_core::signal_session::flow_event(
            "auto_decide_answer",
            json!({
                "run_id": run.id,
                "card_id": decision.card_id,
                "choice": choice,
                "confidence": decision.confidence,
            }),
        );
        actl_flow::explore::execute(spec, run, store, host, Some(choice.as_str()), None)?;
    }
    if run.status == Status::NeedsReview && run.reason.as_deref() == Some("decision_pending") {
        // 撞防御 cap 仍是悬挂态 = 预算与循环失步的 bug 信号,如实报错
        // 而不是静默假完成。
        return Err(CtlError::internal(
            "auto-decide exhausted the decision budget without progress",
        ));
    }
    Ok(())
}
