//! `dumpsys battery` and `/sys/class/power_supply/battery/*`.

use std::collections::HashMap;

use super::{ParseError, parse_i64};

/// `BatteryManager` constants.
pub const STATUS_CHARGING: i64 = 2;
pub const STATUS_DISCHARGING: i64 = 3;
pub const STATUS_NOT_CHARGING: i64 = 4;
pub const STATUS_FULL: i64 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    pub level: i64,
    pub scale: i64,
    /// Volts (dumpsys prints millivolts).
    pub voltage_v: f64,
    /// °C (dumpsys prints tenths).
    pub temp_c: f64,
    pub status: i64,
    pub health: i64,
    pub ac: bool,
    pub usb: bool,
    pub wireless: bool,
    pub dock: bool,
    /// µAh when present.
    pub charge_counter_uah: Option<i64>,
    /// µA when present and non-zero.
    pub max_charging_current_ua: Option<i64>,
    pub technology: Option<String>,
}

impl Battery {
    pub fn percent(&self) -> f64 {
        if self.scale > 0 {
            self.level as f64 * 100.0 / self.scale as f64
        } else {
            self.level as f64
        }
    }
    pub fn powered(&self) -> bool {
        self.ac || self.usb || self.wireless || self.dock
    }
    /// `bat.charging`: charging or full with a power source attached.
    pub fn charging(&self) -> bool {
        (self.status == STATUS_CHARGING || self.status == STATUS_FULL) && self.powered()
    }
}

pub fn parse_dumpsys_battery(text: &str) -> Result<Battery, ParseError> {
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            // Keep the first occurrence: Samsung repeats some keys lower-cased
            // ("charge counter") with vendor semantics.
            kv.entry(k).or_insert(v.trim());
        }
    }
    let int = |k: &'static str| -> Result<i64, ParseError> {
        parse_i64(k, kv.get(k).ok_or(ParseError::Missing(k))?)
    };
    let flag = |k: &str| kv.get(k).is_some_and(|v| *v == "true");
    let opt_int = |k: &str| kv.get(k).and_then(|v| v.parse::<i64>().ok());
    Ok(Battery {
        level: int("level")?,
        scale: int("scale").unwrap_or(100),
        voltage_v: int("voltage")? as f64 / 1000.0,
        temp_c: int("temperature")? as f64 / 10.0,
        status: int("status")?,
        health: int("health").unwrap_or(0),
        ac: flag("AC powered"),
        usb: flag("USB powered"),
        wireless: flag("Wireless powered"),
        dock: flag("Dock powered"),
        charge_counter_uah: opt_int("Charge counter"),
        max_charging_current_ua: opt_int("Max charging current").filter(|v| *v != 0),
        technology: kv.get("technology").map(|s| (*s).to_owned()),
    })
}

/// What `/sys/class/power_supply/battery/` adds when it is readable.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PowerSupply {
    /// Amps, normalized to the protocol's sign: positive while charging.
    pub current_a: Option<f64>,
    pub cycle_count: Option<u32>,
    /// mAh.
    pub charge_full_mah: Option<f64>,
    pub charge_full_design_mah: Option<f64>,
}

/// Input: `name=value` lines (one per file), as produced by
/// `for f in …; do echo "$f=$(cat …/$f 2>/dev/null)"; done`. Vendors
/// disagree on the sign of `current_now`; `charging` (from dumpsys) fixes it.
pub fn parse_power_supply(text: &str, charging: bool) -> Result<PowerSupply, ParseError> {
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim();
            if !v.is_empty() && !v.contains("denied") && !v.contains("No such") {
                kv.insert(k.trim(), v);
            }
        }
    }
    if kv.is_empty() {
        return Err(ParseError::Empty("power_supply"));
    }
    let num = |k: &str| kv.get(k).and_then(|v| v.parse::<f64>().ok());
    let current_a = num("current_now").map(|ua| {
        let a = ua.abs() / 1_000_000.0;
        if charging { a } else { -a }
    });
    Ok(PowerSupply {
        current_a,
        cycle_count: num("cycle_count").map(|c| c as u32),
        charge_full_mah: num("charge_full").map(|uah| uah / 1000.0),
        charge_full_design_mah: num("charge_full_design").map(|uah| uah / 1000.0),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn dumpsys_battery_fixture() {
        let b = parse_dumpsys_battery(&fixture("dumpsys_battery")).unwrap();
        assert_eq!(b.level, 100);
        assert_eq!(b.percent(), 100.0);
        assert_eq!(b.voltage_v, 4.365);
        assert_eq!(b.temp_c, 32.8);
        assert_eq!(b.status, STATUS_FULL);
        assert_eq!(b.health, 2);
        assert!(b.usb && !b.ac && !b.wireless);
        assert!(b.charging());
        assert_eq!(b.charge_counter_uah, Some(7_200_000));
        assert_eq!(b.max_charging_current_ua, None);
        assert_eq!(b.technology.as_deref(), Some("Li-ion"));
    }

    #[test]
    fn power_supply_denied_is_empty() {
        assert!(parse_power_supply(&fixture("power_supply"), true).is_err());
        assert!(parse_power_supply("current_now=cat: Permission denied\n", true).is_err());
    }

    #[test]
    fn power_supply_sign_normalized() {
        let text = "current_now=-812000\ncycle_count=143\ncharge_full=6900000\ncharge_full_design=7040000\n";
        let p = parse_power_supply(text, false).unwrap();
        assert_eq!(p.current_a, Some(-0.812));
        let p = parse_power_supply(text, true).unwrap();
        assert_eq!(p.current_a, Some(0.812));
        assert_eq!(p.cycle_count, Some(143));
        assert_eq!(p.charge_full_mah, Some(6900.0));
        assert_eq!(p.charge_full_design_mah, Some(7040.0));
    }
}
