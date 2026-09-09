//! Battery parsers against `fixtures/ios/**` (lockdown domain + IORegistry).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use statasmon_device_ios::battery::{BatteryBasic, BatteryDetail};
use statasmon_protocol::MetricId;

fn load(name: &str) -> plist::Dictionary {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/ios/ipad13-18-17.4.1")
        .join(name);
    plist::from_file::<_, plist::Value>(path)
        .unwrap()
        .into_dictionary()
        .unwrap()
}

#[test]
fn lockdown_battery_domain() {
    let b = BatteryBasic::parse(&load("lockdown_battery.plist"));
    let pct = b.percent.unwrap();
    assert!(pct <= 100);
    assert!(b.charging.is_some());
    assert!(b.external_connected.is_some());
    let m = b.metrics();
    assert!(
        m.iter()
            .any(|v| v.id == MetricId::BatPercent && v.value == pct as f64)
    );
    assert!(m.iter().any(|v| v.id == MetricId::BatCharging));
}

#[test]
fn ioregistry_power_source() {
    let d = load("ioregistry_battery.plist");
    let b = BatteryDetail::parse(&d);
    let v = b.voltage_v.unwrap();
    assert!((3.0..=5.0).contains(&v), "voltage {v} V");
    let t = b.temp_c.unwrap();
    assert!((0.0..=60.0).contains(&t), "temperature {t} °C");
    assert!(b.cycles.is_some());
    assert!(b.design_mah.unwrap() > 1000);
    assert!(b.full_mah.unwrap() > 1000);
    let health = b.health_pct().unwrap();
    assert!((30.0..=120.0).contains(&health), "health {health} %");
    let a = b.current_a.unwrap();
    match b.charging {
        Some(true) => assert!(a >= 0.0, "charging current must be positive, got {a}"),
        Some(false) if b.fully_charged != Some(true) => assert!(a <= 0.0),
        _ => {}
    }
    let ids: Vec<MetricId> = b.metrics().iter().map(|m| m.id).collect();
    for id in [
        MetricId::BatVoltage,
        MetricId::BatCurrent,
        MetricId::BatPower,
        MetricId::BatTemp,
        MetricId::TempBattery,
        MetricId::BatCycles,
        MetricId::BatDesignCapacity,
        MetricId::BatFullCapacity,
        MetricId::BatHealth,
    ] {
        assert!(ids.contains(&id), "{id:?} missing");
    }
}
