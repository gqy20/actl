//! pack_ref 路线引用展开(2026-10-14 YAML 审计轮 P2):流程步声明
//! `pack_ref: <app>.<route>`,装载期展开为该 pack 路线的普通步骤——
//! 应用知识(pack)与流程编排(workflow)的引用断裂自此闭合:路线修正
//! 单点落 pack,引用它的流程零改动(此前只能整段复抄)。
//!
//! 诚实边界(v1,与协议纪律一致):
//! - pack 路线步是 `[command, args...]` 数组形态,展开为
//!   `{id: <父步id>-p<N>, title: <route id>, run: {command, args}}`;
//! - 占位符**仅支持整值 token** `{X}` → `{"input": X}` 按名绑定
//!   (流程须声明同名 input;输入校验负责 required input missing);
//! - **内插 token 拒绝**(`xy:{a},{b}`、`id:foo_{name}`):流程 v1 无
//!   参数插值(README 协议纪律),内插路线必须显式写步骤——PROTOCOL
//!   带该 token 证据,不是静默跳过;
//! - 运行时 ref 形态的路由({xxx_ref}/{xxx_snapshot_id})整值绑定后
//!   由 input 校验如实暴露(required input missing)——agent 据此识别
//!   该路由需显式步骤表达 ref 语义。
//!
//! 展开发生在模板文本装载层(CLI 的 load_plan/revise 两个装载点,
//! run/validate/revise 全覆盖);冻结计划、指纹、revise diff、恢复
//! 全部只见展开后的普通步骤,零改动(与 prologue/epilogue 同一设计
//! 纪律)。直接调 actl_flow::parse 的消费方不走本层(引擎保持纯)。

use actl_core::CtlError;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// 展开模板文本中的 pack_ref 步。无 pack_ref 时原文返回(零行为变化)。
pub fn expand_refs_in_text(text: &str, template_path: &Path) -> Result<String, CtlError> {
    let mut raw = crate::decode_yaml(text)?;
    expand_in_value(&mut raw, template_path)?;
    serde_json::to_string(&raw).map_err(|e| CtlError::internal(e.to_string()))
}

/// 值层展开:消费 steps 中的 pack_ref 步,原地替换为路线步骤。
pub fn expand_in_value(raw: &mut Value, template_path: &Path) -> Result<(), CtlError> {
    let Some(steps) = raw.get_mut("steps").and_then(Value::as_array_mut) else {
        return Ok(()); // explore 计划/无步骤:无 pack_ref 可言
    };
    let mut expanded: Vec<Value> = Vec::with_capacity(steps.len());
    for step in std::mem::take(steps) {
        let Some(reference) = step.get("pack_ref").and_then(Value::as_str) else {
            expanded.push(step);
            continue;
        };
        let parent_id = step.get("id").and_then(Value::as_str).unwrap_or("step");
        // 引用步只允许 id/title/pack_ref——携带 run/verify 等即规格歧义
        for key in step
            .as_object()
            .map(|o| o.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            if !matches!(key.as_str(), "id" | "title" | "pack_ref") {
                return Err(CtlError::protocol(format!(
                    "pack_ref step {parent_id:?} carries unexpected field {key:?} — a reference step only has id/title/pack_ref"
                )));
            }
        }
        let route_steps = load_route(reference, template_path)?;
        for (i, pack_step) in route_steps.iter().enumerate() {
            expanded.push(convert_step(pack_step, parent_id, reference, i)?);
        }
    }
    if let Some(obj) = raw.as_object_mut() {
        obj.insert("steps".to_string(), Value::Array(expanded));
    }
    Ok(())
}

/// 装载 `<app>.<route>`:根 = 模板目录的 ../apps 与 ./apps(仓库
/// workflows 布局零配置;找不到即 PROTOCOL 列出已搜索路径)。
fn load_route(reference: &str, template_path: &Path) -> Result<Vec<Value>, CtlError> {
    let Some((app, route)) = reference.split_once('.') else {
        return Err(CtlError::protocol(format!(
            "pack_ref {reference:?} must be <app>.<route-id>"
        )));
    };
    let dir = template_path.parent().unwrap_or(Path::new("."));
    let roots: Vec<PathBuf> = vec![dir.join("..").join("apps"), dir.join("apps")];
    let mut searched = Vec::new();
    for root in &roots {
        let pack_path = root.join(format!("{app}.yml"));
        if !pack_path.is_file() {
            searched.push(pack_path.display().to_string());
            continue;
        }
        let text = std::fs::read_to_string(&pack_path).map_err(|e| {
            CtlError::internal(format!("pack {} unreadable: {e}", pack_path.display()))
        })?;
        let pack = crate::decode_yaml(&text)?;
        let Some(routes) = pack.get("routes").and_then(Value::as_array) else {
            return Err(CtlError::protocol(format!(
                "pack {app:?} has no routes array (pack_format contract)"
            )));
        };
        let Some(route_entry) = routes
            .iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(route))
        else {
            let ids: Vec<&str> = routes
                .iter()
                .filter_map(|r| r.get("id").and_then(Value::as_str))
                .take(12)
                .collect();
            return Err(CtlError::with_evidence(
                actl_core::ErrorCode::Protocol,
                format!("pack {app:?} has no route {route:?}"),
                json!({"available": ids, "pack": pack_path.display().to_string()}),
            ));
        };
        let steps = route_entry
            .get("steps")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| CtlError::protocol(format!("route {reference:?} has no steps array")))?;
        return Ok(steps);
    }
    Err(CtlError::with_evidence(
        actl_core::ErrorCode::Protocol,
        format!("pack_ref {reference:?}: pack file {app:?}.yml not found"),
        json!({"searched": searched, "hint": "packs resolve relative to the workflow: <dir>/../apps and <dir>/apps"}),
    ))
}

/// 单条 pack 路线步 → 流程步对象。数组首元素 = 命令;token 为整值
/// `{X}` 时按名绑定输入,含内插字符则拒绝(v1 无插值协议纪律)。
fn convert_step(
    pack_step: &Value,
    parent_id: &str,
    reference: &str,
    index: usize,
) -> Result<Value, CtlError> {
    let arr = pack_step.as_array().ok_or_else(|| {
        CtlError::protocol(format!(
            "pack_ref {reference:?} step {index} must be a [command, args...] string array"
        ))
    })?;
    let mut iter = arr.iter();
    let command = iter
        .next()
        .and_then(Value::as_str)
        .ok_or_else(|| {
            CtlError::protocol(format!(
                "pack_ref {reference:?} step {index}: command must be a string"
            ))
        })?
        .to_string();
    let mut args: Vec<Value> = Vec::new();
    for (j, token) in iter.enumerate() {
        let token = token.as_str().ok_or_else(|| {
            CtlError::protocol(format!(
                "pack_ref {reference:?} step {index} arg {j} must be a string"
            ))
        })?;
        if token.starts_with('{')
            && token.ends_with('}')
            && !token[1..token.len() - 1].contains(['{', '}'])
        {
            let name = &token[1..token.len() - 1];
            if name.is_empty() || name.contains(['{', '}', ' ', ',']) {
                // 空名/非法名:按内插拒绝同路
                return Err(inline_rejected(reference, index, token));
            }
            args.push(json!({"input": name}));
        } else if token.contains(['{', '}']) {
            return Err(inline_rejected(reference, index, token));
        } else {
            args.push(Value::String(token.to_string()));
        }
    }
    Ok(json!({
        "id": format!("{parent_id}-p{index}"),
        "title": format!("{reference}#{index}"),
        "run": {"command": command, "args": args},
    }))
}

fn inline_rejected(reference: &str, index: usize, token: &str) -> CtlError {
    CtlError::with_evidence(
        actl_core::ErrorCode::Protocol,
        format!(
            "pack_ref {reference:?} step {index} uses inline interpolation {token:?} — flow v1 has no parameter interpolation"
        ),
        json!({"token": token, "hint": "whole-value tokens like {name} bind to a same-named input; inline-interpolated routes must be written as explicit steps"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "actl-packref-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("workflows")).unwrap();
        std::fs::create_dir_all(dir.join("apps")).unwrap();
        dir
    }

    const PACK: &str = "routes:\n  - id: scan\n    description: session scan\n    steps:\n      - [snapshot, --app, \"{window}\", --skeleton]\n      - [find, --name, ok, --app, \"{window}\"]\n  - id: interpolated\n    steps:\n      - [click, \"xy:{a},{b}\", --app, w]\n";

    #[test]
    fn whole_token_placeholders_bind_inputs_by_name() {
        let root = temp_root("bind");
        write(&root, "apps/demo.yml", PACK);
        let tpl = root.join("workflows/t.yml");
        write(
            &root,
            "workflows/t.yml",
            "version: 1\nname: T\nsteps:\n  - id: scan\n    pack_ref: demo.scan\n",
        );
        let text = std::fs::read_to_string(&tpl).unwrap();
        let out = expand_refs_in_text(&text, &tpl).unwrap();
        let v: Value = crate::decode_yaml(&out).unwrap();
        let steps = v["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["id"], "scan-p0");
        assert_eq!(steps[0]["run"]["command"], "snapshot");
        assert_eq!(steps[0]["run"]["args"][1], json!({"input": "window"}));
        assert_eq!(steps[1]["run"]["args"][3], json!({"input": "window"}));
    }

    #[test]
    fn no_pack_ref_is_verbatim_passthrough() {
        let tpl_path = Path::new("nowhere.yml");
        let text = "version: 1\nname: T\nsteps:\n  - id: a\n    title: A\n    run: {command: status, args: []}\n";
        assert_eq!(
            expand_refs_in_text(text, tpl_path).unwrap(),
            serde_json::to_string(&serde_json::json!({
                "version": 1, "name": "T",
                "steps": [{"id":"a","title":"A","run":{"command":"status","args":[]}}]
            }))
            .unwrap()
        );
    }

    #[test]
    fn inline_interpolation_is_rejected_with_evidence() {
        let root = temp_root("inline");
        write(&root, "apps/demo.yml", PACK);
        let tpl = root.join("workflows/t.yml");
        write(
            &root,
            "workflows/t.yml",
            "version: 1\nname: T\nsteps:\n  - id: x\n    pack_ref: demo.interpolated\n",
        );
        let text = std::fs::read_to_string(&tpl).unwrap();
        let err = expand_refs_in_text(&text, &tpl).unwrap_err();
        assert_eq!(err.code, actl_core::ErrorCode::Protocol);
        assert!(err.message.contains("inline interpolation"));
    }

    #[test]
    fn missing_pack_and_route_report_searched_paths() {
        let root = temp_root("missing");
        write(&root, "apps/demo.yml", PACK);
        let tpl = root.join("workflows/t.yml");
        write(
            &root,
            "workflows/t.yml",
            "version: 1\nname: T\nsteps:\n  - id: x\n    pack_ref: nosuch.scan\n",
        );
        let text = std::fs::read_to_string(&tpl).unwrap();
        let err = expand_refs_in_text(&text, &tpl).unwrap_err();
        assert_eq!(err.code, actl_core::ErrorCode::Protocol);
        assert!(err.message.contains("nosuch"));
        write(
            &root,
            "workflows/t.yml",
            "version: 1\nname: T\nsteps:\n  - id: x\n    pack_ref: demo.nosuchroute\n",
        );
        let text = std::fs::read_to_string(&tpl).unwrap();
        let err = expand_refs_in_text(&text, &tpl).unwrap_err();
        let evidence = err.evidence.unwrap();
        assert!(
            evidence["available"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a == "scan")
        );
    }

    #[test]
    fn reference_step_rejects_extra_fields() {
        let root = temp_root("extra");
        write(&root, "apps/demo.yml", PACK);
        let tpl = root.join("workflows/t.yml");
        write(
            &root,
            "workflows/t.yml",
            "version: 1\nname: T\nsteps:\n  - id: x\n    pack_ref: demo.scan\n    run: {command: status, args: []}\n",
        );
        let text = std::fs::read_to_string(&tpl).unwrap();
        let err = expand_refs_in_text(&text, &tpl).unwrap_err();
        assert_eq!(err.code, actl_core::ErrorCode::Protocol);
        assert!(err.message.contains("unexpected field"));
    }
}
