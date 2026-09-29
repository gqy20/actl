//! Transfer Windows foreground eligibility only to the authorized sibling CLI.
use actl_core::handoff::Request;
use windows::Win32::{Foundation::*, System::Threading::*, UI::WindowsAndMessaging::*};

fn eligible(
    pid: u32,
    request: &Request,
    actual: &std::path::Path,
    expected: &std::path::Path,
) -> bool {
    pid != 0
        && pid != u32::MAX
        && request.call.starts_with(&format!("{pid}-"))
        && (actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy())
            || actual
                .canonicalize()
                .ok()
                .zip(expected.canonicalize().ok())
                .is_some_and(|(a, b)| a.as_os_str().eq_ignore_ascii_case(b.as_os_str())))
}

pub fn transfer(pid: u32, request: &Request) {
    let result = transfer_checked(pid, request);
    let paths = actl_core::state::SignalPaths::default();
    let call = format!("foreground-{}", actl_core::snapshot::new_snapshot_id());
    if let Ok(mut journal) = actl_core::history::Journal::open_without_retention(
        &paths.dir,
        &call,
        Some(request.task.clone()),
    ) {
        journal.record(
            "call_started",
            serde_json::json!({"command":"foreground-transfer"}),
        );
        journal.record(
            "foreground_transfer",
            serde_json::json!({
                "request_call":request.call, "caller_pid":pid,
                "accepted":result.is_ok(), "failure":result.err(),
            }),
        );
        journal.record("call_finished", serde_json::json!({"phase":"completed"}));
    }
}

fn transfer_checked(pid: u32, request: &Request) -> Result<(), String> {
    if pid == 0 || pid == u32::MAX || !request.fresh(actl_core::state::unix_ms()) {
        return Err("invalid_or_expired_caller".into());
    }
    let expected = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("actl.exe");
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    // Keep the process object alive through validation and delegation to avoid PID reuse.
    let process = Process(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map_err(|e| e.to_string())?,
    );
    let mut path = vec![0u16; 32768];
    let mut size = path.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(path.as_mut_ptr()),
            &mut size,
        )
    }
    .map_err(|e| e.to_string())?;
    let actual = String::from_utf16(&path[..size as usize]).map_err(|e| e.to_string())?;
    if !eligible(pid, request, std::path::Path::new(&actual), &expected)
        || unsafe { WaitForSingleObject(process.0, 0) } != WAIT_TIMEOUT
    {
        return Err("caller_identity_mismatch_or_exited".into());
    }
    // This is a request to Windows, not a replacement for consent or focus checks.
    unsafe { AllowSetForegroundWindow(pid) }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexical_alias_of_the_same_executable_is_eligible() {
        let path = std::env::current_exe().unwrap();
        let alias = path
            .parent()
            .unwrap()
            .join(".")
            .join(path.file_name().unwrap());
        let request = Request {
            approval: Default::default(),
            generation: "g".into(),
            call: "42-call".into(),
            task: "t".into(),
            target: "browser".into(),
            ts_ms: 0,
            action: String::new(),
        };
        assert!(eligible(42, &request, &alias, &path));
    }
    #[test]
    fn delegation_is_bound_to_current_cli_not_all_processes() {
        let mut request = Request {
            approval: Default::default(),
            generation: "g".into(),
            call: "42-call".into(),
            task: "t".into(),
            target: "browser".into(),
            ts_ms: 0,
            action: String::new(),
        };
        let path = std::path::Path::new("C:/actl/actl.exe");
        assert!(eligible(42, &request, path, path));
        assert!(!eligible(u32::MAX, &request, path, path));
        assert!(!eligible(43, &request, path, path));
        assert!(!eligible(
            42,
            &request,
            std::path::Path::new("C:/other/actl.exe"),
            path
        ));
        request.call = "43-next-call".into();
        assert!(eligible(43, &request, path, path));
    }
}
