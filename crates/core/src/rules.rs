//! Headline rules v1 (docs/PLAN.md §3.11): ordered, first match wins, every
//! rule names the app or process when it can. The result is one plain
//! sentence, a cause, a severity and status pills.

use serde::{Deserialize, Serialize};
use statasmon_protocol::{
    Capability, CapabilitySet, EventKind, Headline, MetricId, MetricValue, Pill, Platform,
    Severity, Tone,
};

/// An event inside the rules window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEvent {
    /// When it happened.
    pub ts_ms: i64,
    /// What happened.
    pub kind: EventKind,
    /// Provider detail text.
    pub detail: String,
    /// Process it concerns, when known.
    pub process: Option<String>,
}

/// Everything the rules look at.
#[derive(Debug, Clone)]
pub struct RulesInput<'a> {
    /// Marketing name or user name of the device, used in sentences.
    pub device_name: &'a str,
    /// Platform the device imitates or is.
    pub platform: Platform,
    /// Latest value per metric.
    pub latest: &'a [MetricValue],
    /// Events inside the window (the engine prunes older ones).
    pub events: &'a [RecentEvent],
    /// Capability set (for "Developer Mode off" and "not reported").
    pub capabilities: &'a CapabilitySet,
    /// Now, for "n s ago".
    pub now_ms: i64,
    /// TCP retransmits in the last minute, when the counter is available.
    pub retransmits_per_min: Option<f64>,
    /// Dropped packets in the last minute, when the counter is available.
    pub dropped_per_min: Option<f64>,
}

/// Battery temperature above which the battery counts as hot.
pub const BATTERY_HOT_C: f64 = 40.0;
/// Retransmits per minute above which the network counts as struggling.
pub const RETRANSMITS_PER_MIN: f64 = 50.0;
/// Dropped packets per minute above which the network counts as struggling.
pub const DROPPED_PER_MIN: f64 = 20.0;

fn value(latest: &[MetricValue], id: MetricId) -> Option<f64> {
    latest.iter().find(|m| m.id == id).map(|m| m.value)
}

fn thermal_name(level: f64) -> &'static str {
    match level as i64 {
        0 => "Nominal",
        1 => "Fair",
        2 => "Serious",
        _ => "Critical",
    }
}

fn pressure_name(level: f64) -> &'static str {
    match level as i64 {
        0 => "normal",
        1 => "warning",
        2 => "urgent",
        _ => "critical",
    }
}

fn ago(now_ms: i64, ts_ms: i64) -> String {
    let secs = ((now_ms - ts_ms).max(0) + 500) / 1000;
    if secs == 0 {
        "just now".into()
    } else {
        format!("{secs} s ago")
    }
}

fn developer_mode_reason<'a>(input: &RulesInput<'a>) -> Option<&'a str> {
    if input.platform == Platform::Android {
        return None;
    }
    [
        Capability::CpuTotal,
        Capability::CpuPerCore,
        Capability::Processes,
    ]
    .iter()
    .find_map(|c| input.capabilities.reason(*c))
    .filter(|r| r.contains("Developer Mode"))
}

/// Evaluate the rules for one device.
pub fn evaluate(input: &RulesInput<'_>) -> Headline {
    let name = input.device_name;
    let latest = input.latest;
    let thermal = value(latest, MetricId::ThermalState);
    let pressure = value(latest, MetricId::MemPressure);
    let bat_temp =
        value(latest, MetricId::BatTemp).or_else(|| value(latest, MetricId::TempBattery));
    let newest = |kinds: &[EventKind]| -> Option<&RecentEvent> {
        input
            .events
            .iter()
            .filter(|e| kinds.contains(&e.kind))
            .max_by_key(|e| e.ts_ms)
    };
    let crash = newest(&[EventKind::Crash, EventKind::Anr]).cloned();
    let mem_event = newest(&[
        EventKind::LmkKill,
        EventKind::Jetsam,
        EventKind::MemoryWarning,
    ])
    .cloned();
    let pills = pills(input, thermal, pressure, bat_temp, crash.as_ref());

    let (sentence, cause, severity, rule) = if thermal.is_some_and(|t| t >= 3.0) {
        (
            format!("{name} is critically hot"),
            "Thermal state is Critical: the system is throttling hard and may shut down. Stop the workload and let the device cool.".to_owned(),
            Severity::Critical,
            "thermal_critical",
        )
    } else if thermal.is_some_and(|t| t >= 2.0) {
        (
            format!("{name} is running hot"),
            "Thermal state is Serious; CPU and GPU are being throttled.".to_owned(),
            Severity::Warning,
            "thermal_serious",
        )
    } else if let Some(e) = crash {
        let who = e.process.clone().unwrap_or_else(|| "An app".to_owned());
        let verb = if e.kind == EventKind::Anr {
            "stopped responding"
        } else {
            "crashed"
        };
        let rule = if e.kind == EventKind::Anr {
            "app_anr"
        } else {
            "app_crashed"
        };
        (
            format!("{who} {verb} {}", ago(input.now_ms, e.ts_ms)),
            e.detail.clone(),
            Severity::Warning,
            rule,
        )
    } else if let Some(e) = mem_event {
        let who = e
            .process
            .as_deref()
            .map(display_name)
            .unwrap_or_else(|| "an app".to_owned());
        let sentence = match e.kind {
            EventKind::MemoryWarning => {
                format!("{who} got a memory warning {}", ago(input.now_ms, e.ts_ms))
            }
            _ => format!("{name} killed {who} to free memory"),
        };
        let cause = match pressure {
            Some(p) => format!(
                "{}; memory pressure is {}.",
                e.detail.trim_end_matches('.'),
                pressure_name(p)
            ),
            None => e.detail.clone(),
        };
        let rule = match e.kind {
            EventKind::MemoryWarning => "memory_warning",
            _ => "memory_kill",
        };
        (sentence, cause, Severity::Warning, rule)
    } else if pressure.is_some_and(|p| p >= 2.0) {
        (
            format!("{name} is low on memory"),
            format!(
                "Memory pressure is {}; the system may terminate apps.",
                pressure_name(pressure.unwrap_or(2.0))
            ),
            Severity::Warning,
            "memory_pressure",
        )
    } else if thermal.is_some_and(|t| t >= 1.0) {
        (
            format!("{name} is running a little warm"),
            "Thermal state is Fair; light throttling is possible.".to_owned(),
            Severity::Notice,
            "thermal_fair",
        )
    } else if let Some(t) = bat_temp.filter(|t| *t > BATTERY_HOT_C) {
        (
            format!("{name}'s battery is hot"),
            format!("Battery temperature is {t:.1} °C, above {BATTERY_HOT_C:.0} °C."),
            Severity::Notice,
            "battery_hot",
        )
    } else if input
        .retransmits_per_min
        .is_some_and(|r| r > RETRANSMITS_PER_MIN)
        || input.dropped_per_min.is_some_and(|d| d > DROPPED_PER_MIN)
    {
        (
            format!("{name}'s network connection is struggling"),
            format!(
                "{:.0} retransmits and {:.0} dropped packets in the last minute.",
                input.retransmits_per_min.unwrap_or(0.0),
                input.dropped_per_min.unwrap_or(0.0)
            ),
            Severity::Notice,
            "network_struggling",
        )
    } else if let Some(reason) = developer_mode_reason(input) {
        (
            format!("{name} is in limited mode"),
            format!("{reason}: CPU, memory and processes are not available."),
            Severity::Notice,
            "developer_mode_off",
        )
    } else {
        (
            format!("Everything looks normal on {name}"),
            normal_cause(input, thermal, pressure),
            Severity::Nominal,
            "normal",
        )
    };
    Headline {
        sentence,
        cause,
        severity,
        pills,
        rule: rule.to_owned(),
    }
}

fn normal_cause(input: &RulesInput<'_>, thermal: Option<f64>, pressure: Option<f64>) -> String {
    let mut parts = Vec::new();
    match thermal {
        Some(t) => parts.push(format!("thermal {}", thermal_name(t).to_lowercase())),
        None if input.capabilities.is_available(Capability::ThermalState) => {
            parts.push("thermal not reported yet".to_owned());
        }
        None => {}
    }
    if let Some(p) = pressure {
        parts.push(format!("memory pressure {}", pressure_name(p)));
    }
    if let Some(pct) = value(input.latest, MetricId::BatPercent) {
        let charging = value(input.latest, MetricId::BatCharging).is_some_and(|c| c >= 1.0);
        parts.push(format!(
            "battery {pct:.0} %{}",
            if charging { " charging" } else { "" }
        ));
    }
    if parts.is_empty() {
        "Waiting for the first samples.".to_owned()
    } else {
        let mut s = parts.join(", ");
        if let Some(first) = s.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        s + "."
    }
}

fn pills(
    input: &RulesInput<'_>,
    thermal: Option<f64>,
    pressure: Option<f64>,
    bat_temp: Option<f64>,
    crash: Option<&RecentEvent>,
) -> Vec<Pill> {
    let mut out = Vec::new();
    let pill = |label: String, tone| Pill { label, tone };
    if let Some(e) = crash {
        let who = e.process.as_deref().unwrap_or("App");
        let what = if e.kind == EventKind::Anr {
            "not responding"
        } else {
            "crashed"
        };
        out.push(pill(format!("{who} {what}"), Tone::Bad));
    }
    match thermal {
        Some(t) => out.push(pill(
            format!("Thermal {}", thermal_name(t)),
            match t as i64 {
                0 => Tone::Ok,
                1 => Tone::Warn,
                _ => Tone::Bad,
            },
        )),
        None if input.capabilities.is_available(Capability::ThermalState) => {
            out.push(pill("Thermal not reported yet".into(), Tone::Info));
        }
        None => {}
    }
    if let Some(p) = pressure {
        out.push(pill(
            format!("Memory {}", pressure_name(p)),
            match p as i64 {
                0 => Tone::Ok,
                1 => Tone::Warn,
                _ => Tone::Bad,
            },
        ));
    }
    if let Some(pct) = value(input.latest, MetricId::BatPercent) {
        let charging = value(input.latest, MetricId::BatCharging).is_some_and(|c| c >= 1.0);
        let hot = bat_temp.is_some_and(|t| t > BATTERY_HOT_C);
        let label = match (hot, charging) {
            (true, _) => format!("Battery {pct:.0} % hot"),
            (false, true) => format!("Battery {pct:.0} % charging"),
            (false, false) => format!("Battery {pct:.0} %"),
        };
        out.push(pill(label, if hot { Tone::Bad } else { Tone::Ok }));
    }
    if let Some(r) = input.retransmits_per_min {
        let bad =
            r > RETRANSMITS_PER_MIN || input.dropped_per_min.is_some_and(|d| d > DROPPED_PER_MIN);
        out.push(pill(
            if bad {
                format!("Network {r:.0} retransmits/min")
            } else {
                "Network OK".into()
            },
            if bad { Tone::Warn } else { Tone::Ok },
        ));
    }
    if developer_mode_reason(input).is_some() {
        out.push(pill("Tier 1 · Developer Mode off".into(), Tone::Info));
    }
    out
}

/// A short, human name for a process token as iOS logs print it:
/// `[osservice<com.apple.AppSSODaemon>:268]` → `AppSSODaemon`,
/// `xpcservice<com.apple.MTLCompilerService([osservice<…>:412])>:5123` →
/// `MTLCompilerService`, `com.example.fieldkit` → `fieldkit`, plain names
/// unchanged. Never longer than 40 characters.
pub fn display_name(raw: &str) -> String {
    let mut s = raw.trim();
    // iOS log tokens: `[osservice<com.apple.AppSSODaemon>:268]`,
    // `xpcservice<com.apple.MTLCompilerService([osservice<…>:412])>:5123`.
    if let Some(i) = s.find('<') {
        s = &s[i + 1..];
    }
    let end = s.find(['>', '(', ']']).unwrap_or(s.len());
    s = s[..end].trim();
    if s.is_empty() {
        return "an app".to_owned();
    }
    if s.chars().count() > 40 {
        let cut: String = s.chars().take(39).collect();
        format!("{cut}…")
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use statasmon_protocol::Availability;

    #[test]
    fn display_name_shortens_ios_process_tokens() {
        assert_eq!(
            super::display_name("[osservice<com.apple.AppSSODaemon>:268]"),
            "com.apple.AppSSODaemon"
        );
        assert_eq!(
            super::display_name(
                "xpcservice<com.apple.MTLCompilerService([osservice<com.apple.x>:412])>:5123"
            ),
            "com.apple.MTLCompilerService"
        );
        assert_eq!(
            super::display_name("com.example.fieldkit"),
            "com.example.fieldkit"
        );
        assert_eq!(super::display_name("SpringBoard"), "SpringBoard");
        assert_eq!(super::display_name(""), "an app");
    }

    fn mv(id: MetricId, value: f64) -> MetricValue {
        MetricValue { id, value }
    }

    fn base() -> Vec<MetricValue> {
        vec![
            mv(MetricId::ThermalState, 0.0),
            mv(MetricId::MemPressure, 0.0),
            mv(MetricId::BatPercent, 87.0),
            mv(MetricId::BatCharging, 1.0),
            mv(MetricId::BatTemp, 33.8),
        ]
    }

    fn ev(kind: EventKind, process: &str, ts_ms: i64) -> RecentEvent {
        RecentEvent {
            ts_ms,
            kind,
            detail: format!("{process} {kind:?}"),
            process: Some(process.into()),
        }
    }

    fn run(latest: &[MetricValue], events: &[RecentEvent], caps: &CapabilitySet) -> Headline {
        evaluate(&RulesInput {
            device_name: "iPad (10th generation)",
            platform: Platform::Ios,
            latest,
            events,
            capabilities: caps,
            now_ms: 100_000,
            retransmits_per_min: Some(2.0),
            dropped_per_min: Some(0.0),
        })
    }

    fn with(latest: &mut Vec<MetricValue>, id: MetricId, v: f64) {
        latest.retain(|m| m.id != id);
        latest.push(mv(id, v));
    }

    #[test]
    fn normal_headline_names_the_device() {
        let h = run(&base(), &[], &CapabilitySet::all_available());
        assert_eq!(
            h.sentence,
            "Everything looks normal on iPad (10th generation)"
        );
        assert_eq!(h.severity, Severity::Nominal);
        assert_eq!(h.rule, "normal");
        assert_eq!(
            h.cause,
            "Thermal nominal, memory pressure normal, battery 87 % charging."
        );
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Thermal Nominal" && p.tone == Tone::Ok)
        );
        assert!(h.pills.iter().any(|p| p.label == "Battery 87 % charging"));
        assert!(h.pills.iter().any(|p| p.label == "Network OK"));
    }

    #[test]
    fn critical_thermal() {
        let mut l = base();
        with(&mut l, MetricId::ThermalState, 3.0);
        let h = run(&l, &[], &CapabilitySet::all_available());
        assert_eq!(h.severity, Severity::Critical);
        assert_eq!(h.rule, "thermal_critical");
        assert_eq!(h.sentence, "iPad (10th generation) is critically hot");
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Thermal Critical" && p.tone == Tone::Bad)
        );
    }

    #[test]
    fn serious_thermal() {
        let mut l = base();
        with(&mut l, MetricId::ThermalState, 2.0);
        let h = run(&l, &[], &CapabilitySet::all_available());
        assert_eq!(h.severity, Severity::Warning);
        assert_eq!(h.sentence, "iPad (10th generation) is running hot");
    }

    #[test]
    fn crash_and_anr_name_the_process_and_age() {
        let h = run(
            &base(),
            &[ev(EventKind::Crash, "FieldKit", 88_000)],
            &CapabilitySet::all_available(),
        );
        assert_eq!(h.sentence, "FieldKit crashed 12 s ago");
        assert_eq!(h.severity, Severity::Warning);
        assert_eq!(h.rule, "app_crashed");
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "FieldKit crashed" && p.tone == Tone::Bad)
        );
        let h = run(
            &base(),
            &[ev(EventKind::Anr, "com.example.fieldkit", 99_800)],
            &CapabilitySet::all_available(),
        );
        assert_eq!(
            h.sentence,
            "com.example.fieldkit stopped responding just now"
        );
    }

    #[test]
    fn memory_events_and_pressure() {
        let h = run(
            &base(),
            &[ev(EventKind::LmkKill, "com.example.fieldkit", 90_000)],
            &CapabilitySet::all_available(),
        );
        assert_eq!(
            h.sentence,
            "iPad (10th generation) killed com.example.fieldkit to free memory"
        );
        assert!(h.cause.ends_with("memory pressure is normal."));
        let mut l = base();
        with(&mut l, MetricId::MemPressure, 1.0);
        let h = run(
            &l,
            &[ev(EventKind::MemoryWarning, "FieldKit", 95_000)],
            &CapabilitySet::all_available(),
        );
        assert_eq!(h.sentence, "FieldKit got a memory warning 5 s ago");
        assert_eq!(h.severity, Severity::Warning);
        let mut l = base();
        with(&mut l, MetricId::MemPressure, 2.0);
        let h = run(&l, &[], &CapabilitySet::all_available());
        assert_eq!(h.sentence, "iPad (10th generation) is low on memory");
        assert!(h.cause.contains("urgent"));
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Memory urgent" && p.tone == Tone::Bad)
        );
    }

    #[test]
    fn fair_thermal_is_a_notice() {
        let mut l = base();
        with(&mut l, MetricId::ThermalState, 1.0);
        let h = run(&l, &[], &CapabilitySet::all_available());
        assert_eq!(
            h.sentence,
            "iPad (10th generation) is running a little warm"
        );
        assert_eq!(h.severity, Severity::Notice);
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Thermal Fair" && p.tone == Tone::Warn)
        );
    }

    #[test]
    fn battery_hot() {
        let mut l = base();
        with(&mut l, MetricId::BatTemp, 41.3);
        let h = run(&l, &[], &CapabilitySet::all_available());
        assert_eq!(h.sentence, "iPad (10th generation)'s battery is hot");
        assert_eq!(h.cause, "Battery temperature is 41.3 °C, above 40 °C.");
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Battery 87 % hot" && p.tone == Tone::Bad)
        );
    }

    #[test]
    fn network_retransmits_and_dropped() {
        let latest = base();
        let caps = CapabilitySet::all_available();
        let h = evaluate(&RulesInput {
            device_name: "Galaxy Tab S8",
            platform: Platform::Android,
            latest: &latest,
            events: &[],
            capabilities: &caps,
            now_ms: 0,
            retransmits_per_min: Some(120.0),
            dropped_per_min: Some(3.0),
        });
        assert_eq!(
            h.sentence,
            "Galaxy Tab S8's network connection is struggling"
        );
        assert_eq!(
            h.cause,
            "120 retransmits and 3 dropped packets in the last minute."
        );
        assert_eq!(h.severity, Severity::Notice);
        let h = evaluate(&RulesInput {
            device_name: "Galaxy Tab S8",
            platform: Platform::Android,
            latest: &latest,
            events: &[],
            capabilities: &caps,
            now_ms: 0,
            retransmits_per_min: Some(1.0),
            dropped_per_min: Some(40.0),
        });
        assert_eq!(h.severity, Severity::Notice);
        assert!(h.sentence.contains("network"));
    }

    #[test]
    fn developer_mode_off_on_ios_only() {
        let mut caps = CapabilitySet::all_available();
        caps.set(
            Capability::CpuTotal,
            Availability::Unavailable {
                reason: "Developer Mode is off on this iPad".into(),
            },
        );
        let latest = vec![
            mv(MetricId::BatPercent, 50.0),
            mv(MetricId::BatCharging, 0.0),
        ];
        let h = run(&latest, &[], &caps);
        assert_eq!(h.sentence, "iPad (10th generation) is in limited mode");
        assert_eq!(
            h.cause,
            "Developer Mode is off on this iPad: CPU, memory and processes are not available."
        );
        assert!(
            h.pills
                .iter()
                .any(|p| p.tone == Tone::Info && p.label.contains("Tier 1"))
        );
        assert!(
            h.pills
                .iter()
                .any(|p| p.label == "Thermal not reported yet")
        );
        let h = evaluate(&RulesInput {
            device_name: "Tab",
            platform: Platform::Android,
            latest: &latest,
            events: &[],
            capabilities: &caps,
            now_ms: 0,
            retransmits_per_min: None,
            dropped_per_min: None,
        });
        assert_eq!(h.severity, Severity::Nominal);
    }

    #[test]
    fn precedence_first_match_wins() {
        let caps = CapabilitySet::all_available();
        let crash = [ev(EventKind::Crash, "FieldKit", 99_000)];
        let mut l = base();
        with(&mut l, MetricId::ThermalState, 3.0);
        with(&mut l, MetricId::MemPressure, 3.0);
        with(&mut l, MetricId::BatTemp, 45.0);
        assert_eq!(run(&l, &crash, &caps).severity, Severity::Critical);
        with(&mut l, MetricId::ThermalState, 2.0);
        assert_eq!(
            run(&l, &crash, &caps).sentence,
            "iPad (10th generation) is running hot"
        );
        with(&mut l, MetricId::ThermalState, 1.0);
        assert_eq!(run(&l, &crash, &caps).sentence, "FieldKit crashed 1 s ago");
        assert_eq!(
            run(&l, &[], &caps).sentence,
            "iPad (10th generation) is low on memory"
        );
        with(&mut l, MetricId::MemPressure, 0.0);
        assert_eq!(
            run(&l, &[], &caps).sentence,
            "iPad (10th generation) is running a little warm"
        );
        with(&mut l, MetricId::ThermalState, 0.0);
        assert_eq!(
            run(&l, &[], &caps).sentence,
            "iPad (10th generation)'s battery is hot"
        );
        with(&mut l, MetricId::BatTemp, 30.0);
        assert_eq!(run(&l, &[], &caps).severity, Severity::Nominal);
    }

    #[test]
    fn no_samples_yet() {
        let caps = CapabilitySet::none("connecting");
        let h = evaluate(&RulesInput {
            device_name: "iPad (10th generation)",
            platform: Platform::Ios,
            latest: &[],
            events: &[],
            capabilities: &caps,
            now_ms: 0,
            retransmits_per_min: None,
            dropped_per_min: None,
        });
        assert_eq!(h.severity, Severity::Nominal);
        assert_eq!(h.rule, "normal");
        assert_eq!(h.cause, "Waiting for the first samples.");
        assert!(h.pills.is_empty());
    }
}
