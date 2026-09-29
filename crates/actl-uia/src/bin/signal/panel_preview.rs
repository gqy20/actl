//! Render actual hidden native controls; never request or grant desktop consent.
use windows::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

unsafe fn export(hwnd: HWND, name: &str) {
    let mut r = RECT::default();
    GetWindowRect(hwnd, &mut r).unwrap();
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    let dc = CreateCompatibleDC(None);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old = SelectObject(dc, bitmap.into());
    SendMessageW(
        hwnd,
        WM_PRINT,
        Some(WPARAM(dc.0 as usize)),
        Some(LPARAM(
            (PRF_NONCLIENT | PRF_CLIENT | PRF_ERASEBKGND | PRF_CHILDREN) as isize,
        )),
    );
    let data = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
    // WM_PRINT omits the compositor's window-region clipping. Export that exact region as alpha.
    let region = CreateRectRgn(0, 0, 0, 0);
    assert_ne!(GetWindowRgn(hwnd, region), GDI_REGION_TYPE(0));
    for (index, pixel) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        pixel[3] = if PtInRegion(region, index as i32 % width, index as i32 / width).as_bool() {
            255
        } else {
            0
        };
    }
    let _ = DeleteObject(region.into());
    // A real paint must have produced more than a blank canvas.
    assert!(data.as_chunks::<4>().0.iter().any(|p| p[0] > 120));
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/panel-review");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{name}.bgra")), &*data).unwrap();
    std::fs::write(
        dir.join(format!("{name}.size")),
        format!("{width} {height}"),
    )
    .unwrap();
    SelectObject(dc, old);
    let _ = DeleteObject(bitmap.into());
    let _ = DeleteDC(dc);
}

#[test]
#[ignore = "exports native panel renders without showing windows or granting consent"]
fn export_panel_previews() {
    unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let paths = actl_core::state::SignalPaths::at(
            std::env::temp_dir().join(actl_core::snapshot::new_snapshot_id()),
        );
        let dir = paths.dir.join("flows").join("preview");
        std::fs::create_dir_all(&dir).unwrap();
        let plan = serde_json::json!({"name":"整理会议记录", "steps":[{"id":"read-notes", "title":"读取记录"}, {"id":"write-notes", "title":"写入纪要"}]});
        let state = serde_json::json!({"status":"paused", "updated_ms":100, "reason":"user_paused", "steps":[{"id":"read-notes", "status":"completed", "verified":true}, {"id":"write-notes", "status":"pending"}]});
        std::fs::write(dir.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
        std::fs::write(dir.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        let request = actl_core::handoff::Request {
            approval: actl_core::handoff::Approval::Expiring,
            generation: "preview-only".into(),
            call: "preview-call".into(),
            task: "preview".into(),
            target: "记事本 · 本周会议纪要.txt".into(),
            ts_ms: 0,
            action: "click".into(),
        };
        let handoff = super::handoff::build_panel_with(Some(&request), &paths).unwrap();
        export(handoff, "handoff");
        DestroyWindow(handoff).unwrap();
        let controls = super::controls::create_window().unwrap();
        super::controls::refresh_with(controls, &paths, Some("preview"));
        export(controls, "controls");
        DestroyWindow(controls).unwrap();
        std::fs::remove_dir_all(paths.dir).unwrap();
        let _ = SetThreadDpiAwarenessContext(previous);
    }
}
