//! 背景观察步引擎契约(方案 a,用户裁定 2026-10-13):派发不阻塞、
//! 退出前收割、产物入 background_result、失败如实改判、控制组合 PROTOCOL 拒。

use actl_core::{CtlError, ErrorCode};
use actl_flow::engine::{self, BackgroundCall, Host, Run, Status, StepStatus};
use actl_flow::store::Store;
use actl_flow::{Plan, parse};
use serde_json::{Value, json};

/// 记录调度顺序的 Fake:背景 spawn 即返回凭据(join 收割),顺序可断言。
struct Fake {
    order: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    fail_bg: bool,
}

impl Host for Fake {
    fn execute(&mut self, call: &actl_flow::Invocation) -> Result<(), CtlError> {
        self.order
            .borrow_mut()
            .push(format!("exec:{}", call.command));
        Ok(())
    }
    fn execute_output(&mut self, call: &actl_flow::Invocation) -> Result<Value, CtlError> {
        self.execute(call)?;
        Ok(json!({ "command": call.command, "payload": 42 }))
    }
    fn spawn_background(
        &mut self,
        call: &actl_flow::Invocation,
    ) -> Result<Box<dyn BackgroundCall + Send>, CtlError> {
        // spawn 即记录(真实现为线程;顺序断言只关心派发时序)
        self.order
            .borrow_mut()
            .push(format!("spawn:{}", call.command));
        let fail = self.fail_bg;
        let command = call.command.clone();
        struct Ticket {
            command: String,
            fail: bool,
        }
        impl BackgroundCall for Ticket {
            fn join(self: Box<Self>) -> Result<Value, CtlError> {
                if self.fail {
                    Err(CtlError::new(ErrorCode::NotFound, "observation lost"))
                } else {
                    Ok(json!({ "command": self.command, "result": { "total": 2 } }))
                }
            }
        }
        Ok(Box::new(Ticket { command, fail }))
    }
    fn read_only(&self, call: &actl_flow::Invocation) -> bool {
        call.command != "write"
    }
    fn stopped(&self) -> bool {
        false
    }
    fn publish(&mut self, _: &Run) {}
}

fn background_plan(yaml: &str) -> Plan {
    parse(yaml).unwrap()
}

#[test]
fn background_step_dispatches_without_blocking_and_harvests_at_exit() {
    let p = background_plan(
        "version: 1\nname: bg\nsteps:\n- id: observe\n  title: netlog window\n  background: true\n  run: {command: observe}\n- id: after\n  title: continues immediately\n  run: {command: read}\n",
    );
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "bg").unwrap();
    let mut run = store.create(&p, "bg").unwrap();
    let order = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut host = Fake {
        order: order.clone(),
        fail_bg: false,
    };
    engine::execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    // 派发时序:背景 spawn 先于后续步骤执行(不阻塞)
    let order = order.borrow();
    assert_eq!(order[0], "spawn:observe");
    assert!(order.contains(&"exec:read".to_string()));
    // 收割:背景步 Completed,产物入 background_result
    assert_eq!(run.steps[0].status, StepStatus::Completed);
    let bg = run.steps[0].background_result.clone().unwrap();
    assert_eq!(bg["result"]["total"], 2);
    // 后续步不受影响
    assert_eq!(run.steps[1].status, StepStatus::Completed);
    assert_eq!(run.status, Status::Completed);
}

#[test]
fn background_failure_flips_run_to_failed_with_step_error() {
    let p = background_plan(
        "version: 1\nname: bg\nsteps:\n- id: observe\n  title: netlog window\n  background: true\n  run: {command: observe}\n- id: after\n  title: continues\n  run: {command: read}\n",
    );
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "bg").unwrap();
    let mut run = store.create(&p, "bg").unwrap();
    let order = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut host = Fake {
        order,
        fail_bg: true,
    };
    engine::execute(&p, &mut run, &store, &mut host, None, None).unwrap();
    // 观察失败=步骤失败(不豁免错误契约);run 已 Completed 的判词被改判 Failed
    assert_eq!(run.steps[0].status, StepStatus::Failed);
    assert_eq!(run.steps[0].error_code, Some(ErrorCode::NotFound));
    assert_eq!(run.status, Status::Failed);
    assert!(
        run.reason
            .as_deref()
            .unwrap_or_default()
            .starts_with("background_failed:")
    );
    // 持久化后可见(状态文件如实反映观察结局)
    let (p, reloaded) = store.load().unwrap();
    assert_eq!(reloaded.status, Status::Failed);
    let _ = p;
}

#[test]
fn background_control_combinations_are_protocol_rejected() {
    // verify 与背景组合 → PROTOCOL(观察者不是流程控制点)
    let p = parse(
        "version: 1\nname: bg\nsteps:\n- id: observe\n  title: x\n  background: true\n  run: {command: observe}\n  verify: {command: check}\n",
    )
    .unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "bg").unwrap();
    let mut run = store.create(&p, "bg").unwrap();
    let mut host = Fake {
        order: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        fail_bg: false,
    };
    let err = engine::execute(&p, &mut run, &store, &mut host, None, None).unwrap_err();
    assert_eq!(err.code, ErrorCode::Protocol);
    assert!(err.message.contains("bare observers"));
}

#[test]
fn background_write_run_is_protocol_rejected() {
    let p = parse(
        "version: 1\nname: bg\nsteps:\n- id: observe\n  title: x\n  background: true\n  run: {command: write}\n",
    )
    .unwrap();
    let root = std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id());
    let store = Store::at(&root, "bg").unwrap();
    let mut run = store.create(&p, "bg").unwrap();
    let mut host = Fake {
        order: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        fail_bg: false,
    };
    let err = engine::execute(&p, &mut run, &store, &mut host, None, None).unwrap_err();
    assert_eq!(err.code, ErrorCode::Protocol);
    assert!(err.message.contains("read-only run"));
}
