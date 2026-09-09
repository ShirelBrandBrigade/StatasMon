//! Replay the captured os_trace fixture through `thermal::Derive` and the
//! `thermalmonitord` line list (step 3.5). Both fixtures are recorded by
//! `examples/capture_logs_fixture.rs`; the tests skip (loudly) when the
//! files are not there yet.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use statasmon_device_ios::thermal::{Derive, Derived, parse_thermal_state};
use statasmon_protocol::{EventKind, LogEntry, LogLevel};

fn fixture(name: &str) -> Option<String> {
    let path = format!(
        "{}/../../fixtures/ios/logs/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    match std::fs::read_to_string(&path) {
        Ok(t) => Some(t),
        Err(e) => {
            eprintln!("skipped: {path}: {e}");
            None
        }
    }
}

#[test]
fn ostrace_jsonl_replays_without_panics() {
    let Some(text) = fixture("ipad-17.4.1-ostrace-20s.jsonl") else {
        return;
    };
    let mut d = Derive::new();
    let mut n = 0usize;
    let mut last_seq = 0u64;
    let mut derived = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let e: LogEntry = serde_json::from_str(line).unwrap();
        assert!(e.seq > last_seq, "seq must be monotonic");
        last_seq = e.seq;
        assert!(e.ts_ms > 1_600_000_000_000, "ts_ms must be epoch ms");
        assert!(!e.process.is_empty());
        assert!(e.tag.is_none() && e.buffer.is_none());
        n += 1;
        for x in d.observe(&e) {
            derived += 1;
            if let Derived::Event { kind, detail, .. } = &x {
                assert!(
                    matches!(
                        kind,
                        EventKind::ThermalChange
                            | EventKind::Jetsam
                            | EventKind::MemoryWarning
                            | EventKind::Crash
                    ),
                    "{detail}"
                );
            }
        }
    }
    assert!(n > 0, "fixture is empty");
    let levels = text
        .lines()
        .filter_map(|l| serde_json::from_str::<LogEntry>(l).ok())
        .map(|e| e.level)
        .collect::<std::collections::BTreeSet<LogLevel>>();
    eprintln!(
        "{n} entries, {derived} derived items, levels {levels:?}, last thermal {:?}",
        d.last_state()
    );
}

#[test]
fn thermalmonitord_lines_parse_or_are_chatter() {
    let Some(text) = fixture("thermalmonitord-lines.txt") else {
        return;
    };
    let mut with_level = 0usize;
    let mut total = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        total += 1;
        if parse_thermal_state(line).is_some() {
            with_level += 1;
        }
    }
    eprintln!("{with_level} of {total} thermalmonitord lines carry a level");
}
