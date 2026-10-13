//! read —— 读取与断言:get(属性白名单 + value/text 双通道)、verify(立即断言)、
//! extract --table(Grid 直读/树扁平化)、get-selection(选区读取,
//! TextPattern 范围优先→SelectionPattern 容器→如实 unavailable)、
//! 剪贴板读写(CF_UNICODETEXT)。

use actl_core::target::Target;
use actl_core::{CtlError, ErrorCode};
use uiautomation::UIElement;
use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

use crate::locate::locate;

/// get:读取元素属性;`property` 白名单,缺省返回基本全集。
/// 属性白名单。`value` = ValuePattern 直读;`text` = TextPattern 文档正文
/// (专用通道:Chromium 的 Document Value=URL 会掩盖正文,value 与 text 分工)。
/// `rect` = 物理像素包围盒(2026-10-12 交互轮补:此前元素坐标无 CLI 通道,
/// 调用方被迫手写 PS GetWindowRect 并踩 DPI 虚拟化陷阱)。
pub const PROPS: &[&str] = &[
    "name",
    "role",
    "automation_id",
    "offscreen",
    "enabled",
    "value",
    "text",
    "rect",
];

/// 元素物理像素包围盒(PMv2 口径,与 list-windows rect 同源)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElementRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct PropertyInfo {
    pub name: Option<String>,
    pub role: String,
    pub automation_id: Option<String>,
    pub offscreen: Option<bool>,
    /// UIA IsEnabled(2026-10-13 补:门户预填锁定字段禁用态此前只在写动作
    /// 失败后可见,`get --property enabled` 让写前判读成为只读一步)。
    pub enabled: Option<bool>,
    pub value: Option<String>,
    /// value 的读取通道；仅 ValuePattern 不支持时回退 TextPattern。
    pub value_via: Option<&'static str>,
    /// TextPattern 文档正文(--property text 专用)
    pub text: Option<String>,
    pub rect: Option<ElementRect>,
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
    // 属性读取区段进 UIA 看门狗(真机实证 2026-10-05:get --property value 在
    // 记事本 Document 上跨进程 COM 无界阻塞,进程级 300s 看门狗之后仍不收——
    // value/text/name/role 等模式调用与 click pattern 派发同族,locate 覆盖
    // 止于定位,读取是裸区段)。
    // 2026-10-06 增补:UIElement 的跨进程 Release 同属无界调用——guard 声明
    // 在 loc 之后,反序析构会让 Release 裸奔(真机实证:值读取成功后挂死在
    // 析构,内核态不可杀)。看门狗是单槽不能嵌套,故用显式 drop 保证
    // Release 也在区内、且与声明顺序无关。
    let _watchdog = crate::watchdog::enter();
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
    let rect = if want("rect") {
        let r = loc
            .element
            .get_bounding_rectangle()
            .map_err(|e| crate::read_channel::failure("rect", e))?;
        let rect = ElementRect {
            x: r.get_left(),
            y: r.get_top(),
            width: r.get_right() - r.get_left(),
            height: r.get_bottom() - r.get_top(),
        };
        if rect.width <= 0 || rect.height <= 0 {
            None
        } else {
            Some(rect)
        }
    } else {
        None
    };
    if want("rect") && rect.is_none() {
        unsupported.push("rect");
    }
    if property == Some("rect") && rect.is_none() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "element exposes no usable bounding rectangle (virtualized/offscreen)",
        ));
    }
    let info = PropertyInfo {
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
        enabled: if want("enabled") {
            Some(
                loc.element
                    .is_enabled()
                    .map_err(|e| crate::read_channel::failure("enabled", e))?,
            )
        } else {
            None
        },
        value,
        value_via,
        text,
        rect,
        unsupported,
    };
    // 显式 Release 进看门狗区(反序析构会让它裸奔——见函数头注释)。
    drop(loc);
    Ok(info)
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
    /// 定位元数据(COM 元素不出域:见 ResolvedInfo 注释,Release 必须在
    /// 看门狗区内完成——verify 各分支在区内 into_info)。
    pub resolved: crate::locate::ResolvedInfo,
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
            // Exists 无元素读取,但 Release 本身是跨进程调用——进区段再 into_info。
            let _watchdog = crate::watchdog::enter();
            Ok(VerifyOutcome {
                resolved: loc.into_info(),
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
            // 断言值读取同进 UIA 看门狗(read_property 同族;探索引擎的
            // goal 断言走此路径,裸区段即整轮探索的挂点)。
            let _watchdog = crate::watchdog::enter();
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
            let resolved = loc.into_info();
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
                resolved,
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
            // Toggle 状态读取同进 UIA 看门狗(read_property 同族)。
            let _watchdog = crate::watchdog::enter();
            let state = crate::read_channel::toggle(&loc.element)?;
            crate::read_channel::require(state.as_ref(), "toggle-pattern")?;
            let resolved = loc.into_info();
            if state.as_deref() != Some("On") {
                return Err(fail("checked", state));
            }
            Ok(VerifyOutcome {
                resolved,
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

/// get-selection 结果面(选区读取,docs/10 Quicker 差距登记项):
/// TextPattern 范围优先(文本选区)→ SelectionPattern 容器(列表勾选)→
/// 如实 unavailable。纯读取零注入;物理 Ctrl+C 通道不做(须显式物理授权
/// 的路径留给调用方组合,本命令不伪装成能读到)。
pub struct SelectionOutcome {
    pub available: bool,
    /// available=false 时:无选区 pattern(no_selection_support)或
    /// pattern 在但选区为空(no_selection)。
    pub reason: Option<&'static str>,
    /// "text"(TextPattern 范围)| "items"(SelectionPattern 容器)
    pub kind: Option<&'static str>,
    pub via: Option<&'static str>,
    /// text 通道:各选区范围的文本。
    pub ranges: Vec<String>,
    /// items 通道:各选中元素的元数据。
    pub items: Vec<serde_json::Value>,
}

/// 读取目标元素当前选区。调用方先 locate(元素来源任意,推荐 `focused:`
/// ——用户选区随键盘焦点所在控件)。
pub fn get_selection(
    loc: &crate::locate::Located,
    max_chars: usize,
) -> Result<SelectionOutcome, CtlError> {
    // TextPattern:GetSelection() 返回选区范围(空 vec = 无选区,不失败)
    if let Ok(text) = {
        let _watchdog = crate::watchdog::enter();
        loc.element
            .get_pattern::<uiautomation::patterns::UITextPattern>()
    } {
        let ranges = {
            let _watchdog = crate::watchdog::enter();
            text.get_selection()
                .map_err(|e| crate::read_channel::failure("selection.ranges", e))?
        };
        if !ranges.is_empty() {
            let mut texts = Vec::with_capacity(ranges.len());
            for range in &ranges {
                let text = {
                    let _watchdog = crate::watchdog::enter();
                    range
                        .get_text(max_chars as i32)
                        .map_err(|e| crate::read_channel::failure("selection.text", e))?
                };
                texts.push(text);
            }
            // UIA 退化范围(插入点)读为空串:过滤后全空 = 光标在但**什么都没
            // 选**——如实 no_selection,不把"只有 caret"报成有选区(2026-10-11
            // 记事本真机边界实证:折叠光标 GetSelection 返回 [""])。
            texts.retain(|t| !t.is_empty());
            if !texts.is_empty() {
                return Ok(SelectionOutcome {
                    available: true,
                    reason: None,
                    kind: Some("text"),
                    via: Some("text-pattern"),
                    ranges: texts,
                    items: Vec::new(),
                });
            }
            return Ok(SelectionOutcome {
                available: false,
                reason: Some("no_selection"),
                kind: None,
                via: None,
                ranges: Vec::new(),
                items: Vec::new(),
            });
        }
        // TextPattern 在但无选区:仍要看 SelectionPattern(列表勾选类)
    }
    // SelectionPattern:GetSelection() 返回选中元素(空 vec = 无选区)
    if let Ok(selection) = {
        let _watchdog = crate::watchdog::enter();
        loc.element
            .get_pattern::<uiautomation::patterns::UISelectionPattern>()
    } {
        let elements = {
            let _watchdog = crate::watchdog::enter();
            selection
                .get_selection()
                .map_err(|e| crate::read_channel::failure("selection.items", e))?
        };
        if !elements.is_empty() {
            let _watchdog = crate::watchdog::enter();
            let mut items = Vec::with_capacity(elements.len());
            for element in &elements {
                let role = crate::observation::role_of(element).map(|r| format!("{r:?}"))?;
                let name = crate::observation::name_of(element)?;
                let automation_id = crate::observation::automation_id_of(element)?;
                items.push(serde_json::json!({
                    "role": role,
                    "name": name,
                    "automation_id": automation_id,
                }));
            }
            return Ok(SelectionOutcome {
                available: true,
                reason: None,
                kind: Some("items"),
                via: Some("selection-pattern"),
                ranges: Vec::new(),
                items,
            });
        }
        return Ok(SelectionOutcome {
            available: false,
            reason: Some("no_selection"),
            kind: None,
            via: None,
            ranges: Vec::new(),
            items: Vec::new(),
        });
    }
    // 两个选区 pattern 都不支持:如实 unavailable(不猜测、不回退剪贴板)
    Ok(SelectionOutcome {
        available: false,
        reason: Some("no_selection_support"),
        kind: None,
        via: None,
        ranges: Vec::new(),
        items: Vec::new(),
    })
}
