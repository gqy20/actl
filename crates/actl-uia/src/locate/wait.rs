//! wait 族(耐心等待)——element/property/stable/gone 的轮询循环与共享瞬态
//! 预算(TransientBudget);定位语义由 locate::poll 提供。从 locate.rs 拆出
//! 只因文件行数门禁,语义边界不变(外部路径经 locate 的 pub use 不变)。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;

use super::{WaitPredicate, WaitProp, poll};
use crate::timing;

/// wait 族共享的瞬态预算:deadline 内 STALE_REF/INTERNAL 观察瞬态按
/// "尚未满足"吸收继续轮询,非瞬态不吸收(调用方立即上抛);耗尽后如实
/// 返回最后一次瞬态错误,不冒充 TIMEOUT(docs/81 §8 同族:弹窗收尾/窗口
/// 重构过渡期的定位/读取抖动)。三个耐心等待(element/property/stable)
/// 语义由此统一;权限、歧义等确定性失败不受影响。
struct TransientBudget {
    deadline: std::time::Instant,
    last: Option<CtlError>,
}

impl TransientBudget {
    fn new(timeout_ms: u64) -> Self {
        Self {
            deadline: std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms),
            last: None,
        }
    }
    /// 吸收即 true(继续轮询);非瞬态或已过时限 false(立即上抛)。
    fn absorb(&mut self, error: &CtlError) -> bool {
        if matches!(error.code, ErrorCode::StaleRef | ErrorCode::Internal)
            && std::time::Instant::now() < self.deadline
        {
            self.last = Some(copy_error(error));
            true
        } else {
            false
        }
    }
    fn past_deadline(&self) -> bool {
        std::time::Instant::now() >= self.deadline
    }
    /// 耗尽路径的最终错误:有过瞬态则保留原错误(code/evidence/hint 完整);
    /// 否则用调用方的 TIMEOUT(携带各等待自己的 evidence 形态)。
    fn final_error(self, timeout: impl FnOnce() -> CtlError) -> CtlError {
        self.last.unwrap_or_else(timeout)
    }
}

fn copy_error(error: &CtlError) -> CtlError {
    CtlError {
        code: error.code,
        message: error.message.clone(),
        evidence: error.evidence.clone(),
        hint: error.hint.clone(),
    }
}

/// 可注入的 wait --element 核心(observe 由单测脚本化;生产路径传真实 poll)。
fn wait_element_with(
    timeout_ms: u64,
    poll_ms: u64,
    mut observe: impl FnMut() -> Result<Option<()>, CtlError>,
    timeout_err: impl FnOnce() -> CtlError,
) -> Result<(), CtlError> {
    let mut budget = TransientBudget::new(timeout_ms);
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        match observe() {
            Ok(Some(())) => return Ok(()),
            Ok(None) => {}
            Err(error) if budget.absorb(&error) => {}
            Err(error) => return Err(error),
        }
        if budget.past_deadline() {
            return Err(budget.final_error(timeout_err));
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

/// wait --element:轮询定位直到目标可解析(TIMEOUT 语义;与 --expect 的
/// "立即断言"分工见 06 §3.6"耐心等待")。观察瞬态在时限内按"尚未出现"
/// 继续轮询(wait --gone 同款),超时如实失败;非瞬态立即上抛。
pub fn wait_element(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let describe = target.describe();
    wait_element_with(
        timeout_ms,
        timing().poll_ms,
        || poll(app, target).map(|option| option.map(|_| ())),
        || {
            CtlError::new(
                ErrorCode::Timeout,
                format!("element {describe} did not resolve within {timeout_ms}ms"),
            )
        },
    )
}

/// wait --gone:轮询直到目标**不再**可解析(消失;TIMEOUT 语义)。
/// 观察失败(如窗口拆除期 STALE_REF)视作"尚未消失"继续轮询——耐心命令
/// 不被瞬态观察错误打断,超时仍如实失败。
pub fn wait_gone(app: Option<&str>, target: &Target, timeout_ms: u64) -> Result<(), CtlError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀;原实现只在
    // 循环前重置一次——轮询超期限即误杀,潜伏缺陷)。
    loop {
        actl_core::watchdog::reset();
        match poll(app, target) {
            Ok(None) => return Ok(()),
            Ok(Some(_)) => {}
            Err(_) if std::time::Instant::now() < deadline => {}
            Err(error) => return Err(error),
        }
        if std::time::Instant::now() >= deadline {
            return Err(CtlError::new(
                ErrorCode::Timeout,
                format!(
                    "{} still resolves after {timeout_ms}ms (expected it to disappear)",
                    target.describe()
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(timing().poll_ms));
    }
}

/// wait --property 单轮观察:目标缺席 / 命中并读得样本(读通道成功但无值
/// 与缺席分开——谓词对 None 样本判不满足,last_actual 证据保留 None)。
#[derive(Debug)]
enum PropertySample {
    Absent,
    Present(Option<String>),
}

/// 可注入的 wait --property 核心(observe/satisfied 由单测脚本化)。
/// 定位与属性读取的 STALE_REF/INTERNAL 瞬态在时限内按"未满足"继续轮询,
/// 耗尽如实返回最后一次瞬态错误;非瞬态立即上抛。
fn wait_property_with(
    timeout_ms: u64,
    poll_ms: u64,
    mut observe: impl FnMut() -> Result<PropertySample, CtlError>,
    mut satisfied: impl FnMut(Option<&str>) -> bool,
    timeout_err: impl FnOnce(Option<&str>) -> CtlError,
) -> Result<String, CtlError> {
    let mut budget = TransientBudget::new(timeout_ms);
    let mut last: Option<String> = None;
    // 看门狗心跳:每轮轮询 = 进展(长 --timeout 与慢沉降不误杀)。
    loop {
        actl_core::watchdog::reset();
        match observe() {
            Ok(PropertySample::Present(actual)) => {
                if satisfied(actual.as_deref()) {
                    return Ok(actual.unwrap_or_default());
                }
                last = actual;
            }
            Ok(PropertySample::Absent) => {}
            Err(error) if budget.absorb(&error) => {}
            Err(error) => return Err(error),
        }
        if budget.past_deadline() {
            return Err(budget.final_error(|| timeout_err(last.as_deref())));
        }
        std::thread::sleep(std::time::Duration::from_millis(poll_ms));
    }
}

/// wait --property:轮询直到元素可解析且属性满足期望(06 §3.4 完整谓词语法)。
/// 谓词:`value=<expected>`(Value/TextPattern 双通道)| `name=<substr>` | `checked=on|off`;
/// contains=true 时 value/name 用子串语义。元素尚未可解析视为"未满足"继续等
/// (与 --element 的分工:property 隐含存在性);观察瞬态同 TransientBudget。
pub fn wait_property(
    app: Option<&str>,
    target: &Target,
    predicate: &WaitPredicate,
    timeout_ms: u64,
) -> Result<String, CtlError> {
    let describe = target.describe();
    wait_property_with(
        timeout_ms,
        timing().poll_ms,
        || match poll(app, target) {
            Ok(Some(loc)) => predicate.read(&loc.element).map(PropertySample::Present),
            Ok(None) => Ok(PropertySample::Absent),
            Err(error) => Err(error),
        },
        |actual| predicate.satisfied(actual),
        |last| {
            CtlError::with_evidence(
                ErrorCode::Timeout,
                format!(
                    "{} did not satisfy {} within {timeout_ms}ms",
                    describe,
                    predicate.describe()
                ),
                serde_json::json!({ "last_actual": last, "actual":last, "target":describe, "expected":predicate.expected, "property":predicate.describe().split('=').next(), "channel":match predicate.prop { WaitProp::Value => "value-or-text-pattern", WaitProp::Name => "name", WaitProp::Text => "text-pattern", WaitProp::Checked => "toggle-pattern" }, "comparison":if predicate.contains {"contains"} else {"exact"} }),
            )
        },
    )
}

/// wait --stable 的结果(settle 证据:采样数/变化数/末次样本长度)。
pub struct StableOutcome {
    pub samples: usize,
    pub changes: usize,
    pub last_len: usize,
}

/// 可注入的 wait --stable 核心(observe 由单测脚本化)。定位与采样读取的
/// STALE_REF/INTERNAL 瞬态在时限内继续轮询——元素身份已不可延续,连续
/// 稳定计数随之清零(与缺席同置);耗尽如实返回最后一次瞬态错误。
fn wait_stable_with(
    timeout_ms: u64,
    settle_ms: u64,
    mut observe: impl FnMut() -> Result<Option<String>, CtlError>,
    timeout_err: impl FnOnce(usize, usize, Option<&str>) -> CtlError,
) -> Result<StableOutcome, CtlError> {
    const STABLE_SAMPLES: usize = 2;
    let mut budget = TransientBudget::new(timeout_ms);
    let mut last: Option<String> = None;
    let mut stable = 0usize;
    let mut samples = 0usize;
    let mut changes = 0usize;
    // 看门狗心跳:每轮采样 = 进展(长 --timeout 与慢沉降不误杀)。
    loop {
        actl_core::watchdog::reset();
        match observe() {
            Ok(Some(sample)) => {
                samples += 1;
                stable = match &last {
                    Some(prev) if prev == &sample => stable + 1,
                    Some(_) => {
                        changes += 1;
                        1
                    }
                    None => 1,
                };
                last = Some(sample);
                if stable >= STABLE_SAMPLES {
                    return Ok(StableOutcome {
                        samples,
                        changes,
                        last_len: last.as_deref().map(str::len).unwrap_or(0),
                    });
                }
            }
            Ok(None) => stable = 0,
            Err(error) if budget.absorb(&error) => stable = 0,
            Err(error) => return Err(error),
        }
        if budget.past_deadline() {
            return Err(budget.final_error(|| timeout_err(samples, changes, last.as_deref())));
        }
        std::thread::sleep(std::time::Duration::from_millis(settle_ms));
    }
}

/// wait --stable:settle 谓词——轮询直到元素可解析且观测样本连续两次不变
/// (采样间隔 settle_ms,默认 250ms，跨一个采样间隔)。样本通道:text 优先、
/// value 次之、name 兜底(Chromium Document 的 Value=URL 对内容 settle 无
/// 意义,正文只能走 TextPattern,与 get 的 text/value 分工一致)。元素消失
/// 或观察瞬态期间稳定计数清零(隐含存在性);超时 TIMEOUT 附 samples/
/// changes evidence,瞬态耗尽则如实返回原错误(TransientBudget)。
pub fn wait_stable(
    app: Option<&str>,
    target: &Target,
    timeout_ms: u64,
) -> Result<StableOutcome, CtlError> {
    let describe = target.describe();
    wait_stable_with(
        timeout_ms,
        timing().settle_ms,
        || match poll(app, target) {
            Ok(Some(loc)) => stability_sample(&loc.element).map(Some),
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        },
        |samples, changes, last| {
            CtlError::with_evidence(
                ErrorCode::Timeout,
                format!(
                    "{} did not settle within {timeout_ms}ms (content still changing or element absent)",
                    describe
                ),
                serde_json::json!({
                    "samples": samples,
                    "changes": changes,
                    "last_len": last.map(str::len),
                }),
            )
        },
    )
}

/// 稳定性采样:text > value > name(富文本正文只有 TextPattern 可读;
/// value 通道兜底纯控件,name 最后)。
fn stability_sample(elem: &UIElement) -> Result<String, CtlError> {
    if let Some(text) = crate::read::read_text(elem)? {
        return Ok(text);
    }
    if let Some(value) = crate::read::read_value(elem)? {
        return Ok(value);
    }
    elem.get_name()
        .map_err(|e| crate::read_channel::failure("name", e))
}

#[cfg(test)]
mod tests {
    // ---- wait 族瞬态统一语义(可注入 observe;生产路径见 wait_* 包装) ----

    use actl_core::{CtlError, ErrorCode};

    /// 脚本化 observe:按序弹出步骤;耗尽即 panic(测试用)。
    fn script<T>(steps: Vec<Result<T, CtlError>>) -> impl FnMut() -> Result<T, CtlError> {
        use std::collections::VecDeque;
        let queue = std::sync::Mutex::new(VecDeque::from(steps));
        move || {
            queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("script exhausted before loop finished")
        }
    }

    fn stale() -> CtlError {
        CtlError::new(ErrorCode::StaleRef, "element invalidated")
    }

    #[test]
    fn wait_element_with_absorbs_transient_then_resolves() {
        let observe = script(vec![
            Err(stale()),
            Err(CtlError::new(ErrorCode::Internal, "com")),
            Ok(Some(())),
        ]);
        super::wait_element_with(2000, 1, observe, || {
            CtlError::new(ErrorCode::Timeout, "timeout")
        })
        .unwrap();
    }

    #[test]
    fn wait_element_with_transient_exhaustion_keeps_original_error() {
        let error = super::wait_element_with(
            25,
            1,
            || Err(stale()),
            || CtlError::new(ErrorCode::Timeout, "timeout"),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleRef);
    }

    #[test]
    fn wait_element_with_non_transient_propagates_immediately() {
        let error = super::wait_element_with(
            2000,
            1,
            || Err(CtlError::new(ErrorCode::Ambiguous, "two matches")),
            || CtlError::new(ErrorCode::Timeout, "timeout"),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
    }

    #[test]
    fn wait_property_with_absorbs_transient_read_then_satisfies() {
        let observe = script(vec![
            Err(stale()),
            Ok(super::PropertySample::Present(Some("ready".into()))),
        ]);
        let value = super::wait_property_with(
            2000,
            1,
            observe,
            |actual| actual == Some("ready"),
            |_| CtlError::new(ErrorCode::Timeout, "timeout"),
        )
        .unwrap();
        assert_eq!(value, "ready");
    }

    #[test]
    fn wait_property_with_transient_exhaustion_keeps_original_error() {
        let error = super::wait_property_with(
            25,
            1,
            || Err(CtlError::new(ErrorCode::Internal, "provider")),
            |_| false,
            |_| CtlError::new(ErrorCode::Timeout, "timeout"),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Internal);
    }

    #[test]
    fn wait_property_with_pure_timeout_keeps_last_actual_evidence() {
        let error = super::wait_property_with(
            25,
            1,
            || Ok(super::PropertySample::Present(Some("nope".into()))),
            |actual| actual == Some("ready"),
            |last| {
                CtlError::with_evidence(
                    ErrorCode::Timeout,
                    "did not satisfy",
                    serde_json::json!({ "last_actual": last }),
                )
            },
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Timeout);
        assert_eq!(error.evidence.unwrap()["last_actual"], "nope");
    }

    #[test]
    fn wait_stable_with_resets_count_on_transient() {
        let observe = script(vec![
            Ok(Some("a".into())),
            Err(stale()),
            Ok(Some("a".into())),
            Ok(Some("a".into())),
        ]);
        let outcome = super::wait_stable_with(2000, 1, observe, |samples, changes, _| {
            CtlError::with_evidence(
                ErrorCode::Timeout,
                "did not settle",
                serde_json::json!({ "samples": samples, "changes": changes }),
            )
        })
        .unwrap();
        assert_eq!(outcome.samples, 3);
        assert_eq!(outcome.changes, 0);
    }

    #[test]
    fn wait_stable_with_resets_count_on_absence() {
        let observe = script(vec![
            Ok(Some("a".into())),
            Ok(None),
            Ok(Some("a".into())),
            Ok(Some("a".into())),
        ]);
        let outcome = super::wait_stable_with(2000, 1, observe, |samples, changes, _| {
            CtlError::with_evidence(
                ErrorCode::Timeout,
                "did not settle",
                serde_json::json!({ "samples": samples, "changes": changes }),
            )
        })
        .unwrap();
        assert_eq!(outcome.samples, 3);
    }

    #[test]
    fn wait_stable_with_transient_exhaustion_keeps_original_error() {
        let error = super::wait_stable_with(
            25,
            1,
            || Err(stale()),
            |samples, changes, _| {
                CtlError::with_evidence(
                    ErrorCode::Timeout,
                    "did not settle",
                    serde_json::json!({ "samples": samples, "changes": changes }),
                )
            },
        )
        .err()
        .unwrap();
        assert_eq!(error.code, ErrorCode::StaleRef);
    }
}
