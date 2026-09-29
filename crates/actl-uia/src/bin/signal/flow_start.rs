//! Route a real details-window Continue click to the main consent service.
use actl_core::{
    handoff::Request,
    state::{SignalPaths, unix_ms},
};
use windows::{
    Win32::{Foundation::*, UI::WindowsAndMessaging::*},
    core::w,
};
pub const MESSAGE: u32 = WM_APP + 50;

pub fn allow(id: &str) -> Result<String, String> {
    let paths = SignalPaths::default();
    let generation = paths.execution_generation().map_err(|e| e.message)?;
    let hwnd = unsafe { FindWindowW(w!("actl_signal_window"), None) }
        .map_err(|_| "显示端未运行，请先启动显示端")?;
    if paths
        .display_lease()
        .is_none_or(|l| l.protocol != actl_core::display::DISPLAY_PROTOCOL)
    {
        return Err("显示端版本不匹配，请更新并重启显示端".into());
    }
    let mut pid = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == std::process::id() {
        return super::handoff::allow_flow(id, &generation)
            .then_some(generation)
            .ok_or_else(|| "输入监听或运行状态未就绪".into());
    }
    let request = Request {
        approval: actl_core::handoff::Approval::Expiring,
        generation: generation.clone(),
        call: actl_core::snapshot::new_snapshot_id(),
        task: id.into(),
        target: "flow continue".into(),
        ts_ms: unix_ms(),
    };
    let filename = format!("flow-start-{}.json", std::process::id());
    paths
        .write_display(&filename, &request)
        .map_err(|e| e.to_string())?;
    let mut acknowledged = 0;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            MESSAGE,
            WPARAM(std::process::id() as usize),
            LPARAM(0),
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            2000,
            Some(&mut acknowledged),
        )
    };
    let _ = std::fs::remove_file(paths.dir.join(filename));
    if sent.0 == 0 || acknowledged != 1 {
        return Err("未确认任务授权，请检查显示端和任务状态".into());
    }
    Ok(generation)
}

pub fn receive(pid: u32) -> bool {
    let paths = SignalPaths::default();
    let file = paths.dir.join(format!("flow-start-{pid}.json"));
    let Ok(data) = std::fs::read(&file) else {
        return false;
    };
    if data.len() > 8192 {
        return false;
    }
    let Ok(r) = serde_json::from_slice::<Request>(&data) else {
        return false;
    };
    if std::fs::remove_file(file).is_err() {
        return false;
    }
    if !r.fresh(unix_ms()) || paths.execution_generation().ok().as_ref() != Some(&r.generation) {
        return false;
    }
    let state = super::flow_ui::read(&paths, &r.task);
    if super::flow_ui::resume_guard(state.as_ref(), unix_ms()).is_err() {
        return false;
    }
    super::handoff::allow_flow(&r.task, &r.generation)
}
