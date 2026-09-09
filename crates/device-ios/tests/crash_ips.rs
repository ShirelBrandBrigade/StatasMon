//! `.ips` header/body parsing against the synthetic fixtures under
//! `fixtures/ios/logs/` (step 3.6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use statasmon_device_ios::crash::{ReportKind, parse_ips, parse_ips_timestamp};

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../fixtures/ios/logs/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn app_crash_fixture() {
    let s = parse_ips(&fixture("synthetic-app-crash.ips")).unwrap();
    assert_eq!(s.kind, ReportKind::Crash);
    assert_eq!(s.process.as_deref(), Some("FieldKit"));
    assert_eq!(s.bundle_id.as_deref(), Some("com.example.fieldkit"));
    assert_eq!(s.pid, Some(4242));
    assert_eq!(s.bug_type.as_deref(), Some("309"));
    assert_eq!(
        s.reason.as_deref(),
        Some("EXC_CRASH (SIGABRT) — Abort trap: 6")
    );
    assert_eq!(s.ts_ms, parse_ips_timestamp("2026-09-09 10:10:10.00 +0300"));
}

#[test]
fn jetsam_fixture_is_classified_not_a_crash() {
    let s = parse_ips(&fixture("synthetic-jetsam.ips")).unwrap();
    assert_eq!(s.kind, ReportKind::Jetsam);
    assert_eq!(s.process.as_deref(), Some("Heavy"));
    assert_eq!(s.pid, Some(5151));
    assert_eq!(s.reason.as_deref(), Some("jetsam: vm-pageshortage"));
    assert!(s.bundle_id.is_none());
}
