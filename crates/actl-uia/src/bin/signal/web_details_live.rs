//! Isolated host: no real task, desktop input or authorization changes.
use super::*;
use std::{cell::RefCell, rc::Rc, time::Duration};
use windows::{
    Win32::System::Com::{STREAM_SEEK_SET, StructuredStorage::CreateStreamOnHGlobal},
    core::w,
};

struct Host(HWND);
impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            close(self.0);
            let _ = DestroyWindow(self.0);
        }
    }
}
unsafe fn pump_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(12);
    while !done() && Instant::now() < deadline {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(done(), "WebView2 callback timed out");
}
unsafe fn script(view: &ICoreWebView2, source: &str) -> String {
    let result = Rc::new(RefCell::new(None));
    let output = result.clone();
    let source = super::super::paint::wide(source);
    view.ExecuteScript(
        PCWSTR(source.as_ptr()),
        &ExecuteScriptCompletedHandler::create(Box::new(move |status, json| {
            status?;
            *output.borrow_mut() = Some(json);
            Ok(())
        })),
    )
    .unwrap();
    pump_until(|| result.borrow().is_some());
    result.borrow_mut().take().unwrap()
}

#[test]
#[ignore = "requires interactive desktop and Evergreen WebView2; isolated host only"]
fn embedded_page_loads_updates_discloses_and_falls_back() {
    unsafe {
        let apartment = Apartment::new();
        assert!(apartment.initialized());
        let host = Host(
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("ACTL isolated WebView test"),
                WS_POPUP,
                0,
                0,
                560,
                700,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/webview-review");
        let paths =
            actl_core::state::SignalPaths::at(root.join(actl_core::snapshot::new_snapshot_id()));
        start(host.0, &paths).unwrap();
        pump_until(|| {
            STATE.with(|slot| slot.borrow().as_ref().is_some_and(|s| s.webview.is_some()))
                || error(host.0).is_some()
        });
        bounds(
            host.0,
            RECT {
                left: 0,
                top: 0,
                right: 512,
                bottom: 600,
            },
        );
        let view = STATE.with(|slot| slot.borrow().as_ref().unwrap().webview.clone().unwrap());
        eprintln!(
            "page: {}",
            script(
                &view,
                "JSON.stringify({state:document.readyState,bridge:!!window.chrome?.webview,status:document.querySelector('#status')?.textContent})"
            )
        );
        pump_until(|| active(host.0) || error(host.0).is_some());
        assert!(active(host.0), "{:?}", error(host.0));
        bounds(
            host.0,
            RECT {
                left: 0,
                top: 0,
                right: 512,
                bottom: 600,
            },
        );
        let _ = ShowWindow(host.0, SW_SHOWNOACTIVATE);
        let snapshot = serde_json::json!({"version":1,"title":"隔离预览","status":"已暂停","tone":"waiting","body":"1 / 2 步完成\n写入纪要\n你已请求暂停","records":"✓ 读取记录\n○ 写入纪要","diagnostics":"<img src=x onerror=alert(1)>","feedback":"","primary":"继续","primaryEnabled":true,"stop":"急停","shortcut":"Ctrl+Alt+F12"}).to_string();
        tick(host.0, &snapshot);
        pump_until(|| {
            script(&view, "document.querySelector('#status').textContent") == "\"已暂停\""
        });
        let before = height(host.0);
        assert_eq!(
            script(
                &view,
                "document.querySelector('#records-section').open=true; document.querySelector('#records-section').open"
            ),
            "true"
        );
        pump_until(|| height(host.0) > before);
        assert_eq!(
            script(
                &view,
                "document.querySelector('#diagnostics').children.length"
            ),
            "0"
        );
        tick(host.0, &snapshot);
        assert_eq!(
            script(&view, "document.querySelector('#records-section').open"),
            "true"
        );
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).unwrap();
        let captured = Rc::new(RefCell::new(None));
        let output = captured.clone();
        view.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &CapturePreviewCompletedHandler::create(Box::new(move |status| {
                *output.borrow_mut() = Some(status);
                Ok(())
            })),
        )
        .unwrap();
        pump_until(|| captured.borrow().is_some());
        captured.borrow_mut().take().unwrap().unwrap();
        let mut size = 0;
        stream
            .Seek(
                0,
                windows::Win32::System::Com::STREAM_SEEK_END,
                Some(&mut size),
            )
            .unwrap();
        stream.Seek(0, STREAM_SEEK_SET, None).unwrap();
        let mut data = vec![0u8; size as usize];
        stream
            .Read(data.as_mut_ptr().cast(), data.len() as u32, None)
            .ok()
            .unwrap();
        std::fs::write(root.join("paused-page.png"), data).unwrap();
        fail(host.0, "isolated failure injection");
        assert!(!active(host.0));
        assert!(error(host.0).unwrap().contains("NOT_ACTIONABLE"));
        assert!(STATE.with(|slot| slot.borrow().as_ref().unwrap().controller.is_none()));
        drop(view);
        drop(host);
        assert!(STATE.with(|slot| slot.borrow().is_none()));
    }
}
