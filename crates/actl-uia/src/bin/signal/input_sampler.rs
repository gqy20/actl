//! One replaceable metadata sample, off both the UI and input-hook threads.
use actl_core::state::{SignalPaths, unix_ms};
use actl_uia::observation_metadata::{InputSample, Monitor, SAMPLE_FILE};
use std::{sync::OnceLock, time::Duration};

pub(super) fn start(paths: &SignalPaths, epoch: &str) {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let paths = paths.clone();
        let epoch = epoch.to_string();
        let failure_paths = paths.clone();
        let failure_call = format!("sample-{epoch}");
        let result = std::thread::Builder::new()
            .name("signal-input-sample".into())
            .spawn(move || {
                let pid = std::process::id();
                let mut failed = false;
                loop {
                    if paths
                        .display_lease()
                        .is_some_and(|l| l.pid != pid || l.epoch != epoch || l.state == "closed")
                    {
                        break;
                    }
                    let monitor = match super::input_watch::health_reason() {
                        "healthy" => Monitor::Healthy,
                        "watcher_missing" => Monitor::WatcherMissing,
                        "heartbeat_stale" => Monitor::HeartbeatStale,
                        "registration_missing" => Monitor::RegistrationMissing,
                        _ => Monitor::RegistrationReadFailed,
                    };
                    let [device, foreign_injected, own_injection] =
                        super::input_diagnostics::activity();
                    let sample = InputSample {
                        version: 1,
                        pid,
                        epoch: epoch.clone(),
                        ts_ms: unix_ms(),
                        monitor,
                        device,
                        foreign_injected,
                        own_injection,
                    };
                    match paths.write_display(SAMPLE_FILE, &sample) {
                        Ok(()) => failed = false,
                        Err(error) if !failed => {
                            failed = true;
                            actl_core::log_health::failure(
                                &paths.dir,
                                &format!("sample-{epoch}"),
                                None,
                                "input_sample_write",
                                1,
                                &error,
                            );
                            eprintln!("INTERNAL: input diagnostic sample unavailable: {error}");
                        }
                        Err(_) => {}
                    }
                    std::thread::sleep(Duration::from_millis(if failed { 1000 } else { 200 }));
                }
            });
        if let Err(error) = result {
            actl_core::log_health::failure(
                &failure_paths.dir,
                &failure_call,
                None,
                "input_sampler_start",
                0,
                &error,
            );
            eprintln!("INTERNAL: input diagnostic sampler unavailable: {error}");
        }
    });
}
