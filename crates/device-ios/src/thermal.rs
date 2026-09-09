//! Thermal state and memory events derived from device log lines
//! (ADR-006, step 3.5).
//!
//! iOS has no host-side thermal query: the only signal is what
//! `thermalmonitord` writes to the unified log when the pressure level
//! changes. [`Derive`] watches every [`LogEntry`] of the os_trace stream
//! and turns the interesting ones into `thermal.state` / `thermal.raw`
//! samples and `ThermalChange`, `Jetsam`, `MemoryWarning` and `Crash`
//! events. Everything here is a pure function of the entry text so it can
//! be tested against captured lines (`fixtures/ios/logs/*.txt`).
//!
//! Wordings handled (see the fixture for what iPadOS 17.4.1 really says):
//! - `thermalmonitord`: `Thermal pressure level changed …`, `thermal state:
//!   Nominal|Fair|Serious|Critical`, `… from X to Y`, `X -> Y`, `X → Y`,
//!   bare numeric levels 0–3 and the 0/10/20/30 scale.
//! - `kernel` / `memorystatus`: `memorystatus: killing_… pid N [Name]
//!   (vm-pageshortage …)`, anything mentioning `jetsam` → `Jetsam`.
//! - `Received memory warning`, `didReceiveMemoryWarning`,
//!   `memorystatus_notification`, `memory pressure … warning/critical` →
//!   `MemoryWarning`.
//! - `ReportCrash` / `osanalyticsd`: `Formulating fatal … report for
//!   corpse[pid] Name`, `Saved type 'X' report … at /…/Name-….ips`,
//!   `Crash Report …` → `Crash` (or `Jetsam` for `JetsamEvent-…` reports).

use statasmon_protocol::{EventKind, LogEntry, MetricId};

/// The four iOS thermal pressure levels, in severity order.
pub const STATE_NAMES: [&str; 4] = ["Nominal", "Fair", "Serious", "Critical"];

/// Name of a thermal level 0–3 (`"?"` outside the range).
pub fn state_name(state: u8) -> &'static str {
    STATE_NAMES.get(usize::from(state)).copied().unwrap_or("?")
}

/// What one log entry contributes: a metric sample or a discrete event.
#[derive(Debug, Clone, PartialEq)]
pub enum Derived {
    Sample {
        id: MetricId,
        value: f64,
    },
    Event {
        kind: EventKind,
        detail: String,
        pid: Option<u32>,
        process: Option<String>,
    },
}

/// Stateful deriver: remembers the last thermal level so a sample is
/// emitted at first sight and then only on change (the source may re-emit
/// [`Derive::last_state`] every medium tick).
#[derive(Debug, Default, Clone)]
pub struct Derive {
    last_state: Option<u8>,
}

impl Derive {
    pub fn new() -> Self {
        Self::default()
    }

    /// The last thermal level seen (0–3), if any transition was logged
    /// since the stream started.
    pub fn last_state(&self) -> Option<u8> {
        self.last_state
    }

    /// Inspect one entry. Cheap on the hot path: a process-name check
    /// first, substring scans only for the handful of interesting senders.
    pub fn observe(&mut self, entry: &LogEntry) -> Vec<Derived> {
        let process = entry.process.as_str();
        let msg = entry.message.as_str();
        let mut out = Vec::new();

        if process.eq_ignore_ascii_case("thermalmonitord") || is_thermal_subsystem(entry) {
            if let Some(state) = parse_thermal_state(msg) {
                self.push_thermal(state, &mut out);
            }
            return out;
        }

        if let Some(d) = derive_memory(entry) {
            out.push(d);
            return out;
        }
        if let Some(d) = derive_crash(entry) {
            out.push(d);
        }
        out
    }

    fn push_thermal(&mut self, state: u8, out: &mut Vec<Derived>) {
        let prev = self.last_state;
        if prev == Some(state) {
            return;
        }
        self.last_state = Some(state);
        out.push(Derived::Sample {
            id: MetricId::ThermalState,
            value: f64::from(state),
        });
        out.push(Derived::Sample {
            id: MetricId::ThermalRaw,
            value: f64::from(state),
        });
        if let Some(prev) = prev {
            out.push(Derived::Event {
                kind: EventKind::ThermalChange,
                detail: format!("{} → {}", state_name(prev), state_name(state)),
                pid: None,
                process: Some("thermalmonitord".to_owned()),
            });
        }
    }
}

fn is_thermal_subsystem(entry: &LogEntry) -> bool {
    entry
        .subsystem
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("com.apple.thermalmonitord"))
}

/// Extract the *new* thermal level from a `thermalmonitord` line, or
/// `None` when the line is not a state announcement.
pub fn parse_thermal_state(msg: &str) -> Option<u8> {
    let lower = msg.to_ascii_lowercase();
    // Named levels: the last one on the line is the new state
    // ("Nominal -> Fair", "changed from Fair to Serious", "state: Critical").
    let mut named: Option<(usize, u8)> = None;
    for (level, name) in STATE_NAMES.iter().enumerate() {
        let needle = name.to_ascii_lowercase();
        let mut from = 0;
        while let Some(pos) = lower[from..].find(&needle) {
            let at = from + pos;
            if is_word(&lower, at, needle.len()) && named.is_none_or(|(p, _)| at > p) {
                named = Some((at, level as u8));
            }
            from = at + needle.len();
        }
    }
    if let Some((_, level)) = named {
        return Some(level);
    }
    // Numeric levels only after an explicit key ("level", "state",
    // "pressure") so temperatures and pids are never mistaken for a level.
    let keyed = lower.contains("pressure level")
        || lower.contains("thermal level")
        || lower.contains("thermal state")
        || lower.contains("pressure state")
        || lower.contains("thermal pressure");
    if !keyed {
        return None;
    }
    numeric_level(&lower)
}

/// Last small integer on the line, on the 0–3 or 0/10/20/30 scales.
fn numeric_level(lower: &str) -> Option<u8> {
    let mut last: Option<u32> = None;
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let after_ok = i >= bytes.len() || !bytes[i].is_ascii_alphanumeric();
            if before_ok && after_ok && i - start <= 2 {
                last = lower[start..i].parse().ok();
            }
        } else {
            i += 1;
        }
    }
    match last? {
        n @ 0..=3 => Some(n as u8),
        10..=19 => Some(1),
        20..=29 => Some(2),
        30..=99 => Some(3),
        _ => None,
    }
}

fn is_word(s: &str, at: usize, len: usize) -> bool {
    let b = s.as_bytes();
    let before = at == 0 || !b[at - 1].is_ascii_alphanumeric();
    let after = at + len >= b.len() || !b[at + len].is_ascii_alphanumeric();
    before && after
}

/// Jetsam kills and memory warnings.
fn derive_memory(entry: &LogEntry) -> Option<Derived> {
    let msg = entry.message.as_str();
    let lower = msg.to_ascii_lowercase();
    let first = msg.lines().next().unwrap_or("").trim();

    let jetsam_kill = (lower.contains("memorystatus") && lower.contains("kill"))
        || lower.contains("jetsam")
        || (lower.contains("vm_pageshortage") || lower.contains("vm-pageshortage"))
            && lower.contains("kill");
    if jetsam_kill && !lower.contains("saved type") && !lower.contains(".ips") {
        let pid = after_key(first, "pid ").and_then(leading_u32);
        let process = between(first, "[", "]")
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.chars().all(|c| c.is_ascii_digit()))
            .map(str::to_owned);
        return Some(Derived::Event {
            kind: EventKind::Jetsam,
            detail: first.to_owned(),
            pid,
            process,
        });
    }

    let warning = lower.contains("received memory warning")
        || lower.contains("didreceivememorywarning")
        || lower.contains("memorystatus_notification")
        || lower.contains("memory pressure notification")
        || (lower.contains("memory pressure")
            && (lower.contains("warning") || lower.contains("critical")));
    if warning {
        return Some(Derived::Event {
            kind: EventKind::MemoryWarning,
            detail: first.to_owned(),
            pid: entry.pid,
            process: Some(entry.process.clone()),
        });
    }
    None
}

/// Crash-reporter lines. Only `ReportCrash` / `osanalyticsd` /
/// `SpringBoard` senders are considered so app chatter never matches.
fn derive_crash(entry: &LogEntry) -> Option<Derived> {
    let sender = entry.process.as_str();
    let reporter = sender.eq_ignore_ascii_case("ReportCrash")
        || sender.eq_ignore_ascii_case("osanalyticsd")
        || entry
            .subsystem
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("com.apple.ReportCrash"));
    if !reporter {
        return None;
    }
    let msg = entry.message.as_str();
    let first = msg.lines().next().unwrap_or("").trim();
    let lower = first.to_ascii_lowercase();

    // "Formulating fatal 309 report for corpse[1234] Foo"
    if lower.contains("formulating") && lower.contains("report") {
        let pid = between(first, "corpse[", "]").and_then(|s| s.trim().parse().ok());
        let process = first
            .split_once("] ")
            .map(|(_, rest)| rest.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let kind = if lower.contains("jetsam") || lower.contains(" 298 ") {
            EventKind::Jetsam
        } else {
            EventKind::Crash
        };
        return Some(Derived::Event {
            kind,
            detail: process
                .clone()
                .map(|p| format!("Crash: {p}"))
                .unwrap_or_else(|| first.to_owned()),
            pid,
            process,
        });
    }

    // "Saved type '309' report (2 of max 25) at /…/Foo-2026-09-09-101010.ips"
    if lower.contains("saved") && lower.contains(".ips") {
        let file = first
            .rsplit(['/', ' '])
            .find(|s| s.ends_with(".ips"))
            .unwrap_or("");
        let stem = file.trim_end_matches(".ips");
        let process = report_stem_process(stem);
        let kind = if stem.starts_with("JetsamEvent")
            || lower.contains("type '298'")
            || lower.contains("jetsam")
        {
            EventKind::Jetsam
        } else {
            EventKind::Crash
        };
        return Some(Derived::Event {
            kind,
            detail: match (&process, kind) {
                (Some(p), EventKind::Crash) => format!("Crash report saved: {p}"),
                (Some(p), _) => format!("Jetsam report saved: {p}"),
                (None, _) => first.to_owned(),
            },
            pid: None,
            process,
        });
    }

    if lower.contains("crash report") {
        return Some(Derived::Event {
            kind: EventKind::Crash,
            detail: first.to_owned(),
            pid: None,
            process: None,
        });
    }
    None
}

/// `Foo-2026-09-09-101010` → `Foo`; `JetsamEvent-…` → `JetsamEvent`;
/// `log-…` (analytics) → `None`.
pub fn report_stem_process(stem: &str) -> Option<String> {
    let name = stem.split('-').next().unwrap_or("").trim();
    if name.is_empty() || name == "log" {
        return None;
    }
    Some(name.to_owned())
}

fn after_key<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let at = s.find(key)? + key.len();
    Some(&s[at..])
}

fn leading_u32(s: &str) -> Option<u32> {
    let digits: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let a = s.find(start)? + start.len();
    let b = s[a..].find(end)? + a;
    Some(&s[a..b])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use statasmon_protocol::LogLevel;

    fn entry(process: &str, message: &str) -> LogEntry {
        LogEntry {
            seq: 0,
            ts_ms: 0,
            pid: Some(71),
            process: process.into(),
            tag: None,
            subsystem: None,
            category: None,
            buffer: None,
            level: LogLevel::Notice,
            message: message.into(),
        }
    }

    #[test]
    fn thermal_named_transitions() {
        assert_eq!(
            parse_thermal_state("Thermal pressure state changed from Nominal to Fair"),
            Some(1)
        );
        assert_eq!(parse_thermal_state("thermal state: Serious"), Some(2));
        assert_eq!(parse_thermal_state("Critical -> Nominal"), Some(0));
        assert_eq!(parse_thermal_state("Nominal → Critical"), Some(3));
        // "fairly" is not "Fair".
        assert_eq!(parse_thermal_state("thermal: fairly warm"), None);
        assert_eq!(parse_thermal_state("cpu temp 45"), None);
    }

    #[test]
    fn thermal_numeric_levels() {
        assert_eq!(
            parse_thermal_state("Thermal pressure level changed from 0 to 10"),
            Some(1)
        );
        assert_eq!(parse_thermal_state("thermal pressure level: 20"), Some(2));
        assert_eq!(parse_thermal_state("Thermal state 3"), Some(3));
        assert_eq!(parse_thermal_state("Thermal pressure level = 0"), Some(0));
        // A level key without a parsable level.
        assert_eq!(parse_thermal_state("thermal pressure level unknown"), None);
        // Temperatures alone never count.
        assert_eq!(parse_thermal_state("Thermal sensors: 37 38 41"), None);
    }

    #[test]
    fn derive_emits_sample_first_then_change_events() {
        let mut d = Derive::new();
        assert_eq!(d.last_state(), None);
        let out = d.observe(&entry("thermalmonitord", "thermal state: Nominal"));
        assert_eq!(out.len(), 2, "first sight: samples only");
        assert!(matches!(
            out[0],
            Derived::Sample {
                id: MetricId::ThermalState,
                value
            } if value == 0.0
        ));
        assert_eq!(d.last_state(), Some(0));
        // Same state again: nothing.
        assert!(
            d.observe(&entry("thermalmonitord", "thermal state: Nominal"))
                .is_empty()
        );
        let out = d.observe(&entry(
            "thermalmonitord",
            "Thermal pressure changed from Nominal to Fair",
        ));
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[2],
            Derived::Event {
                kind: EventKind::ThermalChange,
                detail: "Nominal → Fair".into(),
                pid: None,
                process: Some("thermalmonitord".into()),
            }
        );
        assert_eq!(d.last_state(), Some(1));
        // Other processes' thermal chatter is ignored.
        assert!(
            d.observe(&entry("SpringBoard", "thermal state: Critical"))
                .is_empty()
        );
    }

    #[test]
    fn jetsam_kill_lines() {
        let mut d = Derive::new();
        let out = d.observe(&entry(
            "kernel",
            "memorystatus: killing_top_process pid 1234 [FieldKit] (vm-pageshortage 1) 123456KB - memorystatus_available_pages: 8000",
        ));
        assert_eq!(
            out,
            vec![Derived::Event {
                kind: EventKind::Jetsam,
                detail: "memorystatus: killing_top_process pid 1234 [FieldKit] (vm-pageshortage 1) 123456KB - memorystatus_available_pages: 8000".into(),
                pid: Some(1234),
                process: Some("FieldKit".into()),
            }]
        );
        let out = d.observe(&entry(
            "kernel",
            "memorystatus: killing_specific_process pid 987 [com.example.app] (per-process-limit 2) 210000KB",
        ));
        assert!(
            matches!(&out[0], Derived::Event { kind: EventKind::Jetsam, pid: Some(987), process: Some(p), .. } if p == "com.example.app")
        );
        let out = d.observe(&entry("kernel", "jetsam: killed pid 42 (Foo)"));
        assert!(matches!(
            &out[0],
            Derived::Event {
                kind: EventKind::Jetsam,
                pid: Some(42),
                ..
            }
        ));
    }

    #[test]
    fn memory_warning_lines() {
        let mut d = Derive::new();
        let out = d.observe(&entry("FieldKit", "Received memory warning."));
        assert_eq!(
            out,
            vec![Derived::Event {
                kind: EventKind::MemoryWarning,
                detail: "Received memory warning.".into(),
                pid: Some(71),
                process: Some("FieldKit".into()),
            }]
        );
        let out = d.observe(&entry(
            "SpringBoard",
            "-[UIApplication didReceiveMemoryWarning] called",
        ));
        assert!(matches!(
            &out[0],
            Derived::Event {
                kind: EventKind::MemoryWarning,
                ..
            }
        ));
        let out = d.observe(&entry(
            "runningboardd",
            "memorystatus_notification: pressure level warning",
        ));
        assert!(matches!(
            &out[0],
            Derived::Event {
                kind: EventKind::MemoryWarning,
                ..
            }
        ));
        assert!(
            d.observe(&entry("Foo", "memory usage is fine, no pressure"))
                .is_empty()
        );
    }

    #[test]
    fn crash_reporter_lines() {
        let mut d = Derive::new();
        let out = d.observe(&entry(
            "ReportCrash",
            "Formulating fatal 309 report for corpse[4242] FieldKit",
        ));
        assert_eq!(
            out,
            vec![Derived::Event {
                kind: EventKind::Crash,
                detail: "Crash: FieldKit".into(),
                pid: Some(4242),
                process: Some("FieldKit".into()),
            }]
        );
        let out = d.observe(&entry(
            "osanalyticsd",
            "Saved type '309' report (2 of max 25) at /private/var/mobile/Library/Logs/CrashReporter/FieldKit-2026-09-09-101010.ips",
        ));
        assert!(
            matches!(&out[0], Derived::Event { kind: EventKind::Crash, process: Some(p), .. } if p == "FieldKit")
        );
        let out = d.observe(&entry(
            "osanalyticsd",
            "Saved type '298' report (1 of max 25) at /private/var/mobile/Library/Logs/CrashReporter/JetsamEvent-2026-09-09-101010.ips",
        ));
        assert!(matches!(
            &out[0],
            Derived::Event {
                kind: EventKind::Jetsam,
                ..
            }
        ));
        // Analytics "log-…" reports are not crashes of a named process.
        let out = d.observe(&entry(
            "osanalyticsd",
            "Saved type '211' report (1 of max 25) at /…/log-power-2026-09-09-101010.ips",
        ));
        assert!(matches!(
            &out[0],
            Derived::Event {
                kind: EventKind::Crash,
                process: None,
                ..
            }
        ));
        // A user app saying "crash report" is not the reporter.
        assert!(
            d.observe(&entry("FieldKit", "uploading crash report"))
                .is_empty()
        );
        assert_eq!(
            report_stem_process("FieldKit-2026-09-09-101010"),
            Some("FieldKit".into())
        );
        assert_eq!(report_stem_process("log-power-2026"), None);
    }

    #[test]
    fn fixture_lines_when_present() {
        // Captured thermalmonitord lines (scrubbed). Every line must either
        // parse to a level or be recognised as chatter without panicking.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/ios/logs/thermalmonitord-lines.txt"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("fixture not captured yet: {path}");
            return;
        };
        let mut d = Derive::new();
        let mut parsed = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let msg = line.split_once(": ").map_or(line, |x| x.1);
            if parse_thermal_state(msg).is_some() {
                parsed += 1;
            }
            let _ = d.observe(&entry("thermalmonitord", msg));
        }
        eprintln!(
            "{parsed} of {} thermalmonitord lines carried a level",
            text.lines().count()
        );
    }
}
