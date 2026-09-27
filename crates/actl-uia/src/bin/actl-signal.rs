//! Compact native display. Execution facts arrive exclusively through the side channel.
#![allow(unsafe_code, unsafe_op_in_unsafe_fn)]
#![windows_subsystem = "windows"]
#[path = "signal/icon.rs"]
mod icon;
#[path = "signal/model.rs"]
mod model;
#[path = "signal/overlay.rs"]
mod overlay;
#[path = "signal/paint.rs"]
mod paint;
#[path = "signal/popup.rs"]
mod popup;
#[path = "signal/shell.rs"]
mod shell;
fn main() {
    if let Err(e) = unsafe { shell::run() } {
        eprintln!("INTERNAL: actl-signal: {e}");
        std::process::exit(1);
    }
}
