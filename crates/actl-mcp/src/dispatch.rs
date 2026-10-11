//! 子进程派发:适配器的全部资产面(docs/82 §5)。
//!
//! 每次调用 spawn `actl <命令> <参数...>`,stdout 的 JSON envelope 是
//! 唯一权威结果;子进程非零退出或 stdout 不可解析(看门狗自毙/clap
//! 参数错/崩溃)时合成诚实的错误 envelope(带退出码与 stderr 尾巴),
//! 绝不伪造 ok。每个调用映射一个 `ACTL_TASK_ID`(`mcp-<hex>`),
//! `actl history --task` 全程可查。子进程带 `kill_on_drop`:请求取消
//! (HTTP 断连)即杀子进程——run 留 interrupted/uncertain,恢复走既有
//! flow-resume 语义(docs/82 §5.4)。

use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;

/// 调用超时上限:子进程自带看门狗(默认 300s,docs/06 §7.7),本层再加
/// 一道外层保险(略宽),防二进制版本不匹配等极端形态卡死 worker。
const CHILD_TIMEOUT: Duration = Duration::from_secs(420);

/// 通用工具描述携带的紧凑速查(docs/82 §5.2 三层懒加载的第 1 层,
/// ~700 tokens 一次性;排除表已过滤)。与 skills/actl/SKILL.md 命令速查
/// 同源——完整参数用 `actl://help/{command}` 资源模板按需读。
pub const QUICKREF: &str = "\
感知: snapshot find get list-windows screenshot status commands
执行: click right-click double-click middle-click triple-click hover drag scroll type paste press key-down key-up set-value select toggle
窗口: launch focus-window close-window resize-window minimize-window restore-window wait batch
验证: verify extract verify-file verify-rows verify-owner verify-closed verify-window verify-clipboard
剪贴板: get-clipboard set-clipboard
流程: flow-validate flow-run flow-status flow-pause flow-resume flow-revise flow-evidence flow-clean explore-writeback
录制: record-start record-stop record-status
浏览器: browser-launch browser-navigate browser-read browser-type browser-click browser-close browser-list browser-verify
应用注册: app-register app-list app-remove
桌面与诊断: desktop-prepare execution-status display-status focus-report front-report history task-archive preview
完整参数与示例:读取资源模板 actl://help/{command}(按需,每命令一次)。\
";

/// 解析被控端 actl 可执行文件:ACTL_EXE 环境变量(测试/部署覆盖)>
/// 与本进程同目录(zip 并排分发)> 上级目录(测试二进制在 deps/ 下)>
/// PATH 上的 `actl`。
pub fn resolve_actl_exe() -> PathBuf {
    if let Ok(path) = std::env::var("ACTL_EXE")
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    if let Ok(current) = std::env::current_exe()
        && let Some(dir) = current.parent()
    {
        let sibling = dir.join("actl.exe");
        if sibling.exists() {
            return sibling;
        }
        let parent = dir.join("..").join("actl.exe");
        if parent.exists() {
            return parent;
        }
    }
    PathBuf::from("actl")
}

/// 每调用一个审计任务 id(`mcp-<8 hex>`,docs/82 §5.1)。
pub fn new_task_id() -> String {
    let mut bytes = [0u8; 4];
    // getrandom 纯安全 API;失败(极端)退化为时间熵——id 只需唯一不需机密。
    if getrandom::fill(&mut bytes).is_err() {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
            .unwrap_or(0);
        return format!("mcp-{ms:08x}");
    }
    format!("mcp-{}", hex(&bytes))
}

fn hex(bytes: &[u8; 4]) -> String {
    let mut text = String::with_capacity(8);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// 派发一次 `actl` 调用并归一化为 envelope(唯一权威形态)。
pub async fn dispatch(exe: &std::path::Path, command: &str, args: &[String]) -> Value {
    let task_id = new_task_id();
    let mut invocation: Vec<String> = Vec::with_capacity(args.len());
    invocation.extend(args.iter().cloned());

    let output = tokio::process::Command::new(exe)
        .arg(command)
        .args(&invocation)
        .env("ACTL_TASK_ID", &task_id)
        // 请求取消(HTTP 断连/客户端取消)即杀子进程:run 留中断态。
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    let output = match tokio::time::timeout(CHILD_TIMEOUT, output).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return synthetic_error(
                &task_id,
                "INTERNAL",
                &format!("actl child spawn failed: {error}"),
                None,
            );
        }
        Err(_) => {
            // 外层超时:子进程被 timeout 的 kill_on_drop 杀死。
            return synthetic_error(
                &task_id,
                "NOT_ACTIONABLE",
                "actl child exceeded the adapter wall clock (420s)",
                Some(
                    "the child watchdog (ACTL_WATCHDOG_MS) should fire first; \
                      this outer bound is a version-mismatch safety net",
                ),
            );
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    match serde_json::from_str::<Value>(stdout.trim()) {
        Ok(envelope) => envelope,
        Err(_) => {
            // clap 参数错(exit 2,stderr 人类文本)/看门狗自毙/崩溃:
            // stdout 无 envelope,如实合成(保留退出码与 stderr 尾巴)。
            let stderr = String::from_utf8_lossy(&output.stderr);
            let tail: String = stderr
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            synthetic_error(
                &task_id,
                "PROTOCOL",
                "actl child produced no JSON envelope (bad arguments or abnormal exit)",
                Some(&format!(
                    "exit={:?} stderr_tail={tail}",
                    output.status.code()
                )),
            )
        }
    }
}

/// 排除动词的明确错误(不静默;契约测试钉住)。
pub fn excluded_error(command: &str) -> Value {
    json!({
        "version": "1",
        "ok": false,
        "command": command,
        "error": {
            "code": "PROTOCOL",
            "message": format!(
                "'{command}' is local-only on the controlled device and excluded \
                 from the remote surface (docs/82 exclusion list)"
            ),
            "hint": "this verb operates the local supervision/recovery machinery",
        },
    })
}

fn synthetic_error(task_id: &str, code: &str, message: &str, hint: Option<&str>) -> Value {
    json!({
        "version": "1",
        "ok": false,
        "command": "mcp-dispatch",
        "error": {
            "code": code,
            "message": message,
            "hint": hint,
            "evidence": {"task_id": task_id},
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EXCLUDED_COMMANDS, is_excluded};

    fn exe() -> PathBuf {
        // 测试二进制位于 target/debug/deps/ —— 上级目录的 actl.exe 即同构建产物。
        let exe = resolve_actl_exe();
        assert!(exe.exists(), "actl binary not found at {exe:?}");
        exe
    }

    #[tokio::test]
    async fn dispatch_returns_the_envelope_verbatim() {
        let envelope = dispatch(&exe(), "status", &[]).await;
        assert_eq!(envelope["version"], "1");
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["command"], "status");
    }

    #[tokio::test]
    async fn bad_arguments_synthesize_an_honest_error() {
        // clap 对未知子命令走 stderr 人类文本+exit 2,无 envelope。
        let envelope = dispatch(&exe(), "definitely-not-a-command", &[]).await;
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["error"]["code"], "PROTOCOL");
        assert!(
            envelope["error"]["message"]
                .as_str()
                .unwrap()
                .contains("no JSON envelope"),
            "{envelope}"
        );
    }

    #[tokio::test]
    async fn flags_flow_through_to_the_child() {
        // verify-window 对不存在窗口断言 absent 成立(ok:true)——参数正确透传。
        let envelope = dispatch(
            &exe(),
            "verify-window",
            &[
                "--app".into(),
                "definitely-missing-window-xyz".into(),
                "--state".into(),
                "absent".into(),
            ],
        )
        .await;
        assert_eq!(envelope["ok"], true, "{envelope}");
    }

    #[test]
    fn task_ids_are_unique_and_prefixed() {
        let a = new_task_id();
        let b = new_task_id();
        assert!(a.starts_with("mcp-"), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn exclusion_table_filters_the_four_local_only_verbs() {
        for verb in [
            "execution-recover",
            "display-start",
            "display-stop",
            "desktop-restore",
            "record-worker",
        ] {
            assert!(is_excluded(verb), "{verb} must be excluded");
        }
        for verb in ["status", "click", "flow-run", "record-start"] {
            assert!(!is_excluded(verb), "{verb} must be remote-callable");
        }
    }

    #[test]
    fn quickref_and_exclusion_table_are_consistent() {
        // 速查表不得出现排除动词(资源模板/速查/排除三面一致,契约钉住)。
        for verb in EXCLUDED_COMMANDS {
            assert!(
                !QUICKREF.lines().any(|line| {
                    line.split_whitespace()
                        .any(|token| token.trim_end_matches(':') == *verb)
                }),
                "quickref must not advertise excluded verb {verb}"
            );
        }
    }

    #[test]
    fn excluded_error_is_explicit_not_silent() {
        let envelope = excluded_error("execution-recover");
        assert_eq!(envelope["error"]["code"], "PROTOCOL");
        assert!(
            envelope["error"]["message"]
                .as_str()
                .unwrap()
                .contains("local-only")
        );
    }
}
