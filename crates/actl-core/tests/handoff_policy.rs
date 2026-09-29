use actl_core::handoff::{Approval, Consent};

#[test]
fn revoked_consent_preserves_the_actual_reason() {
    let mut c = Consent::default();
    c.allow_with_policy("run", 1, 100, Approval::OncePerRun);
    assert!(!c.check("run", 2, 1000));
    assert_eq!(c.reason(), "external_input");
    c.revoke_with_reason("input_monitor_unavailable");
    assert_eq!(
        c.reason(),
        "external_input",
        "keep the first revocation cause"
    );
    assert!(!c.check("run", 2, 1100));
    assert_eq!(c.reason(), "external_input");
    c.allow("run", 2, 1200);
    assert!(!c.check("run", 2, 61_200));
    assert_eq!(c.reason(), "idle_expired");
}

#[test]
fn once_per_run_survives_idle_and_total_duration_but_not_takeover() {
    let mut consent = Consent::default();
    assert!(!consent.check("run-a", 0, 1));
    consent.allow_with_policy("run-a", 0, 100, Approval::OncePerRun);
    assert!(consent.check_policy("run-a", 0, 70_000, Approval::OncePerRun));
    assert!(consent.check_policy("run-a", 0, 3_600_000, Approval::OncePerRun));
    assert!(!consent.check_policy("run-b", 0, 3_600_001, Approval::OncePerRun));
    assert!(!consent.check_policy("run-a", 1, 3_600_002, Approval::OncePerRun));
    assert!(!consent.check_policy("run-a", 1, 3_600_003, Approval::OncePerRun));
}

#[test]
fn policy_cannot_be_upgraded_without_confirmation() {
    let mut consent = Consent::default();
    consent.allow("run-a", 0, 100);
    assert!(!consent.check_policy("run-a", 0, 101, Approval::OncePerRun));
    consent.allow_with_policy("run-a", 0, 100, Approval::OncePerRun);
    assert!(!consent.check_policy("run-a", 0, 99, Approval::OncePerRun));
    consent.allow_with_policy("run-a", 0, 100, Approval::OncePerRun);
    consent.revoke();
    assert!(!consent.check("run-a", 0, 101));
}
