//! read —— 读取与断言:get(属性白名单 + value/text 双通道)、verify(立即断言)、
//! extract --table(Grid 直读/树扁平化)、剪贴板读写(CF_UNICODETEXT)。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::patterns::UIValuePattern;
use uiautomation::{UIAutomation, UIElement, UITreeWalker};
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

use crate::internal;
use crate::locate::{Located, locate, role_of};
use crate::window::MAX_ELEMENTS;

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
    /// value 的读取通道:value-pattern | text-pattern(value 为空时的兜底)
    pub value_via: Option<&'static str>,
    /// TextPattern 文档正文(--property text 专用)
    pub text: Option<String>,
}

pub fn read_property(
    app: Option<&str>,
    target: &Target,
    property: Option<&str>,
    near: Option<&str>,
) -> Result<PropertyInfo, CtlError> {
    if let Some(p) = property {
        if !PROPS.contains(&p) {
            return Err(CtlError::protocol(format!(
                "unknown property {p:?}: expected one of {PROPS:?}"
            )));
        }
    }
    let loc = locate(app, target, near)?;
    let want = |p: &str| property.map(|sel| sel == p).unwrap_or(true);
    let mut value_via = None;
    let value = want("value")
        .then(|| {
            let (v, via) = read_value_via(&loc.element);
            value_via = via;
            v
        })
        .flatten();
    // text = TextPattern 文档正文专用通道(Chromium Document 的 Value=URL,
    // 正文只能走这里)
    let text = want("text")
        .then(|| {
            use uiautomation::patterns::UITextPattern;
            loc.element
                .get_pattern::<UITextPattern>()
                .ok()
                .and_then(|t| t.get_document_range().ok())
                .and_then(|r| r.get_text(-1).ok())
                .filter(|t| !t.is_empty())
        })
        .flatten();
    Ok(PropertyInfo {
        name: want("name").then_some(loc.name.clone()).flatten(),
        role: if want("role") {
            loc.role.clone()
        } else {
            String::new()
        },
        automation_id: want("automation_id")
            .then_some(loc.automation_id.clone())
            .flatten(),
        offscreen: want("offscreen")
            .then(|| loc.element.is_offscreen().ok())
            .flatten(),
        value,
        value_via,
        text,
    })
}

/// 元素文本读取双通道:ValuePattern 直读 → TextPattern 文档范围兜底。
/// 富文本(浏览器页面/Word 文档)不暴露 Value pattern,TextPattern 是唯一通道;
/// 返回值附带来源标记(get 的 via 上报,与 type --paste 同风格)。
fn read_value_via(elem: &UIElement) -> (Option<String>, Option<&'static str>) {
    use uiautomation::patterns::UITextPattern;

    if let Some(v) = elem
        .get_pattern::<UIValuePattern>()
        .ok()
        .and_then(|p| p.get_value().ok())
        .filter(|v| !v.is_empty())
    {
        return (Some(v), Some("value-pattern"));
    }
    let text = elem
        .get_pattern::<UITextPattern>()
        .ok()
        .and_then(|t| t.get_document_range().ok())
        .and_then(|r| r.get_text(-1).ok())
        .filter(|t| !t.is_empty());
    (text.clone(), text.is_some().then_some("text-pattern"))
}

pub(crate) fn read_value(elem: &UIElement) -> Option<String> {
    read_value_via(elem).0
}

/// verify:立即断言(与 wait 的"耐心等待"分工,06 §3.6)。
/// 断言失败 → ASSERTION_FAILED,evidence 携带实际值。
pub enum VerifyKind {
    /// 元素存在(经定位链解析成功)
    Exists,
    /// ValuePattern 值 == 期望(exact)或包含期望(contains)
    Value { expected: String, contains: bool },
    /// Toggle 状态为 On
    Checked,
}

pub struct VerifyOutcome {
    pub located: Located,
    pub actual: Option<String>,
    pub mode: &'static str,
}

pub fn verify(
    app: Option<&str>,
    target: &Target,
    near: Option<&str>,
    kind: VerifyKind,
) -> Result<VerifyOutcome, CtlError> {
    use uiautomation::patterns::UITogglePattern;

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
            })
        }
        VerifyKind::Value { expected, contains } => {
            let loc = locate(app, target, near)?;
            let actual = read_value(&loc.element);
            let ok = match (&actual, contains) {
                (Some(a), true) => a.contains(&expected),
                (Some(a), false) => a == &expected,
                (None, _) => false,
            };
            if !ok {
                return Err(fail("value", actual));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual,
                mode: if contains { "value-contains" } else { "value" },
            })
        }
        VerifyKind::Checked => {
            let loc = locate(app, target, near)?;
            let state = loc
                .element
                .get_pattern::<UITogglePattern>()
                .ok()
                .and_then(|t| t.get_toggle_state().ok())
                .map(|s| format!("{s:?}"));
            if state.as_deref() != Some("On") {
                return Err(fail("checked", state));
            }
            Ok(VerifyOutcome {
                located: loc,
                actual: state,
                mode: "checked",
            })
        }
    }
}

/// extract --table:GridPattern 直读优先(带 row/col 语义,虚拟化行可达),
/// 树扁平化兜底(容器 → 行(DataItem/ListItem)→ 单元格(Text/Edit 值))。
/// 返回 (行数据, 来源标记);来源随结果上报,agent 可判断结构可信度。
pub fn extract_table(
    app: Option<&str>,
    target: &Target,
) -> Result<(Vec<Vec<String>>, &'static str), CtlError> {
    use uiautomation::patterns::UIGridPattern;

    let loc = locate(app, target, None)?;
    // 直读路径:GridPattern.GetItemAt(row, col)——provider 侧行列寻址;
    // 单元格值优先 Value pattern,退 Name。pattern 宣称支持但读不全
    // → 如实走兜底路径,不输出半张表
    if let Ok(grid) = loc.element.get_pattern::<UIGridPattern>() {
        let rows = grid.get_row_count().unwrap_or(0);
        let cols = grid.get_column_count().unwrap_or(0);
        if rows > 0 && cols > 0 && (rows * cols) as usize <= MAX_ELEMENTS {
            let mut table = Vec::with_capacity(rows as usize);
            let mut ok = true;
            for r in 0..rows {
                let mut row = Vec::with_capacity(cols as usize);
                for c in 0..cols {
                    match grid.get_item(r, c) {
                        Ok(cell) => row.push(
                            read_value(&cell)
                                .unwrap_or_else(|| cell.get_name().unwrap_or_default()),
                        ),
                        Err(_) => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    break;
                }
                table.push(row);
            }
            if ok && !table.is_empty() {
                return Ok((table, "grid-pattern"));
            }
        }
    }

    let auto = UIAutomation::new().map_err(internal)?;
    let walker = auto.create_tree_walker().map_err(internal)?;

    fn rows_under(walker: &UITreeWalker, elem: &UIElement, depth: u32, out: &mut Vec<UIElement>) {
        if depth > crate::window::MAX_DEPTH {
            return;
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            let role = role_of(&c);
            if role == "DataItem" || role == "ListItem" {
                out.push(c.clone());
            } else {
                rows_under(walker, &c, depth + 1, out);
            }
            child = walker.get_next_sibling(&c).ok();
        }
    }
    fn cells_under(walker: &UITreeWalker, elem: &UIElement, depth: u32, out: &mut Vec<String>) {
        if depth > crate::window::MAX_DEPTH {
            return;
        }
        let role = role_of(elem);
        if role == "Text" || role == "Edit" {
            if let Some(n) = elem.get_name().ok().filter(|n| !n.is_empty()) {
                out.push(n);
            }
        }
        let mut child = walker.get_first_child(elem).ok();
        while let Some(c) = child {
            cells_under(walker, &c, depth + 1, out);
            child = walker.get_next_sibling(&c).ok();
        }
    }

    let mut row_elems = Vec::new();
    rows_under(&walker, &loc.element, 0, &mut row_elems);
    if row_elems.is_empty() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            format!(
                "{} ({}) contains no DataItem/ListItem rows",
                target.describe(),
                loc.role
            ),
        ));
    }
    let mut table = Vec::new();
    for r in &row_elems {
        let mut cells = Vec::new();
        cells_under(&walker, r, 0, &mut cells);
        if !cells.is_empty() {
            table.push(cells);
        }
    }
    Ok((table, "tree-flatten"))
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
