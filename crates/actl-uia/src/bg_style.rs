//! bg_style —— `--no-activate` 后台物理点击的窗口样式守护。
//!
//! 机制(对齐 wechat-ui-collector 两件套的第二件,docs/09 §6.2):
//! 物理注入前给目标窗口根 HWND 临时加 `WS_EX_NOACTIVATE`——点击仍按坐标
//! 命中落地,但不激活、不抢前台、不改 Z 序;动作完成即恢复。
//!
//! 崩溃残留由 journal 兜底:**置位前先落盘(WAL)** → 之后任何 actl 物理
//! 路径入口(`InputLock::acquire`)与本模块下一次使用时扫描 → 死进程的
//! 条目安全清位。恢复方向永远是"清位"(让窗口回到正常行为)——安全方向;
//! 危险方向(置位)只发生在持锁动作段内。
//!
//! 不解决键盘:窗口无焦点时 type/press 会打进用户当前前台,因此
//! `--no-activate` 仅对指针族生效(CLI 层 fail-closed)。

use actl_core::state::SignalPaths;
use actl_core::{CtlError, ErrorCode};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, GetWindowThreadProcessId, SetWindowLongPtrW, WS_EX_NOACTIVATE,
};

/// 进程启动时刻(FILETIME 折叠为 u64)——PID 复用判定:pid 相同但启动
/// 时刻不同 = 原 pid 的进程已死,句柄已被复用。查不到(None)= 无法确认。
fn process_start(pid: u32) -> Option<u64> {
    // SAFETY: 按句柄语义调用;句柄立即关闭,不跨语句持有。
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut create = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        let queried =
            GetProcessTimes(handle, &mut create, &mut exit, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(handle);
        queried.then_some(((create.dwHighDateTime as u64) << 32) | create.dwLowDateTime as u64)
    }
}

/// 窗口句柄当前是否仍属于记录的 pid(hwnd 复用防线)。
fn hwnd_belongs_to(hwnd: isize, pid: u32) -> bool {
    // SAFETY: 出参写入门;HWND 由调用方保证来自 journal 记录。
    unsafe {
        let mut current: u32 = 0;
        GetWindowThreadProcessId(HWND(hwnd as _), Some(&mut current));
        current == pid
    }
}

fn exstyle(hwnd: isize) -> Result<isize, CtlError> {
    // SAFETY: 只读查询,无前置条件。
    unsafe {
        let style = GetWindowLongPtrW(HWND(hwnd as _), GWL_EXSTYLE);
        if style == 0 {
            // 0 本身是合法 exstyle,但 API 失败也返回 0——区分唯一途径是
            // GetLastError;此处保守处理:窗口大概率已销毁,按"不可操作"上抛,
            // 调用方(恢复路径)将其视为已消亡。
            let err = windows::core::HRESULT::from_thread();
            if err.is_err() {
                return Err(CtlError::internal(format!(
                    "GetWindowLongPtrW(GWL_EXSTYLE) failed: {err}"
                )));
            }
            Ok(0)
        } else {
            Ok(style)
        }
    }
}

/// 只清 `WS_EX_NOACTIVATE` 这一位,不整体写回旧值——置位期间其他 exstyle
/// 位可能被合法改动,最小干预才不吞掉它们。
fn clear_no_activate_bit(hwnd: isize) {
    // SAFETY: 同 exstyle;清位失败无补救动作(窗口销毁即自然恢复),静默。
    unsafe {
        let cur = GetWindowLongPtrW(HWND(hwnd as _), GWL_EXSTYLE);
        if cur != 0 && (cur as u32 & WS_EX_NOACTIVATE.0) != 0 {
            let _ = SetWindowLongPtrW(
                HWND(hwnd as _),
                GWL_EXSTYLE,
                cur & !(WS_EX_NOACTIVATE.0 as isize),
            );
        }
    }
}

// ---- journal(单文件 JSON 数组,原子替换;条目数 ≤ 并发 actl 实例数) ----

fn journal_path(paths: &SignalPaths) -> std::path::PathBuf {
    paths.dir.join("no-activate-journal.json")
}

fn read_journal(paths: &SignalPaths) -> Vec<(isize, u32, u64)> {
    let text = std::fs::read_to_string(journal_path(paths)).unwrap_or_default();
    let Some(arr) = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.as_array().cloned())
    else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            let hwnd = e["hwnd"].as_i64()?;
            let pid = e["pid"].as_u64()?;
            let start = e["start"].as_u64()?;
            Some((hwnd as isize, pid as u32, start))
        })
        .collect()
}

fn write_journal(paths: &SignalPaths, entries: &[(isize, u32, u64)]) -> Result<(), CtlError> {
    std::fs::create_dir_all(&paths.dir)
        .map_err(|e| CtlError::internal(format!("create {}: {e}", paths.dir.display())))?;
    let file = journal_path(paths);
    let tmp = file.with_extension("journal-tmp");
    let body: Vec<serde_json::Value> = entries
        .iter()
        .map(|(hwnd, pid, start)| serde_json::json!({ "hwnd": hwnd, "pid": pid, "start": start }))
        .collect();
    std::fs::write(&tmp, serde_json::to_vec(&body).unwrap_or_default())
        .and_then(|_| std::fs::rename(&tmp, &file))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            CtlError::internal(format!("journal write {}: {e}", file.display()))
        })
}

/// 扫描并清理残留条目:pid 已死或启动时刻不符(pid 复用)→ 清位 + 删除;
/// pid 活着(他处会话可能正在动作段内)→ 跳过不动。
/// 恢复清位以 hwnd 归属复核为门——句柄已被别的进程复用时不动它
/// (清一个陌生窗口的位虽无害,但窗口消亡路径无需任何写操作)。
pub(crate) fn recover_stale(paths: &SignalPaths) {
    let entries = read_journal(paths);
    if entries.is_empty() {
        return;
    }
    let me = std::process::id();
    let mut kept = Vec::new();
    for (hwnd, pid, start) in entries {
        let live = pid == me || process_start(pid).is_some_and(|s| s == start);
        if live {
            kept.push((hwnd, pid, start));
            continue;
        }
        if hwnd_belongs_to(hwnd, pid) {
            clear_no_activate_bit(hwnd);
        }
        // 无论窗口是否仍在,死进程的条目都删除(窗口销毁即样式自然消失)
    }
    let _ = write_journal(paths, &kept);
}

/// 置位守护:enter 记账并置位,Drop 清位并销账。声明顺序约定:必须在
/// `InputLock` 之后创建(Drop 先于锁释放,恢复发生在持锁段内)。
pub(crate) struct NoActivateGuard {
    hwnd: isize,
    paths: SignalPaths,
}

impl NoActivateGuard {
    pub(crate) fn enter(hwnd: isize, paths: &SignalPaths) -> Result<Self, CtlError> {
        // 先清别人(崩溃实例)的残留,再记自己
        recover_stale(paths);
        let style = exstyle(hwnd)?;
        if (style as u32 & WS_EX_NOACTIVATE.0) != 0 {
            // 位已被置(非本进程所置,或窗口本身如此)——不动它,也不承担
            // 恢复责任,避免把别人(用户设置/其他工具)的状态当作自己的擦掉。
            return Ok(Self {
                hwnd,
                paths: paths.clone(),
            });
        }
        // WAL:先落盘再置位——置位后进程死亡也能被恢复扫描发现
        let pid = std::process::id();
        let start = process_start(pid).unwrap_or_default();
        let mut entries = read_journal(paths);
        entries.push((hwnd, pid, start));
        write_journal(paths, &entries)?;
        // SAFETY: 常规窗口样式修改;失败(如 UIPI 拒绝提权窗口)→ 上抛
        // PERM_DENIED,提示降级为前台物理点击(fail-closed:承诺不激活
        // 就不静默破坏承诺)。
        unsafe {
            if SetWindowLongPtrW(
                HWND(hwnd as _),
                GWL_EXSTYLE,
                style | WS_EX_NOACTIVATE.0 as isize,
            ) == 0
            {
                // 置位失败:销账后报错(位置未变,无恢复义务)
                let entries: Vec<_> = entries
                    .into_iter()
                    .filter(|(h, p, _)| !(*h == hwnd && *p == pid))
                    .collect();
                let _ = write_journal(paths, &entries);
                let err = windows::core::HRESULT::from_thread();
                return Err(CtlError::new(
                    ErrorCode::PermDenied,
                    format!(
                        "cannot set WS_EX_NOACTIVATE on the target window \
                         (elevated or protected? HRESULT {err}); refusing: --no-activate \
                         promised no activation — rerun without it for a normal physical click"
                    ),
                ));
            }
        }
        Ok(Self {
            hwnd,
            paths: paths.clone(),
        })
    }
}

impl Drop for NoActivateGuard {
    fn drop(&mut self) {
        let pid = std::process::id();
        let entries: Vec<_> = read_journal(&self.paths)
            .into_iter()
            .filter(|(h, p, _)| !(*h == self.hwnd && *p == pid))
            .collect();
        // 窗口仍在且仍归属本进程 → 清位;已销毁/复用 → 什么都不用做
        if hwnd_belongs_to(self.hwnd, pid) {
            clear_no_activate_bit(self.hwnd);
        }
        let _ = write_journal(&self.paths, &entries);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> SignalPaths {
        let dir = std::env::temp_dir().join(format!("actl-bgstyle-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        SignalPaths::at(dir)
    }

    #[test]
    fn journal_roundtrip_and_remove() {
        let paths = tmp_dir("roundtrip");
        write_journal(&paths, &[(1, 42, 100), (2, 43, 200)]).expect("write");
        assert_eq!(read_journal(&paths), vec![(1, 42, 100), (2, 43, 200)]);
        let kept: Vec<_> = read_journal(&paths)
            .into_iter()
            .filter(|(h, p, _)| !(*h == 2 && *p == 43))
            .collect();
        write_journal(&paths, &kept).expect("rewrite");
        assert_eq!(read_journal(&paths), vec![(1, 42, 100)]);
        let _ = std::fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn corrupt_journal_reads_as_empty() {
        let paths = tmp_dir("corrupt");
        std::fs::create_dir_all(&paths.dir).unwrap();
        std::fs::write(journal_path(&paths), "not json").unwrap();
        assert!(read_journal(&paths).is_empty());
        let _ = std::fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn recover_keeps_live_owner_and_drops_dead_entries() {
        // 活条目 = 本测试进程自身(一定活着);死条目 = 不存在的 pid。
        let paths = tmp_dir("recover");
        let me = std::process::id();
        let my_start = process_start(me);
        let dead_pid = 4_000_000; // 极大概率无此进程;若有也几乎不可能同启动时刻
        let entries = vec![
            (111isize, me, my_start.unwrap_or(0)),
            (222isize, dead_pid, 0),
        ];
        write_journal(&paths, &entries).expect("write");
        recover_stale(&paths);
        let left = read_journal(&paths);
        // 死条目必被清走;活条目(自己)保留——不动在用的样式
        assert_eq!(left, vec![(111, me, my_start.unwrap_or(0))]);
        let _ = std::fs::remove_dir_all(&paths.dir);
    }

    #[test]
    fn empty_or_missing_journal_is_noop() {
        let paths = tmp_dir("missing");
        recover_stale(&paths); // 不存在 → 不 panic、不建文件
        assert!(!journal_path(&paths).exists());
    }
}
