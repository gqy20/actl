//! headless_chrome probe: attach / selflaunch / interception demo.
//! Fully sync API — direct comparison baseline vs the async bridge.
//! RESULT lines are ASCII for evidence extraction.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use headless_chrome::browser::tab::{RequestInterceptor, RequestPausedDecision};
use headless_chrome::browser::LaunchOptions;
use headless_chrome::Browser;

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
                "usage: headless-chrome-probe <attach <ws_url> | selflaunch <exe> <profile_dir>>"
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
    let t_connect = Instant::now();
    // attach: close_on_drop=false inside connect — dropping must NOT kill actl's browser
    let browser = Browser::connect(ws_url.to_string()).map_err(|e| format!("connect: {e}"))?;
    println!(
        "RESULT attach_connect_ms={}",
        t_connect.elapsed().as_millis()
    );
    let version = browser.get_version().map_err(|e| format!("version: {e}"))?;
    let browser_field = serde_json::to_value(&version)
        .ok()
        .and_then(|v| v.get("Browser").and_then(|b| b.as_str().map(String::from)))
        .unwrap_or_default();
    println!("RESULT browser_version={:?}", browser_field);

    let tab = browser
        .wait_for_initial_tab()
        .map_err(|e| format!("initial tab: {e}"))?;
    demo_tab(&tab)?;

    drop(browser); // close_on_drop=false for connect — actl session must survive
    println!("RESULT attach=ok (drop sent no Browser.Close; actl owns browser)");
    Ok(())
}

fn run_selflaunch(exe: &str, profile_dir: &str) -> Result<(), String> {
    let t_launch = Instant::now();
    let mut builder = LaunchOptions::default_builder();
    builder
        .headless(true)
        .user_data_dir(Some(std::path::PathBuf::from(profile_dir)))
        .args(vec![
            std::ffi::OsStr::new("--no-first-run"),
            std::ffi::OsStr::new("--no-default-browser-check"),
        ]);
    if !exe.is_empty() {
        builder.path(Some(std::path::PathBuf::from(exe)));
    }
    let options = builder.build().map_err(|e| format!("options: {e}"))?;
    let browser = Browser::new(options).map_err(|e| format!("launch: {e}"))?;
    println!(
        "RESULT selflaunch_launch_ms={} pid={:?}",
        t_launch.elapsed().as_millis(),
        browser.get_process_id()
    );

    let tab = browser.new_tab().map_err(|e| format!("new_tab: {e}"))?;
    demo_tab(&tab)?;

    // close_on_drop=true for launched browsers: drop sends Browser.Close
    let t_close = Instant::now();
    drop(browser);
    std::thread::sleep(Duration::from_millis(1500));
    println!(
        "RESULT selflaunch_close_ms={} (drop -> Browser.Close, waited 1.5s)",
        t_close.elapsed().as_millis()
    );
    Ok(())
}

fn demo_tab(tab: &Arc<headless_chrome::Tab>) -> Result<(), String> {
    let count = Arc::new(AtomicUsize::new(0));
    let demo_url = std::env::var("PROBE_URL").unwrap_or_else(|_| DEMO_URL.to_string());

    // --- deep capability: request interception (continue-all) ---
    let counter = Arc::clone(&count);
    let interceptor: Arc<dyn RequestInterceptor + Send + Sync> = Arc::new(
        move |_transport, _session, _event| {
            counter.fetch_add(1, Ordering::SeqCst);
            RequestPausedDecision::Continue(None)
        },
    );
    tab.enable_request_interception(interceptor)
        .map_err(|e| format!("interception: {e}"))?;
    // Fetch.enable is NOT sent by enable_request_interception — explicit opt-in (hc semantics)
    tab.enable_fetch(None, None)
        .map_err(|e| format!("fetch enable: {e}"))?;

    let t_goto = Instant::now();
    tab.navigate_to(&demo_url)
        .map_err(|e| format!("navigate: {e}"))?;
    println!("RESULT goto_ms={}", t_goto.elapsed().as_millis());

    let t_eval = Instant::now();
    let robj = tab
        .evaluate("document.title", false)
        .map_err(|e| format!("evaluate: {e}"))?;
    let title = robj
        .value
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    println!(
        "RESULT evaluate_ms={} title={:?}",
        t_eval.elapsed().as_millis(),
        title
    );

    // drain interception callbacks (bounded)
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = 0usize;
    while Instant::now() < deadline {
        let c = count.load(Ordering::SeqCst);
        if c == last && c > 0 {
            break;
        }
        last = c;
        std::thread::sleep(Duration::from_millis(200));
    }
    println!(
        "RESULT intercepted_requests={}",
        count.load(Ordering::SeqCst)
    );
    Ok(())
}
