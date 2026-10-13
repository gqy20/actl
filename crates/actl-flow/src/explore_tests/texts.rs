//! texts 输入原语测试(② focused 键收口,2026-10-12;file-lines 拆分)。
use super::*;

#[test]
fn texts_type_into_edit_completes() {
    // 输入原语:texts 键的值并入关键词(激活 Edit 评分),候选命中执行
    // type 而非 click;输入后 goal(存在型)经折叠判定完成。
    let mut spec = spec(&["文件名"], "name:报告.txt");
    spec.texts.insert("id:fileName".into(), "报告.txt".into());
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Edit", "文件名", Some("fileName")),
            el("Button", "取消", Some("cancelBtn")),
        ],
        vec![
            el("Edit", "文件名", Some("fileName")),
            el("Text", "报告.txt", None),
        ],
    ]);
    host.transitions.insert("id:fileName".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("type completes");
    assert_eq!(run.status, Status::Completed);
    // type 通道被使用,click 未发生
    assert_eq!(
        host.types,
        vec![("id:fileName".to_string(), "报告.txt".to_string())]
    );
    // 非 ASCII 文本自动切剪贴板通道(内容判据,与录制编译器 record.rs 同口径)
    assert!(
        host.raw_actions[0].iter().any(|a| a == "--paste"),
        "non-ASCII texts must use the --paste channel"
    );
    assert!(host.clicks.is_empty());
    let state = load(&store).unwrap().expect("state");
    assert_eq!(state.actions, 1);
    assert_eq!(state.decisions, 0);
    assert_eq!(
        state.action_log[0].text.as_deref(),
        Some("报告.txt"),
        "type 动作的文本入日志(路线导出用)"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn texts_ascii_keeps_keyboard_channel() {
    // ASCII 文本保持键盘通道:--paste 不注入(内容判据只看非 ASCII)。
    let mut spec = spec(&["文件名"], "name:report.txt");
    spec.texts.insert("id:fileName".into(), "report.txt".into());
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el("Edit", "文件名", Some("fileName"))],
        vec![
            el("Edit", "文件名", Some("fileName")),
            el("Text", "report.txt", None),
        ],
    ]);
    host.transitions.insert("id:fileName".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("type completes");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.types,
        vec![("id:fileName".to_string(), "report.txt".into())]
    );
    assert!(
        !host.raw_actions[0].iter().any(|a| a == "--paste"),
        "ASCII texts keep the keyboard channel"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn texts_non_ascii_ref_bound_uses_paste() {
    // ref 绑定形态的输入原语:非 ASCII 文本同样注入 --paste(位置在文本后,
    // --snapshot 之前——CLI 交错旗标语义)。
    let mut spec = spec(&["文件名"], "name:报告.txt");
    spec.texts.insert("id:fileName".into(), "报告.txt".into());
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![el_ref("Edit", "文件名", Some("fileName"), "@e1")],
        vec![
            el("Edit", "文件名", Some("fileName")),
            el("Text", "报告.txt", None),
        ],
    ]);
    host.transitions.insert("id:fileName".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("type completes");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.raw_actions[0],
        vec![
            "@e1".to_string(),
            "报告.txt".to_string(),
            "--paste".to_string(),
            "--snapshot".to_string(),
            "s-mock1".to_string(),
            "--app".to_string(),
            "测试应用".to_string()
        ],
        "ref-bound non-ASCII texts carry --paste after the text argument"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn texts_focused_key_never_autoruns_and_types_on_explicit_answer() {
    // ② focused: 键(docs/77 已知限制收口,2026-10-12):合成候选 dangerous
    // 标注——焦点可被任意窗口持有,键入永不自动执行;应答者显式裁决即
    // 授权,type focused: 落地(焦点可达性由 CLI 角色核验 fail-closed)。
    let mut spec = spec(&["保存"], "name:报告草稿");
    spec.texts.insert("focused:Edit".into(), "报告草稿".into());
    let (root, store, mut run) = setup(&spec);
    let mut host = MockHost::new(vec![
        vec![
            el("Button", "保存甲", Some("a1")),
            el("Button", "保存乙", Some("a2")),
            el("Edit", "文件名", Some("fileName")),
        ],
        vec![el("Text", "报告草稿", None)],
    ]);
    host.transitions.insert("focused:Edit".into(), Some(1));
    execute(&spec, &mut run, &store, &mut host, None, None).expect("externalize");
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("decision_pending"));
    let state = load(&store).unwrap().expect("state");
    let card = state.pending.as_ref().expect("card").card.clone();
    let focused = card
        .candidates
        .iter()
        .find(|c| c.selector == "focused:Edit")
        .expect("focused candidate must appear on the card");
    assert!(focused.dangerous, "focused typing is decision-gated");
    assert_eq!(state.actions, 0, "focused typing never auto-executes");
    // 显式应答即授权(按卡内真实 choice id,不假设卡序)→ type focused:Edit 落地
    let choice = focused.id.clone();
    execute(&spec, &mut run, &store, &mut host, Some(&choice), None).expect("authorized");
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.types,
        vec![("focused:Edit".to_string(), "报告草稿".to_string())]
    );
    assert!(
        host.clicks.is_empty(),
        "no click happened for focused typing"
    );
    let _ = std::fs::remove_dir_all(&root);
}
