mod support;

#[test]
fn native_inventory_envelope_and_unique_handles() {
    let output = support::IsolatedCommand::new()
        .command
        .args(["list-windows", "--native"])
        .output()
        .expect("CLI launch");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let mut envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = envelope["data"]["windows"].as_array().unwrap();
    assert_eq!(envelope["data"]["count"].as_u64(), Some(rows.len() as u64));
    let mut handles = std::collections::HashSet::new();
    for row in rows {
        let hwnd = row["hwnd"].as_i64().expect("native HWND");
        assert_ne!(hwnd, 0);
        assert!(handles.insert(hwnd), "duplicate native window");
        assert!(row["pid"].as_u64().is_some_and(|pid| pid > 0));
        assert!(row["title"].is_string());
        assert!(row["class"].is_string());
        assert!(row["foreground"].is_boolean());
    }
    assert!(envelope["duration_ms"].is_u64());
    envelope["duration_ms"] = "<duration>".into();
    envelope["data"]["count"] = "<count>".into();
    envelope["data"]["windows"] = "<windows>".into();
    // monitors 是 spec 字段(2026-10-14):内容随真实显示器拓扑变化,
    // golden 钉形状占位;拓扑语义(≥1 屏/恰一主屏)在 cli_contract 断言
    envelope["data"]["monitors"] = "<monitors>".into();
    let golden: serde_json::Value =
        serde_json::from_str(include_str!("golden/list_windows_native.json")).unwrap();
    assert_eq!(envelope, golden);
}

#[test]
fn exact_native_handle_requires_explicit_mode_and_valid_handle() {
    let missing_mode = support::IsolatedCommand::new()
        .command
        .args(["list-windows", "--hwnd", "42"])
        .output()
        .unwrap();
    assert_eq!(missing_mode.status.code(), Some(2));
    let zero = support::IsolatedCommand::new()
        .command
        .args(["list-windows", "--native", "--hwnd", "0"])
        .output()
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&zero.stdout).unwrap();
    assert_eq!(zero.status.code(), Some(2));
    assert_eq!(envelope["error"]["code"], "PROTOCOL");
}
