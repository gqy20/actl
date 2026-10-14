//! query —— 观察与编排类命令执行体:commands/status 元命令、snapshot/find/get/
//! verify/extract/wait 观察、list-windows、剪贴板数据面、batch 编排器。
//! 操作类(注入/指针/窗口操作/launch)见 `interact`。

use std::time::Instant;

use actl_core::timing;
use actl_core::{CtlError, SnapshotBuilder};
use clap::{CommandFactory, Parser};
use serde_json::{Value, json};

use crate::cli::{Cli, Command};
use crate::interact::replay_guard;
use crate::{CommandOutput, dispatch};

/// L2 自发现(06 §3.5):从 clap 命令树同源生成 schema,不手写命令清单。
pub(crate) fn commands() -> Result<Value, CtlError> {
    let root = Cli::command();
    let mut list = Vec::new();
    for sub in root.get_subcommands() {
        if sub.get_name() == "help" {
            continue; // clap 自动生成的 help 不是命令面的一部分
        }
        let args: Vec<Value> = sub
            .get_arguments()
            .filter(|a| !matches!(a.get_id().as_str(), "help" | "version"))
            .map(|a| {
                json!({
                    "name": a.get_id().as_str(),
                    "help": a.get_help().map(|h| h.to_string()),
                })
            })
            .collect();
        let name = sub.get_name();
        list.push(json!({
            "name": name,
            "family": family_of(name),
            "about": sub.get_about().map(|h| h.to_string()),
            "args": args,
        }));
    }
    Ok(json!({ "commands": list }))
}

/// 命令族归类(2026-10-14 审计轮 P1):97 命令平铺无子命令,族信息此前
/// 只在命名惯例里——family 让机器消费方一读即知分族(verify 族里
/// verify-owner 是治理动作、clipboard 族是快照对这类子结构)。
/// 规则:连字符前缀即族;显式映射覆盖例外(无前缀的观察/桌面命令)。
fn family_of(name: &str) -> String {
    match name {
        // 观察族(无前缀的核心读命令)
        "commands" | "status" | "snapshot" | "find" | "get" | "wait" | "preview"
        | "preview-map" | "image-regions" | "verify-visual" | "screenshot" | "find-image"
        | "collect" | "extract" | "history" => "observe".into(),
        // 输入动作族
        "click" | "type" | "press" | "paste" | "set-value" | "focus" | "right-click"
        | "double-click" | "middle-click" | "triple-click" | "hover" | "drag" | "scroll"
        | "key-down" | "key-up" | "select" | "toggle" => "input".into(),
        // 窗口/桌面治理族
        "focus-window" | "minimize-window" | "maximize-window" | "restore-window"
        | "resize-window" | "close-window" | "list-windows" | "desktop-prepare"
        | "desktop-restore" | "get-selection" | "task-archive" => "window".into(),
        // 决策/恢复族
        "decide" | "execution-status" | "execution-recover" => "decision".into(),
        // 剪贴板读写对(get/set 与 save/restore 同族:读写+快照两对偶)
        "get-clipboard" | "set-clipboard" => "clipboard".into(),
        // 桌面诊断三报告(display 侧监督诊断;非输入动作)
        "focus-report" | "front-report" | "residue-report" => "diagnostic".into(),
        // 应用生命周期(launch 与 app-locate/register 同域)
        "launch" => "app".into(),
        // 挑战解题(browser 轨的闭环动作)
        "challenge-solve" => "browser".into(),
        // 探索回写(flow 轨)
        "explore-writeback" => "flow".into(),
        // batch 是多命令批执行器(自成一族)
        "batch" => "batch".into(),
        // 其余:连字符首段即族(flow-* -> flow, browser-* -> browser ...)
        _ => name.split('-').next().unwrap_or(name).to_string(),
    }
}

pub(crate) fn status() -> Result<Value, CtlError> {
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": actl_core::ENVELOPE_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    }))
}

/// snapshot 的 CLI 参数束(多参告警规避;main.rs 从 clap 变体直构)。
pub(crate) struct SnapshotArgs<'a> {
    pub app: Option<&'a str>,
    pub hwnd: Option<&'a str>,
    pub skeleton: bool,
    pub view: Option<crate::cli::ViewMode>,
    pub keywords: Option<&'a str>,
    pub within: Option<&'a str>,
    pub depth: Option<u32>,
    pub limit: Option<usize>,
}

pub(crate) fn snapshot_cmd(args: SnapshotArgs<'_>) -> Result<CommandOutput, CtlError> {
    let SnapshotArgs {
        app,
        hwnd,
        skeleton,
        view,
        keywords,
        within,
        depth,
        limit,
    } = args;
    use actl_core::Projection;
    let (cap, scope) = if let Some(within) = within {
        let window = hwnd
            .map(|h| format!("hwnd:{h}"))
            .or_else(|| app.map(str::to_owned))
            .ok_or_else(|| CtlError::protocol("snapshot --within requires --app or --hwnd"))?;
        let (cap, scope) = actl_uia::scoped_snapshot::capture(
            &window,
            within,
            depth.unwrap_or(actl_uia::MAX_DEPTH),
            limit.unwrap_or(actl_uia::MAX_ELEMENTS),
        )?;
        (cap, Some(scope))
    } else {
        (
            match (app, hwnd) {
                (_, Some(h)) => actl_uia::capture_hwnd(actl_uia::parse_hwnd(h)?)?,
                (a, None) => actl_uia::capture(a)?,
            },
            None,
        )
    };
    let observed_nodes = cap.nodes.len();
    if matches!(view, Some(crate::cli::ViewMode::Content))
        && !cap.nodes.iter().any(|node| node.role == "Document")
    {
        cap.require_complete()?;
    }
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    // 投影映射:--view content 与 --skeleton 正交组合(折叠仅作用于 Document
    // 子树内部);content 模式下无 Document → NOT_FOUND(core 层裁定);
    // --keywords 走任务关键词投影(与 --view 互斥,可与 --skeleton 组合)
    let projection = if let Some(raw) = keywords {
        let list: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if list.is_empty() || list.len() > 16 || list.iter().any(|k| k.len() > 64) {
            return Err(CtlError::protocol(
                "--keywords requires 1-16 non-empty comma-separated keywords (each ≤64 chars)",
            ));
        }
        Projection::Keyword {
            keywords: list,
            skeleton,
        }
    } else {
        match view {
            Some(crate::cli::ViewMode::Content) => Projection::Content { skeleton },
            None if skeleton => Projection::Skeleton,
            None => Projection::Full,
        }
    };
    let out = builder.finish(projection)?;
    actl_uia::persist_snapshot_scoped(
        &out.snapshot_id,
        &cap.window.title,
        &cap.window_runtime_id,
        &cap.element_identities,
        scope.as_ref(),
    )?;
    let mut data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "truncated": cap.truncated || out.truncated,
        "truncation_reasons": cap.truncation_reasons,
        // 物化形态(2026-10-12 交互轮):回落态下文本通道可能为空,调用方自查
        "materialization": cap.materialization,
        // 激活感知:目标曾挂起并被唤醒时如实上报等待量(0/未挂起时省略)
        "wake_ms": cap.wake_ms,
        "elements": serde_json::to_value(&out.elements).map_err(|e| CtlError::internal(e.to_string()))?,
    });
    if matches!(view, Some(crate::cli::ViewMode::Content)) {
        data["view"] = json!("content");
    }
    if let Some(scope) = scope {
        data["scope"] = scope;
        data["observed_nodes"] = json!(observed_nodes);
        data["ref_scope"] = json!("container");
    }
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

pub(crate) fn get(
    target_raw: &str,
    property: Option<&str>,
    app: Option<&str>,
    near: Option<&str>,
    snapshot: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let _replay = replay_guard(snapshot, app)?;
    let info = actl_uia::read_property(app, &target, property, near)?;
    Ok(property_data(&target.describe(), info).into())
}

fn property_data(target: &str, info: actl_uia::PropertyInfo) -> Value {
    let mut props = serde_json::Map::new();
    if let Some(v) = &info.name {
        props.insert("name".into(), json!(v));
    }
    if !info.role.is_empty() {
        props.insert("role".into(), json!(info.role));
    }
    if let Some(v) = &info.automation_id {
        props.insert("automation_id".into(), json!(v));
    }
    if let Some(v) = info.offscreen {
        props.insert("offscreen".into(), json!(v));
    }
    if let Some(v) = info.enabled {
        props.insert("enabled".into(), json!(v));
    }
    if let Some(v) = info.selected {
        props.insert("selected".into(), json!(v));
    }
    if let Some(v) = &info.value {
        props.insert("value".into(), json!(v));
    }
    if let Some(via) = info.value_via {
        props.insert("value_via".into(), json!(via));
    }
    if let Some(v) = &info.text {
        props.insert("text".into(), json!(v));
    }
    if let Some(r) = &info.rect {
        props.insert(
            "rect".into(),
            json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height}),
        );
    }
    json!({ "target": target, "properties": props, "unsupported": info.unsupported })
}

/// find --property 的逐元素读批(2026-10-14 读批轮):对单个命中元素读一个
/// 属性并按属性名附加到条目(键=value/text/enabled/...)。**单元素读失败
/// 诚实降级 read_error,不整体失败**——批量采集语义下一条坏不该毁全批
/// (虚拟化重建/离屏/通道不支持都是元素级事实);成本=每元素一次定位+读
/// (原生窗百 ms 级;Chromium 慢墙窗实测 ~2s/条,见 docs/83 慢墙观察项)。
fn attach_property(
    el: &mut Value,
    app: Option<&str>,
    property: &str,
    read_target: &actl_core::Target,
) {
    match actl_uia::read_property(app, read_target, Some(property), None) {
        Ok(info) => {
            let v = match property {
                "name" => info.name.map(|v| json!(v)),
                "role" => (!info.role.is_empty()).then(|| json!(info.role)),
                "automation_id" => info.automation_id.map(|v| json!(v)),
                "offscreen" => info.offscreen.map(|v| json!(v)),
                "enabled" => info.enabled.map(|v| json!(v)),
                "selected" => info.selected.map(|v| json!(v)),
                "value" => info.value.map(|v| json!(v)),
                "text" => info.text.map(|v| json!(v)),
                "rect" => info
                    .rect
                    .map(|r| json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height})),
                _ => None,
            };
            match v {
                Some(v) => el[property] = v,
                // 通道不支持(NOT_ACTIONABLE 的读形态):值面 None=没读到,
                // 与读失败的 read_error 分开——元素存在,属性不可读
                None => {
                    el["read_error"] = json!("property_unreadable");
                }
            }
        }
        Err(e) => {
            el["read_error"] = json!(e.code);
        }
    }
}

/// find:条件查元素,复用 snapshot 的 ref 编号(同一 DFS,同一编号空间,
/// find 的 @eN 与 snapshot 的 @eN 可互换)。至少一个过滤条件,反对全树 dump。
/// `--capable click|type`:role 表派生的建议性动作兼容过滤(06 §3.6),
/// 零额外属性读;滤空即如实空结果,实际能力以动作时 pattern 判定为准。
#[allow(clippy::too_many_arguments)]
pub(crate) fn find_cmd(
    target: Option<&str>,
    name: Option<&str>,
    role: Option<&str>,
    id: Option<&str>,
    capable: Option<&str>,
    property: Option<&str>,
    app: Option<&str>,
    limit: Option<usize>,
) -> Result<CommandOutput, CtlError> {
    // 取值校验先于任何 UIA 触碰(参数语义错误与观察失败分开)
    let capability = capable
        .map(actl_core::snapshot::Capability::parse)
        .transpose()?;
    // 读批属性(--property):白名单与 get 同源(read::PROPS 的 CLI 侧镜像,
    // clap value_parser 已限值,此处防漂移双保险)
    let property = property
        .map(|p| {
            actl_uia::PROPS.contains(&p).then_some(p).ok_or_else(|| {
                CtlError::protocol(format!(
                    "unknown --property {p:?}: expected one of {:?}",
                    actl_uia::PROPS
                ))
            })
        })
        .transpose()?;
    if let Some(raw) = target {
        let target = actl_core::parse_target(raw)?;
        let loc = actl_uia::locate(app, &target, None)?;
        let mut el = json!({
            "ref":null,"role":loc.role,"name":loc.name,
            "automation_id":loc.automation_id,"target":target.describe()});
        if let Some(p) = property {
            attach_property(&mut el, app, p, &target);
        }
        return Ok(json!({
            "window":{"title":loc.window_title}, "total":1,
            "elements":[el],
            "resolved_by":loc.resolved_by,
        })
        .into());
    }
    if name.is_none() && role.is_none() && id.is_none() {
        return Err(CtlError::protocol(
            "find needs at least one of --name/--role/--id (full-tree dump is `snapshot`)",
        ));
    }
    // --id 单一条件:provider 侧精确快路径(单次跨进程调用,冷 provider 重树
    // 分钟级→秒级;id 精确等值语义与全树过滤等价不漏报)。命中走 L4 同形态
    // (无 @eN ref,target 语义);无命中回落原全树路径(保快照面与 L4 兜底)。
    // --name/--role 为子串语义,UIA 属性条件只支持等值,不走此路径。
    // --capable 对快路径命中做同口径过滤;滤空回落全树路径重判,非静默分叉。
    if id.is_some() && name.is_none() && role.is_none() {
        let hits = actl_uia::find_by_id_provider(app, id.unwrap_or_default())?;
        let filtered: Vec<_> = hits
            .into_iter()
            .filter(|h| capability.is_none_or(|c| c.matches_role(&h.role)))
            .collect();
        if !filtered.is_empty() {
            let limit_n = limit.unwrap_or(20);
            let total = filtered.len();
            let elements: Vec<Value> = filtered
                .iter()
                .take(limit_n)
                .map(|h| {
                    let mut el = json!({
                        "ref": null, "role": h.role, "name": h.name,
                        "automation_id": h.automation_id,
                        "target": format!("id:{}", id.unwrap_or_default()),
                    });
                    if let Some(p) = property {
                        // 快路径命中无 ref,按 id: 语义逐个重读
                        let t = actl_core::parse_target(&format!("id:{}", id.unwrap_or_default()))
                            .expect("id target parses");
                        attach_property(&mut el, app, p, &t);
                    }
                    el
                })
                .collect();
            return Ok(json!({
                "window": {"title": filtered[0].window_title},
                "total": total,
                "elements": elements,
                "resolved_by": "provider-exact",
            })
            .into());
        }
    }
    let cap = actl_uia::capture(app)?;
    cap.require_complete()?;
    let mut builder = SnapshotBuilder::new(actl_uia::MAX_ELEMENTS + 1);
    for node in cap.nodes {
        if !builder.push(node) {
            break;
        }
    }
    // find 定向查找不做投影:命中集本就窄,投影反而可能滤掉候选
    let out = builder.finish(actl_core::Projection::Full)?;
    actl_uia::persist_snapshot(
        &out.snapshot_id,
        &cap.window.title,
        &cap.window_runtime_id,
        &cap.element_identities,
    )?;
    let role_want = role.map(str::to_lowercase);
    let matched: Vec<&actl_core::snapshot::ElementOut> = out
        .elements
        .iter()
        .filter(|e| {
            name.map(|n| e.name.as_deref().is_some_and(|en| en.contains(n)))
                .unwrap_or(true)
        })
        .filter(|e| {
            role_want
                .as_ref()
                .map(|r| e.role.to_lowercase().contains(r.as_str()))
                .unwrap_or(true)
        })
        .filter(|e| {
            id.map(|i| e.automation_id.as_deref() == Some(i))
                .unwrap_or(true)
        })
        .filter(|e| capability.map(|c| c.matches_role(&e.role)).unwrap_or(true))
        .collect();
    // 虚拟化容器兜底:已物化树 0 命中且查询可映射为精确属性时,走定位链 L4
    // (ItemContainerPattern;Explorer 文件列表等,US1 开放项)。命中元素不在
    // 快照树内 → 无 @eN ref,返回 name/id 选择器语义(agent 重放走同一查询)。
    let mut virtual_hit: Option<(actl_core::snapshot::ElementOut, &'static str)> = None;
    if matched.is_empty()
        && let Some(raw_target) = name
            .map(|n| format!("name:{n}"))
            .or_else(|| id.map(|i| format!("id:{i}")))
        && let Ok(target) = actl_core::parse_target(&raw_target)
        && let Some(loc) = match actl_uia::locate(app, &target, None) {
            Ok(loc) => Some(loc),
            Err(e) if e.code == actl_core::ErrorCode::NotFound => None,
            Err(e) => return Err(e),
        }
        && loc.resolved_by == "item-container"
        && role_want
            .as_ref()
            .is_none_or(|r| loc.role.to_lowercase().contains(r))
        && id.is_none_or(|i| loc.automation_id.as_deref() == Some(i))
        && capability.is_none_or(|c| c.matches_role(&loc.role))
    {
        virtual_hit = Some((
            actl_core::snapshot::ElementOut {
                ref_id: None,
                role: loc.role.clone(),
                name: loc.name.clone(),
                automation_id: loc.automation_id.clone(),
                depth: 0,
            },
            loc.resolved_by,
        ));
    }
    let total = matched.len() + virtual_hit.is_some() as usize;
    let limit = limit.unwrap_or(20);
    let mut elements: Vec<Value> = matched
        .iter()
        .take(limit)
        .map(|e| serde_json::to_value(e).expect("element serializes"))
        .collect();
    // 读批核心(--property):limit 内全部命中逐个读属性,一次往返 N 字段全回。
    // @eN 与 find 输出同源(刚拍快照,新鲜);每元素一次定位+读是诚实成本,
    // 原生窗百 ms 级;单元素读失败降级 read_error 不毁全批。
    if let Some(p) = property {
        for e in elements.iter_mut() {
            let Some(n) = e
                .get("ref")
                .and_then(|r| r.as_str())
                .and_then(|r| r.strip_prefix("@e"))
                .and_then(|d| d.parse::<u32>().ok())
            else {
                continue;
            };
            attach_property(e, app, p, &actl_core::Target::Ref(n));
        }
    }
    if let Some((elem, level)) = virtual_hit {
        let mut el_v = serde_json::to_value(&elem).expect("element serializes");
        // 虚拟化命中无 ref:按 id:/name: 语义重读(与命中时同选择器)
        if let Some(p) = property {
            let t = elem
                .automation_id
                .as_deref()
                .map(|i| actl_core::Target::Id(i.to_owned()))
                .or_else(|| {
                    elem.name
                        .as_deref()
                        .map(|n| actl_core::Target::Name(n.to_owned()))
                });
            if let Some(t) = t {
                attach_property(&mut el_v, app, p, &t);
            }
        }
        elements.push(el_v);
        let data = json!({
            "window": {
                "title": cap.window.title,
                "class": cap.window.class,
                "pid": cap.window.pid,
            },
            "total": total,
            "returned": elements.len(),
            "resolved_by": level,
            "wake_ms": cap.wake_ms,
            "elements": elements,
        });
        return Ok(CommandOutput {
            data,
            snapshot_id: Some(out.snapshot_id),
        });
    }
    let data = json!({
        "window": {
            "title": cap.window.title,
            "class": cap.window.class,
            "pid": cap.window.pid,
        },
        "total": total,
        "returned": elements.len(),
        "wake_ms": cap.wake_ms,
        "elements": elements,
    });
    Ok(CommandOutput {
        data,
        snapshot_id: Some(out.snapshot_id),
    })
}

/// collect:虚拟化容器有界收集(06 §3.4;枚举主通道+滚动收敛兜底,两通道
/// 零输入注入)。预算校验先于 UIA 触碰(参数语义错误与观察失败分开)。
pub(crate) fn collect_cmd(
    target_raw: &str,
    role: Option<&str>,
    rounds: Option<usize>,
    max_items: Option<usize>,
    app: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    const DEFAULT_ROUNDS: usize = 50;
    const DEFAULT_MAX_ITEMS: usize = 500;
    const ROUNDS_LIMIT: usize = 200;
    const MAX_ITEMS_LIMIT: usize = 5000;
    let rounds = rounds.unwrap_or(DEFAULT_ROUNDS);
    let max_items = max_items.unwrap_or(DEFAULT_MAX_ITEMS);
    if !(1..=ROUNDS_LIMIT).contains(&rounds) {
        return Err(CtlError::protocol(format!(
            "collect --rounds must be 1..={ROUNDS_LIMIT}, got {rounds}"
        )));
    }
    if !(1..=MAX_ITEMS_LIMIT).contains(&max_items) {
        return Err(CtlError::protocol(format!(
            "collect --max-items must be 1..={MAX_ITEMS_LIMIT}, got {max_items}"
        )));
    }
    let target = actl_core::parse_target(target_raw)?;
    let opts = actl_uia::CollectOptions {
        max_rounds: rounds,
        max_items,
        settle_ms: 150,
        role_filter: role.map(str::to_lowercase),
    };
    let report = actl_uia::collect(app, &target, &opts)?;
    Ok(collect_data(&report, &target.describe()).into())
}

/// collect 成功 envelope 的 data 面(纯函数,golden 钉契约)。
fn collect_data(report: &actl_uia::CollectReport, target: &str) -> Value {
    json!({
        "action": "collect",
        "target": target,
        "resolved_by": report.resolved_by,
        "container": {
            "role": report.container_role,
            "name": report.container_name,
            "automation_id": report.container_automation_id,
        },
        "total": report.items.len(),
        "items": report.items,
        "rounds": report.rounds,
        "new_per_round": report.new_per_round,
        "complete": report.complete,
        "converged_by": report.converged_by,
        "truncation_reason": report.truncation_reason,
        "input_effects": { "keyboard": "none", "clipboard": "untouched", "pointer": "none" },
    })
}

pub(crate) fn get_clipboard_cmd() -> Result<CommandOutput, CtlError> {
    let text = actl_uia::get_clipboard_text()?;
    Ok(json!({ "text": text }).into())
}

pub(crate) fn set_clipboard_cmd(text: &str) -> Result<CommandOutput, CtlError> {
    actl_uia::set_clipboard_text(text)?;
    Ok(json!({ "chars": text.chars().count() }).into())
}

/// clipboard-save / clipboard-restore / clipboard-formats(2026-10-13 剪贴板轮):
/// RPA 经典坑——粘贴通道毁掉用户剪贴板(UiPath "Restore clipboard" 同款解法)。
/// 语义边界:恢复是编排层显式决定,paste 不自动恢复(防"agent 粘贴后用户
/// 新复制的内容被静默覆盖回旧值"的数据丢失事故)。
pub(crate) fn clipboard_save_cmd(id: &str) -> Result<CommandOutput, CtlError> {
    let data = actl_uia::clipboard_save(id)?;
    Ok(data.into())
}

pub(crate) fn clipboard_restore_cmd(id: &str) -> Result<CommandOutput, CtlError> {
    let data = actl_uia::clipboard_restore(id)?;
    Ok(data.into())
}

pub(crate) fn clipboard_formats_cmd() -> Result<CommandOutput, CtlError> {
    let data = actl_uia::clipboard_formats()?;
    Ok(data.into())
}

#[allow(clippy::too_many_arguments)] // 参数面与 clap 命令定义一一对应
pub(crate) fn wait_cmd(
    element: Option<&str>,
    window: Option<&str>,
    gone: Option<&str>,
    idle: Option<u64>,
    property: Option<&str>,
    contains: bool,
    stable: bool,
    app: Option<&str>,
    timeout: Option<u64>,
) -> Result<CommandOutput, CtlError> {
    let timeout = timeout.unwrap_or(timing::WAIT_MS);
    let started = Instant::now();
    let what = if let Some(threshold) = idle {
        // --idle:系统空闲门(GetLastInputInfo;人不在的判定,与元素/窗口
        // 无关——长批任务的前置闸,"输入占用协调"的人类侧补集)。
        let observed = actl_uia::wait_idle(threshold as u32, timeout)?;
        json!({ "idle": true, "threshold_ms": threshold, "idle_ms": observed })
    } else {
        match (element, window, gone) {
            (Some(t), _, _) => {
                let target = actl_core::parse_target(t)?;
                if stable {
                    // settle 谓词:内容稳定等待(滚动/导航加载判定;与 --property
                    // 的"到达期望值"分工——stable 回答"什么时候停",不回答"是什么")
                    let out = actl_uia::wait_stable(app, &target, timeout)?;
                    json!({
                        "element": target.describe(),
                        "stable": true,
                        "samples": out.samples,
                        "changes": out.changes,
                        "last_len": out.last_len,
                    })
                } else if let Some(raw) = property {
                    // --property:谓词等待(隐含存在性;元素未解析继续等)
                    let mut pred = actl_uia::WaitPredicate::parse(raw)?;
                    pred.contains = contains;
                    let actual = actl_uia::wait_property(app, &target, &pred, timeout)?;
                    json!({ "element": target.describe(), "predicate": pred.describe(), "actual": actual })
                } else {
                    actl_uia::wait_element(app, &target, timeout)?;
                    json!({ "element": target.describe() })
                }
            }
            (None, Some(w), _) => {
                let title = actl_uia::wait_window(w, timeout)?;
                json!({ "window": title })
            }
            (None, None, Some(g)) => {
                let target = actl_core::parse_target(g)?;
                actl_uia::wait_gone(app, &target, timeout)?;
                json!({ "gone": target.describe() })
            }
            (None, None, None) => {
                return Err(CtlError::protocol(
                    "wait needs --element/--window/--gone/--idle",
                ));
            }
        }
    };
    Ok(json!({
        "waited": what,
        "waited_ms": started.elapsed().as_millis() as u64,
    })
    .into())
}

/// batch:单进程顺序执行(--json '["cmd args", ...]'),首错即停。
/// 每步结果与耗时进 `steps[]`(编排层可观测:哪步慢、哪步开始等待);
/// 失败时部分步骤随错误 evidence 带出(失败前的已执行步不再丢失)。
pub(crate) fn batch_cmd(json_str: &str) -> Result<CommandOutput, CtlError> {
    let cmds: Vec<String> = serde_json::from_str(json_str).map_err(|e| {
        CtlError::protocol(format!(
            "--json must be a JSON array of command strings: {e}"
        ))
    })?;
    if cmds.is_empty() {
        return Err(CtlError::protocol("batch needs at least one command"));
    }
    let mut executed = 0usize;
    let mut steps: Vec<Value> = Vec::with_capacity(cmds.len());
    // 窗口基线(进入时的 hwnd 集):结束时差集 = 本轮新开的窗口。
    // 如实上报(windows_created: title/pid/hwnd),不代管生命周期——
    // 关不关是编排层(agent/fixture)的决定,清理武器是 close-window --pid/--hwnd。
    let baseline = window_baseline(actl_uia::native_windows())?;
    // 弹窗闸门的合法目标集:batch 自身声明的 --app 值(launch-then-interact 模式里,
    // 自启窗口会成为前台,不该误报)
    let app_patterns: Vec<String> = cmds
        .iter()
        .filter_map(|l| {
            // 与步骤解析同源分词:引号里的含空格 --app 值同样要提取
            let toks = split_command_tokens(l).ok()?;
            toks.iter()
                .position(|t| *t == "--app")
                .and_then(|p| toks.get(p + 1).cloned())
        })
        .collect();
    for (i, line) in cmds.iter().enumerate() {
        // 步间双闸门:停止标志(用户热键)优先;随后步级状态落盘(阶段信号)
        actl_core::stop::check_current().map_err(|mut e| {
            e.message = format!(
                "batch halted at step {i} (after {executed} ok): {}",
                e.message
            );
            e
        })?;

        // 意外弹窗闸门(warning 不阻断):前台窗口若既不在基线、也不匹配本批任何
        // --app 目标,记入该步 blocked_by——弹窗可能是良性的,裁决权归编排层;
        // 步失败时它随 evidence 直接指出元凶
        let fg_blocked = || {
            actl_uia::foreground_window().and_then(|(title, hwnd)| {
                if baseline.contains(&hwnd)
                    || app_patterns
                        .iter()
                        .any(|p| batch_window_matches(p, &title, hwnd))
                {
                    None
                } else {
                    Some(json!({ "title": title, "hwnd": hwnd }))
                }
            })
        };
        // 步前采样一次(launch 后自启窗未成形的竞态由失败时刻补采样兜住)
        let mut blocked_by = fg_blocked();
        let tokens = split_command_tokens(line)
            .map_err(|e| CtlError::protocol(format!("batch[{i}]: {e}")))?;
        if tokens.is_empty() {
            return Err(CtlError::protocol(format!("batch[{i}] is empty")));
        }
        let sub =
            Cli::try_parse_from(std::iter::once("actl".to_string()).chain(tokens.iter().cloned()))
                .map_err(|e| CtlError::protocol(format!("batch[{i}] {line:?}: {e}")))?;
        if matches!(sub.command, Command::Batch { .. } | Command::Flow(_)) {
            return Err(CtlError::protocol(
                "batch cannot nest batch or flow commands",
            ));
        }
        actl_core::signal_session::step(
            crate::cli_help::command_name(&sub.command),
            crate::cli::app_of(&sub.command).as_deref(),
            i + 1,
            cmds.len(),
        );
        let step_started = Instant::now();
        match dispatch(&sub.command) {
            Ok(out) => {
                steps.push(ok_step(
                    i,
                    line,
                    blocked_by.as_ref(),
                    step_started.elapsed().as_millis() as u64,
                    &out,
                ));
            }
            Err(err) => {
                // 失败时刻补采样:launch 竞态/慢弹窗在步执行中才成形,
                // 此刻仍占据前台者即元凶候选
                if blocked_by.is_none() {
                    blocked_by = fg_blocked();
                }
                steps.push(err_step(
                    i,
                    line,
                    blocked_by.as_ref(),
                    step_started.elapsed().as_millis() as u64,
                    &err,
                ));
                let mut evidence = json!({"executed": executed, "steps": steps});
                attach_created_windows(&mut evidence, windows_created_since(&baseline));
                return Err(CtlError::with_evidence_hint(
                    err.code,
                    format!(
                        "batch[{i}] {:?} failed after {executed} ok: {}",
                        line, err.message
                    ),
                    evidence,
                    // 顶层错误继承首错的精确恢复路径:只读顶层 envelope 的
                    // 调用方与单命令调用拿到同一 hint,不退化到错误码缺省
                    err.effective_hint(),
                ));
            }
        }
        executed += 1;
    }
    let mut data = json!({ "executed": executed, "steps": steps });
    attach_created_windows(&mut data, windows_created_since(&baseline));
    Ok(data.into())
}

fn window_baseline(
    windows: Result<Vec<actl_uia::WindowEntry>, CtlError>,
) -> Result<std::collections::HashSet<isize>, CtlError> {
    Ok(windows?.iter().filter_map(|w| w.hwnd).collect())
}

fn batch_window_matches(pattern: &str, title: &str, hwnd: isize) -> bool {
    match pattern.strip_prefix("hwnd:") {
        Some(raw) => actl_uia::parse_hwnd(raw).is_ok_and(|target| target == hwnd),
        None => title.contains(pattern),
    }
}

/// batch 步命令分词:支持双/单引号包裹含空格的值(shell 语义子集)。
/// 反斜杠不是转义符——Windows 路径按字面保留(实证:`"E:\dir\file"` 在
/// POSIX 转义下会被吃成 `E:dirfile`,残渣参数曾实际产出)。未闭合引号
/// 拒绝(PROTOCOL),不静默吞掉半开的值。
fn split_command_tokens(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in line.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if (ch == '"' || ch == '\'') && !started => {
                // 行首(或空白后)的引号开启引用段;值中间的引号按字面保留
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(ch);
                started = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("unterminated quote {q} in command"));
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

/// 成功步条目:data 与 snapshot_id 进 steps[](2026-10-13 契约修复:
/// 读类命令在 batch 中曾丢值,9 字段读取误判全空引发连锁盲写;
/// snapshot 类步骤缺 snapshot_id 则后续步无 ref 可绑)。
fn ok_step(
    i: usize,
    cmd: &str,
    blocked_by: Option<&Value>,
    duration_ms: u64,
    out: &CommandOutput,
) -> Value {
    let mut entry = json!({
        "i": i, "cmd": cmd, "ok": true,
        "duration_ms": duration_ms,
        "data": out.data,
    });
    if let Some(id) = &out.snapshot_id {
        entry["snapshot_id"] = json!(id);
    }
    if let Some(b) = blocked_by {
        entry["blocked_by"] = b.clone();
    }
    entry
}

/// 失败步条目:error 与顶层 ErrorBody 同形(code/message/hint/evidence)——
/// 同一错误在单调用与 batch 步里形状一致,失败处置协议(先读 evidence/hint)
/// 在 batch 中不再失效(2026-10-13 实证:target_disabled 的 evidence 丢失
/// 致使误诊为身份漂移)。hint 走 effective_hint 单一渲染源。
fn err_step(
    i: usize,
    cmd: &str,
    blocked_by: Option<&Value>,
    duration_ms: u64,
    err: &CtlError,
) -> Value {
    let mut error = json!({
        "code": err.code,
        "message": err.message,
        "hint": err.effective_hint(),
    });
    if let Some(ev) = &err.evidence {
        error["evidence"] = ev.clone();
    }
    let mut entry = json!({
        "i": i, "cmd": cmd, "ok": false,
        "duration_ms": duration_ms,
        "error": error,
    });
    if let Some(b) = blocked_by {
        entry["blocked_by"] = b.clone();
    }
    entry
}

fn attach_created_windows(data: &mut Value, result: Result<Value, CtlError>) {
    match result {
        Ok(created) => {
            data["windows_created"] = created;
        }
        Err(error) => {
            data["window_observation_error"] =
                json!({"code": error.code, "message": error.message});
        }
    }
}

/// Native HWND difference, not task ownership. Observation errors propagate so
/// callers cannot mistake an unknown result for an empty window list.
fn windows_created_since(baseline: &std::collections::HashSet<isize>) -> Result<Value, CtlError> {
    let created: Vec<Value> = actl_uia::native_windows()?
        .into_iter()
        .filter(|w| w.hwnd.map(|h| !baseline.contains(&h)).unwrap_or(false))
        .map(|w| json!({ "title": w.title, "pid": w.pid, "hwnd": w.hwnd }))
        .collect();
    Ok(json!(created))
}

pub(crate) fn list_windows_cmd(
    native: bool,
    hwnd: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let wins = if let Some(raw) = hwnd {
        actl_uia::native_window(actl_uia::parse_hwnd(raw)?)?
    } else if native {
        actl_uia::native_windows()?
    } else {
        actl_uia::list_windows()?
    };
    let list: Vec<Value> = wins
        .into_iter()
        .map(|w| {
            let mut m = serde_json::Map::new();
            m.insert("title".into(), json!(w.title));
            m.insert("class".into(), json!(w.class));
            m.insert("pid".into(), json!(w.pid));
            if let Some(h) = w.hwnd {
                m.insert("hwnd".into(), json!(h));
            }
            m.insert("foreground".into(), json!(w.foreground));
            if w.topmost {
                m.insert("topmost".into(), json!(true));
            }
            if w.minimized {
                m.insert("minimized".into(), json!(true));
            }
            if !w.visible {
                m.insert("visible".into(), json!(false));
            }
            if let Some(z) = w.z_order {
                m.insert("z_order".into(), json!(z));
            }
            if let Some(r) = w.rect {
                m.insert("rect".into(), json!(r));
            }
            if let Some(o) = w.owner_title {
                m.insert("owner_title".into(), json!(o));
            }
            Value::Object(m)
        })
        .collect();
    Ok(json!({
        "windows": list,
        "count": list.len(),
        // 显示器拓扑(几何事实外露,2026-10-14):跨调用对比 monitors 数组
        // 可检测分辨率/DPI/主屏切换/拔插,提前触发重观察
        "monitors": actl_uia::monitors()?
            .into_iter()
            .map(|m| {
                json!({"rect": [m.rect.0, m.rect.1, m.rect.2, m.rect.3],
                       "primary": m.primary, "dpi": [m.dpi.0, m.dpi.1]})
            })
            .collect::<Vec<_>>(),
    })
    .into())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_cmd(
    exists: Option<&str>,
    value_target: Option<&str>,
    property: Option<&str>,
    expected: Option<&str>,
    contains: bool,
    checked: Option<&str>,
    app: Option<&str>,
    near: Option<&str>,
) -> Result<CommandOutput, CtlError> {
    let (raw, kind) = if let Some(t) = exists {
        (t, actl_uia::VerifyKind::Exists)
    } else if let Some(t) = value_target {
        (
            t,
            actl_uia::VerifyKind::Value {
                expected: expected.unwrap_or_default().to_string(),
                contains,
                property: property.unwrap_or("auto").into(),
            },
        )
    } else if let Some(t) = checked {
        (t, actl_uia::VerifyKind::Checked)
    } else {
        return Err(CtlError::protocol(
            "verify needs one of --exists/--value/--checked",
        ));
    };
    let target = actl_core::parse_target(raw)?;
    let out = actl_uia::verify(app, &target, near, kind).map_err(|mut error| {
        let mut evidence = error.evidence.take().unwrap_or_else(|| json!({}));
        if !evidence.is_object() {
            evidence = json!({"details":evidence});
        }
        evidence["target"] = json!(target.describe());
        evidence["property"] = json!(if exists.is_some() {
            "exists"
        } else if checked.is_some() {
            "checked"
        } else {
            property.unwrap_or("value")
        });
        evidence["expected"] = json!(if checked.is_some() {
            Some("On")
        } else {
            expected
        });
        if evidence.get("actual").is_none() {
            evidence["actual"] = Value::Null;
        }
        if evidence.get("channel").is_none() {
            evidence["channel"] = Value::Null;
        }
        error.evidence = Some(evidence);
        error
    })?;
    Ok(json!({
        "action": "verify",
        "mode": out.mode,
        "target": target.describe(),
        "resolved_by": out.resolved.resolved_by,
        "actual": out.actual,
        "channel": out.channel,
    })
    .into())
}

pub(crate) fn extract_cmd(target_raw: &str, app: Option<&str>) -> Result<CommandOutput, CtlError> {
    let target = actl_core::parse_target(target_raw)?;
    let table = actl_uia::extract_table(app, &target)?;
    Ok(table_data(&target.describe(), table).into())
}

fn table_data(target: &str, table: actl_uia::TableResult) -> Value {
    json!({
        "action": "extract",
        "target": target,
        "source": table.source,
        "fallback_reason": table.fallback_reason,
        "rows": table.rows.len(),
        "table": table.rows,
    })
}

#[cfg(test)]
mod collect_contract_tests {
    use super::*;

    #[test]
    fn collect_data_matches_golden_envelope() {
        // 契约 golden(06 §3.4):成功 envelope 的 data 面钉死——字段集、
        // items 形态(无 ref,role/name/automation_id/depth)、截断语义位
        // (complete/truncation_reason)与零输入注入申报。
        let report = actl_uia::CollectReport {
            resolved_by: "primary",
            container_role: "List".into(),
            container_name: Some("文件".into()),
            container_automation_id: None,
            items: vec![
                actl_core::collect::CollectItem {
                    role: "ListItem".into(),
                    name: Some("a.txt".into()),
                    automation_id: None,
                    depth: 3,
                },
                actl_core::collect::CollectItem {
                    role: "ListItem".into(),
                    name: Some("b.txt".into()),
                    automation_id: Some("i2".into()),
                    depth: 3,
                },
            ],
            rounds: 0,
            new_per_round: vec![],
            complete: true,
            converged_by: Some("empty-streak"),
            truncation_reason: None,
        };
        let data = collect_data(&report, "role:List");
        let golden: Value =
            serde_json::from_str(include_str!("../tests/golden/collect_report.json")).unwrap();
        assert_eq!(serde_json::to_value(data).unwrap(), golden);
    }

    #[test]
    fn collect_budget_validation_rejects_out_of_range() {
        // 参数语义错误先于 UIA 触碰(与观察失败分开)
        assert!(collect_cmd("role:List", None, Some(0), None, None).is_err());
        assert!(collect_cmd("role:List", None, Some(201), None, None).is_err());
        assert!(collect_cmd("role:List", None, None, Some(0), None).is_err());
        assert!(collect_cmd("role:List", None, None, Some(5001), None).is_err());
        assert!(collect_cmd("not-a-target", None, None, None, None).is_err());
    }
}
#[cfg(test)]
mod window_baseline_tests {
    use super::*;
    #[test]
    fn enumeration_error_is_not_an_empty_baseline() {
        let error = CtlError::new(actl_core::ErrorCode::Internal, "enumeration failed");
        assert!(window_baseline(Err(error)).is_err());
    }
    #[test]
    fn hwnd_target_does_not_match_by_title() {
        assert!(batch_window_matches("hwnd:42", "Excel", 42));
        assert!(batch_window_matches("hwnd:0x2a", "Excel", 42));
        assert!(!batch_window_matches("hwnd:42", "hwnd:42", 7));
        assert!(batch_window_matches("Excel", "Book - Excel", 7));
    }
    #[test]
    fn failed_final_inventory_preserves_steps_and_reports_unknown_windows() {
        let mut data =
            json!({"executed":1,"steps":[{"i":0,"cmd":"status","ok":true,"duration_ms":0}]});
        attach_created_windows(
            &mut data,
            Err(CtlError::new(
                actl_core::ErrorCode::Internal,
                "enumeration failed",
            )),
        );
        let golden: Value = serde_json::from_str(include_str!(
            "../tests/golden/batch_window_observation_error.json"
        ))
        .unwrap();
        assert_eq!(data, golden);
        assert!(data.get("windows_created").is_none());
    }
}
#[cfg(test)]
mod batch_step_tests {
    use super::*;

    #[test]
    fn split_command_tokens_keeps_quoted_spaces_and_literal_backslashes() {
        let toks = split_command_tokens(
            r#"set-value id:address1 "350 5th Avenue" --verify-value "350 5th Avenue" --app hwnd:42"#,
        )
        .unwrap();
        assert_eq!(
            toks,
            vec![
                "set-value",
                "id:address1",
                "350 5th Avenue",
                "--verify-value",
                "350 5th Avenue",
                "--app",
                "hwnd:42",
            ]
        );
        // Windows 路径按字面保留:反斜杠不是转义符
        let path = split_command_tokens(r#"type id:line "E:\dir\file.txt" --app Notepad"#).unwrap();
        assert_eq!(path[2], r"E:\dir\file.txt");
        // 裸值含空格照旧按空白切分(旧行为)
        assert_eq!(
            split_command_tokens("status --app Notepad").unwrap(),
            vec!["status", "--app", "Notepad"]
        );
        // 值中间的引号按字面保留,不开启引用段
        assert_eq!(
            split_command_tokens("set-value id:x it\"s --app Notepad").unwrap()[2],
            "it\"s"
        );
        // 单引号同语义
        assert_eq!(
            split_command_tokens("set-value id:x 'New York' --app Notepad").unwrap()[2],
            "New York"
        );
        // 未闭合引号拒绝,不静默截断
        assert!(split_command_tokens(r#"set-value id:x "open --app Notepad"#).is_err());
    }

    #[test]
    fn ok_step_carries_data_and_snapshot_id() {
        let out = CommandOutput {
            data: json!({"version": "0.2.3"}),
            snapshot_id: Some("s123".into()),
        };
        let entry = ok_step(0, "snapshot", None, 5, &out);
        assert_eq!(entry["ok"], true);
        assert_eq!(entry["data"]["version"], "0.2.3");
        assert_eq!(entry["snapshot_id"], "s123");
        let no_snap = CommandOutput {
            data: Value::Null,
            snapshot_id: None,
        };
        let entry = ok_step(1, "status", Some(&json!({"title": "X"})), 3, &no_snap);
        assert!(entry.get("snapshot_id").is_none());
        assert_eq!(entry["blocked_by"]["title"], "X");
    }

    #[test]
    fn err_step_preserves_first_error_evidence_and_hint() {
        let err = CtlError::with_evidence_hint(
            actl_core::ErrorCode::NotActionable,
            "target is disabled at action time",
            json!({"reason": "target_disabled", "enabled": false}),
            "pick a writable target",
        );
        let entry = err_step(0, "set-value id:firstName X", None, 9, &err);
        assert_eq!(entry["ok"], false);
        assert_eq!(entry["error"]["code"], "NOT_ACTIONABLE");
        assert_eq!(entry["error"]["evidence"]["reason"], "target_disabled");
        assert_eq!(entry["error"]["hint"], "pick a writable target");
        // 无 evidence 的错误只缺 evidence 字段;hint 恒在(与顶层信封一致)
        let plain = err_step(1, "status", None, 1, &CtlError::protocol("bad"));
        assert!(plain["error"].get("evidence").is_none());
        assert_eq!(
            plain["error"]["hint"],
            actl_core::ErrorCode::Protocol.recovery_hint()
        );
    }
}
#[cfg(test)]
mod property_contract_tests {
    use super::*;
    #[test]
    fn table_fallback_preserves_empty_cells_and_explains_source() {
        let data = table_data(
            "id:table",
            actl_uia::TableResult {
                rows: vec![vec![String::new(), "second".into()]],
                source: "tree-flatten",
                fallback_reason: Some("grid_pattern_unsupported"),
            },
        );
        let envelope = actl_core::Envelope::success("extract", Some(data), 0);
        let expected: Value =
            serde_json::from_str(include_str!("../tests/golden/table_fallback.json")).unwrap();
        assert_eq!(serde_json::to_value(envelope).unwrap(), expected);
    }
    #[test]
    fn empty_value_and_unsupported_text_are_distinct_in_envelope() {
        let data = property_data(
            "id:input",
            actl_uia::PropertyInfo {
                name: None,
                role: String::new(),
                automation_id: None,
                offscreen: None,
                enabled: None,
                selected: None,
                value: Some(String::new()),
                value_via: Some("value-pattern"),
                text: None,
                rect: None,
                unsupported: vec!["text"],
            },
        );
        let envelope = actl_core::Envelope::success("get", Some(data), 0);
        let expected: Value =
            serde_json::from_str(include_str!("../tests/golden/get_empty_value.json")).unwrap();
        assert_eq!(serde_json::to_value(envelope).unwrap(), expected);
    }
    #[test]
    fn rect_property_maps_to_xywh_object() {
        // 2026-10-12 交互轮:get --property rect → {x,y,width,height}(PMv2)
        let data = property_data(
            "id:input",
            actl_uia::PropertyInfo {
                name: None,
                role: String::new(),
                automation_id: None,
                offscreen: None,
                enabled: Some(true),
                selected: None,
                value: None,
                value_via: None,
                text: None,
                rect: Some(actl_uia::ElementRect {
                    x: 10,
                    y: 20,
                    width: 300,
                    height: 40,
                }),
                unsupported: Vec::new(),
            },
        );
        assert_eq!(
            data["properties"]["rect"],
            serde_json::json!({"x": 10, "y": 20, "width": 300, "height": 40})
        );
    }
    #[test]
    fn enabled_serializes_into_properties_when_observed() {
        let data = property_data(
            "id:firstName",
            actl_uia::PropertyInfo {
                name: Some("First Name".into()),
                role: "Edit".into(),
                automation_id: None,
                offscreen: Some(false),
                enabled: Some(false),
                selected: Some(true),
                value: Some("Qingyu".into()),
                value_via: Some("value-pattern"),
                text: None,
                rect: None,
                unsupported: Vec::new(),
            },
        );
        assert_eq!(data["properties"]["selected"], true);
        assert_eq!(data["properties"]["enabled"], false);
        assert_eq!(data["properties"]["offscreen"], false);
        // None 保持省略语义:未观察不伪装缺席
        assert!(data["properties"].get("text").is_none());
    }
}
