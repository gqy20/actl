//! browser-collect 实现(2026-10-12 spec)——页级滚动全量收集:读差分去重
//! 累积→页滚→沉降→空连两轮收敛。从 ops.rs 拆出(file-lines 门禁);
//! 会话/求值/校验等共享部件经 ops 的 pub(crate) 面复用。

use crate::SelectorKind;
use crate::bridge;
use crate::ops::{
    OP_DEFAULT_TIMEOUT_MS, declare_action, dur, protocol, truncate_text, validate_property,
    validate_selector,
};
use crate::scripts::{locate_all_script, locate_deep_script, scroll_page_script};
use crate::session::{self, Paths};
use actl_core::CtlError;
use serde_json::{Value, json};
use std::time::Duration;

/// collect 的参数束(browser-collect,2026-10-12 spec,docs/06 §3.4 浏览器组)。
pub struct CollectSpec<'a> {
    pub session_id: &'a str,
    pub kind: SelectorKind,
    pub value: &'a str,
    pub property: Option<&'a str>,
    pub deep: bool,
    /// 每轮滚动后的沉降间隔(100..=5000ms,默认 500——与 --wait-for 轮询同节奏)
    pub interval_ms: Option<u64>,
    /// 轮数预算(1..=200,默认 100);触顶 complete=false + truncation_reason
    pub max_rounds: Option<u64>,
    /// 条目预算(1..=5000,默认 2000);触顶同上
    pub max_items: Option<u64>,
    pub timeout_ms: Option<u64>,
}

/// 页级滚动全量收集:读(复用 --all/--deep 枚举脚本)→ 指纹差分去重累积 →
/// 页滚一屏 → 沉降 → 再读,循环至**空连两轮零新增**收敛(UIA collect 同
/// 判据;滚动触底信号不作为完成判据——懒加载页 scrollHeight 会增长,位置
/// 信号说谎是 UIA 侧否证过的同款教训)。滚动是页面写入:声明动作卡(显示
/// 层协同同 browser-navigate);停止/暂停在轮间检查;读失败原样上抛
/// (导航离开属调用方现场),滚动失败按 truncation_reason=scroll_failed
/// 如实截断(UIA collect 的 scroll_call_failed 同款软失败)。
pub fn collect(paths: &Paths, spec: CollectSpec<'_>) -> Result<Value, CtlError> {
    let CollectSpec {
        session_id,
        kind,
        value,
        property,
        deep,
        interval_ms,
        max_rounds,
        max_items,
        timeout_ms,
    } = spec;
    validate_selector(kind, value)?;
    let property = property.unwrap_or("text");
    validate_property(property)?;
    let interval = interval_ms.unwrap_or(500);
    if !(100..=5000).contains(&interval) {
        return Err(protocol(format!(
            "browser-collect --interval-ms must be 100..=5000, got {interval}"
        )));
    }
    let max_rounds = max_rounds.unwrap_or(100);
    if !(1..=200).contains(&max_rounds) {
        return Err(protocol(format!(
            "browser-collect --max-rounds must be 1..=200, got {max_rounds}"
        )));
    }
    let max_items = max_items.unwrap_or(2000);
    if !(1..=5000).contains(&max_items) {
        return Err(protocol(format!(
            "browser-collect --max-items must be 1..=5000, got {max_items}"
        )));
    }
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    let record = session::load(paths, session_id)?;
    let lane = bridge::open_lane(&record, None, timeout)?;
    let script = if deep {
        locate_deep_script(kind, value, Some(property))
    } else {
        locate_all_script(kind, value, Some(property))
    };
    let mut action = declare_action("browser-collect", &format!("cdp:{session_id}"));
    action.executing();

    let mut items: Vec<Value> = Vec::new();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut new_per_round: Vec<u64> = Vec::new();
    let mut empty_streak = 0u32;
    let mut complete = false;
    let mut truncation_reason: Option<&str> = None;
    let mut last_scroll: Option<Value> = None;
    for round in 1..=max_rounds {
        actl_core::stop::check_current()?;
        actl_core::watchdog::reset();
        let located = bridge::eval_json(&lane.page, &script, None, timeout)?;
        let elements = located
            .get("elements")
            .and_then(Value::as_array)
            .ok_or_else(|| CtlError::internal("enumeration script returned no elements"))?;
        let mut fresh = 0u64;
        for element in elements {
            if items.len() >= max_items as usize {
                break;
            }
            let digest = element_digest(element);
            if seen.insert(digest) {
                fresh += 1;
                items.push(collect_item(element, property));
            }
        }
        new_per_round.push(fresh);
        if items.len() >= max_items as usize {
            truncation_reason = Some("max_items");
            break;
        }
        if fresh == 0 {
            empty_streak += 1;
        } else {
            empty_streak = 0;
        }
        // 页滚一屏 + 沉降再读;滚动 evaluate 失败=软截断(如实上报已收部分)。
        // at_bottom 是"继续滚的门":没到当前页底时空连**不许**收敛——懒加载
        // 分段可能在更下方(B 站视频页评论区分页实证:播放器占 2-3 屏,纯
        // 空连两轮会在抵达评论区前提前收敛);到底后完成权威仍是空连两轮
        // 差分(scrollHeight 随加载增长,触底后新内容出现继续滚)。
        let at_bottom = match bridge::eval_json(&lane.page, &scroll_page_script(), None, timeout) {
            Ok(scrolled) => {
                last_scroll = Some(scrolled.clone());
                scrolled
                    .get("at_bottom")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            }
            Err(_) => {
                truncation_reason = Some("scroll_failed");
                break;
            }
        };
        if empty_streak >= 2 && round >= 2 && at_bottom {
            complete = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(interval));
    }
    let mut payload = json!({
        "action": "browser-collect",
        "session_id": session_id,
        "selector_kind": kind.as_str(),
        "total": items.len(),
        "rounds": new_per_round.len(),
        "new_items_per_round": new_per_round,
        "complete": complete,
    });
    if property != "text" {
        payload["property"] = Value::String(property.to_string());
    }
    if deep {
        payload["deep"] = Value::Bool(true);
    }
    if complete {
        payload["converged_by"] = Value::String("empty-streak".into());
    }
    if let Some(reason) = truncation_reason {
        payload["truncation_reason"] = Value::String(reason.into());
    }
    if let Some(scroll) = last_scroll {
        payload["last_scroll"] = scroll;
    }
    payload["items"] = Value::Array(items);
    Ok(payload)
}

/// 条目指纹:tag+id+text+property 的稳定散列(差分去重键;跨轮 DOM 顺序
/// 变化不影响集合语义)。
fn element_digest(element: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    element
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("")
        .hash(&mut hasher);
    element
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .hash(&mut hasher);
    element
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .hash(&mut hasher);
    element.get("property").hash(&mut hasher);
    hasher.finish()
}

fn collect_item(element: &Value, property: &str) -> Value {
    let (text, truncated) = truncate_text(
        element
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut item = json!({ "text": text, "truncated": truncated });
    if let Some(tag) = element.get("tag").and_then(Value::as_str) {
        item["tag"] = Value::String(tag.to_string());
    }
    if let Some(id) = element.get("id").and_then(Value::as_str) {
        item["id"] = Value::String(id.to_string());
    }
    if property != "text" {
        item["property"] = Value::String(property.to_string());
        item["value"] = element.get("property").cloned().unwrap_or(Value::Null);
    }
    item
}

#[cfg(test)]
mod collect_tests {
    use super::*;
    use actl_core::ErrorCode;

    #[test]
    fn digest_is_content_stable_and_order_free() {
        let a = json!({"tag":"LI","id":null,"text":"item-1","property":null});
        let b = json!({"tag":"LI","id":null,"text":"item-1","property":null});
        let c = json!({"tag":"LI","id":null,"text":"item-2","property":null});
        assert_eq!(element_digest(&a), element_digest(&b));
        assert_ne!(element_digest(&a), element_digest(&c));
    }

    #[test]
    fn budget_validation_rejects_out_of_range() {
        let spec_of = |interval: Option<u64>, rounds: Option<u64>| CollectSpec {
            session_id: "s",
            kind: SelectorKind::Css,
            value: ".x",
            property: None,
            deep: false,
            interval_ms: interval,
            max_rounds: rounds,
            max_items: None,
            timeout_ms: None,
        };
        let err = collect(&Paths::default(), spec_of(Some(50), None)).unwrap_err();
        assert_eq!(err.code, ErrorCode::Protocol);
        assert!(err.message.contains("interval-ms"));
        let err = collect(&Paths::default(), spec_of(Some(500), Some(999))).unwrap_err();
        assert_eq!(err.code, ErrorCode::Protocol);
        assert!(err.message.contains("max-rounds"));
    }
}
