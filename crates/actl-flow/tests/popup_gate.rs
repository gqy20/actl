use actl_core::{CtlError, ErrorCode};
use actl_flow::{engine::*, store::Store, *};
use std::cell::RefCell;

/// 门禁测试宿主:可编程的前台弹层 + 调用/事件记录。
struct PopupHost {
    calls: Vec<String>,
    read_only: fn(&Invocation) -> bool,
    layers: RefCell<Vec<FrontLayer>>,
    popups: RefCell<Vec<serde_json::Value>>,
    captures: RefCell<Vec<String>>,
}

impl PopupHost {
    /// 每次观察弹出 layers 的下一层(末层重复);空 = 恒 clear。
    fn next_layer(&self) -> FrontLayer {
        let mut layers = self.layers.borrow_mut();
        if layers.is_empty() {
            return FrontLayer {
                clear: true,
                surfaces: Vec::new(),
            };
        }
        if layers.len() == 1 {
            return layers[0].clone();
        }
        layers.remove(0)
    }
}

impl Host for PopupHost {
    fn execute(&mut self, call: &Invocation) -> Result<(), CtlError> {
        self.calls.push(format!(
            "{}:{}",
            call.command,
            call.args.first().cloned().unwrap_or_default()
        ));
        Ok(())
    }
    fn read_only(&self, call: &Invocation) -> bool {
        (self.read_only)(call)
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
    fn front_layer(&mut self, _app: &str) -> Result<FrontLayer, CtlError> {
        Ok(self.next_layer())
    }
    fn popup_event(&mut self, _step_id: &str, event: serde_json::Value) {
        self.popups.borrow_mut().push(event);
    }
    fn capture_failure(&mut self, step_id: &str, phase: &str, _call: Option<&Invocation>) {
        self.captures
            .borrow_mut()
            .push(format!("{step_id}/{phase}"));
    }
}

fn surface(classification: &str, name: &str) -> Surface {
    Surface {
        classification: classification.into(),
        name: name.into(),
        text: String::new(),
        // 默认带一个按钮 = 可交互面板(降噪规则只豁免无按钮壳;
        // 需要空壳形态的用例用 Surface 字面量自备)。
        buttons: vec!["确定".into()],
        hwnd: None,
    }
}

fn plan_with_handlers(handlers: &str, run_call: &str) -> Plan {
    parse(&format!(
        "version: 1\nname: PopupGate\n{handlers}steps:\n - id: w1\n   title: W\n   run: {run_call}\n"
    ))
    .unwrap()
}

fn setup(plan: &Plan) -> (std::path::PathBuf, Store, Run) {
    let root = std::env::temp_dir().join(format!(
        "flow-popup-{}",
        actl_core::snapshot::new_snapshot_id()
    ));
    let store = Store::at(&root, "test").unwrap();
    let run = store.create(plan, "test").unwrap();
    (root, store, run)
}

fn write_call() -> String {
    "{command: write, args: [--app, myapp]}".into()
}

#[test]
fn unknown_popup_blocks_write_step_as_popup_pending() {
    let p = plan_with_handlers("", &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![Surface {
                classification: "unknown".into(),
                name: "神秘弹窗".into(),
                text: String::new(),
                buttons: vec!["确定".into()],
                hwnd: None,
            }],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    // 步保持 Pending(未执行,无副作用),write 从未派发
    assert_eq!(run.steps[0].status, StepStatus::Pending);
    assert!(host.calls.is_empty());
    // 留证:popup_pending 相位
    assert_eq!(
        *host.captures.borrow(),
        vec!["w1/popup_pending".to_string()]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn read_only_step_is_not_gated() {
    let p = plan_with_handlers("", "{command: peek, args: [--app, myapp]}");
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |_| true,
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "神秘弹窗")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.calls, vec!["peek:--app".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn declared_handler_dismisses_popup_then_step_runs() {
    let handlers =
        "popup_handlers:\n  - app: myapp\n    classification: save\n    click: 'name:不保存'\n";
    let p = plan_with_handlers(handlers, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        // 首层:save 面板;复查层:clear
        layers: RefCell::new(vec![
            FrontLayer {
                clear: false,
                surfaces: vec![surface("save", "是否保存?")],
            },
            FrontLayer {
                clear: true,
                surfaces: Vec::new(),
            },
        ]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    // 处置 click + 业务 write 都派发,处置先于业务
    assert_eq!(
        host.calls,
        vec!["click:name:不保存".to_string(), "write:--app".to_string()]
    );
    assert_eq!(host.popups.borrow().len(), 1);
    assert_eq!(host.popups.borrow()[0]["classification"], "save");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn handler_that_fails_to_clear_escalates_to_popup_pending() {
    let handlers =
        "popup_handlers:\n  - app: myapp\n    classification: save\n    click: 'name:不保存'\n";
    let p = plan_with_handlers(handlers, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        // 处置后复查仍是未清层 → 上抛
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("save", "是否保存?")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    assert_eq!(run.steps[0].status, StepStatus::Pending);
    assert_eq!(host.calls, vec!["click:name:不保存".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn classification_mismatch_is_not_handled() {
    // 声明的是 error,来的是 save → 不匹配 → popup_pending
    let handlers =
        "popup_handlers:\n  - app: myapp\n    classification: error\n    click: 'name:确定'\n";
    let p = plan_with_handlers(handlers, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("save", "是否保存?")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert!(host.calls.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn write_step_without_app_anchor_is_not_gated() {
    let p = plan_with_handlers("", "{command: write}");
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "神秘弹窗")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.calls, vec!["write:".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn popup_handlers_schema_rejects_unknown_classification_and_empty_fields() {
    let bad = [
        "popup_handlers:\n  - app: a\n    classification: unknown\n    click: c\n",
        "popup_handlers:\n  - app: a\n    classification: save\n    click: ''\n",
        "popup_handlers:\n  - app: ''\n    classification: save\n    click: c\n",
    ];
    for handlers in bad {
        // parse 阶段即校验(data::validate 在 schema 解析内联)
        let yaml = format!(
            "version: 1\nname: P\n{handlers}steps:\n - id: w1\n   title: W\n   run: {{command: write}}\n"
        );
        let result = parse(&yaml);
        assert!(result.is_err(), "{handlers:?} should be rejected at parse");
        assert_eq!(result.unwrap_err().code, ErrorCode::Protocol);
    }
    let good = "popup_handlers:\n  - app: a\n    classification: save\n    name_contains: 保存\n    click: c\n";
    let plan = plan_with_handlers(good, &write_call());
    assert_eq!(plan.popup_handlers.len(), 1);
    assert_eq!(plan.popup_handlers[0].classification, "save");
}

#[test]
fn sequence_run_app_anchor_is_extracted_for_gating() {
    // sequence 子调用内嵌 --app 也能定位门禁锚
    let call =
        r#"{command: workflow-sequence, args: ['{"command":"write","args":["--app","seqapp"]}']}"#;
    let p = plan_with_handlers("", call);
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "workflow-sequence",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "神秘弹窗")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn buttonless_unknown_shell_is_noise_and_does_not_block() {
    // 门禁降噪(真机 E2E 发现):Win11 内容对话框自带「弹出窗口」宿主壳
    // (unknown 且无按钮)——不可交互即非引导层,不阻断写步。
    let p = plan_with_handlers("", &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![
                Surface {
                    classification: "unknown".into(),
                    name: "弹出窗口".into(),
                    text: String::new(),
                    buttons: Vec::new(),
                    hwnd: None,
                },
                Surface {
                    classification: "unknown".into(),
                    name: "空壳".into(),
                    text: String::new(),
                    buttons: Vec::new(),
                    hwnd: None,
                },
            ],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(host.calls, vec!["write:--app".to_string()]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_surface_with_buttons_still_blocks() {
    // 降噪只豁免"无按钮"壳:有按钮的 unknown 面板仍须上抛判读。
    let p = plan_with_handlers("", &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![Surface {
                classification: "unknown".into(),
                name: "神秘向导".into(),
                text: String::new(),
                buttons: vec!["同意".into(), "拒绝".into()],
                hwnd: None,
            }],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn notice_handler_requires_name_contains_at_parse() {
    // notice 最窄形态(2026-10-11 spec):name_contains 必填——
    // 禁止「任意 unknown 一律处置」的宽形态在 schema 层就拒绝。
    for handlers in [
        "popup_handlers:\n  - app: a\n    classification: notice\n    click: c\n",
        "popup_handlers:\n  - app: a\n    classification: notice\n    name_contains: ''\n    click: c\n",
    ] {
        let yaml = format!(
            "version: 1\nname: P\n{handlers}steps:\n - id: w1\n   title: W\n   run: {{command: write}}\n"
        );
        let result = parse(&yaml);
        assert!(result.is_err(), "{handlers:?} should be rejected at parse");
        assert_eq!(result.unwrap_err().code, ErrorCode::Protocol);
    }
    let good = "popup_handlers:\n  - app: a\n    classification: notice\n    name_contains: 扩展已禁用\n    click: c\n";
    let plan = plan_with_handlers(good, &write_call());
    assert_eq!(plan.popup_handlers[0].classification, "notice");
}

#[test]
fn notice_handler_dismisses_named_unknown_surface() {
    // INFO 级表面落 unknown(带按钮=可处置面板):按名认领后自动处置放行
    let handlers = "popup_handlers:\n  - app: myapp\n    classification: notice\n    name_contains: 扩展已禁用\n    click: 'name:关闭通知'\n";
    let p = plan_with_handlers(handlers, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![
            FrontLayer {
                clear: false,
                surfaces: vec![surface("unknown", "扩展已禁用(3)")],
            },
            FrontLayer {
                clear: true,
                surfaces: Vec::new(),
            },
        ]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.calls,
        vec!["click:name:关闭通知".to_string(), "write:--app".to_string()]
    );
    // 审计面:面板本体分类如实记 unknown,认领声明记 handler=notice
    assert_eq!(host.popups.borrow()[0]["classification"], "unknown");
    assert_eq!(host.popups.borrow()[0]["handler"], "notice");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn notice_handler_neither_overclaims_nor_underclaims() {
    // 名字不符的 unknown 面板:notice 处理器不认领 → 仍上抛(不放宽)
    let handlers = "popup_handlers:\n  - app: myapp\n    classification: notice\n    name_contains: 扩展已禁用\n    click: 'name:关闭通知'\n";
    let p = plan_with_handlers(handlers, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "许可证过期")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    assert!(host.calls.is_empty(), "no click before escalation");
    std::fs::remove_dir_all(root).unwrap();

    // 已分类面板(save)也不被 notice 认领——分类语义不被 notice 吞并
    let p2 = plan_with_handlers(handlers, &write_call());
    let (root2, store2, mut run2) = setup(&p2);
    let mut host2 = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("save", "扩展已禁用?是否保存")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p2, &mut run2, &store2, &mut host2, None, None).unwrap();
    assert_eq!(run2.status, Status::NeedsReview);
    std::fs::remove_dir_all(root2).unwrap();
}

#[test]
fn ignore_declaration_skips_claimed_persistent_surface() {
    // 降噪第二轮(2026-10-11):声明认领的常驻表面(VS Code Find 面板形态)
    // 门禁直接放行——无处置 click、无 popup_pending,步执行完成
    let ignores = "popup_ignores:\n  - app: myapp\n    name_contains: Find / Replace\n";
    let p = plan_with_handlers(ignores, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        // 常驻表面持续存在(开与关都在树上)——门禁必须跳过而非阻断
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "Find / Replace No results")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    // 无处置 click(表面被认领放行,不是被处置),只有业务 write
    assert_eq!(host.calls, vec!["write:--app".to_string()]);
    assert!(host.popups.borrow().is_empty());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn undeclared_or_mismatched_surfaces_still_block() {
    // 认领词不符的表面不受 ignore 豁免——认领是按名的,不是按 app 的
    let ignores = "popup_ignores:\n  - app: myapp\n    name_contains: Find / Replace\n";
    let p = plan_with_handlers(ignores, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        layers: RefCell::new(vec![FrontLayer {
            clear: false,
            surfaces: vec![surface("unknown", "许可证过期")],
        }]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::NeedsReview);
    assert_eq!(run.reason.as_deref(), Some("popup_pending:w1"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ignore_and_handler_coexist_for_distinct_surfaces() {
    // 双层共存:常驻表面被 ignore 放行,真弹层被 handler 处置——两不相扰
    let decl = "popup_handlers:\n  - app: myapp\n    classification: save\n    click: 'name:不保存'\npopup_ignores:\n  - app: myapp\n    name_contains: Find / Replace\n";
    let p = plan_with_handlers(decl, &write_call());
    let (root, store, mut run) = setup(&p);
    let mut host = PopupHost {
        calls: Vec::new(),
        read_only: |c| c.command != "write",
        // 同层两表面:save 弹窗(处置)+ Find 常驻面板(认领放行);
        // 复查层:常驻面板仍在(它的本性)——复查必须不因它而失败
        layers: RefCell::new(vec![
            FrontLayer {
                clear: false,
                surfaces: vec![
                    surface("save", "是否保存?"),
                    surface("unknown", "Find / Replace No results"),
                ],
            },
            FrontLayer {
                clear: false,
                surfaces: vec![surface("unknown", "Find / Replace No results")],
            },
        ]),
        popups: RefCell::new(Vec::new()),
        captures: RefCell::new(Vec::new()),
    };
    execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    assert_eq!(run.status, Status::Completed);
    assert_eq!(
        host.calls,
        vec!["click:name:不保存".to_string(), "write:--app".to_string()]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ignore_and_handler_on_the_same_surface_is_rejected_at_parse() {
    // 处置与放行语义互斥:同 app+同认领词 → schema 层 PROTOCOL
    let decl = "popup_handlers:\n  - app: myapp\n    classification: notice\n    name_contains: Find / Replace\n    click: c\npopup_ignores:\n  - app: myapp\n    name_contains: Find / Replace\n";
    let yaml = format!(
        "version: 1\nname: P\n{decl}steps:\n - id: w1\n   title: W\n   run: {{command: write}}\n"
    );
    let result = parse(&yaml);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, ErrorCode::Protocol);
}
