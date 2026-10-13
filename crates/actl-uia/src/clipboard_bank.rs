//! clipboard_bank —— 剪贴板全格式备份/恢复(2026-10-13 剪贴板轮,docs/06 §3.8)。
//!
//! 动机(RPA 经典坑:粘贴通道毁掉用户剪贴板):`type --paste`/`set-clipboard`
//! 只动 CF_UNICODETEXT,但 EmptyClipboard 会清掉**全部格式**——用户刚复制的
//! 图片/文件列表/富文本一并丢失。RPA 工具(UiPath "Restore clipboard")的
//! 标准解法是保存/恢复对。actl 的形态:**恢复是编排层的显式决定**(与
//! windows_created "不代管生命周期"同款哲学)——paste 不自动恢复(用户在
//! agent 粘贴后新复制的内容会被静默覆盖回旧值,反而制造数据丢失事故);
//! batch/flow 域开关在组合层实现,core 只提供原子双动词。
//!
//! 备份落盘 %LOCALAPPDATA%\actl\clipboard\<id>.json + 同名 blob 目录;
//! 格式逐个 EnumClipboardFormats 采集,标准格式(CF_UNICODETEXT/CF_HDROP/
//! CF_BITMAP/CF_DIB 等)存语义化字段,未知/私有格式存原始字节——恢复时
//! 全部回放。诚实边界:①剪贴板历史(Win+V)/云同步把恢复表现为新条目,
//! 不伪造"历史回滚";②延迟渲染格式(source 未即时提供数据)备不到内容,
//! 如实登记在 skipped_delayed;③同一时刻只有一份全局剪贴板,备份不锁
//! 语义(期间新复制会覆盖,恢复以调用时为准)。

use actl_core::{CtlError, ErrorCode};
use serde_json::{Value, json};
use std::path::PathBuf;

const BANK_DIR: &str = "clipboard";

fn bank_path(id: &str) -> Result<PathBuf, CtlError> {
    // id 语法与 desktop 会话令牌同款:字母数字+短横,防路径穿越
    if id.is_empty() || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') || id.len() > 64
    {
        return Err(CtlError::protocol(
            "clipboard id must be 1-64 chars of [A-Za-z0-9-]",
        ));
    }
    let base = actl_core::state::SignalPaths::default().dir.join(BANK_DIR);
    std::fs::create_dir_all(&base).map_err(crate::internal)?;
    Ok(base.join(format!("{id}.json")))
}

/// 打开剪贴板并确保任何返回路径都关闭(read.rs 同款守卫:占用不释放会拖死系统)。
/// 空结构仅表意"已取得剪贴板所有权"(CloseClipboard 无参,不携带窗口句柄)。
struct ClipboardGuard;
impl ClipboardGuard {
    fn open() -> Result<Self, CtlError> {
        // 有界重试:剪贴板可能被其他进程短暂持有(常见竞态)
        for _ in 0..10 {
            if unsafe { windows::Win32::System::DataExchange::OpenClipboard(None) }.is_ok() {
                return Ok(ClipboardGuard);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        Err(CtlError::new(
            ErrorCode::Timeout,
            "clipboard stayed locked by another process",
        ))
    }
}
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe { windows::Win32::System::DataExchange::CloseClipboard() }.ok();
    }
}

/// clipboard-save:全格式备份。标准格式语义化存储,未知格式存原始字节。
/// envelope 带 formats/skipped_delayed/bytes,备份文件路径 path。
pub fn clipboard_save(id: &str) -> Result<Value, CtlError> {
    use windows::Win32::System::DataExchange::GetClipboardFormatNameW;
    use windows::Win32::System::DataExchange::{EnumClipboardFormats, GetClipboardData};

    // id 校验先于任何副作用(prepare/开剪贴板)——坏 id 是参数层错误,
    // 不该在信封里以业务错误形态出现(真机契约测试实证:顺序反了会把
    // PROTOCOL 变 exit 1)
    bank_path(id)?;
    let mut action = crate::feedback::prepare(
        "clipboard-save",
        actl_core::activity::InputEffects {
            clipboard: actl_core::activity::ClipboardEffect::Read,
            ..Default::default()
        },
        Default::default(),
    )?;
    crate::feedback::check_stop()?;
    action.executing();

    let mut formats: Vec<Value> = Vec::new();
    let mut total_bytes = 0usize;
    let mut delayed = Vec::new();
    {
        let _clip = ClipboardGuard::open()?;
        let mut fmt: u32 = unsafe { EnumClipboardFormats(0) };
        while fmt != 0 {
            let mut name_buf = [0u16; 64];
            let named = unsafe { GetClipboardFormatNameW(fmt, &mut name_buf) } as usize;
            let name = if named > 0 {
                String::from_utf16_lossy(&name_buf[..named])
            } else {
                String::new()
            };
            let Ok(handle) = (unsafe { GetClipboardData(fmt) }) else {
                // 延迟渲染格式此刻拿不到数据——如实登记,不伪装已备
                delayed.push(if name.is_empty() { fmt } else { 0 });
                fmt = unsafe { EnumClipboardFormats(fmt) };
                continue;
            };
            let (payload, kind) =
                read_format_payload(fmt, windows::Win32::Foundation::HGLOBAL(handle.0));
            total_bytes += payload.len();
            formats.push(json!({
                "format": fmt,
                "name": name,
                "kind": kind,
                "data_b64": base64_encode(&payload),
            }));
            fmt = unsafe { EnumClipboardFormats(fmt) };
        }
    }
    let doc = json!({
        "version": 1,
        "saved_at_unix_ms": actl_core::state::unix_ms(),
        "formats": formats,
    });
    let path = bank_path(id)?;
    std::fs::write(&path, serde_json::to_vec(&doc).map_err(crate::internal)?)
        .map_err(crate::internal)?;
    action.delivered();
    Ok(json!({
        "id": id,
        "formats": formats.len(),
        "bytes": total_bytes,
        "skipped_delayed": delayed.len(),
        "path": path.to_string_lossy(),
    }))
}

/// clipboard-restore:按备份回放全部格式。恢复是覆盖操作(EmptyClipboard
/// 后逐格式 SetClipboardData),与 set-clipboard 同副作用面(Write)。
pub fn clipboard_restore(id: &str) -> Result<Value, CtlError> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{EmptyClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    let path = bank_path(id)?;
    if !path.exists() {
        // 专属 hint:NOT_FOUND 缺省句"relax the selector"对备份库语义完全
        // 误导(没有选择器可放宽)——错误契约的 hint 按上下文分化原则
        return Err(CtlError::with_evidence_hint(
            ErrorCode::NotFound,
            format!("no clipboard backup named {id:?} (run clipboard-save {id} first)"),
            json!({"bank": path.to_string_lossy()}),
            format!(
                "this id has no saved backup — create it with `clipboard-save {id}` before overwriting the clipboard, or list existing ids under the bank path in evidence"
            ),
        ));
    }
    let doc: Value = serde_json::from_slice(&std::fs::read(&path).map_err(crate::internal)?)
        .map_err(crate::internal)?;
    let formats = doc["formats"].as_array().cloned().unwrap_or_default();
    if formats.is_empty() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "backup holds no formats (was the clipboard empty at save time?)",
        ));
    }
    let mut action = crate::feedback::prepare(
        "clipboard-restore",
        actl_core::activity::InputEffects {
            clipboard: actl_core::activity::ClipboardEffect::Write,
            ..Default::default()
        },
        Default::default(),
    )?;
    crate::feedback::check_stop()?;
    action.executing();
    let mut restored = 0usize;
    let mut failed: Vec<u32> = Vec::new();
    {
        let _clip = ClipboardGuard::open()?;
        unsafe { EmptyClipboard() }.map_err(crate::internal)?;
        for f in &formats {
            let fmt = f["format"].as_u64().unwrap_or(0) as u32;
            let data = base64_decode(f["data_b64"].as_str().unwrap_or(""));
            // 简单格式(CF_TEXT 等值格式族)回放为 CF_UNICODETEXT 语义由
            // kind 标注;这里统一按原始字节回放原格式 id——系统负责族映射
            let h = unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) }
                .map_err(crate::internal)?;
            let p = unsafe { GlobalLock(h) } as *mut u8;
            if p.is_null() {
                return Err(CtlError::internal("GlobalLock failed"));
            }
            if !data.is_empty() {
                unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len()) };
            }
            unsafe { GlobalUnlock(h) }.ok();
            if unsafe { SetClipboardData(fmt, Some(HANDLE(h.0))) }.is_err() {
                failed.push(fmt);
                unsafe {
                    let _ = GlobalFree(Some(h));
                };
                continue;
            }
            restored += 1;
        }
    }
    action.delivered();
    if restored == 0 {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!("all {} formats failed to restore", failed.len()),
        ));
    }
    Ok(json!({
        "id": id,
        "restored": restored,
        "failed_formats": failed,
    }))
}

/// clipboard-formats:只读枚举当前剪贴板格式清单(诊断/判读辅助)。
/// 不读数据内容,只报格式 id/name——回答"用户剪贴板里现在有什么类型"。
pub fn clipboard_formats() -> Result<Value, CtlError> {
    use windows::Win32::System::DataExchange::EnumClipboardFormats;
    use windows::Win32::System::DataExchange::GetClipboardFormatNameW;

    let mut list: Vec<Value> = Vec::new();
    {
        let _clip = ClipboardGuard::open()?;
        let mut fmt: u32 = unsafe { EnumClipboardFormats(0) };
        while fmt != 0 {
            let mut name_buf = [0u16; 64];
            let named = unsafe { GetClipboardFormatNameW(fmt, &mut name_buf) } as usize;
            let name = if named > 0 {
                String::from_utf16_lossy(&name_buf[..named])
            } else {
                standard_format_name(fmt).to_owned()
            };
            list.push(json!({ "format": fmt, "name": name }));
            fmt = unsafe { EnumClipboardFormats(fmt) };
        }
    }
    Ok(json!({
        "formats": list,
        "total": list.len(),
    }))
}

/// 标准格式 id 的名字(0x00-0x18 区间;私有格式走 GetClipboardFormatName)。
fn standard_format_name(fmt: u32) -> &'static str {
    match fmt {
        1 => "CF_TEXT",
        2 => "CF_BITMAP",
        3 => "CF_METAFILEPICT",
        4 => "CF_SYLK",
        5 => "CF_DIF",
        6 => "CF_TIFF",
        7 => "CF_OEMTEXT",
        8 => "CF_DIB",
        9 => "CF_PALETTE",
        10 => "CF_PENDATA",
        11 => "CF_RIFF",
        12 => "CF_WAVE",
        13 => "CF_UNICODETEXT",
        14 => "CF_ENHMETAFILE",
        15 => "CF_HDROP",
        16 => "CF_LOCALE",
        _ => "",
    }
}

/// 读一个格式句柄的数据载荷。**全格式一律原始字节,不做语义转换**
/// (真机实证 2026-10-13:save 时把 CF_UNICODETEXT 转 UTF-8、restore 按原始
/// 字节回放 → 系统把 UTF-8 字节当 UTF-16 解读,恢复全乱码"獵牥椠..."——
/// 备份保真优先,kind 只是判读标注,任何转换都会在恢复侧引入不对称)。
/// GlobalSize 对某些格式可能比实际内容多分配(系统按需),恢复按原大小回放,
/// 多余零字节不影响文本语义(UTF-16 以零终止符为界)。
fn read_format_payload(
    fmt: u32,
    handle: windows::Win32::Foundation::HGLOBAL,
) -> (Vec<u8>, &'static str) {
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    let size = unsafe { GlobalSize(handle) };
    let p = unsafe { GlobalLock(handle) } as *const u8;
    if p.is_null() || size == 0 {
        return (Vec::new(), "empty");
    }
    let kind = if fmt == 13 {
        "unicode-text"
    } else if fmt == 15 {
        "file-list"
    } else if fmt == 2 || fmt == 8 || fmt == 14 {
        "image"
    } else {
        "raw"
    };
    let payload = unsafe { std::slice::from_raw_parts(p, size) }.to_vec();
    unsafe { GlobalUnlock(handle) }.ok();
    (payload, kind)
}

/// Base64(无外部依赖:crate 直依赖预算 ≤9,手写 30 行比引 base64 crate 划算)。
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(s: &str) -> Vec<u8> {
    fn val(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    }
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for chunk in b.chunks(4) {
        if chunk.len() < 4 {
            break;
        }
        let n =
            (val(chunk[0]) << 18) | (val(chunk[1]) << 12) | (val(chunk[2]) << 6) | val(chunk[3]);
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_all_byte_values() {
        // 全字节值 + 非对齐长度:编码-解码恒等(备份/恢复的载荷完整性)
        let data: Vec<u8> = (0..=255u8).chain([1, 2, 3]).collect();
        let enc = base64_encode(&data);
        assert_eq!(base64_decode(&enc), data);
        // 已知向量(RFC 4648):f/ba/fo
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_decode("Zm8="), b"fo".to_vec());
    }

    #[test]
    fn bank_id_rejects_traversal_and_oversize() {
        assert!(bank_path("abc-123").is_ok());
        assert!(bank_path("../etc").is_err(), "path traversal rejected");
        assert!(bank_path("a b").is_err(), "whitespace rejected");
        assert!(bank_path("").is_err());
        assert!(bank_path(&"x".repeat(65)).is_err());
    }

    #[test]
    fn standard_format_names_cover_common_ids() {
        assert_eq!(standard_format_name(13), "CF_UNICODETEXT");
        assert_eq!(standard_format_name(15), "CF_HDROP");
        assert_eq!(standard_format_name(8), "CF_DIB");
        assert_eq!(standard_format_name(500), "");
    }
}
