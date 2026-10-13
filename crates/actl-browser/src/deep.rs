//! 深水层动词:chromiumoxide(同步门面 bridge 之后)驱动的协议级观察面。
//! 浅层动词(cdp.rs 手写客户端)不动——双栈过渡态,终态吸收与否由 ROADMAP 裁定。

use std::time::Duration;

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};

use crate::bridge;
use crate::session::{self, Paths};

/// 有界网络采集(请求清单快照)。窗口、条数、过滤器全部有界——
/// 与 envelope 的请求/响应形态对齐,不引入流式输出语义。
pub struct NetlogSpec<'a> {
    pub session_id: &'a str,
    /// URL 子串过滤(可选,1..=2048 字节)
    pub filter: Option<&'a str>,
    /// 窗口时长 ms(默认 5000,范围 100..=60000)
    pub window_ms: Option<u64>,
    /// 条数上限(默认 500,最大 2000;触顶即停,truncated:true)
    pub max_entries: Option<u64>,
}

const NETLOG_WINDOW_DEFAULT_MS: u64 = 5_000;
const NETLOG_WINDOW_MIN_MS: u64 = 100;
const NETLOG_WINDOW_MAX_MS: u64 = 60_000;
const NETLOG_ENTRIES_DEFAULT: u64 = 500;
const NETLOG_ENTRIES_MAX: u64 = 2_000;
/// Network.enable 送达与首个事件到达之间的订阅沉降(事件早于 enable 确认会静默丢失)
const NETLOG_SETTLE_MS: u64 = 300;
/// target 发现竞态上限:attach 后 targetCreated 事件异步回流,
/// pages() 立即调用可能空列表(spike 未暴露:new_page 显式等待自身 target)
const DISCOVERY_SETTLE_MAX_MS: u64 = 3_000;
const DISCOVERY_POLL_MS: u64 = 100;

pub fn netlog(paths: &Paths, spec: NetlogSpec<'_>) -> Result<Value, CtlError> {
    // 参数校验先于会话 load(与浅层动词同序:坏参数不必触达会话文件)
    let window = match spec.window_ms {
        None => NETLOG_WINDOW_DEFAULT_MS,
        Some(v) if (NETLOG_WINDOW_MIN_MS..=NETLOG_WINDOW_MAX_MS).contains(&v) => v,
        Some(v) => {
            return Err(CtlError::protocol(format!(
                "window must be {NETLOG_WINDOW_MIN_MS}..={NETLOG_WINDOW_MAX_MS} ms, got {v}"
            )));
        }
    };
    let max_entries = match spec.max_entries {
        None => NETLOG_ENTRIES_DEFAULT,
        Some(v) if (1..=NETLOG_ENTRIES_MAX).contains(&v) => v,
        Some(v) => {
            return Err(CtlError::protocol(format!(
                "max-entries must be 1..={NETLOG_ENTRIES_MAX}, got {v}"
            )));
        }
    };
    if let Some(f) = spec.filter
        && (f.is_empty() || f.len() > 2048)
    {
        return Err(CtlError::protocol("filter must be 1..=2048 bytes"));
    }
    let session = session::load(paths, spec.session_id)?;

    let ws_url = format!(
        "ws://127.0.0.1:{}{}",
        session.devtools_port, session.browser_ws_path
    );
    let browser = bridge::connect(&ws_url, Duration::from_secs(10))?;
    let target_id = session.target_id.clone();
    let filter = spec.filter.map(str::to_string);

    let work = async move {
        use chromiumoxide::cdp::browser_protocol::network;
        use futures::StreamExt;

        // 有界轮询等 target 发现落位(竞态防护,见 DISCOVERY_SETTLE_MAX_MS 注释)
        let discover_deadline =
            tokio::time::Instant::now() + Duration::from_millis(DISCOVERY_SETTLE_MAX_MS);
        let page = loop {
            let pages = browser
                .pages()
                .await
                .map_err(|e| format!("enumerate pages: {e}"))?;
            if let Some(page) = pages
                .into_iter()
                .find(|p| p.target_id().as_ref() == target_id)
            {
                break page;
            }
            if tokio::time::Instant::now() >= discover_deadline {
                return Err("bound target not among live pages".to_string());
            }
            tokio::time::sleep(Duration::from_millis(DISCOVERY_POLL_MS)).await;
        };

        page.execute(network::EnableParams::default())
            .await
            .map_err(|e| format!("network enable: {e}"))?;
        // 订阅沉降:事件监听建立早于浏览器侧的 enable 生效会静默丢首拍
        tokio::time::sleep(Duration::from_millis(NETLOG_SETTLE_MS)).await;

        let mut listener = page
            .event_listener::<network::EventRequestWillBeSent>()
            .await
            .map_err(|e| format!("network listener: {e}"))?;

        let deadline = tokio::time::Instant::now() + Duration::from_millis(window);
        let mut collected: Vec<Value> = Vec::new();
        let mut truncated = false;
        loop {
            let next = tokio::time::timeout_at(deadline, listener.next()).await;
            let Ok(Some(ev)) = next else { break };
            let url = ev.request.url.as_str();
            if let Some(f) = &filter
                && !url.contains(f.as_str())
            {
                continue;
            }
            if collected.len() as u64 >= max_entries {
                truncated = true;
                break;
            }
            collected.push(json!({
                "url": url,
                "method": ev.request.method,
                "resource_type": ev.r#type.as_ref().map(|t| t.as_ref().to_string()),
            }));
        }
        let _ = page.execute(network::DisableParams::default()).await;
        Ok::<(Vec<Value>, bool), String>((collected, truncated))
    };
    // 窗口 + 余量(attach/enable/沉降/收尾),单次 block_on 覆盖全程
    let budget = Duration::from_millis(window + 15_000);
    let (requests, truncated) = bridge::block_on(work, budget).and_then(|inner| {
        inner.map_err(|msg| {
            // 目标不在场→会话失效(与浅层 verify_target_alive 同判);其余按内部错误
            if msg.contains("bound target not among live pages") {
                CtlError::with_evidence(
                    ErrorCode::NotFound,
                    "bound target no longer exists (new tab took over or browser \
                         restarted); re-run browser-navigate or browser-launch"
                        .to_string(),
                    json!({ "target_id": session.target_id }),
                )
            } else {
                CtlError::internal(format!("deep-lane netlog: {msg}"))
            }
        })
    })?;

    Ok(json!({
        "session_id": spec.session_id,
        "window_ms": window,
        "filter": spec.filter,
        "total": requests.len(),
        "truncated": truncated,
        "requests": requests,
    }))
}
