//! decide 契约测试(docs/06 §3 decide;spec 提交配套):全部离线——本地
//! std TcpListener 桩 HTTP 服务扮演 System One provider,golden 断言钉
//! envelope 形状与请求构造契约(危险候选结构性剥离、none 强制入选项)。
//! 真网验收(LM Studio/Kev)属真机轮,不进 CI。
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "actl-decide-contract-{}",
            actl_core::snapshot::new_snapshot_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_actl"));
        cmd.env("LOCALAPPDATA", &self.root)
            .env_remove("ACTL_TASK_ID")
            .stdin(Stdio::null());
        cmd
    }
    fn invoke(&self, args: &[&str]) -> (i32, Value) {
        let out = self.command().args(args).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let data = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("envelope json expected, got: {text}"));
        (out.status.code().unwrap(), data)
    }
    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
    fn write_config(&self, endpoint: &str) {
        std::fs::create_dir_all(self.root.join("actl")).unwrap();
        std::fs::write(
            self.root.join("actl").join("decision.json"),
            json!({"endpoint": endpoint}).to_string(),
        )
        .unwrap();
    }
}

/// 卡固件:两个安全候选 + 一个危险候选(name:删除全部)。
fn card_json() -> String {
    json!({
        "version": 1,
        "card_id": "d-contract",
        "task": "Save the document as notes.txt",
        "app": "Notepad",
        "goal": "verify-file notes.txt",
        "step": 4,
        "diff": {"added": 1, "removed": 0, "changed": 0},
        "candidates": [
            {"id": "c1", "selector": "name:保存", "role": "Button", "name": "保存", "score": 8, "dangerous": false},
            {"id": "c2", "selector": "name:另存为", "role": "MenuItem", "name": "另存为", "score": 6, "dangerous": false},
            {"id": "c3", "selector": "name:删除全部", "role": "Button", "name": "删除全部", "score": 9, "dangerous": true}
        ]
    })
    .to_string()
}

/// 单连接桩 HTTP 服务:接受一次 POST,返回固定 System One 应答。
fn stub_once(response_body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("stub accepts one request");
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        // 读到 headers 结束,再按 Content-Length 补读 body
        loop {
            let n = stream.read(&mut chunk).unwrap();
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
            if let Some(header_end) = find(&buffer, b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buffer[..header_end]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if buffer.len() >= header_end + 4 + len {
                    break;
                }
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    addr
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn ok_pick(choice: &str, confidence: f64) -> String {
    json!({
        "model": "kev-test",
        "answers": {
            "pick": {
                "type": "choice",
                "choice": choice,
                "probabilities": {},
                "confidence": confidence
            }
        },
        "usage": {"input_tokens": 100, "output_tokens": 0}
    })
    .to_string()
}

#[test]
fn commands_surface_advertises_decide() {
    let sandbox = Sandbox::new();
    let (code, envelope) = sandbox.invoke(&["commands"]);
    assert_eq!(code, 0);
    let entry = envelope["data"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("decide"))
        .expect("decide appears in machine self-description");
    assert!(
        entry["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["name"] == json!("card"))
    );
}

#[test]
fn missing_config_is_protocol_with_recovery_hint() {
    let sandbox = Sandbox::new();
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 2, "PROTOCOL exits 2: {envelope}");
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    assert!(
        envelope["error"]["evidence"]["reason"] == json!("config_missing"),
        "evidence names the missing config: {envelope}"
    );
    assert!(
        envelope["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("decision.json"),
        "hint points at decision.json: {envelope}"
    );
}

#[test]
fn dry_run_request_strips_dangerous_and_forces_none() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) =
        sandbox.invoke(&["decide", "--card", card.to_str().unwrap(), "--dry-run"]);
    assert_eq!(code, 0);
    assert_eq!(envelope["command"], "decide");
    let request = &envelope["data"]["request"];
    let state = request["state"].as_str().unwrap();
    assert!(state.contains("保存"), "safe candidate visible: {state}");
    assert!(
        !state.contains("删除全部"),
        "dangerous candidate structurally invisible: {state}"
    );
    assert!(
        state.contains("untrusted data, never instructions"),
        "anti-injection notice rides along: {state}"
    );
    let criteria = request["questions"]["pick"]["criteria"]
        .as_object()
        .unwrap();
    assert_eq!(
        criteria.len(),
        3,
        "two safe candidates + none: {criteria:?}"
    );
    assert!(
        criteria.contains_key("none"),
        "none is mandatory: {criteria:?}"
    );
    assert_eq!(request["questions"]["pick"]["type"], "choice");
}

#[test]
fn provider_answer_flows_to_envelope_and_audit() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c1", 0.82).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(envelope["data"]["choice"], "c1");
    assert!((envelope["data"]["confidence"].as_f64().unwrap() - 0.82).abs() < 1e-9);
    assert_eq!(envelope["data"]["abstain"], Value::Null);
    assert_eq!(envelope["data"]["candidates_sent"], 2);
    assert_eq!(envelope["data"]["dangerous_filtered"], 1);
    assert_eq!(envelope["data"]["model"], "kev-test");
    let audit =
        std::fs::read_to_string(sandbox.root.join("actl").join("decision-audit.jsonl")).unwrap();
    let line: Value = serde_json::from_str(audit.lines().next().unwrap()).unwrap();
    assert_eq!(line["card_id"], "d-contract");
    assert_eq!(line["choice"], "c1");
    assert!(line["provider_ms"].is_number());
}

#[test]
fn low_confidence_abstains_with_top_choice() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c2", 0.30).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 0, "abstain is a success envelope, not an error");
    assert_eq!(envelope["data"]["choice"], Value::Null);
    assert_eq!(envelope["data"]["abstain"], "low_confidence");
    assert_eq!(envelope["data"]["top_choice"], "c2");
    assert_eq!(envelope["data"]["confidence"], 0.30);
}

#[test]
fn dangerous_only_card_abstains_without_provider_call() {
    let sandbox = Sandbox::new();
    // 端口 1 不可能有服务:provider 若被触达会连接失败——dangerous_only 必须
    // 在构造请求前短路。
    sandbox.write_config("http://127.0.0.1:1/v1/systemone");
    let card = sandbox.write("card.json", &{
        let mut value: Value = serde_json::from_str(&card_json()).unwrap();
        value["candidates"] = json!([
            {"id": "c3", "selector": "name:删除全部", "role": "Button", "name": "删除全部", "score": 9, "dangerous": true}
        ]);
        value.to_string()
    });
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert_eq!(envelope["data"]["abstain"], "dangerous_only");
    assert_eq!(envelope["data"]["dangerous_filtered"], 1);
}

#[test]
fn unreachable_provider_maps_to_dedicated_code() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:1/v1/systemone");
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 1, "PROVIDER_UNAVAILABLE exits 1: {envelope}");
    assert_eq!(envelope["error"]["code"], "PROVIDER_UNAVAILABLE");
    assert!(
        envelope["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("decision endpoint"),
        "recovery hint names the endpoint: {envelope}"
    );
}

#[test]
fn hallucinated_choice_is_rejected_as_provider_fault() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c99", 0.99).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 1);
    assert_eq!(envelope["error"]["code"], "PROVIDER_UNAVAILABLE");
    assert_eq!(
        envelope["error"]["evidence"]["reason"], "choice_not_offered",
        "hallucinated choice carries structured evidence: {envelope}"
    );
}

#[test]
fn envelope_form_card_via_stdin_pipeline() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c1", 0.75).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let envelope_form = json!({
        "command": "flow-status",
        "ok": true,
        "data": {"decision_card": serde_json::from_str::<Value>(&card_json()).unwrap()}
    })
    .to_string();
    let mut child = sandbox
        .command()
        .arg("decide")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(envelope_form.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let envelope: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(envelope["data"]["choice"], "c1");
}

#[test]
fn invalid_card_is_protocol_not_provider_fault() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let bad = sandbox.write("bad.json", "this is not json");
    let (code, envelope) = sandbox.invoke(&["decide", "--card", bad.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    let no_card = sandbox.write("nocard.json", "{}");
    let (code, envelope) = sandbox.invoke(&["decide", "--card", no_card.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
}

/// AMBIGUOUS 错误信封固件:两个同名「浏览」按钮,context 消歧行不同
/// (docs/12 §4 结构化候选的 decide 消费形态)。
fn ambiguous_envelope_json() -> String {
    json!({
        "version": "1", "ok": false, "command": "click",
        "error": {
            "code": "AMBIGUOUS",
            "message": "name:浏览 matches 2 elements in the window",
            "evidence": { "candidates": [
                {"ref": "@e3", "role": "Button", "name": "浏览", "automation_id": "btnLocal", "context": "位于文件名输入框右侧"},
                {"ref": "@e7", "role": "Button", "name": "浏览", "automation_id": "btnOnline", "context": "联机图片选项卡内"}
            ]}
        }
    })
    .to_string()
}

#[test]
fn ambiguous_envelope_without_task_is_protocol_with_hint() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let card = sandbox.write("ambiguous.json", &ambiguous_envelope_json());
    let (code, envelope) = sandbox.invoke(&["decide", "--card", card.to_str().unwrap()]);
    assert_eq!(code, 2, "PROTOCOL exits 2: {envelope}");
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    assert_eq!(
        envelope["error"]["evidence"]["reason"], "task_missing",
        "evidence names the missing intent: {envelope}"
    );
    assert!(
        envelope["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("--task"),
        "hint points at --task: {envelope}"
    );
}

#[test]
fn ambiguous_envelope_answers_with_resolved_replay_meta() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c2", 0.88).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let card = sandbox.write("ambiguous.json", &ambiguous_envelope_json());
    let (code, envelope) = sandbox.invoke(&[
        "decide",
        "--card",
        card.to_str().unwrap(),
        "--task",
        "open the ONLINE picture search",
    ]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["choice"], "c2");
    // resolved 回显:可重放元数据,调用方不必回读错误信封
    assert_eq!(envelope["data"]["resolved"]["ref"], "@e7");
    assert_eq!(envelope["data"]["resolved"]["automation_id"], "btnOnline");
    assert_eq!(envelope["data"]["resolved"]["context"], "联机图片选项卡内");
    // context 进判别描述(消歧信息抵达 provider)
    // ——由 stub 不校验;请求形态由单测 ambiguous_envelope_requires_task_and_maps_candidates 钉住
}

#[test]
fn ambiguous_envelope_low_confidence_resolves_top_choice() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c1", 0.30).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let card = sandbox.write("ambiguous.json", &ambiguous_envelope_json());
    let (code, envelope) = sandbox.invoke(&[
        "decide",
        "--card",
        card.to_str().unwrap(),
        "--task",
        "open the local file picker",
    ]);
    assert_eq!(code, 0, "abstain is success: {envelope}");
    assert_eq!(envelope["data"]["abstain"], "low_confidence");
    assert_eq!(envelope["data"]["top_choice"], "c1");
    assert_eq!(
        envelope["data"]["resolved"]["ref"], "@e3",
        "top_choice also carries replay meta for human review: {envelope}"
    );
}

#[test]
fn task_on_plain_card_is_rejected() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let card = sandbox.write("card.json", &card_json());
    let (code, envelope) = sandbox.invoke(&[
        "decide",
        "--card",
        card.to_str().unwrap(),
        "--task",
        "点击保存",
    ]);
    assert_eq!(code, 2, "{envelope}");
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("--task"),
        "error names the misused flag: {envelope}"
    );
}

/// --run:从 run 状态直读悬挂卡(flow 步无 stdin 管道,应答循环 YAML 化的卡源)。
#[test]
fn run_source_reads_pending_card_and_answers() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(ok_pick("c1", 0.86).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    // 构造探索 run 状态:flows/decide-run-1/explore.json 带 pending.card
    let run_dir = sandbox.root.join("actl").join("flows").join("decide-run-1");
    std::fs::create_dir_all(&run_dir).unwrap();
    std::fs::write(
        run_dir.join("explore.json"),
        json!({"pending": {"card": serde_json::from_str::<Value>(&card_json()).unwrap()}})
            .to_string(),
    )
    .unwrap();
    let (code, envelope) = sandbox.invoke(&["decide", "--run", "decide-run-1"]);
    assert_eq!(code, 0, "{envelope}");
    assert_eq!(envelope["data"]["choice"], "c1");
    assert_eq!(envelope["data"]["card_id"], "d-contract");
    // 危险剥离照常(c3 score 9 dangerous)
    assert_eq!(envelope["data"]["dangerous_filtered"], 1);
}

#[test]
fn run_source_unknown_or_cardless_is_protocol() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    // 未知 run:PROTOCOL 附 run_state_missing 证据
    let (code, envelope) = sandbox.invoke(&["decide", "--run", "no-such-run"]);
    assert_eq!(code, 2, "{envelope}");
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    assert_eq!(envelope["error"]["evidence"]["reason"], "run_state_missing");
    // 状态存在但无悬挂卡(已被应答/完成):PROTOCOL 如实
    let run_dir = sandbox.root.join("actl").join("flows").join("done-run");
    std::fs::create_dir_all(&run_dir).unwrap();
    std::fs::write(
        run_dir.join("explore.json"),
        json!({"pending": null}).to_string(),
    )
    .unwrap();
    let (code, envelope) = sandbox.invoke(&["decide", "--run", "done-run"]);
    assert_eq!(code, 2, "{envelope}");
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
    // 路径白名单:注入 ../ 的 id 拒绝
    let (code, _) = sandbox.invoke(&["decide", "--run", "../escape"]);
    assert_eq!(code, 2, "path-injection id rejected");
}

fn front_pick(family: &str, confidence: f64, blocking: f64) -> String {
    json!({
        "model": "kev-test",
        "answers": {
            "family": {"type": "choice", "choice": family, "confidence": confidence},
            "blocking": {"type": "noul", "noul": blocking}
        },
        "usage": {"input_tokens": 120, "output_tokens": 0}
    })
    .to_string()
}

/// 真实形态 front-report 信封固件:一个 owned 崩溃框 + 一个 in_tree 常驻表面
/// (计算器门禁误报轮 2026-10-13 的实测形态)。
fn front_envelope_json(front_clear: bool) -> String {
    json!({
        "version": "1", "ok": true, "command": "front-report",
        "data": {
            "anchor_foreground": true,
            "front_clear": front_clear,
            "owned": if front_clear { json!([]) } else { json!([
                {"title": "wmexternal.exe - 应用程序错误", "class": "#32770", "pid": 4888,
                 "hwnd": 654321, "visible": true, "foreground": true}
            ])},
            "in_tree": if front_clear { json!([]) } else { json!([
                {"name": "计算器", "classification": "unknown",
                 "buttons": ["最小化 计算器", "关闭 计算器"],
                 "text": "计算器 计算器 表达式为 显示为 7"}
            ])},
            "window": {"class": "ApplicationFrameWindow", "foreground": true}
        }
    })
    .to_string()
}

#[test]
fn front_envelope_without_task_is_protocol() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let envelope = sandbox.write("front.json", &front_envelope_json(false));
    let (code, out) = sandbox.invoke(&["decide", "--card", envelope.to_str().unwrap()]);
    assert_eq!(code, 2, "{out}");
    assert_eq!(out["error"]["code"], "PROTOCOL");
    assert_eq!(out["error"]["evidence"]["reason"], "task_missing");
    assert!(
        out["error"]["hint"]
            .as_str()
            .unwrap_or_default()
            .contains("--task"),
        "hint points at --task: {out}"
    );
}

#[test]
fn front_clear_envelope_short_circuits_without_provider() {
    let sandbox = Sandbox::new();
    // 端口 1 不可达:front_clear 若触网会 PROVIDER_UNAVAILABLE——短路必须免网。
    sandbox.write_config("http://127.0.0.1:1/v1/systemone");
    let envelope = sandbox.write("front.json", &front_envelope_json(true));
    let (code, out) = sandbox.invoke(&[
        "decide",
        "--card",
        envelope.to_str().unwrap(),
        "--task",
        "export the worksheet as CSV",
    ]);
    assert_eq!(
        code, 0,
        "front_clear is honest absence, not a provider call: {out}"
    );
    assert_eq!(out["data"]["family"], "front_clear");
    assert_eq!(out["data"]["blocking"], 0.0);
    assert_eq!(out["data"]["provider_ms"], Value::Null);
}

#[test]
fn front_envelope_answers_family_and_blocking() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(front_pick("crash", 0.93, 0.9).into_boxed_str()));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let envelope = sandbox.write("front.json", &front_envelope_json(false));
    let (code, out) = sandbox.invoke(&[
        "decide",
        "--card",
        envelope.to_str().unwrap(),
        "--task",
        "continue the export flow; the dialog was not opened by this flow",
    ]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out["data"]["family"], "crash",
        "classification is advisory: {out}"
    );
    assert_eq!(out["data"]["blocking"], 0.9);
    assert_eq!(out["data"]["abstain"], Value::Null);
    assert_eq!(
        out["data"]["candidates_sent"], 8,
        "family choice set size: {out}"
    );
    assert!(out["data"]["provider_ms"].is_number());
}

#[test]
fn front_hallucinated_family_is_rejected_as_provider_fault() {
    let sandbox = Sandbox::new();
    let addr = stub_once(Box::leak(
        front_pick("not_a_real_family", 0.99, 0.5).into_boxed_str(),
    ));
    sandbox.write_config(&format!("http://{addr}/v1/systemone"));
    let envelope = sandbox.write("front.json", &front_envelope_json(false));
    let (code, out) = sandbox.invoke(&[
        "decide",
        "--card",
        envelope.to_str().unwrap(),
        "--task",
        "continue the flow",
    ]);
    assert_eq!(code, 1, "{out}");
    assert_eq!(out["error"]["code"], "PROVIDER_UNAVAILABLE");
    assert_eq!(out["error"]["evidence"]["reason"], "choice_not_offered");
}

#[test]
fn ambiguous_truncated_envelope_abstains_without_provider_call() {
    let sandbox = Sandbox::new();
    // 端口 1 不可达:截断护栏若触网会 PROVIDER_UNAVAILABLE——必须免网弃权。
    sandbox.write_config("http://127.0.0.1:1/v1/systemone");
    let envelope = sandbox.write(
        "truncated.json",
        &json!({
            "version": "1", "ok": false, "command": "click",
            "error": {
                "code": "AMBIGUOUS",
                "message": "name:x matches 12 elements in the window",
                "evidence": {
                    "candidates": [
                        {"ref": "@e1", "role": "Button", "name": "a"},
                        {"ref": "@e2", "role": "Button", "name": "b"}
                    ],
                    "total": 12, "returned": 2, "truncated": true
                }
            }
        })
        .to_string(),
    );
    let (code, out) = sandbox.invoke(&[
        "decide",
        "--card",
        envelope.to_str().unwrap(),
        "--task",
        "narrow the selector instead",
    ]);
    assert_eq!(
        code, 0,
        "truncation abstain is success, not a provider fault: {out}"
    );
    assert_eq!(out["data"]["abstain"], "candidates_truncated");
    assert_eq!(out["data"]["choice"], Value::Null);
    assert_eq!(out["data"]["candidates_sent"], 2);
}

#[test]
fn front_owned_texts_reach_the_consult_request() {
    let sandbox = Sandbox::new();
    sandbox.write_config("http://127.0.0.1:9/v1/systemone");
    let envelope = sandbox.write(
        "front-texts.json",
        &json!({
            "version": "1", "ok": true, "command": "front-report",
            "data": {
                "anchor_foreground": true, "front_clear": false,
                "owned": [{
                    "title": "app.exe - 应用程序错误", "class": "#32770",
                    "hwnd": 111, "visible": true, "foreground": true,
                    "texts": ["Fail Fast。单击确定关闭程序。"],
                    "buttons": ["确定", "取消"]
                }],
                "in_tree": []
            }
        })
        .to_string(),
    );
    let (code, out) = sandbox.invoke(&[
        "decide",
        "--card",
        envelope.to_str().unwrap(),
        "--task",
        "continue the flow",
        "--dry-run",
    ]);
    assert_eq!(code, 0, "{out}");
    let state = out["data"]["request"]["state"].as_str().unwrap();
    assert!(
        state.contains("Fail Fast。单击确定关闭程序。"),
        "subwindow full text rides the consult state: {state}"
    );
    assert!(
        state.contains("[确定 / 取消]"),
        "subwindow buttons render: {state}"
    );
}
