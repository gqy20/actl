//! 深水层同步门面桥(ADR-001 修订面):受控 tokio runtime 把 chromiumoxide
//! 的 async 圈在本 crate 内部——crate 对外 API 保持同步,CLI/flow 调用方
//! 零 async 概念。spike 真机实证形态的入库版(docs/72,prototypes/browser-lib-probe)。
//!
//! - runtime 惰性启动(进程首次深水层调用时创建,CLI 单命令生命周期常驻);
//! - block_on:future 投递至 runtime,调用线程限时阻塞收结果——与 cdp.rs
//!   单属主事件循环同款的外部形态(spike 实测 attach 全链 ~1.6s/拦截工作);
//! - connect:attach 既有 CDP 端点(actl 拥有会话,本层不派生浏览器进程)
//!   + handler 泵任务(handler 不被泵送则事件断流——spike 实证);

use std::future::Future;
use std::sync::OnceLock;
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::Duration;

use actl_core::{CtlError, ErrorCode};

/// runtime 构建失败也只初始化一次(错误原样复述,不反复重试)。
static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

fn runtime() -> Result<&'static tokio::runtime::Runtime, CtlError> {
    let cell = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("actl-deep-lane")
            .enable_all()
            .build()
            .map_err(|e| format!("cannot start deep-lane runtime: {e}"))
    });
    cell.as_ref()
        .map_err(|msg| CtlError::internal(format!("deep-lane runtime unavailable: {msg}")))
}

/// 在受控 runtime 上执行 future,调用线程限时阻塞等待结果。
/// 超时→Timeout;任务 panic/早退→Internal(fail-closed,不静默)。
pub(crate) fn block_on<T, F>(future: F, timeout: Duration) -> Result<T, CtlError>
where
    T: Send + 'static,
    F: Future<Output = T> + Send + 'static,
{
    let (tx, rx) = sync_channel::<T>(1);
    runtime()?.handle().spawn(async move {
        let _ = tx.send(future.await);
    });
    match rx.recv_timeout(timeout) {
        Ok(value) => Ok(value),
        Err(RecvTimeoutError::Timeout) => Err(CtlError::new(
            ErrorCode::Timeout,
            format!(
                "deep-lane call did not finish within {}ms",
                timeout.as_millis()
            ),
        )),
        Err(RecvTimeoutError::Disconnected) => Err(CtlError::internal(
            "deep-lane task ended without a result (panic or early return)",
        )),
    }
}

/// 连接既有 CDP 浏览器端点(attach 形态:会话归 actl 所有,本层不派生进程)。
/// attach 失败=端点死=会话失效,沿 docs/72 §12 既有映射归 NOT_FOUND。
pub(crate) fn connect(ws_url: &str, timeout: Duration) -> Result<chromiumoxide::Browser, CtlError> {
    let url = ws_url.to_string();
    let result = block_on(
        async {
            chromiumoxide::Browser::connect(url)
                .await
                .map_err(|e| format!("{e}"))
        },
        timeout,
    )?;
    match result {
        Ok((browser, handler)) => {
            pump(handler)?;
            Ok(browser)
        }
        Err(msg) => Err(CtlError::new(
            ErrorCode::NotFound,
            format!("deep-lane endpoint unreachable (session likely gone): {msg}"),
        )),
    }
}

/// handler 泵:chromiumoxide 的事件循环必须被持续 poll,否则响应/事件全部断流。
/// 连接关闭时流自然结束,泵任务随之退出(CLI 单命令生命周期,无泄漏面)。
fn pump(mut handler: chromiumoxide::Handler) -> Result<(), CtlError> {
    use futures::StreamExt;
    runtime()?.handle().spawn(async move {
        while let Some(item) = handler.next().await {
            if item.is_err() {
                break;
            }
        }
    });
    Ok(())
}

/// 骨干操作面(Phase 2 终态吸收,2026-10-13):Browser + 已发现目标的 Page。
pub(crate) struct Lane {
    /// 骨干连接持有者(生命期锚:Page 依赖其 handler 存活;现役消费方为
    /// page 侧操作,close 走独立 connect)。
    #[allow(dead_code)]
    pub browser: chromiumoxide::Browser,
    pub page: chromiumoxide::Page,
}

/// target 发现竞态上限:attach 后 pages() 需等 discover 快照回流
/// (SetDiscoverTargets(true) 对已存在 target 也发 TargetCreated;
/// 立即调用可能空表——smoke 首轮实证)。
pub(crate) const DISCOVERY_SETTLE_MAX_MS: u64 = 3_000;
const DISCOVERY_POLL_MS: u64 = 100;

/// 打开骨干会话:connect → pages() 有界发现(默认会话绑 tab;--target 覆盖)。
/// 发现失败 = 目标不在场(新 tab 顶替/popup 收起)→ NOT_FOUND 与浅层同判。
pub(crate) fn open_lane(
    record: &crate::session::BrowserSession,
    target: Option<&str>,
    timeout: Duration,
) -> Result<Lane, CtlError> {
    let want = target.unwrap_or(&record.target_id).to_string();
    let ws_url = format!(
        "ws://127.0.0.1:{}{}",
        record.devtools_port, record.browser_ws_path
    );
    let browser = connect(&ws_url, timeout)?;
    let want_clone = want.clone();
    let work = async move {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(DISCOVERY_SETTLE_MAX_MS);
        loop {
            let pages = browser
                .pages()
                .await
                .map_err(|e| format!("enumerate pages: {e}"))?;
            if let Some(page) = pages
                .into_iter()
                .find(|p| p.target_id().as_ref() == want_clone)
            {
                return Ok::<(chromiumoxide::Browser, chromiumoxide::Page), String>((
                    browser, page,
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("target not among live pages".to_string());
            }
            tokio::time::sleep(Duration::from_millis(DISCOVERY_POLL_MS)).await;
        }
    };
    match block_on(
        work,
        timeout + Duration::from_millis(DISCOVERY_SETTLE_MAX_MS),
    ) {
        Ok(Ok((browser, page))) => Ok(Lane { browser, page }),
        Ok(Err(msg)) => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!(
                "target no longer exists (new tab took over or popup closed); \
                 re-enumerate with browser-targets: {msg}"
            ),
            serde_json::json!({ "target_id": want }),
        )),
        Err(e) => Err(e),
    }
}

/// JS 求值(浅层 eval_json 同约定,scripts.rs 全量复用零语义变化):
/// 脚本恒返回 JSON.stringify 字符串,returnByValue 后取 value 解析。
pub(crate) fn eval_json(
    page: &chromiumoxide::Page,
    script: &str,
    context_id: Option<u64>,
    timeout: Duration,
) -> Result<serde_json::Value, CtlError> {
    use chromiumoxide::cdp::js_protocol::runtime::{EvaluateParams, ExecutionContextId};
    let mut builder = EvaluateParams::builder()
        .expression(script)
        .await_promise(true)
        .return_by_value(true);
    if let Some(id) = context_id {
        builder = builder.context_id(ExecutionContextId::new(id as i64));
    }
    let params = builder
        .build()
        .map_err(|e| CtlError::protocol(format!("evaluate params: {e}")))?;
    let page_owned = page.clone();
    let text = block_on(
        async move {
            let response = page_owned
                .execute(params)
                .await
                .map_err(|e| format!("evaluate: {e}"))?;
            if let Some(details) = &response.result.exception_details {
                return Err(format!("page script raised: {}", details.text));
            }
            let value = response
                .result
                .result
                .value
                .ok_or("evaluate returned no value (exception path)")?;
            let text = value
                .as_str()
                .ok_or("evaluate returned non-string value")?
                .to_string();
            Ok::<String, String>(text)
        },
        timeout,
    )
    .and_then(|inner| {
        inner.map_err(|msg| CtlError::internal(format!("page script evaluation failed: {msg}")))
    })?;
    serde_json::from_str(&text)
        .map_err(|e| CtlError::internal(format!("page script returned non-JSON payload: {e}")))
}

/// 派发导航并等主 frame 导航事件(浅层 navigate 竞态判定的骨干等价物)。
/// **订阅必须先于派发**:骨干 EventStream 只收订阅后的事件(与浅层
/// drain_events 的积压回放语义结构性不同——那是 backlog 模型),先导航后
/// 订阅会恒错过事件(2026-10-13 真机回归实证:go_search 恒 TIMEOUT)。
/// 返回 frame_id;errorText 网络级失败以 net_error: 前缀上抛由调用方映射。
pub(crate) fn navigate_and_wait(
    page: &chromiumoxide::Page,
    url: &str,
    timeout: Duration,
) -> Result<(String,), CtlError> {
    use chromiumoxide::cdp::browser_protocol::page::{EnableParams, EventFrameNavigated};
    use futures::StreamExt;
    let page_owned = page.clone();
    let url_owned = url.to_string();
    let work = async move {
        page_owned
            .execute(EnableParams::default())
            .await
            .map_err(|e| format!("page enable: {e}"))?;
        let mut listener = page_owned
            .event_listener::<EventFrameNavigated>()
            .await
            .map_err(|e| format!("frame listener: {e}"))?;
        let params = chromiumoxide::cdp::browser_protocol::page::NavigateParams::builder()
            .url(url_owned)
            .build()
            .map_err(|e| format!("navigate params: {e}"))?;
        let nav = page_owned
            .execute(params)
            .await
            .map_err(|e| format!("navigate dispatch: {e}"))?;
        if let Some(net_error) = nav.result.error_text.as_deref()
            && !net_error.is_empty()
        {
            return Err(format!("net_error:{net_error}"));
        }
        let frame = nav.result.frame_id.as_ref().to_string();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(deadline, listener.next()).await {
                Ok(Some(ev)) => {
                    if ev.frame.id.as_ref() == frame {
                        return Ok((frame,));
                    }
                }
                Ok(None) => return Err("frame navigated stream ended".to_string()),
                Err(_) => return Err("frame navigated not observed".to_string()),
            }
        }
    };
    match block_on(work, timeout) {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(msg)) if msg.starts_with("net_error:") => Err(CtlError::with_evidence(
            ErrorCode::Internal,
            format!("page load failed at network level: {}", &msg[10..]),
            serde_json::json!({ "url": url, "net_error": &msg[10..] }),
        )),
        Ok(Err(msg)) => Err(CtlError::new(
            ErrorCode::Timeout,
            format!(
                "navigation not observed within {}ms: {msg}",
                timeout.as_millis()
            ),
        )),
        Err(e) => Err(e),
    }
}

/// iframe 执行上下文发现(浅层 iframe_context 的骨干等价物):
/// 主 frame 首个子 frame + Runtime.executionContextCreated 事件配对。
pub(crate) fn find_iframe_context(
    page: &chromiumoxide::Page,
    timeout: Duration,
) -> Result<(String, u64), CtlError> {
    use chromiumoxide::cdp::browser_protocol::page::GetFrameTreeParams;
    use chromiumoxide::cdp::js_protocol::runtime::{
        EnableParams as RuntimeEnable, EventExecutionContextCreated,
    };
    use futures::StreamExt;
    let page_owned = page.clone();
    let work = async move {
        let page = page_owned;
        let tree = page
            .execute(GetFrameTreeParams::default())
            .await
            .map_err(|e| format!("frame tree: {e}"))?;
        let child_id = tree
            .result
            .frame_tree
            .child_frames
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|c| c.frame.id.as_ref().to_string())
            .next()
            .ok_or("no child iframe in frame tree")?;
        page.execute(RuntimeEnable::default())
            .await
            .map_err(|e| format!("runtime enable: {e}"))?;
        let mut listener = page
            .event_listener::<EventExecutionContextCreated>()
            .await
            .map_err(|e| format!("context listener: {e}"))?;
        let deadline = tokio::time::Instant::now() + timeout;
        while let Some(ev) = match tokio::time::timeout_at(deadline, listener.next()).await {
            Ok(item) => item,
            Err(_) => return Err("iframe context not observed".to_string()),
        } {
            if ev
                .context
                .aux_data
                .as_ref()
                .and_then(|a| a.get("frameId"))
                .and_then(|v| v.as_str())
                == Some(child_id.as_str())
            {
                let numeric = *ev.context.id.inner() as u64;
                return Ok((child_id.clone(), numeric));
            }
        }
        Err("context stream ended".to_string())
    };
    match block_on(work, timeout) {
        Ok(Ok(pair)) => Ok(pair),
        Ok(Err(msg)) => Err(CtlError::with_evidence(
            ErrorCode::NotFound,
            format!("no cross-origin iframe execution context found: {msg}"),
            serde_json::json!({ "hint": "iframe must be attached and its context created" }),
        )),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_on_returns_future_output() {
        let value = block_on(async { 21 * 2 }, Duration::from_secs(10)).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn block_on_timeout_maps_to_timeout_code() {
        let err = block_on(
            async {
                tokio::time::sleep(Duration::from_secs(30)).await;
            },
            Duration::from_millis(50),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Timeout);
    }

    #[test]
    fn panicking_task_maps_to_internal() {
        let err = block_on(
            async { panic!("deep-lane task panicked") },
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal);
    }

    #[test]
    fn connect_bad_url_maps_to_not_found() {
        // 离线安全:URL 解析在握手前失败,无网络往返
        let err = connect("not-a-ws-url", Duration::from_secs(5)).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }
}
