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
