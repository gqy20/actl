//! Compact native display. Execution facts arrive exclusively through the side channel.
#![allow(unsafe_code, unsafe_op_in_unsafe_fn)]
#![windows_subsystem = "windows"]
#[path = "signal/controls.rs"]
mod controls;
#[path = "signal/flow_ui.rs"]
mod flow_ui;
#[path = "signal/frame.rs"]
mod frame;
#[path = "signal/handoff.rs"]
mod handoff;
#[path = "signal/icon.rs"]
mod icon;
#[path = "signal/input_watch.rs"]
mod input_watch;
#[path = "signal/model.rs"]
mod model;
#[path = "signal/overlay.rs"]
mod overlay;
#[path = "signal/paint.rs"]
mod paint;
#[path = "signal/panel.rs"]
mod panel;
#[cfg(test)]
#[path = "signal/panel_preview.rs"]
mod panel_preview;
#[path = "signal/perimeter.rs"]
mod perimeter;
#[path = "signal/popup.rs"]
mod popup;
#[path = "signal/shell.rs"]
mod shell;
#[path = "signal/ui_text.rs"]
mod ui_text;
fn main() {
    let result = unsafe {
        if std::env::args().any(|arg| arg == "--controls") {
            controls::run()
        } else {
            shell::run()
        }
    };
    if let Err(e) = result {
        eprintln!("INTERNAL: actl-signal: {e}");
        std::process::exit(1);
    }
}
