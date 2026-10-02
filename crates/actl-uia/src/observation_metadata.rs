//! Content-free sampled input metadata. Never used for consent or execution decisions.
use actl_core::{display::DisplayLease, state::SignalPaths};
use serde::{Deserialize, Serialize};
use std::io::Read;

pub const SAMPLE_FILE: &str = "observation-input.json";
const MAX_AGE_MS: u64 = 1500;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activity {
    pub signals: u64,
    pub last_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Monitor {
    Healthy,
    WatcherMissing,
    HeartbeatStale,
    RegistrationReadFailed,
    RegistrationMissing,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSample {
    pub version: u32,
    pub pid: u32,
    pub epoch: String,
    pub ts_ms: u64,
    pub monitor: Monitor,
    pub device: Activity,
    pub foreign_injected: Activity,
    pub own_injection: Activity,
}

#[derive(Debug, Serialize)]
pub(crate) struct SampleRead {
    pub status: &'static str,
    pub sample: Option<InputSample>,
}

/// This interval belongs to the sampler, not exactly to the provider call.
/// Its end can lag the operation; zero deltas never prove absence of user input.
pub(crate) fn delta(
    before: Option<&SampleRead>,
    after: Option<&SampleRead>,
) -> Option<serde_json::Value> {
    let before = before
        .filter(|v| v.status == "available")?
        .sample
        .as_ref()?;
    let after = after.filter(|v| v.status == "available")?.sample.as_ref()?;
    if before.epoch != after.epoch || before.pid != after.pid || before.ts_ms > after.ts_ms {
        return None;
    }
    Some(serde_json::json!({
        "from_ms":before.ts_ms,"to_ms":after.ts_ms,
        "device":after.device.signals.checked_sub(before.device.signals)?,
        "foreign_injected":after.foreign_injected.signals.checked_sub(before.foreign_injected.signals)?,
        "own_injection":after.own_injection.signals.checked_sub(before.own_injection.signals)?
    }))
}

fn validate(sample: InputSample, lease: Option<&DisplayLease>, now: u64) -> SampleRead {
    let status = if sample.version != 1 || !actl_core::history::valid_id(&sample.epoch) {
        "invalid"
    } else if sample.ts_ms > now || now.saturating_sub(sample.ts_ms) >= MAX_AGE_MS {
        "stale"
    } else if !lease.is_some_and(|l| {
        l.compatible(now) && l.state != "closed" && l.epoch == sample.epoch && l.pid == sample.pid
    }) {
        "display_unavailable_or_changed"
    } else if !matches!(sample.monitor, Monitor::Healthy) {
        "monitor_unavailable"
    } else {
        "available"
    };
    // Invalid/stale data must not become evidence of zero activity.
    SampleRead {
        status,
        sample: (status == "available" || status == "monitor_unavailable").then_some(sample),
    }
}

pub(crate) fn read(paths: &SignalPaths, now: u64) -> SampleRead {
    let parsed = (|| -> std::io::Result<InputSample> {
        let mut bytes = Vec::new();
        std::fs::File::open(paths.dir.join(SAMPLE_FILE))?
            .take(4097)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(std::io::Error::other("oversized input sample"));
        }
        Ok(serde_json::from_slice(&bytes)?)
    })();
    match parsed {
        Ok(sample) => validate(sample, paths.display_lease().as_ref(), now),
        Err(error) => SampleRead {
            status: if error.kind() == std::io::ErrorKind::NotFound {
                "missing"
            } else {
                "unreadable_or_invalid"
            },
            sample: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> InputSample {
        InputSample {
            version: 1,
            pid: 42,
            epoch: "display-42".into(),
            ts_ms: 100,
            monitor: Monitor::Healthy,
            device: Activity {
                signals: 3,
                last_ms: Some(99),
            },
            foreign_injected: Activity::default(),
            own_injection: Activity::default(),
        }
    }
    fn lease() -> DisplayLease {
        DisplayLease {
            protocol: actl_core::display::DISPLAY_PROTOCOL,
            build: env!("CARGO_PKG_VERSION").into(),
            pid: 42,
            epoch: "display-42".into(),
            ts_ms: 100,
            state: "ready".into(),
        }
    }
    #[test]
    fn missing_stale_or_restarted_monitor_is_unknown_activity() {
        assert_eq!(validate(sample(), Some(&lease()), 101).status, "available");
        for now in [99, 1600] {
            let read = validate(sample(), Some(&lease()), now);
            assert_eq!(read.status, "stale");
            assert!(read.sample.is_none());
        }
        let mut changed = lease();
        changed.epoch = "display-next".into();
        assert!(validate(sample(), Some(&changed), 101).sample.is_none());
        assert!(validate(sample(), None, 101).sample.is_none());
        let mut unhealthy = sample();
        unhealthy.monitor = Monitor::HeartbeatStale;
        assert_eq!(
            validate(unhealthy, Some(&lease()), 101).status,
            "monitor_unavailable"
        );
    }
    #[test]
    fn sample_rejects_content_and_has_a_metadata_golden() {
        let value = serde_json::to_value(sample()).unwrap();
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/golden/observation_input_sample.json"
        ))
        .unwrap();
        assert_eq!(value, expected);
        let mut private = value;
        private["keys"] = serde_json::json!("private input");
        assert!(serde_json::from_value::<InputSample>(private).is_err());
    }
    #[test]
    fn activity_delta_cannot_cross_monitor_restarts_or_unknown_samples() {
        let before = validate(sample(), Some(&lease()), 101);
        let mut next = sample();
        next.device.signals += 2;
        next.ts_ms = 102;
        let after = validate(next.clone(), Some(&lease()), 103);
        assert_eq!(delta(Some(&before), Some(&after)).unwrap()["device"], 2);
        assert!(delta(None, Some(&after)).is_none());
        next.device.signals = 0;
        assert!(
            delta(
                Some(&before),
                Some(&validate(next.clone(), Some(&lease()), 103))
            )
            .is_none()
        );
        next.epoch = "restarted".into();
        assert!(delta(Some(&before), Some(&validate(next, Some(&lease()), 103))).is_none());
        assert!(
            delta(
                Some(&before),
                Some(&validate(sample(), Some(&lease()), 2000))
            )
            .is_none()
        );
    }
}
