//! read —— 读取与断言:get(属性白名单 + value/text 双通道)、verify(立即断言)、
//! extract --table(Grid 直读/树扁平化)、剪贴板读写(CF_UNICODETEXT)。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

use crate::locate::{Located, locate};

/// get:读取元素属性;`property` 白名单,缺省返回基本全集。
/// 属性白名单。`value` = ValuePattern 直读;`text` = TextPattern 文档正文
/// (专用通道:Chromium 的 Document Value=URL 会掩盖正文,value 与 text 分工)。
pub const PROPS: &[&str] = &[
    "name",
    "role",
    "automation_id",
    "offscreen",
    "value",
    "text",
];

pub struct PropertyInfo {
    pub name: Option<String>,
    pub role: String,
    pub automation_id: Option<String>,
    pub offscreen: Option<bool>,
    pub value: Option<String>,
    /// value 的读取通道；仅 ValuePattern 不支持时回退 TextPattern。
    pub value_via: Option<&'static str>,
    /// TextPattern 文档正文(--property text 专用)
    pub text: Option<String>,
    pub unsupported: Vec<&'static str>,
}

pub fn read_property(
    app: Option<&str>,
    target: &Target,
    property: Option<&str>,
    near: Option<&str>,
) -> Result<PropertyInfo, CtlError> {
    if let Some(p) = property
        && !PROPS.contains(&p)
    {
        return Err(CtlError::protocol(format!(
            "unknown property {p:?}: expected one of {PROPS:?}"
        )));
    }
    let loc = locate(app, target, near)?;
    let want = |p: &str| property.map(|sel| sel == p).unwrap_or(true);
    let (value, value_via) = if want("value") {
        read_value_via(&loc.element)?
    } else {
        (None, None)
    };
    let text = if want("text") {
        read_text(&loc.element)?
    } else {
        None
    };
    let mut unsupported = Vec::new();
    if want("value") && value.is_none() {
        unsupported.push("value");
    }
    if want("text") && text.is_none() {
        unsupported.push("text");
    }
    if property == Some("value") {
        crate::read_channel::require(value.as_ref(), "value-or-text-pattern")?;
    }
    if property == Some("text") {
        crate::read_channel::require(text.as_ref(), "text-pattern")?;
    }
    Ok(PropertyInfo {
        name: if want("name") {
            Some(
                loc.element
                    .get_name()
                    .map_err(|e| crate::read_channel::failure("name", e))?,
            )
        } else {
            None
        },
        role: if want("role") {
            format!(
                "{:?}",
                loc.element
                    .get_control_type()
                    .map_err(|e| crate::read_channel::failure("role", e))?
            )
        } else {
            String::new()
        },
        automation_id: if want("automation_id") {
            Some(
                loc.element
                    .get_automation_id()
                    .map_err(|e| crate::read_channel::failure("automation_id", e))?,
            )
        } else {
            None
        },
        offscreen: if want("offscreen") {
            Some(
                loc.element
                    .is_offscreen()
                    .map_err(|e| crate::read_channel::failure("offscreen", e))?,
            )
        } else {
            None
        },
        value,
        value_via,
        text,
        unsupported,
    })
}

fn read_value_via(elem: &UIElement) -> Result<(Option<String>, Option<&'static str>), CtlError> {
    crate::read_channel::choose(crate::read_channel::value(elem), || read_text(elem))
}

pub(crate) fn read_value(elem: &UIElement) -> Result<Option<String>, CtlError> {
    Ok(read_value_via(elem)?.0)
}

pub(crate) fn read_text(elem: &UIElement) -> Result<Option<String>, CtlError> {
    crate::read_channel::text(elem)
}

/// verify:立即断言(与 wait 的"耐心等待"分工,06 §3.6)。
/// 断言失败 → ASSERTION_FAILED,evidence 携带实际值。
pub enum VerifyKind {
    /// 元素存在(经定位链解析成功)
    Exists,
    /// ValuePattern 值 == 期望(exact)或包含期望(contains)
    Value {
        expected: String,
        contains: bool,
        property: String,
    },
    /// Toggle 状态为 On
    Checked,
}

pub struct VerifyOutcome {
    pub located: Located,
    pub actual: Option<String>,
    pub mode: &'static str,
    pub channel: Option<&'static str>,
}

pub fn verify(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    kind: VerifyKind,
) -> Result<VerifyOutcome, CtlError> {
    let fail = |what: &str, actual: Option<String>| {
        CtlError::with_evidence(
            ErrorCode::AssertionFailed,
            format!("verify {what} failed"),
            serde_json::json!({ "actual": actual }),
        )
    };
    match kind {
        VerifyKind::Exists => {
            let loc = locate(app, target, near)?;
            Ok(VerifyOutcome {
                located: loc,
                actual: None,
                mode: "exists",
                channel: None,
            })
        }
        VerifyKind::Value {
            expected,
            contains,
            property,
        } => {
            let loc = locate(app, target, near)?;
            let (actual, channel) = match property.as_str() {
                "name" => (
                    Some(
                        loc.element
                            .get_name()
                            .map_err(|e| crate::read_channel::failure("name", e))?,
                    ),
                    Some("name"),
                ),
                "text" => (read_text(&loc.element)?, Some("text-pattern")),
                "value" => (
                    crate::read_channel::value(&loc.element)?,
                    Some("value-pattern"),
                ),
                _ => read_value_via(&loc.element)?,
            };
            crate::read_channel::require(
                actual.as_ref(),
                channel.unwrap_or("value-or-text-pattern"),
            )?;
            let ok = match (&actual, contains) {
                (Some(a), true) => a.contains(&expected),
                (Some(a), false) => a == &expected,
                (None, _) => false,
            };
            if !ok {
                return Err(CtlError::with_evidence(
                    ErrorCode::AssertionFailed,
                    "verify result did not match",
                    serde_json::json!({
                          "target":target.describe(),"property":if property == "auto" {"value"} else {&property},"channel":channel,
                        "expected":expected,"actual":actual,"comparison":if contains {"contains"} else {"exact"}
                    }),
                ));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual,
                mode: match (property.as_str(), contains) {
                    ("name", false) => "name",
                    ("name", true) => "name-contains",
                    ("text", false) => "text",
                    ("text", true) => "text-contains",
                    (_, true) => "value-contains",
                    _ => "value",
                },
                channel,
            })
        }
        VerifyKind::Checked => {
            let loc = locate(app, target, near)?;
            let state = crate::read_channel::toggle(&loc.element)?;
            crate::read_channel::require(state.as_ref(), "toggle-pattern")?;
            if state.as_deref() != Some("On") {
                return Err(fail("checked", state));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual: state,
                mode: "checked",
                channel: Some("toggle-pattern"),
            })
        }
    }
}

// ─── 剪贴板(CF_UNICODETEXT;剪贴板是共享状态,doc 09 §6.2 提醒并发) ──────

/// CF_UNICODETEXT(=13)。不引 Win32_System_Ole 只为一个常量。
const CF_UNICODETEXT: u32 = 13;

/// 确保任何返回路径都 CloseClipboard(占用不释放会拖死系统剪贴板)。
struct ClipGuard;
impl Drop for ClipGuard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() }.ok();
    }
}

fn open_clipboard() -> Result<ClipGuard, CtlError> {
    if unsafe { OpenClipboard(None) }.is_ok() {
        Ok(ClipGuard)
    } else {
        Err(CtlError::new(
            ErrorCode::NotActionable,
            "clipboard is held by another process; retry shortly",
        ))
    }
}

/// get-clipboard:读文本(None = 无文本格式/空)。非文本格式(图片等)返回 None。
pub fn get_clipboard_text() -> Result<Option<String>, CtlError> {
    let mut action = actl_core::signal_session::ActionGuard::begin(
        "get-clipboard",
        actl_core::activity::InputEffects {
            clipboard: actl_core::activity::ClipboardEffect::Read,
            ..Default::default()
        },
        Default::default(),
    );
    action.executing();
    let result = get_clipboard_text_inner()?;
    action.delivered();
    Ok(result)
}
fn get_clipboard_text_inner() -> Result<Option<String>, CtlError> {
    unsafe {
        let _guard = open_clipboard()?;
        if !windows::Win32::System::DataExchange::IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok()
        {
            return Ok(None);
        }
        let Ok(h) = GetClipboardData(CF_UNICODETEXT) else {
            return Ok(None);
        };
        let hg = HGLOBAL(h.0);
        let p = GlobalLock(hg) as *const u16;
        if p.is_null() {
            return Ok(None);
        }
        // 手动扫 nul 结尾(免引 Globalization 的 lstrlenW)
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
        let _ = GlobalUnlock(hg);
        Ok(Some(text))
    }
}

/// set-clipboard:写入文本(整体替换当前剪贴板内容——这是本命令的语义)。
pub fn set_clipboard_text(text: &str) -> Result<(), CtlError> {
    use windows::Win32::Foundation::{GlobalFree, HANDLE};

    let mut action = crate::feedback::prepare(
        "set-clipboard",
        actl_core::activity::InputEffects {
            clipboard: actl_core::activity::ClipboardEffect::Write,
            ..Default::default()
        },
        Default::default(),
    )?;
    crate::feedback::check_stop()?;
    action.executing();
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    unsafe {
        let _guard = open_clipboard()?;
        EmptyClipboard().map_err(|e| CtlError::internal(format!("EmptyClipboard: {e}")))?;
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)
            .map_err(|e| CtlError::internal(format!("GlobalAlloc: {e}")))?;
        let p = GlobalLock(h) as *mut u16;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(CtlError::internal("GlobalLock failed"));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
        let _ = GlobalUnlock(h);
        // 所有权移交系统;失败则自毁防泄漏
        if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(h.0))).is_err() {
            let _ = GlobalFree(Some(h));
            return Err(CtlError::internal("SetClipboardData failed"));
        }
        action.delivered();
        Ok(())
    }
}

/// 焦点三视图诊断(只读):UIA 焦点元素 / 物理焦点根窗(GetGUIThreadInfo)/
/// 物理前台窗。WinUI 上 UIA focused element 实测会谎报(kbd.rs 注释),
/// 三视图并列供交叉比对,不做裁决——判读按 docs/31 组件选择判定树留给调用方。
/// `focus_root_processes_agree` 只是进程一致性的廉价事实信号,不是结论。
pub fn focus_report() -> Result<serde_json::Value, CtlError> {
    let auto = uiautomation::UIAutomation::new().map_err(crate::internal)?;
    let uia_focused = auto.get_focused_element().ok().map(|e| {
        serde_json::json!({
            "name": e.get_name().unwrap_or_default(),
            "role": e
                .get_control_type()
                .map(|t| format!("{t:?}"))
                .unwrap_or_default(),
            "automation_id": e.get_automation_id().unwrap_or_default(),
            "process_id": e.get_process_id().unwrap_or_default(),
        })
    });
    let physical_focus_root = crate::kbd::physical_focus_identity();
    let foreground = crate::kbd::foreground_identity();
    let agree = match (&physical_focus_root, &uia_focused) {
        (Some(root), Some(uia)) => root["pid"] == uia["process_id"],
        _ => false,
    };
    Ok(serde_json::json!({
        "uia_focused": uia_focused,
        "physical_focus_root": physical_focus_root,
        "foreground": foreground,
        "focus_root_processes_agree": agree,
    }))
}
