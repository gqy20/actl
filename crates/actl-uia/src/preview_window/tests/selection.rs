//! Real CLI and snapshot refs on owned buttons; posted selection is not desktop consent.
use super::*;
use serde_json::{Value, json};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
enum Mode {
    Select,
    Dismiss,
    Expire,
    Move,
    Stop,
    Manual,
}

#[test]
#[ignore = "interactive desktop; checks only owned selection-window focus policy"]
fn choice_dialog_accepts_explicit_user_focus_without_activating_on_open() {
    let foreground = unsafe { GetForegroundWindow() };
    let window = Window::open_choice(
        [100, 200, 400, 250],
        "Choice focus test",
        "Select only",
        0,
        2,
    )
    .unwrap();
    assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    assert_eq!(
        unsafe { GetWindowLongPtrW(window.notice, GWL_EXSTYLE) } as u32 & WS_EX_NOACTIVATE.0,
        0
    );
    assert_eq!(
        unsafe { SendMessageW(window.notice, WM_MOUSEACTIVATE, None, None) }.0,
        MA_ACTIVATE as isize
    );
}

fn run(mode: Mode) {
    assert!(!actl_core::state::SignalPaths::default().stop_requested());
    let host = OwnedHost::with_choices(true);
    let foreground = unsafe { GetForegroundWindow() };
    let binary = std::env::var("ACTL_PREVIEW_CLI").expect("ACTL_PREVIEW_CLI");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/preview-choice")
        .join(format!(
            "{}-{mode:?}",
            actl_core::snapshot::new_snapshot_id()
        ));
    std::fs::create_dir_all(&root).unwrap();
    let app = format!("hwnd:{}", host.hwnd);
    let out = Command::new(&binary)
        .env("LOCALAPPDATA", &root)
        .args(["snapshot", "--app", &app])
        .output()
        .unwrap();
    std::fs::write(root.join("snapshot.json"), &out.stdout).unwrap();
    let snapshot: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(snapshot["ok"], true, "{snapshot}");
    let elements = snapshot["data"]["elements"].as_array().unwrap();
    let reference = |name: &str| {
        elements.iter().find(|e| e["name"] == name).unwrap()["ref"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let first = reference("Option A");
    let second = reference("Option B");
    let snapshot_id = snapshot["snapshot_id"].as_str().unwrap();
    let duration = if matches!(mode, Mode::Manual) {
        "30000"
    } else {
        "4000"
    };
    let mut child = Command::new(&binary)
        .env("LOCALAPPDATA", &root)
        .env_remove("ACTL_TASK_ID")
        .args([
            "preview",
            &first,
            "--candidate",
            &second,
            "--snapshot",
            snapshot_id,
            "--app",
            &app,
            "--message",
            "Select Option B; test only",
            "--duration",
            duration,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let until = Instant::now() + Duration::from_secs(8);
    let mut saw_first = false;
    let mut saw_second = false;
    loop {
        let windows = super::lifecycle::previews(pid);
        if child.try_wait().unwrap().is_some() || Instant::now() >= until {
            break;
        }
        for notice in windows {
            unsafe {
                let mut title = [0; 512];
                let len = GetWindowTextW(notice, &mut title);
                let title = String::from_utf16_lossy(&title[..len.max(0) as usize]);
                if title.contains("1/2") && !saw_first {
                    saw_first = true;
                    match mode {
                        Mode::Select | Mode::Move => {
                            PostMessageW(Some(notice), WM_COMMAND, WPARAM(2), LPARAM(0)).unwrap();
                        }
                        Mode::Dismiss => {
                            PostMessageW(Some(notice), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
                        }
                        Mode::Stop => {
                            actl_core::state::SignalPaths::at(root.join("actl"))
                                .request_stop()
                                .unwrap();
                        }
                        _ => {}
                    }
                }
                if title.contains("2/2") && !saw_second {
                    saw_second = true;
                    if matches!(mode, Mode::Move) {
                        let button = GetDlgItem(Some(HWND(host.hwnd as *mut _)), 11).unwrap();
                        SetWindowPos(
                            button,
                            None,
                            220,
                            40,
                            0,
                            0,
                            SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
                        )
                        .unwrap();
                    }
                    if matches!(mode, Mode::Select | Mode::Move) {
                        PostMessageW(Some(notice), WM_COMMAND, WPARAM(3), LPARAM(0)).unwrap();
                    }
                }
            }
        }
        if matches!(mode, Mode::Manual) && saw_first {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = child.wait_with_output().unwrap();
    std::fs::write(root.join("result.json"), &out.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &out.stderr).unwrap();
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    let windows = super::lifecycle::previews(pid);
    let unchanged_focus = unsafe { GetForegroundWindow() } == foreground;
    let host_handle = HWND(host.hwnd as *mut _);
    drop(host);
    let cleaned = !unsafe { IsWindow(Some(host_handle)).as_bool() };
    std::fs::write(root.join("evidence.json"), serde_json::to_vec_pretty(&json!({
        "binary":binary,"pid":pid,"host":host_handle.0 as usize,"mode":format!("{mode:?}"),
        "saw_first":saw_first,"saw_second":saw_second,"foreground_preserved":unchanged_focus,
        "preview_cleanup":windows.is_empty(),"host_cleanup":cleaned,"exit_code":out.status.code()
    })).unwrap()).unwrap();
    assert!(windows.is_empty() && cleaned);
    assert!(saw_first, "{result}");
    if !matches!(mode, Mode::Manual) {
        assert!(unchanged_focus);
    }
    match mode {
        Mode::Select | Mode::Manual => {
            assert_eq!(result["data"]["exit_reason"], "selected", "{result}");
            assert_eq!(result["data"]["selection"]["target"], second);
            assert_eq!(result["data"]["selection"]["identity"]["name"], "Option B");
            assert_eq!(result["data"]["executed"], false);
            assert_eq!(result["data"]["snapshot_id"], snapshot_id);
        }
        Mode::Move => {
            assert_eq!(result["error"]["code"], "STALE_REF", "{result}");
        }
        Mode::Stop => {
            assert_eq!(result["error"]["code"], "ABORTED", "{result}");
        }
        Mode::Expire | Mode::Dismiss => {
            assert_eq!(result["data"]["selected"], false, "{result}");
            assert!(result["data"]["selection"].is_null());
        }
    }
}

#[test]
#[ignore = "interactive desktop, real CLI, owned buttons only"]
fn candidate_switch_and_select() {
    run(Mode::Select);
}
#[test]
#[ignore = "interactive desktop, real CLI, owned buttons only"]
fn candidate_close_returns_no_selection() {
    run(Mode::Dismiss);
}
#[test]
#[ignore = "interactive desktop, real CLI, owned buttons only"]
fn candidate_timeout_returns_no_selection() {
    run(Mode::Expire);
}
#[test]
#[ignore = "interactive desktop, real CLI, owned buttons only"]
fn candidate_movement_rejects_confirmation() {
    run(Mode::Move);
}
#[test]
#[ignore = "interactive desktop, real CLI, isolated stop state"]
fn candidate_stop_cancels_selection() {
    run(Mode::Stop);
}
#[test]
#[ignore = "manual or computer-use click of Next and Select within 30s; not consent"]
fn mouse_selection_acceptance() {
    run(Mode::Manual);
}
