//! Bounded capture of one read-only production query; never runs or resumes a flow.
use actl_core::{history::read_timeline, state::SignalPaths};
use actl_devtools::{capture, save};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

fn native_context() -> Result<Value, Box<dyn std::error::Error>> {
    let windows = actl_uia::native_windows()?;
    let visible: Vec<_> = windows
        .iter()
        .filter(|w| w.visible && !w.minimized)
        .map(|w| json!({"hwnd":w.hwnd,"pid":w.pid,"class":w.class,"foreground":w.foreground}))
        .collect();
    Ok(json!({"count":windows.len(),"visible":visible}))
}

#[test]
#[ignore = "manual read-only evidence capture; set ACTL_OBSERVATION_BINARY/OUTPUT"]
fn production_query_phases() -> Result<(), Box<dyn std::error::Error>> {
    let binary = PathBuf::from(std::env::var("ACTL_OBSERVATION_BINARY")?).canonicalize()?;
    if binary.file_name().and_then(|s| s.to_str()) != Some("actl.exe") {
        return Err("only the read-only actl query may be captured".into());
    }
    let output = std::path::absolute(std::env::var("ACTL_OBSERVATION_OUTPUT")?)?;
    std::fs::create_dir(&output)?;
    let task = actl_core::snapshot::new_snapshot_id();
    let mut env = BTreeMap::new();
    env.insert("ACTL_TASK_ID".into(), task.clone());
    env.insert("ACTL_TRACE".into(), "1".into());
    let mut report = json!({"task_id":task,"binary":binary,
        "sha256":actl_uia::evidence::sha256(&std::fs::read(&binary)?)?,
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "ts_ms":actl_core::state::unix_ms(),"input_injected":false,
        "cleanup":"no_business_resources_created","capture_deadline_ms":30000});
    report["inventory_completeness"] = json!("not_verified");
    report["business_acceptance"] = json!("not_run");
    let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
        report["native_before"] = native_context()?;
        let prefix = output.join("query");
        report["capture"] = capture::process(
            &binary,
            &["list-windows".into()],
            &output,
            &prefix,
            &env,
            Duration::from_secs(30),
        )?;
        report["native_after"] = native_context()?;
        let timeline = read_timeline(&SignalPaths::default().dir, Some(&task), 500)?;
        save(&output.join("timeline.json"), &timeline)?;
        report["timeline_truncated"] = timeline["truncated"].clone();
        report["timeline_incomplete_files"] = timeline["incomplete_files"].clone();
        let events = timeline["events"]
            .as_array()
            .ok_or("timeline events missing")?;
        let phases: Vec<_> = events.iter().filter(|e| e["event"]=="observation_finished")
            .map(|e| json!({"stage":e["data"]["stage"],"query_scope":e["data"]["query_scope"],
                "result_count":e["data"]["result_count"],"processed_count":e["data"]["processed_count"],
                "failed_index":e["data"]["failed_index"],"duration_ms":e["data"]["duration_ms"],
                "outcome":e["data"]["outcome"],"error_code":e["data"]["error_code"],"native_code":e["data"]["native_code"]})).collect();
        report["phases"] = json!(phases);
        if !events.iter().any(|e| {
            e["event"] == "observation_started" && e["data"]["stage"] == "find_all_build_cache"
        }) {
            return Err(
                "production query did not record a native query start; inspect capture".into(),
            );
        }
        if report["capture"]["status"] == "timeout" {
            report["status"] = json!("capture_timeout");
            // Only the owned read-only query process was stopped; no synthetic completion or retry.
            return Ok(());
        }
        let envelope = capture::read_json(&prefix.with_extension("stdout.json"))?;
        report["query_ok"] = envelope["ok"].clone();
        report["query_error"] = envelope["error"].clone();
        report["query_count"] = envelope["data"]["count"].clone();
        let native_visible = report["native_before"]["visible"]
            .as_array()
            .ok_or("native context missing")?;
        let root_zero = phases.iter().any(|p| {
            p["stage"] == "array_length"
                && p["query_scope"] == "desktop_children"
                && p["result_count"] == 0
        });
        report["status"] = json!(if envelope["ok"] == false {
            "provider_failed"
        } else if root_zero && !native_visible.is_empty() {
            "uia_native_disagreement"
        } else {
            "observed"
        });
        Ok(())
    })();
    if let Err(error) = &outcome {
        report["capture_error"] = json!(error.to_string());
    }
    save(&output.join("report.json"), &report)?;
    println!(
        "{}",
        serde_json::to_string(&json!({"output":output,"status":report["status"],"task_id":task}))?
    );
    outcome
}
