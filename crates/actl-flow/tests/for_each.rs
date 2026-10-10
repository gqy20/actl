//! for_each 数据驱动有界迭代(docs/63 流程扩展;2026-10-11 spec)。
//! 纯函数测试,形态对齐 sequence.rs:expand 冻结/decode 校验/列表读取/
//! 占位注入/失败位置。执行循环在 CLI Adapter(与 sequence 同构),真机
//! 验收覆盖。

use actl_flow::{data, for_each, inputs, store::Store, *};

fn source_yaml(for_each_block: &str) -> String {
    format!(
        "version: 1\nname: Each\nsteps:\n - id: source\n   title: S\n   run: {{command: read}}\n   outputs:\n     items: {{pointer: /list, type: array}}\n - id: loop\n   title: L\n   run:\n{for_each_block}\n     body:\n       - {{command: click, args: [\"@each\"]}}\n   verify: {{command: check}}\n"
    )
}

#[test]
fn for_each_expands_and_freezes_as_a_pseudo_invocation() {
    let plan = inputs::resolve(&source_yaml("     for_each: {from: source.items}"), None).unwrap();
    let each = for_each::decode(&plan.steps[1].run).unwrap().unwrap();
    assert_eq!(each.spec.from, "source.items");
    assert_eq!(each.spec.max, for_each::DEFAULT_MAX);
    assert_eq!(each.body.len(), 1);
    assert_eq!(each.body[0].args, ["@each"]);
    // 冻结计划可 round-trip(重解析逐字节一致)
    assert_eq!(parse(&serde_json::to_string(&plan).unwrap()).unwrap(), plan);
}

#[test]
fn for_each_max_and_field_parse_from_yaml() {
    let plan = inputs::resolve(
        &source_yaml("     for_each: {from: source.items, max: 7, field: name}"),
        None,
    )
    .unwrap();
    let each = for_each::decode(&plan.steps[1].run).unwrap().unwrap();
    assert_eq!(each.spec.max, 7);
    assert_eq!(each.spec.field.as_deref(), Some("name"));
}

#[test]
fn for_each_schema_rejects_bad_shapes() {
    // 嵌套 sequence/for_each → PROTOCOL
    for body in [
        "       - {command: workflow-sequence, args: ['{}']}",
        "       - {command: workflow-for-each, args: ['{}']}",
    ] {
        let yaml = source_yaml("     for_each: {from: source.items}")
            .replace("       - {command: click, args: [\"@each\"]}", body);
        let error = inputs::resolve(&yaml, None).unwrap_err();
        assert!(error.message.contains("nest"), "{error}");
    }
    // body 无占位符 → PROTOCOL(与迭代无关的调用不属于循环体)
    let yaml =
        source_yaml("     for_each: {from: source.items}").replace("\"@each\"", "\"name:static\"");
    let error = inputs::resolve(&yaml, None).unwrap_err();
    assert!(error.message.contains("@each"));
    // from 引用不存在/非 array 输出 → PROTOCOL
    let yaml = source_yaml("     for_each: {from: source.absent}");
    let error = inputs::resolve(&yaml, None).unwrap_err();
    assert!(error.message.contains("array output"));
    let yaml = "version: 1\nname: Each\nsteps:\n - id: source\n   title: S\n   run: {command: read}\n   outputs:\n     name: {pointer: /n, type: string}\n - id: loop\n   title: L\n   run:\n     for_each: {from: source.name}\n     body:\n       - {command: click, args: [\"@each\"]}\n   verify: {command: check}\n";
    let error = inputs::resolve(yaml, None).unwrap_err();
    assert!(error.message.contains("array output"));
    // for_each 步禁 outputs
    let yaml = source_yaml("     for_each: {from: source.items}")
        + "   outputs:\n     total: {pointer: /executed, type: number}\n";
    let error = inputs::resolve(&yaml, None).unwrap_err();
    assert!(error.message.contains("outputs"));
    // from 指向后继步骤(前向引用)→ PROTOCOL
    let yaml = "version: 1\nname: Each\nsteps:\n - id: loop\n   title: L\n   run:\n     for_each: {from: source.items}\n     body:\n       - {command: click, args: [\"@each\"]}\n   verify: {command: check}\n - id: source\n   title: S\n   run: {command: read}\n   outputs:\n     items: {pointer: /list, type: array}\n";
    let error = inputs::resolve(yaml, None).unwrap_err();
    assert!(error.message.contains("array output"), "{error}");
    // max 越界 → PROTOCOL
    let error = inputs::resolve(
        &source_yaml("     for_each: {from: source.items, max: 999}"),
        None,
    )
    .unwrap_err();
    assert!(error.message.contains("max"));
}

#[test]
fn for_each_list_reads_saved_output_and_enforces_max() {
    let root = std::env::temp_dir().join(format!(
        "flow-each-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let plan = inputs::resolve(&source_yaml("     for_each: {from: source.items}"), None).unwrap();
    let store = Store::at(&root, "t").unwrap();
    let mut run = store.create(&plan, "t").unwrap();
    run.steps[0].status = engine::StepStatus::Completed;
    run.steps[0]
        .outputs
        .insert("items".into(), serde_json::json!(["a", "b", "c"]));
    // 正常读取
    let list = data::for_each_list(&run, "source.items", 8).unwrap();
    assert_eq!(list.len(), 3);
    // 超 max → PROTOCOL,不静默截断
    let error = data::for_each_list(&run, "source.items", 2).unwrap_err();
    assert!(error.message.contains("exceeds max 2"));
    // 非数组输出 → PROTOCOL
    run.steps[0]
        .outputs
        .insert("items".into(), serde_json::json!("scalar"));
    let error = data::for_each_list(&run, "source.items", 8).unwrap_err();
    assert!(error.message.contains("not an array"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn for_each_inject_replaces_placeholders_with_item_scalar() {
    let call = Invocation {
        command: "click".into(),
        args: vec!["@each".into(), "--app".into(), "@each".into()],
        bind: Default::default(),
    };
    let injected = data::for_each_inject(&call, "f_001.txt").unwrap();
    assert_eq!(injected.args, ["f_001.txt", "--app", "f_001.txt"]);
    // 项值守卫:选项形态/瞬态 ref/NUL/超长 → PROTOCOL
    for bad in [
        "-danger".to_string(),
        "@e12".to_string(),
        "a\0b".to_string(),
        format!("x{}", "y".repeat(17000)),
    ] {
        assert!(data::for_each_inject(&call, &bad).is_err(), "{bad:?}");
    }
    // 数字项也接受(to_string 形态)
    let injected = data::for_each_inject(&call, "42").unwrap();
    assert_eq!(injected.args[0], "42");
}

#[test]
fn for_each_item_text_takes_scalars_or_object_field() {
    assert_eq!(
        for_each::item_text(&serde_json::json!("x"), None).unwrap(),
        "x"
    );
    assert_eq!(
        for_each::item_text(&serde_json::json!(7), None).unwrap(),
        "7"
    );
    assert_eq!(
        for_each::item_text(&serde_json::json!({"name": "y"}), Some("name")).unwrap(),
        "y"
    );
    // 对象项无 field / field 缺失 → PROTOCOL
    assert!(for_each::item_text(&serde_json::json!({"a": 1}), None).is_err());
    assert!(for_each::item_text(&serde_json::json!({"a": 1}), Some("name")).is_err());
}

#[test]
fn for_each_failure_carries_iteration_position() {
    let error = actl_core::CtlError::with_evidence(
        actl_core::ErrorCode::NotActionable,
        "item rejected",
        serde_json::json!({"reason":"x"}),
    );
    let error = for_each::failure(error, 2, 0, true);
    let evidence = error.evidence.as_ref().unwrap();
    assert_eq!(evidence["for_each_item"], 2);
    assert_eq!(evidence["for_each_child"], 0);
    assert_eq!(evidence["action_started"], true);
    // 未开始(只读失败)不追加位置——与 sequence::failure 同语义
    let error = actl_core::CtlError::new(actl_core::ErrorCode::NotFound, "gone");
    let error = for_each::failure(error, 1, 0, false);
    assert!(error.evidence.is_none());
}
