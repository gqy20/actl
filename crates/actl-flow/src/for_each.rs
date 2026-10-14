//! for_each —— 数据驱动的有界迭代(docs/63 流程扩展收口;2026-10-11 spec):
//! 迭代对象 = 前置步骤 `outputs` 保存的**列表**(如 collect 的条目),子调用
//! 组每项执行一轮;迭代次数 = 列表长度(天然有界),另加显式 `max` 兜底
//! (超限 PROTOCOL,fail-closed 不静默截断)。
//!
//! 协议形态(YAML 层,expand 重写为 command 形态):
//! ```yaml
//! run:
//!   for_each: {from: "collect-steps.items", max: 32, field: name}
//!   body: [{command: click, args: ["name:搜索框"]}, ...]
//! ```
//! 子调用 args 中的字面 `"@each"` 占位符在派发时替换为当前项标量值
//! (值校验与 bind 同规:非选项/非 ref/无 NUL/≤16KiB——迭代体强制语义
//! 选择器,每轮重新观察,与"持久化流程禁 @eN"纪律同构)。
//!
//! 嵌套禁止(for_each/sequence 互不嵌套)、body ≤16 子调用、for_each 步
//! 禁 outputs;失败语义:第 i 项失败 → 步 Uncertain(写)/Failed(只读),
//! `for_each_progress` 持久化到失败项索引,`--retry-step` 自该项续跑——
//! 已完成项不重放(迭代粒度的幂等)。停止/暂停检查在项间进行。
use crate::Invocation;
use actl_core::CtlError;
pub const COMMAND: &str = "workflow-for-each";

/// 迭代配置(args[0] JSON;expand 重写自 YAML `for_each:` 块)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForEachSpec {
    /// 引用前置步骤的 array 型输出("stepid.output")。
    pub from: String,
    /// 显式上限(1..=256;缺省 32)。列表超限 PROTOCOL,不静默截断。
    pub max: usize,
    /// 可选:每项从对象成员取标量(如 collect 条目的 name)。
    pub field: Option<String>,
}

/// 长度上限(register:与 bind/输出同量级的防御预算)。
pub const MAX_ITEMS: usize = 256;
pub const DEFAULT_MAX: usize = 32;
/// body 子调用上限(与 sequence 同规)。
pub const MAX_BODY: usize = 16;
/// 子调用 args 中的项占位符(整参数精确匹配,派发时替换)。
pub const ITEM_TOKEN: &str = "@each";

#[derive(Clone, Debug)]
pub struct ForEach {
    pub spec: ForEachSpec,
    pub body: Vec<Invocation>,
}

/// 展开 YAML 授权形态(`run: {for_each: {...}, body: [...]}`)为 command
/// 形态(`{command: workflow-for-each, args: [config-json, child-json...]}`),
/// 并在展开时校验(children 形状/嵌套/bind/占位规则)。镜像 sequence::expand。
pub fn expand(raw: &mut serde_json::Value) -> Result<(), CtlError> {
    // 2026-10-14 bookends 组合面:三段统一展开(镜像 sequence::expand;
    // 键消失后幂等 no-op)
    for section in ["prologue", "steps", "epilogue"] {
        let Some(steps) = raw
            .get_mut(section)
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for step in steps {
            for field in ["run", "otherwise"] {
                let Some(run) = step.get_mut(field) else {
                    continue;
                };
                if run.get("for_each").is_none() {
                    continue;
                }
                if run.as_object().is_none_or(|object| object.len() != 2)
                    || run.get("body").is_none()
                {
                    return Err(CtlError::protocol(
                        "for_each requires exactly the keys: for_each (config) and body (calls)",
                    ));
                }
                let from = run
                    .get("for_each")
                    .and_then(|block| block.get("from"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| CtlError::protocol("for_each.from is required"))?
                    .to_owned();
                let max = match run.get("for_each").and_then(|block| block.get("max")) {
                    None => DEFAULT_MAX,
                    Some(value) => value
                        .as_u64()
                        .filter(|v| (1..=MAX_ITEMS as u64).contains(v))
                        .ok_or_else(|| CtlError::protocol("for_each.max must be 1..=256"))?
                        as usize,
                };
                let field_name = match run.get("for_each").and_then(|block| block.get("field")) {
                    None => None,
                    Some(value) => Some(
                        value
                            .as_str()
                            .filter(|f| !f.is_empty())
                            .ok_or_else(|| {
                                CtlError::protocol("for_each.field must be a non-empty string")
                            })?
                            .to_owned(),
                    ),
                };
                if !valid_reference(&from) {
                    return Err(CtlError::protocol(
                        "for_each.from must be \"stepid.output\"",
                    ));
                }
                let body = run
                    .get("body")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| CtlError::protocol("for_each body must be an array"))?;
                let args = std::iter::once(
                    serde_json::json!({"from": from, "max": max, "field": field_name}),
                )
                .chain(body.iter().cloned())
                .map(|value| {
                    serde_json::to_string(&value).map_err(|e| CtlError::internal(e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?;
                let call = Invocation {
                    command: COMMAND.into(),
                    args,
                    bind: Default::default(),
                };
                decode(&call)?;
                *run = serde_json::to_value(call).map_err(|e| CtlError::internal(e.to_string()))?;
            }
        }
    }
    Ok(())
}

/// command 形态解码:args[0] = 配置,其余 = body 子调用(1..=16)。
/// 校验:子调用禁嵌套(for_each/sequence)、禁 bind、args 至少含一个
/// ITEM_TOKEN 占位(占位缺失 = 该子调用与迭代无关,应放普通步骤)。
pub fn decode(call: &Invocation) -> Result<Option<ForEach>, CtlError> {
    if call.command != COMMAND {
        return Ok(None);
    }
    if !call.bind.is_empty() || call.args.len() < 2 || call.args.len() > MAX_BODY + 1 {
        return Err(CtlError::protocol(
            "for_each requires 1-16 body calls and no output bindings",
        ));
    }
    let config: serde_json::Value = serde_json::from_str(&call.args[0])
        .map_err(|_| CtlError::protocol("invalid for_each configuration"))?;
    let spec = ForEachSpec {
        from: config
            .get("from")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| CtlError::protocol("for_each.from is required"))?
            .to_owned(),
        max: config
            .get("max")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_MAX as u64) as usize,
        field: config
            .get("field")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    };
    if !(1..=MAX_ITEMS).contains(&spec.max) {
        return Err(CtlError::protocol("for_each.max must be 1..=256"));
    }
    let body = call.args[1..]
        .iter()
        .map(|text| {
            serde_json::from_str::<Invocation>(text)
                .map_err(|_| CtlError::protocol("invalid for_each invocation"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for child in &body {
        if matches!(child.command.as_str(), COMMAND | crate::sequence::COMMAND)
            || !child.bind.is_empty()
        {
            return Err(CtlError::protocol(
                "for_each body cannot nest for_each/sequence or bind outputs",
            ));
        }
        if !child.args.iter().any(|a| a.contains(ITEM_TOKEN)) {
            return Err(CtlError::protocol(format!(
                "for_each body call must reference the item via the {ITEM_TOKEN:?} placeholder"
            )));
        }
    }
    Ok(Some(ForEach { spec, body }))
}

/// 项标量值:field 给定 → 取对象成员;否则项即标量。非标量 PROTOCOL。
pub fn item_text(item: &serde_json::Value, field: Option<&str>) -> Result<String, CtlError> {
    let value = match field {
        Some(field) => item
            .get(field)
            .ok_or_else(|| CtlError::protocol("for_each item field missing"))?,
        None => item,
    };
    match value {
        serde_json::Value::String(text) => Ok(text.clone()),
        serde_json::Value::Number(_) | serde_json::Value::Bool(_) => Ok(value.to_string()),
        _ => Err(CtlError::protocol(
            "for_each items must be scalars (or objects with a scalar field)",
        )),
    }
}

/// 失败证据:迭代位置随错误携带(与 sequence::failure 同形追加)。
pub fn failure(mut error: CtlError, item: usize, child: usize, started: bool) -> CtlError {
    if started {
        let evidence = error.evidence.get_or_insert_with(|| serde_json::json!({}));
        if !evidence.is_object() {
            *evidence = serde_json::json!({});
        }
        evidence["action_started"] = serde_json::json!(true);
        evidence["for_each_item"] = serde_json::json!(item);
        evidence["for_each_child"] = serde_json::json!(child);
    }
    error
}

/// `from` 引用形态:两段有效 id("stepid.output",id 规则同 outputs 名)。
fn valid_reference(from: &str) -> bool {
    match from.split_once('.') {
        Some((producer, output)) => {
            actl_core::history::valid_id(producer) && actl_core::history::valid_id(output)
        }
        None => false,
    }
}
