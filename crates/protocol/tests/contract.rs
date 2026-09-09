#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Wire-format tests: shapes stay stable, the catalog is complete, and the
//! TypeScript bindings are exported (run by `pnpm protocol:gen`).

use statasmon_protocol::*;

#[test]
fn metric_catalog_is_complete_and_unique() {
    let cat = MetricId::catalog();
    assert_eq!(cat.len(), MetricId::ALL.len());
    let mut wires: Vec<&str> = MetricId::ALL.iter().map(|m| m.as_str()).collect();
    wires.sort_unstable();
    wires.dedup();
    assert_eq!(wires.len(), MetricId::ALL.len(), "duplicate wire names");
}

#[test]
fn metric_wire_name_matches_serde() {
    for id in MetricId::ALL {
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{}\"", id.as_str()));
        let back: MetricId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}

#[test]
fn message_is_tagged_by_type() {
    let m = Message::Samples {
        device_id: "fake:1".into(),
        ts_ms: 1,
        values: vec![MetricValue {
            id: MetricId::CpuTotal,
            value: 38.0,
        }],
    };
    let v: serde_json::Value = serde_json::to_value(&m).unwrap();
    assert_eq!(v["type"], "samples");
    assert_eq!(v["values"][0]["id"], "cpu.total");
    let back: Message = serde_json::from_value(v).unwrap();
    assert_eq!(back, m);
}

#[test]
fn connection_state_and_availability_are_tagged() {
    let s = ConnectionState::Error {
        code: "tunnel_failed".into(),
        message: "x".into(),
    };
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["state"], "error");
    let a = Availability::Unavailable {
        reason: "Developer Mode is off".into(),
    };
    let v = serde_json::to_value(&a).unwrap();
    assert_eq!(v["status"], "unavailable");
}

#[test]
fn capability_set_helpers() {
    let mut set = CapabilitySet::none("connecting");
    assert_eq!(set.entries.len(), Capability::ALL.len());
    assert!(!set.is_available(Capability::CpuPerCore));
    assert_eq!(set.reason(Capability::CpuPerCore), Some("connecting"));
    set.set(Capability::CpuPerCore, Availability::Available);
    assert!(set.is_available(Capability::CpuPerCore));
    assert!(CapabilitySet::all_available().is_available(Capability::Logs));
}

#[test]
fn device_info_default_is_all_none() {
    let info = DeviceInfo::default();
    let v = serde_json::to_value(&info).unwrap();
    assert!(v["identity"]["marketing_name"].is_null());
    assert!(v["raw"].as_object().unwrap().is_empty());
}
