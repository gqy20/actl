//! Interactive release-CLI checks on owned native windows; never inject input.
use super::*;
use std::{
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
enum Change {
    Move,
    Rename,
    Hide,
    Destroy,
    Cover,
    Stop,
    Rebuild,
    Dismiss,
    AlreadyStopped,
}

// EnumWindows is synchronous; the caller retains this vector throughout the call.
unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    unsafe { &mut *(data.0 as *mut Vec<HWND>) }.push(hwnd);
    windows::core::BOOL(1)
}

fn previews(pid: u32) -> Vec<HWND> {
    let mut windows = Vec::<HWND>::new();
    unsafe {
        EnumWindows(
            Some(collect),
            LPARAM((&mut windows as *mut Vec<HWND>) as isize),
        )
    }
    .unwrap();
    windows
        .into_iter()
        .filter(|hwnd| unsafe {
            let mut actual = 0;
            GetWindowThreadProcessId(*hwnd, Some(&mut actual));
            let mut class = [0; 128];
            let len = GetClassNameW(*hwnd, &mut class);
            actual == pid
                && String::from_utf16_lossy(&class[..len.max(0) as usize]) == "actl_target_preview"
        })
        .collect()
}

fn run(change: Change, expected: &[&str]) {
    assert!(
        !actl_core::state::SignalPaths::default().stop_requested(),
        "existing stop: preserve and report blocked"
    );
    let _dpi = crate::capture::Pmv2Guard::enter();
    let foreground = unsafe { GetForegroundWindow() };
    let host = OwnedHost::start();
    let hwnd = HWND(host.hwnd as *mut _);
    let binary = std::env::var("ACTL_PREVIEW_CLI").expect("set ACTL_PREVIEW_CLI");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/preview-lifecycle")
        .join(format!(
            "{}-{}-{change:?}",
            std::process::id(),
            actl_core::state::unix_ms()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let paths = actl_core::state::SignalPaths::at(root.join("state/actl"));
    if matches!(change, Change::AlreadyStopped) {
        paths.request_stop().unwrap();
    }
    let args = [
        "preview".to_string(),
        r#"uia:{"match":{"name":"ACTL isolated preview target"}}"#.into(),
        "--app".into(),
        format!("hwnd:{}", host.hwnd),
        "--duration".into(),
        "3000".into(),
    ];
    let mut child = std::process::Command::new(&binary)
        .args(&args)
        .env("LOCALAPPDATA", root.join("state"))
        .env_remove("ACTL_TASK_ID")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(10);
    let handles = loop {
        let handles = previews(pid);
        if handles.len() == 2
            && handles
                .iter()
                .all(|h| unsafe { IsWindowVisible(*h) }.as_bool())
        {
            break handles;
        }
        if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
            break vec![];
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Mutate only after this invocation owns two visible preview windows.
    let mut cover = None;
    if handles.len() == 2 {
        assert!(
            !actl_core::state::SignalPaths::default().stop_requested(),
            "external stop during test"
        );
        unsafe {
            match change {
                Change::Move => SetWindowPos(
                    hwnd,
                    None,
                    180,
                    210,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                )
                .unwrap(),
                Change::Rename => SetWindowTextW(hwnd, w!("ACTL changed preview target")).unwrap(),
                Change::Hide => {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                Change::Destroy => {
                    PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap()
                }
                Change::Rebuild => {
                    PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                    let until = Instant::now() + Duration::from_secs(2);
                    while IsWindow(Some(hwnd)).as_bool() && Instant::now() < until {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    assert!(
                        !IsWindow(Some(hwnd)).as_bool(),
                        "owned target did not close"
                    );
                    cover = Some(OwnedHost::start());
                }
                Change::Dismiss => {
                    let notice = handles
                        .iter()
                        .find(|h| GetWindowTextLengthW(**h) > 0)
                        .unwrap();
                    PostMessageW(Some(*notice), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                }
                Change::Cover => cover = Some(OwnedHost::start()),
                Change::AlreadyStopped => {}
                Change::Stop => paths
                    .request_stop_from("isolated_preview_test", None)
                    .unwrap(),
            }
        }
    }
    let output = child.wait_with_output().unwrap();
    std::fs::write(root.join("stdout.json"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let remaining = previews(pid);
    let foreground_unchanged = unsafe { GetForegroundWindow() } == foreground;
    let cover_hwnd = cover.as_ref().map(|c| HWND(c.hwnd as *mut _));
    drop(cover);
    drop(host);
    let host_closed = !unsafe { IsWindow(Some(hwnd)) }.as_bool();
    let cover_closed = cover_hwnd.is_none_or(|h| !unsafe { IsWindow(Some(h)) }.as_bool());
    std::fs::write(root.join("evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "binary":binary,"args":args,"cli_pid":pid,"case":format!("{change:?}"),
        "target_hwnd":hwnd.0 as usize,"preview_hwnds":handles.iter().map(|h|h.0 as usize).collect::<Vec<_>>(),
        "exit_code":output.status.code(),"remaining_previews":remaining.len(),
        "foreground_unchanged":foreground_unchanged,"host_closed":host_closed,"cover_closed":cover_closed,
        "isolated_stop_retained":paths.stop_requested()
    })).unwrap()).unwrap();
    assert_eq!(
        handles.len(),
        2,
        "preview not ready: {value}; evidence {}",
        root.display()
    );
    if matches!(change, Change::Dismiss | Change::AlreadyStopped) {
        assert!(output.status.success(), "{value}");
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["executed"], false);
        assert_eq!(
            value["data"]["exit_reason"],
            if matches!(change, Change::Dismiss) {
                "dismissed"
            } else {
                "expired"
            }
        );
    } else {
        assert!(!output.status.success(), "{value}");
        assert_eq!(value["ok"], false);
        assert!(
            expected.contains(&value["error"]["code"].as_str().unwrap()),
            "{value}"
        );
        if value["error"]["code"] == "INTERNAL" {
            // A disappearing provider can fail any property read. Preserve the
            // native failure, rather than require an invented stale classification.
            assert!(matches!(change, Change::Destroy | Change::Rebuild));
            let evidence = &value["error"]["evidence"];
            assert_eq!(evidence["reason"], "observation_failed");
            assert!(evidence["native_code"].as_i64().is_some_and(|n| n < 0));
            let channel = evidence["channel"].as_str().unwrap();
            assert!(channel.starts_with("identity.") || channel.starts_with("preview."));
        }
    }
    assert!(remaining.is_empty(), "preview leaked: {}", root.display());
    assert!(
        foreground_unchanged && host_closed && cover_closed,
        "cleanup/focus: {}",
        root.display()
    );
    if matches!(change, Change::Stop | Change::AlreadyStopped) {
        assert!(paths.stop_requested(), "must not clear stop");
    }
}

#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn moving_target_removes_preview() {
    run(Change::Move, &["STALE_REF"]);
}
#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn renamed_target_removes_preview() {
    run(Change::Rename, &["STALE_REF"]);
}
#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn hidden_target_removes_preview() {
    run(Change::Hide, &["NOT_ACTIONABLE"]);
}
#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn destroyed_target_removes_preview() {
    run(Change::Destroy, &["STALE_REF", "INTERNAL"]);
}
#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn covered_target_removes_preview() {
    run(Change::Cover, &["NOT_ACTIONABLE"]);
}
#[test]
#[ignore = "interactive desktop; isolated stop state; requires ACTL_PREVIEW_CLI"]
fn stop_removes_preview_and_preserves_marker() {
    run(Change::Stop, &["ABORTED"]);
}

#[test]
#[ignore = "interactive desktop; owned windows; requires ACTL_PREVIEW_CLI"]
fn rebuilt_target_does_not_inherit_old_preview() {
    run(Change::Rebuild, &["STALE_REF", "INTERNAL"]);
}

#[test]
#[ignore = "interactive desktop; WM_CLOSE message, not a physical mouse test"]
fn closing_notice_returns_dismissed_without_execution() {
    run(Change::Dismiss, &[]);
}

#[test]
#[ignore = "interactive desktop; historical stop must not block a new read-only preview"]
fn historical_stop_allows_new_preview_without_clearing_marker() {
    run(Change::AlreadyStopped, &[]);
}
