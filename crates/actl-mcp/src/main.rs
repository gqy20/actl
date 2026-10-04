//! actl-mcp 入口:模式 A(LAN 直连,docs/82 §3.1)。
//!
//! 默认绑定 127.0.0.1(fail-safe:不意外暴露;规范 SHOULD)——LAN 使用
//! 显式 `--listen 0.0.0.0:7600`(首次触发 Windows 防火墙确认,属预期)。
//! `--token` 缺省时生成 256-bit 随机 token 并打印配对横幅(stderr——
//! stdout 不承载,保持机器通道纯净):人工抄到控制端 = 设备主人知情
//! 授权的配对仪式。明文 HTTP 仅限可信局域网(跨公网不支持,docs/82)。
//! Bearer 与 Origin 校验在 axum 壳层(规范 MUST:防 DNS rebinding)。

use actl_mcp::dispatch::resolve_actl_exe;
use actl_mcp::tools::Adapter;
use axum::http::{HeaderName, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use clap::Parser;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use std::sync::Arc;

#[derive(Parser)]
#[allow(clippy::doc_markdown)]
#[command(
    name = "actl-mcp",
    about = "actl MCP adapter: remote access service for the controlled device (docs/82)",
    version,
    after_help = "Example:\n  actl-mcp --listen 127.0.0.1:7600\n  actl-mcp --listen 0.0.0.0:7600            # LAN: first run triggers the Windows firewall prompt\n  actl-mcp --listen 0.0.0.0:7600 --token pre-shared\nMode A (docs/82): plaintext HTTP + Bearer token, trusted LAN only — never across untrusted networks. The pairing token prints to stderr on startup when --token is omitted (copy it to the controlling device). Every remote call spawns a real actl process: the local signal bar, action previews, operator approval, emergency stop and the watchdog stay authoritative here. Remote calls of local-only verbs (execution-recover, display-start, desktop-restore, record-worker) are rejected with an explicit error.\nErrors: PROTOCOL bad arguments | INTERNAL spawn failure"
)]
struct Args {
    /// Bind address, e.g. 127.0.0.1:7600 (default) or 0.0.0.0:7600 for LAN
    #[arg(long, default_value = "127.0.0.1:7600")]
    listen: String,
    /// Pre-shared pairing token; omit to generate one and print it to stderr
    #[arg(long)]
    token: Option<String>,
}

fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    // 失败即退化为时间熵并如实提示(仅影响唯一性,不影响机密性的场景不存在
    // ——token 本就依赖 getrandom 的强度;生成失败时宁可中止)。
    if getrandom::fill(&mut bytes).is_err() {
        eprintln!("INTERNAL: system entropy unavailable; refusing to start with a weak token");
        std::process::exit(1);
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn banner(listen: &str, token: &str) {
    let host = if listen.starts_with("0.0.0.0") || listen.starts_with("::") {
        "<this-device-address>"
    } else {
        listen
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or("127.0.0.1")
    };
    eprintln!("actl-mcp listening on http://{listen}/mcp");
    eprintln!("pairing token (copy to the controlling device):");
    eprintln!("  {token}");
    eprintln!("client config (.mcp.json):");
    eprintln!(
        "  {{\"mcpServers\":{{\"actl\":{{\"url\":\"http://{host}:{}/mcp\",\"headers\":{{\"Authorization\":\"Bearer {token}\"}}}}}}}}",
        listen
            .rsplit_once(':')
            .map(|(_, port)| port)
            .unwrap_or("7600")
    );
    eprintln!(
        "mode A: plaintext HTTP — trusted LAN only, never across untrusted networks (docs/82)"
    );
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let exe = resolve_actl_exe();
    if !exe.exists() {
        eprintln!(
            "PROTOCOL: actl executable not found (looked at {exe:?}); \
             install actl beside actl-mcp or set ACTL_EXE"
        );
        std::process::exit(2);
    }
    let token = args.token.clone().unwrap_or_else(generate_token);
    banner(&args.listen, &token);

    let token = Arc::new(token);
    let service = StreamableHttpService::new(
        move || Ok(Adapter::new(exe.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    );
    let router =
        axum::Router::new()
            .nest_service("/mcp", service)
            .layer(axum::middleware::from_fn(move |request, next| {
                let token = token.clone();
                async move { authorize(request, next, &token).await }
            }));

    let listener = match tokio::net::TcpListener::bind(&args.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "PROTOCOL: cannot bind {listen}: {error}",
                listen = args.listen
            );
            std::process::exit(2);
        }
    };
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("actl-mcp shutting down");
    };
    if let Err(error) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
    {
        eprintln!("INTERNAL: http server failed: {error}");
        std::process::exit(1);
    }
}

/// Bearer + Origin 校验(规范 MUST:防 DNS rebinding;缺 Origin 的非浏览器
/// 客户端放行——token 已门禁)。拒绝体按规范可带无 id 的 JSON-RPC error。
async fn authorize(request: axum::extract::Request, next: Next, token: &str) -> Response {
    let expected = format!("Bearer {token}");
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == expected);
    if !authorized {
        return json_rpc_rejection(
            StatusCode::UNAUTHORIZED,
            "missing or invalid Bearer token (pair via the token printed on the controlled device)",
        );
    }
    let origin_ok = request
        .headers()
        .get(HeaderName::from_static("origin"))
        .and_then(|value| value.to_str().ok())
        .is_none_or(|origin| {
            // 浏览器同源探测:仅放行本机开发形态;API 客户端无 Origin。
            origin.starts_with("http://localhost") || origin.starts_with("http://127.0.0.1")
        });
    if !origin_ok {
        return json_rpc_rejection(StatusCode::FORBIDDEN, "cross-origin request rejected");
    }
    next.run(request).await
}

fn json_rpc_rejection(status: StatusCode, message: &str) -> Response {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "error": {"code": -32000, "message": message},
    });
    (status, axum::Json(body)).into_response()
}
