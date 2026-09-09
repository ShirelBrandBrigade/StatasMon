//! Text encodings of protocol enums as stored in SQLite (the serde wire
//! names, so a stored row reads the same as the JSON on the bus).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;
use serde::de::DeserializeOwned;
use statasmon_protocol::{CoreKind, EventKind, LogLevel, MetricId, Platform, ProcessState};

fn wire_name<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

fn parse_wire<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).ok()
}

/// "cpu.total"
pub fn metric_name(id: MetricId) -> &'static str {
    id.as_str()
}

/// Parse "cpu.total"; unknown names (a newer protocol) are `None`.
pub fn parse_metric(text: &str) -> Option<MetricId> {
    static TABLE: OnceLock<HashMap<&'static str, MetricId>> = OnceLock::new();
    TABLE
        .get_or_init(|| MetricId::ALL.iter().map(|m| (m.as_str(), *m)).collect())
        .get(text)
        .copied()
}

/// "error"
pub fn level_name(level: LogLevel) -> String {
    wire_name(&level)
}

/// Parse "error"; unknown → `Info`.
pub fn parse_level(text: &str) -> LogLevel {
    parse_wire(text).unwrap_or(LogLevel::Info)
}

/// "thermal_change"
pub fn event_kind_name(kind: EventKind) -> String {
    wire_name(&kind)
}

/// Parse "thermal_change"; unknown → `Connection`.
pub fn parse_event_kind(text: &str) -> EventKind {
    parse_wire(text).unwrap_or(EventKind::Connection)
}

/// "performance"
pub fn core_kind_name(kind: CoreKind) -> String {
    wire_name(&kind)
}

/// Parse "performance"; unknown → `Unknown`.
pub fn parse_core_kind(text: &str) -> CoreKind {
    parse_wire(text).unwrap_or(CoreKind::Unknown)
}

/// "foreground"
pub fn process_state_name(state: ProcessState) -> String {
    wire_name(&state)
}

/// Parse "foreground"; unknown → `Unknown`.
pub fn parse_process_state(text: &str) -> ProcessState {
    parse_wire(text).unwrap_or(ProcessState::Unknown)
}

/// "ios"
pub fn platform_name(platform: Platform) -> String {
    wire_name(&platform)
}

/// Parse "ios"; unknown → `Fake`.
pub fn parse_platform(text: &str) -> Platform {
    parse_wire(text).unwrap_or(Platform::Fake)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert_eq!(parse_metric("cpu.total"), Some(MetricId::CpuTotal));
        assert_eq!(parse_metric("nope"), None);
        assert_eq!(level_name(LogLevel::Fault), "fault");
        assert_eq!(parse_level("fault"), LogLevel::Fault);
        assert_eq!(event_kind_name(EventKind::LmkKill), "lmk_kill");
        assert_eq!(parse_event_kind("lmk_kill"), EventKind::LmkKill);
        assert_eq!(
            parse_core_kind(&core_kind_name(CoreKind::Efficiency)),
            CoreKind::Efficiency
        );
        assert_eq!(parse_process_state("background"), ProcessState::Background);
        assert_eq!(
            parse_platform(&platform_name(Platform::Android)),
            Platform::Android
        );
    }
}
