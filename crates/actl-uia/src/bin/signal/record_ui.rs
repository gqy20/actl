//! GUI 录制会话的显示端入口(docs/79 §8 触发面 R4):右键菜单动态项。
//! 发现:扫描 `records/<id>/state.json`(status==recording 且 pid 存活——
//! 托盘 tick 轮询,目录秒级扫描开销可忽略;单活跃会话不变式由 CLI 维护)。
//! 执行:spawn `actl record-start/record-stop`(flow_ui::execute 同款
//! CREATE_NO_WINDOW + envelope 解析;后台线程——record-stop 等终态 ≤15s,
//! 不能阻塞信号条窗口线程)。反馈:MESSAGE 经 tick 进 details 行
//! (flow_ui::message 同模式)。菜单点击发生在 actl-signal 窗口上,录制
//! trace 的 signal pid 过滤会排除它——"点菜单开始录制"不进 trace 本身。
use std::sync::Mutex;
use std::sync::mpsc;

static JOBS: Mutex<Vec<mpsc::Receiver<Result<String, String>>>> = Mutex::new(Vec::new());
static MESSAGE: Mutex<String> = Mutex::new(String::new());

/// 活跃录制会话 id(None = 无)。多活跃属维护态异常(CLI 维护单活跃不变式),
/// 取扫描序首个——停止侧的歧义守卫会如实报错。
pub fn active_session(paths: &actl_core::state::SignalPaths) -> Option<String> {
    let records = paths.dir.join("records");
    let entries = std::fs::read_dir(records).ok()?;
    for entry in entries.flatten() {
        let id = entry.file_name().into_string().ok()?;
        let Ok(raw) = std::fs::read(entry.path().join("state.json")) else {
            continue;
        };
        let Ok(state) = serde_json::from_slice::<serde_json::Value>(&raw) else {
            continue;
        };
        if state["status"].as_str() != Some("recording") {
            continue;
        }
        let pid = state["pid"].as_u64().unwrap_or_default() as u32;
        if pid != 0 && alive(pid) {
            return Some(id);
        }
    }
    None
}

fn alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::*;
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(handle) => {
                let alive = WaitForSingleObject(handle, 0) != WAIT_OBJECT_0;
                let _ = CloseHandle(handle);
                alive
            }
            // 打不开且不是"参数无效"= 进程多半已不存在。
            Err(error) => error.code() != windows::core::HRESULT::from_win32(87),
        }
    }
}

/// 菜单动作入口(窗口线程调用,立即返回):后台线程执行,结果经 message() 反馈。
pub fn request(start: bool) -> Result<(), String> {
    let (tx, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("record-ui".into())
        .spawn(move || {
            let _ = tx.send(execute(start));
        })
        .map_err(|_| "INTERNAL：无法启动录制操作线程".to_string())?;
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.push(receiver);
    }
    if let Ok(mut message) = MESSAGE.lock() {
        *message = if start {
            "正在开始录制…".into()
        } else {
            "正在停止录制并编译草稿…".into()
        };
    }
    Ok(())
}

/// tick 轮询:收割完成的后台任务,返回当前反馈文案(空 = 无)。
pub fn message() -> String {
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.retain(|receiver| match receiver.try_recv() {
            Ok(result) => {
                if let Ok(mut message) = MESSAGE.lock() {
                    *message = result.unwrap_or_else(|error| error);
                }
                false
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if let Ok(mut message) = MESSAGE.lock() {
                    *message = "INTERNAL：录制操作线程已退出".into();
                }
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
        });
    }
    MESSAGE.lock().map(|s| s.clone()).unwrap_or_default()
}

fn execute(start: bool) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()
        .map_err(|_| "INTERNAL：无法定位程序".to_string())?
        .with_file_name("actl.exe");
    let output = std::process::Command::new(exe)
        .args([if start { "record-start" } else { "record-stop" }])
        .stdin(std::process::Stdio::null())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .map_err(|_| "INTERNAL：无法启动录制命令".to_string())?;
    let body: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "INTERNAL：录制命令输出不可解析".to_string())?;
    if body["ok"].as_bool() != Some(true) {
        let code = body["error"]["code"].as_str().unwrap_or("INTERNAL");
        let detail = body["error"]["message"].as_str().unwrap_or_default();
        return Err(format!("{code}：{detail}"));
    }
    Ok(if start {
        // 默认时长上限 600s(CLI 默认);到期 worker 自行诚实收口。
        "录制已开始（上限 10 分钟，右键菜单可停止）".into()
    } else {
        let data = &body["data"];
        format!(
            "录制已完成：{} 事件 → {} 步 + {} TODO（草稿 {}）",
            data["events"].as_u64().unwrap_or_default(),
            data["steps"].as_u64().unwrap_or_default(),
            data["todos"].as_u64().unwrap_or_default(),
            data["draft"].as_str().unwrap_or_default(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use actl_core::state::SignalPaths;
    use serde_json::json;

    fn sandbox() -> SignalPaths {
        SignalPaths::at(std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()))
    }

    #[test]
    fn active_session_requires_recording_status_and_live_pid() {
        let paths = sandbox();
        let dir = paths.dir.join("records").join("rec-x");
        std::fs::create_dir_all(&dir).unwrap();
        let live_pid = std::process::id();
        std::fs::write(
            dir.join("state.json"),
            serde_json::to_vec(&json!({
                "id": "rec-x", "status": "recording", "pid": live_pid,
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(active_session(&paths).as_deref(), Some("rec-x"));

        // 已完成的会话不算活跃。
        std::fs::write(
            dir.join("state.json"),
            serde_json::to_vec(&json!({
                "id": "rec-x", "status": "completed", "pid": live_pid,
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(active_session(&paths), None);

        // pid 死亡(999999 极不可能存在)→ 不算活跃(abandoned 派生态)。
        std::fs::write(
            dir.join("state.json"),
            serde_json::to_vec(&json!({
                "id": "rec-x", "status": "recording", "pid": 999999,
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(active_session(&paths), None);
    }

    #[test]
    fn active_session_is_none_without_records_dir() {
        let paths = sandbox();
        assert_eq!(active_session(&paths), None);
    }
}
