use crate::{Args, Result, io, require};
use serde_json::{Value, json};
pub fn unattended(case: &Value) -> bool {
    case["state"] == "ready" && case["input"] == "none" && case["unattended"] == true
}
pub fn select(inventory: &[Value], args: &Args) -> Result<Vec<Value>> {
    let mut ids = std::collections::BTreeSet::new();
    for case in inventory {
        require(
            matches!(case["state"].as_str(), Some("ready" | "manual" | "blocked")),
            "invalid case state",
        )?;
        require(
            case["id"].as_str().is_some_and(|id| ids.insert(id)),
            "duplicate or missing case ID",
        )?;
    }
    if let Some(id) = args.values.get("case") {
        require(ids.contains(id.as_str()), "unknown regression case")?;
    }
    if let Some(suite) = args.values.get("suite") {
        require(
            suite == "all" || inventory.iter().any(|c| c["suite"] == *suite),
            "unknown regression suite",
        )?;
    }
    Ok(inventory
        .iter()
        .filter(|c| {
            args.values.get("case").is_none_or(|id| c["id"] == *id)
                && args
                    .values
                    .get("suite")
                    .is_none_or(|s| s == "all" || c["suite"] == *s)
        })
        .cloned()
        .collect())
}
pub fn run(args: &Args) -> Result<Value> {
    args.check(
        &["actl", "output", "case", "suite"],
        &["list", "unattended"],
    )?;
    let inventory: Vec<Value> =
        serde_json::from_str(include_str!("../../xtask/regress-cases.json")).map_err(io)?;
    let selected = select(&inventory, args)?;
    if args.flags.iter().any(|f| f == "list") {
        return Ok(json!({"cases": selected}));
    }
    let directory = std::path::PathBuf::from(
        args.value(
            "output",
            crate::root()
                .join("target")
                .join(crate::id("regress"))
                .to_string_lossy()
                .as_ref(),
        ),
    );
    std::fs::create_dir(&directory).map_err(io)?;
    let mut results = vec![];
    let mut failed = false;
    let mut blocked = false;
    for case in selected {
        let id = case["id"].as_str().ok_or_else(|| io("missing case ID"))?;
        let reason = if failed || blocked {
            Some("previous case failed or blocked")
        } else if case["state"] != "ready" {
            Some("manual or blocked case")
        } else if args.flags.iter().any(|f| f == "unattended") && !unattended(&case) {
            Some("not explicitly unattended")
        } else {
            None
        };
        if let Some(reason) = reason {
            results.push(json!({"id":id,"status":"skipped","reason":reason}));
            continue;
        }
        let mut options = Args {
            values: Default::default(),
            flags: vec![],
        };
        options
            .values
            .insert("output".into(), directory.join(id).to_string_lossy().into());
        if let Some(binary) = args.values.get("actl") {
            options.values.insert("actl".into(), binary.clone());
        }
        eprintln!("regress: {id}");
        match crate::fixtures::run(id, &options) {
            Ok(report) => {
                blocked = report["status"] == "blocked";
                failed = !blocked && report["passed"] != true;
                results.push(json!({"id":id,"status":if blocked{"blocked"}else if failed{"failed"}else{"passed"},"report":report}));
            }
            Err(error) => {
                failed = true;
                results.push(json!({"id":id,"status":"failed","error":{"code":error.code,"message":error.message}}));
            }
        }
        crate::save(
            &directory.join("report.json"),
            &json!({"passed":!failed,"cases":results}),
        )?;
    }
    let executed = results
        .iter()
        .any(|r| r["status"] == "passed" || r["status"] == "failed");
    let report = json!({"passed":!failed && !blocked && executed,"status":if failed{"failed"}else if blocked || !executed{"blocked"}else{"passed"},"cases":results,"directory":directory});
    crate::save(&directory.join("report.json"), &report)?;
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_rejects_unknown_and_duplicates() {
        let a = Args::parse(&["--case".into(), "x".into()]).unwrap();
        assert!(select(&[json!({"id":"y","state":"ready"})], &a).is_err());
        assert!(
            select(
                &[
                    json!({"id":"x","state":"ready"}),
                    json!({"id":"x","state":"ready"})
                ],
                &a
            )
            .is_err()
        );
        assert_eq!(
            select(&[json!({"id":"x","state":"ready"})], &a)
                .unwrap()
                .len(),
            1
        );
    }
}

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[test]
    fn unattended_requires_boolean_and_ready_read_only_case() {
        for case in [
            json!({"state":"ready","input":"none"}),
            json!({"state":"ready","input":"keyboard","unattended":true}),
            json!({"state":"manual","input":"none","unattended":true}),
            json!({"state":"ready","input":"none","unattended":"true"}),
        ] {
            assert!(!unattended(&case));
        }
        assert!(unattended(
            &json!({"state":"ready","input":"none","unattended":true})
        ));
    }
}
