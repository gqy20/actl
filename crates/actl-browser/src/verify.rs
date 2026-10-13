//! browser-verify 断言族 + 弹层机器(从 ops.rs 拆出,file-lines 门禁)。
//!
//! 语义要点:只读断言动词(flow 的 verify 位只认断言族);页内弹层谓词
//! --no-popups 与写步弹层门禁共享 DEFAULT_POPUP_SELECTOR 检测面——
//! 谓词管复核/分支,门禁管写步拒发(docs/06 §3.4 浏览器组)。

use crate::SelectorKind;
use crate::ops::{OP_DEFAULT_TIMEOUT_MS, dur, list, truncate_text, validate_selector};
use crate::scripts::{locate_script, popup_scan_script};
use crate::session::{self, Paths};
use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::time::Duration;

/// 只读枚举活跃会话(新→旧排序)。flow 的 outputs 只允许只读生产者,
/// browser-launch 是生命周期命令——会话句柄经本命令以观察者身份产出(docs/20 治理模型)。
/// browser-verify 的断言模式(CLI 层负责互斥校验后构造)。
pub enum VerifyMode {
    /// 最新会话 target 应答(launch 后)
    NewestAlive,
    /// 零活跃会话(close 后)
    NoneActive,
    /// 按 id:target 存活
    SessionAlive(String),
    /// 按 id:会话记录已注销
    SessionGone(String),
    /// 登录态核验谓词:选择器唯一解析(+文本包含)——resume_check 与例行登录检查的判据
    Selector {
        session: String,
        kind: SelectorKind,
        value: String,
        contains: Option<String>,
    },
    /// 页内弹层清零断言(2026-10-11 v1):枚举可见弹层表面,零命中=通过,
    /// 非零=ASSERTION_FAILED 带样本——flow condition 位映射 false
    /// (ASSERTION_FAILED→false 是既有语义),when/unless 分支可组合。
    /// 原生弹层门禁结构性不覆盖页内 DOM 弹层(browser-* 走 --session,
    /// front-report 只收 role:Window),本断言是那个空档的检测原语。
    NoPopups {
        session: String,
        selector: Option<String>,
    },
}

/// 默认弹层枚举选择器:标准弹层结构(<dialog open>/ARIA dialog/
/// alertdialog/aria-modal)。如实边界:无标注的自绘 fixed 浮层(cookie 条/
/// 推广层)不在默认面——站点特有横幅走 --popup-selector 显式声明;
/// 在流内的静态 role=dialog 段落会过检(过度检测由 samples 证据暴露,
/// 调用方收窄选择器)。
pub const DEFAULT_POPUP_SELECTOR: &str =
    "dialog[open],[role=dialog],[role=alertdialog],[aria-modal=true]";

/// 写步弹层门禁(browser-click/type 前置,与 front-report「停下判读」同纪律):
/// 动作派发前扫页内可见弹层(默认标准 dialog 语义面)。站点模态态下点击/输入
/// 会被静默吞——派发成功 ≠ 语义生效,与其让动作空放后靠读回兜底,不如拒发
/// 并附样本。NOT_ACTIONABLE(非断言失败:动作还没发生,是护栏拒绝);
/// --allow-popups 显式放行(流内静态 role=dialog 的误报面,调用方拿样本证据决策)。
pub(crate) fn pre_action_popup_gate(
    page: &chromiumoxide::Page,
    timeout: Duration,
) -> Result<(), CtlError> {
    let scanned = crate::bridge::eval_json(
        page,
        &popup_scan_script(DEFAULT_POPUP_SELECTOR),
        None,
        timeout,
    )?;
    // 选择器语法错误如实 PROTOCOL——吞错成 total=0 等于门禁自废
    if let Some(err) = scanned.get("error").and_then(Value::as_str) {
        return Err(CtlError::protocol(format!(
            "popup gate selector is not valid CSS: {err}"
        )));
    }
    let total = scanned.get("total").and_then(Value::as_u64).unwrap_or(0);
    if total > 0 {
        return Err(CtlError::with_evidence_hint(
            ErrorCode::NotActionable,
            format!(
                "{total} visible in-page popup surface(s) block this write action \
                 (dismiss them, or pass --allow-popups after judging the samples)"
            ),
            json!({
                "gate": "popup",
                "selector": DEFAULT_POPUP_SELECTOR,
                "total": total,
                "samples": scanned.get("samples").cloned().unwrap_or(Value::Null),
            }),
            "judge the samples: click the popup's own dismiss/close control with \
             --allow-popups, then re-check with browser-verify --no-popups; pass \
             --allow-popups on the original target only for judged false positives",
        ));
    }
    Ok(())
}

/// 只读断言动词(flow 的 verify 位只认断言族;浏览器步骤的副作用核验入口)。
pub fn verify(paths: &Paths, mode: VerifyMode, timeout_ms: Option<u64>) -> Result<Value, CtlError> {
    let timeout = dur(timeout_ms, OP_DEFAULT_TIMEOUT_MS);
    match mode {
        VerifyMode::NoneActive => {
            let total = list(paths)?
                .get("total")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if total != 0 {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    format!("{total} browser sessions still active"),
                    json!({ "total": total }),
                ));
            }
            Ok(json!({ "assertion": "none", "passed": true }))
        }
        VerifyMode::NewestAlive => {
            let sessions = list(paths)?;
            let newest = sessions.pointer("/sessions/0").cloned().ok_or_else(|| {
                CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "no active browser session to assert",
                    json!({ "total": 0 }),
                )
            })?;
            let id = newest["session_id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let record = session::load(paths, &id)?;
            // open_lane 的发现本身即端点+target 存活的第一重核验
            crate::bridge::open_lane(&record, None, timeout)?;
            Ok(json!({ "assertion": "newest_alive", "passed": true, "session_id": id }))
        }
        VerifyMode::SessionGone(session_id) => match session::load(paths, &session_id) {
            Err(e) if e.code == ErrorCode::NotFound => Ok(json!({
                "assertion": "gone",
                "passed": true,
                "session_id": session_id
            })),
            Ok(_) => Err(CtlError::with_evidence(
                ErrorCode::AssertionFailed,
                format!("browser session {session_id} still registered"),
                json!({ "session_id": session_id }),
            )),
            Err(e) => Err(e),
        },
        VerifyMode::SessionAlive(session_id) => {
            let record = session::load(paths, &session_id)?;
            crate::bridge::open_lane(&record, None, timeout)?;
            Ok(json!({ "assertion": "alive", "passed": true, "session_id": session_id }))
        }
        VerifyMode::Selector {
            session,
            kind,
            value,
            contains,
        } => {
            validate_selector(kind, &value)?;
            let record = session::load(paths, &session)?;
            let lane = crate::bridge::open_lane(&record, None, timeout)?;
            let located = crate::bridge::eval_json(
                &lane.page,
                &locate_script(kind, &value, None),
                None,
                timeout,
            )?;
            let matches = located.get("matches").and_then(Value::as_u64).unwrap_or(0);
            if matches != 1 {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    format!(
                        "login-state selector did not resolve uniquely ({matches} matches; expected 1)"
                    ),
                    json!({
                        "selector_kind": kind.as_str(),
                        "selector": value,
                        "matches": matches,
                    }),
                ));
            }
            let (text, truncated) = truncate_text(
                located
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            if let Some(expected) = &contains
                && !text.contains(expected.as_str())
            {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "login-state selector text does not contain the expected substring",
                    json!({
                        "selector_kind": kind.as_str(),
                        "selector": value,
                        "expected_contains": expected,
                        "actual": text,
                        "truncated": truncated,
                    }),
                ));
            }
            Ok(json!({
                "assertion": "selector",
                "passed": true,
                "session_id": session,
                "text": text,
                "truncated": truncated,
            }))
        }
        VerifyMode::NoPopups { session, selector } => {
            let sel = selector.unwrap_or_else(|| DEFAULT_POPUP_SELECTOR.to_string());
            if sel.is_empty() || sel.chars().count() > 4096 {
                return Err(CtlError::protocol("popup selector must be 1-4096 chars"));
            }
            let record = session::load(paths, &session)?;
            let lane = crate::bridge::open_lane(&record, None, timeout)?;
            let scanned =
                crate::bridge::eval_json(&lane.page, &popup_scan_script(&sel), None, timeout)?;
            // 选择器语法错误如实 PROTOCOL——吞错成 total=0 等于谎称「无弹层」
            if let Some(err) = scanned.get("error").and_then(Value::as_str) {
                return Err(CtlError::protocol(format!(
                    "popup selector is not valid CSS: {err}"
                )));
            }
            let total = scanned.get("total").and_then(Value::as_u64).unwrap_or(0);
            if total > 0 {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    format!("{total} visible in-page popup surface(s) present"),
                    json!({
                        "assertion": "no_popups",
                        "selector": sel,
                        "total": total,
                        "samples": scanned.get("samples").cloned().unwrap_or(Value::Null),
                        "session_id": session,
                    }),
                ));
            }
            Ok(json!({
                "assertion": "no_popups",
                "passed": true,
                "selector": sel,
                "total": 0,
                "session_id": session,
            }))
        }
    }
}
