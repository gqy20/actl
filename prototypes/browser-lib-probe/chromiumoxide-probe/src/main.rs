//! chromiumoxide probe: attach / selflaunch / interception demo.
//! Sync main + hosted tokio runtime = sync-facade bridge validation.
//! RESULT lines are ASCII for evidence extraction.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chromiumoxide::browser::BrowserConfig;
use chromiumoxide::cdp::browser_protocol::fetch;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;

const DEMO_URL: &str = "https://example.com";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let t0 = Instant::now();
    let res = match mode {
        "attach" => run_attach(args.get(2).map(String::as_str).unwrap_or("")),
        "selflaunch" => run_selflaunch(
            args.get(2).map(String::as_str).unwrap_or(""),
            args.get(3).map(String::as_str).unwrap_or(""),
        ),
        _ => {
            eprintln!(
                "usage: chromiumoxide-probe <attach <ws_url> | selflaunch <exe> <profile_dir>>"
            );
            std::process::exit(2);
        }
    };
    match res {
        Ok(()) => println!("RESULT mode={} overall_ms={}", mode, t0.elapsed().as_millis()),
        Err(e) => {
            println!("RESULT mode={} error={}", mode, e);
            std::process::exit(1);
        }
    }
}

fn run_attach(ws_url: &str) -> Result<(), String> {
    // --- sync facade over hosted runtime (the bridge pattern under test) ---
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    let t_connect = Instant::now();
    let browser = rt
        .block_on(async {
            tokio::time::timeout(Duration::from_secs(45), async {
                let (browser, mut handler) = Browser::connect(ws_url)
                    .await
                    .map_err(|e| format!("connect: {e}"))?;
                // handler must be polled or no events flow — hosted task
                let _pump = tokio::spawn(async move {
                    while let Some(h) = handler.next().await {
                        if h.is_err() {
                            break;
                        }
                    }
                });
                Ok::<Browser, String>(browser)
            })
            .await
            .map_err(|e| format!("timeout: {e:?}"))?
        })
        .map_err(|e: String| e)?;
    println!(
        "RESULT attach_connect_ms={}",
        t_connect.elapsed().as_millis()
    );

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(45), async { demo_page(&browser).await })
            .await
            .map_err(|e| format!("timeout: {e:?}"))?
    })
    .map_err(|e: String| e)?;
    // NO browser.close(): actl owns the session; drop must not kill it.
    drop(browser);
    println!("RESULT attach=ok (no close sent; actl owns browser)");
    Ok(())
}

fn run_selflaunch(exe: &str, profile_dir: &str) -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
    let t_launch = Instant::now();
    let mut browser = rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(60), async {
            let mut config = BrowserConfig::builder()
                .new_headless_mode()
                .user_data_dir(profile_dir)
                .arg("--no-first-run")
                .arg("--no-default-browser-check");
            if !exe.is_empty() {
                config = config.chrome_executable(exe);
            }
            let config = config.build().map_err(|e| format!("config: {e}"))?;
            let (browser, mut handler) = Browser::launch(config)
                .await
                .map_err(|e| format!("launch: {e}"))?;
            let _pump = tokio::spawn(async move {
                while let Some(h) = handler.next().await {
                    if h.is_err() {
                        break;
                    }
                }
            });
            Ok::<Browser, String>(browser)
        })
        .await
        .map_err(|e| format!("timeout: {e:?}"))?
    })?;
    println!(
        "RESULT selflaunch_launch_ms={}",
        t_launch.elapsed().as_millis()
    );

    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(45), async { demo_page(&browser).await })
            .await
            .map_err(|e| format!("timeout: {e:?}"))?
    })
    .map_err(|e: String| e)?;

    let t_close = Instant::now();
    rt.block_on(async {
        tokio::time::timeout(Duration::from_secs(30), async {
            browser.close().await.map_err(|e| format!("close: {e}"))
        })
        .await
        .map_err(|e| format!("close timeout: {e:?}"))?
    })
    .map_err(|e: String| e)?;
    println!(
        "RESULT selflaunch_close_ms={} (Browser.close via CDP)",
        t_close.elapsed().as_millis()
    );
    Ok(())
}

async fn demo_page(browser: &Browser) -> Result<(), String> {
    let count = Arc::new(AtomicUsize::new(0));
    let demo_url = std::env::var("PROBE_URL").unwrap_or_else(|_| DEMO_URL.to_string());

    let t_newpage = Instant::now();
    let page: Page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| format!("new_page: {e}"))?;
    println!("RESULT newpage_ms={}", t_newpage.elapsed().as_millis());

    // --- deep capability: Fetch-domain request interception (continue-all) ---
    let pattern = fetch::RequestPattern::builder().url_pattern("*").build();
    let enable = fetch::EnableParams::builder().pattern(pattern).build();
    page.execute(enable)
        .await
        .map_err(|e| format!("fetch enable: {e}"))?;

    let mut listener = page
        .event_listener::<fetch::EventRequestPaused>()
        .await
        .map_err(|e| format!("listener: {e}"))?;
    let count_task = Arc::clone(&count);
    let page_for_continue = page.clone();
    let _interceptor = tokio::spawn(async move {
        while let Some(ev) = listener.next().await {
            count_task.fetch_add(1, Ordering::SeqCst);
            let req_id = ev.request_id.clone();
            let _ = page_for_continue
                .execute(fetch::ContinueRequestParams {
                    request_id: req_id,
                    url: None,
                    method: None,
                    post_data: None,
                    headers: None,
                    intercept_response: None,
                })
                .await;
        }
    });

    let t_goto = Instant::now();
    page.goto(demo_url.as_str())
        .await
        .map_err(|e| format!("goto: {e}"))?;
    println!("RESULT goto_ms={}", t_goto.elapsed().as_millis());

    let t_eval = Instant::now();
    let title: String = page
        .evaluate("document.title")
        .await
        .map_err(|e| format!("evaluate: {e}"))?
        .into_value()
        .map_err(|e| format!("title value: {e}"))?;
    println!(
        "RESULT evaluate_ms={} title={:?}",
        t_eval.elapsed().as_millis(),
        title
    );

    // let interception events drain (bounded)
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = 0usize;
    while Instant::now() < deadline {
        let c = count.load(Ordering::SeqCst);
        if c == last && c > 0 {
            break;
        }
        last = c;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!(
        "RESULT intercepted_requests={}",
        count.load(Ordering::SeqCst)
    );
    Ok(())
}
