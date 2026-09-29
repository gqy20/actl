use crate::{Result, require};
use serde_json::{Value, json};
pub fn observable_window(window: &Value) -> bool {
    // list-windows deliberately omits visible=true and minimized=false.
    window["hwnd"].is_number()
        && window["pid"].is_number()
        && window["class"].is_string()
        && window["visible"] != false
        && window["minimized"] != true
}
pub fn percentile(values: &[f64], p: f64) -> Result<f64> {
    require(
        !values.is_empty() && p > 0. && p <= 1. && values.iter().all(|v| v.is_finite()),
        "invalid percentile samples",
    )?;
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    Ok(v[(p * v.len() as f64).ceil() as usize - 1])
}
fn complete(v: &Value) -> bool {
    v["truncated"] == false && v["elements"].is_array()
}
pub fn projection(before: &Value, compact: &Value, after: &Value) -> Value {
    if ![before, compact, after].iter().all(|v| complete(v)) {
        return json!({"status":"blocked","reason":"missing or truncated sample"});
    }
    if before["elements"] != after["elements"] {
        return json!({"status":"blocked","reason":"content changed"});
    }
    let full = before["elements"].to_string().len();
    let small = compact["elements"].to_string().len();
    json!({"status":"measured","full_bytes":full,"compact_bytes":small,"representative":before["elements"].as_array().is_some_and(|a|a.len()>1)&&compact["elements"].as_array().is_some_and(|a|!a.is_empty()),"reduction":1.-small as f64/full as f64,"unit":"UTF-8 bytes, not tokenizer tokens","atomic":false})
}
pub fn local(
    before: &Value,
    local: &Value,
    repeat: &Value,
    after: &Value,
    full_bytes: u64,
    local_bytes: u64,
) -> Value {
    let baseline = projection(before, local, after);
    if baseline["status"] != "measured" {
        return baseline;
    }
    if !complete(repeat)
        || local["ref_scope"] != "container"
        || repeat["ref_scope"] != "container"
        || local.get("scope").is_none()
        || local["scope"] != repeat["scope"]
        || local["elements"] != repeat["elements"]
        || full_bytes == 0
        || local_bytes == 0
        || baseline["representative"] != true
    {
        return json!({"status":"blocked","reason":"unstable, incomplete or unrepresentative local sample"});
    }
    let mut result = baseline;
    result["full_nodes"] = json!(before["elements"].as_array().map(Vec::len));
    result["local_nodes"] = json!(local["elements"].as_array().map(Vec::len));
    result["full_envelope_bytes"] = json!(full_bytes);
    result["local_envelope_bytes"] = json!(local_bytes);
    result["envelope_reduction"] = json!(1. - local_bytes as f64 / full_bytes as f64);
    result
}
pub fn matching_identity(owner: &Value, window: &Value, start: &Value) -> bool {
    owner["hwnd"] == window["hwnd"]
        && owner["pid"] == window["pid"]
        && owner["class"] == window["class"]
        && owner["start"] == *start
        && owner["marker"].as_str().is_some_and(|m| {
            !m.is_empty() && window["title"].as_str().is_some_and(|t| t.contains(m))
        })
}
pub fn owned_tab(elements: &[Value], marker: &str, notepad: bool) -> Result<String> {
    let indices: Vec<_> = elements
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            v["role"] == "TabItem" && v["name"].as_str().is_some_and(|s| s.contains(marker))
        })
        .map(|(i, _)| i)
        .collect();
    require(indices.len() == 1, "owned tab is not unique")?;
    let i = indices[0];
    let depth = elements[i]["depth"]
        .as_u64()
        .ok_or_else(|| crate::io("missing tab depth"))?;
    if notepad {
        require(
            elements[i]["name"]
                .as_str()
                .is_some_and(|s| s.contains("未修改")),
            "task tab not proven saved",
        )?;
        let children: Vec<_> = elements[i + 1..]
            .iter()
            .take_while(|v| v["depth"].as_u64().is_some_and(|d| d > depth))
            .filter(|v| v["role"] == "Button" && v["automation_id"] == "CloseButton")
            .collect();
        require(children.len() == 1, "owned close button not unique")?;
        return children[0]["ref"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| crate::io("close ref missing"));
    }
    let parent = (0..i)
        .rev()
        .find(|&n| elements[n]["depth"].as_u64().is_some_and(|d| d < depth))
        .ok_or_else(|| crate::io("tab strip missing"))?;
    require(
        elements[parent]["role"] == "Tab",
        "tab strip not identified",
    )?;
    let pd = elements[parent]["depth"]
        .as_u64()
        .ok_or_else(|| crate::io("tab strip depth missing"))?;
    let siblings = elements[parent + 1..]
        .iter()
        .take_while(|v| v["depth"].as_u64().is_some_and(|d| d > pd))
        .filter(|v| v["role"] == "TabItem" && v["depth"] == pd + 1)
        .count();
    require(siblings == 1, "user tabs present; preserve window")?;
    Ok(String::new())
}
pub fn summary_table(table: &Value) -> Result<()> {
    require(
        ["tree-flatten", "grid-pattern"].contains(&table["source"].as_str().unwrap_or("")),
        "unexpected table source",
    )?;
    let rows = table["table"]
        .as_array()
        .ok_or_else(|| crate::io("missing table rows"))?;
    require(
        rows.len() >= 30 && table["rows"] == rows.len(),
        "incomplete table",
    )?;
    for row in rows {
        let cells = row.as_array().ok_or_else(|| crate::io("invalid row"))?;
        require(
            cells.len() == 2
                || (table["source"] == "tree-flatten"
                    && cells.len() == 1
                    && cells[0] == "已检测到虚拟机监控程序。将不显示 Hyper-V 所需的功能。"),
            "invalid summary row",
        )?;
    }
    require(
        rows.iter()
            .filter(|r| {
                r[0] == "操作系统名称"
                    && r[1]
                        .as_str()
                        .is_some_and(|s| s.contains("Microsoft Windows"))
            })
            .count()
            == 1
            && rows
                .iter()
                .any(|r| r[0].as_str().is_some_and(|s| s.contains("BIOS"))),
        "OS or BIOS evidence missing",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_visibility_is_observable_but_explicit_hidden_is_not() {
        let mut w = json!({"hwnd":1,"pid":2,"class":"Static"});
        assert!(observable_window(&w));
        w["visible"] = json!(false);
        assert!(!observable_window(&w));
        assert!(!observable_window(&json!({})));
    }
    #[test]
    fn sample_math_and_unstable_refusal() {
        assert_eq!(percentile(&[10., 1., 4., 5.], 0.5).unwrap(), 4.);
        assert!(percentile(&[], 0.5).is_err());
        let a = json!({"truncated":false,"elements":[{"name":"中文"},{"name":"b"}]});
        let mut b = a.clone();
        b["elements"] = json!([{"name":"中文"}]);
        assert_eq!(projection(&a, &b, &a)["status"], "measured");
        assert_eq!(projection(&a, &b, &b)["status"], "blocked");
        b["truncated"] = json!(true);
        assert_eq!(projection(&a, &b, &a)["status"], "blocked");
    }
    #[test]
    fn local_requires_scope_repeat_and_complete_samples() {
        let a = json!({"truncated":false,"elements":[1,2,3]});
        let b =
            json!({"truncated":false,"elements":[1],"scope":{"id":"x"},"ref_scope":"container"});
        assert_eq!(
            local(&a, &b, &b, &a, 100, 90)["envelope_reduction"],
            json!(0.09999999999999998)
        );
        let mut c = b.clone();
        c["scope"]["id"] = json!("other");
        assert_eq!(local(&a, &b, &c, &a, 100, 90)["status"], "blocked");
    }
    #[test]
    fn identity_and_tab_cleanup_never_accept_reuse_or_user_tabs() {
        let owner = json!({"hwnd":42,"pid":7,"class":"Window","marker":"owned","start":"one"});
        let mut w = json!({"hwnd":42,"pid":7,"class":"Window","title":"owned"});
        assert!(matching_identity(&owner, &w, &json!("one")));
        assert!(!matching_identity(&owner, &w, &json!("two")));
        w["title"] = json!("user");
        assert!(!matching_identity(&owner, &w, &json!("one")));
        let nodes = vec![
            json!({"role":"Tab","depth":0}),
            json!({"role":"TabItem","depth":1,"name":"owned 未修改"}),
            json!({"role":"Button","depth":2,"automation_id":"CloseButton","ref":"@e3"}),
            json!({"role":"TabItem","depth":1,"name":"user"}),
        ];
        assert_eq!(owned_tab(&nodes, "owned", true).unwrap(), "@e3");
        assert!(owned_tab(&nodes, "owned", false).is_err());
    }
}
