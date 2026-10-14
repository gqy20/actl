//! 自身进程资源快照(诊断旁路):GetProcessMemoryInfo + GetProcessTimes
//! 一次调用拿 WorkingSet/峰值与进程创建以来的累计 user/kernel CPU 时间
//! (CreationTime 起累计,无需基线采样差分)。像素命令族的内存剖面
//! (全窗捕获 ~11MB BGRA/次、--motion 翻倍、金字塔降采样层)与
//! CPU-bound(扫描) vs 纯等待(睡眠)的定性,在此一次读全。
//! 只读自身进程,零目标应用接触;经 trace 旁路输出,不进信封契约面。

use actl_core::CtlError;
use serde_json::{Value, json};

/// 自身进程资源快照;调用失败如实 Internal(诊断旁路不静默捏造)。
pub fn self_snapshot() -> Result<Value, CtlError> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::ProcessStatus::GetProcessMemoryInfo;
    use windows::Win32::System::ProcessStatus::PROCESS_MEMORY_COUNTERS;
    use windows::Win32::System::Threading::GetCurrentProcess;
    use windows::Win32::System::Threading::GetProcessTimes;

    let proc = unsafe { GetCurrentProcess() };
    let mut pmc = PROCESS_MEMORY_COUNTERS::default();
    unsafe {
        GetProcessMemoryInfo(
            proc,
            &mut pmc,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    }
    .map_err(crate::internal)?;
    let mut create = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(proc, &mut create, &mut exit, &mut kernel, &mut user) }
        .map_err(crate::internal)?;
    // FILETIME 100ns 单位 → ms(10_000 = 1ms)
    let ft_ms =
        |ft: FILETIME| ((ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64) / 10_000;
    Ok(json!({
        "working_set_kb": pmc.WorkingSetSize / 1024,
        "peak_working_set_kb": pmc.PeakWorkingSetSize / 1024,
        "pagefile_kb": pmc.PagefileUsage / 1024,
        "cpu_user_ms": ft_ms(user),
        "cpu_kernel_ms": ft_ms(kernel),
    }))
}
