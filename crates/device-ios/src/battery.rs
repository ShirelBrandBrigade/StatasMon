//! Battery at the medium cadence (step 3.3): lockdown
//! `com.apple.mobile.battery` for percent + charging, and the
//! `IOPMPowerSource` IORegistry entry via `diagnostics_relay` for voltage,
//! amperage, temperature, cycles and capacities. Both parsers are pure and
//! tested against `fixtures/ios/**`.
//!
//! Units on the wire: `Voltage` mV, `InstantAmperage` mA (signed, positive
//! while charging; some builds send the two's-complement unsigned value),
//! `Temperature` in hundredths of °C, capacities in mAh.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use idevice::IdeviceService;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::services::diagnostics_relay::DiagnosticsRelayClient;
use statasmon_device::{EventSink, now_ms};
use statasmon_protocol::{MetricId, MetricValue};
use tokio_util::sync::CancellationToken;

use crate::error::IosError;
use crate::lockdown::{Lockdown, bool as pbool, int, string, uint};

/// IORegistry class queried for the battery.
pub const IOREG_CLASS: &str = "IOPMPowerSource";

/// The lockdown battery domain, parsed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryBasic {
    pub percent: Option<u64>,
    pub charging: Option<bool>,
    pub external_connected: Option<bool>,
    pub fully_charged: Option<bool>,
}

impl BatteryBasic {
    /// Parse `com.apple.mobile.battery`.
    pub fn parse(d: &plist::Dictionary) -> Self {
        Self {
            percent: uint(d, "BatteryCurrentCapacity").map(|p| p.min(100)),
            charging: pbool(d, "BatteryIsCharging"),
            external_connected: pbool(d, "ExternalConnected"),
            fully_charged: pbool(d, "FullyCharged"),
        }
    }

    /// `bat.percent` and `bat.charging`.
    pub fn metrics(&self) -> Vec<MetricValue> {
        let mut out = Vec::with_capacity(2);
        if let Some(p) = self.percent {
            out.push(MetricValue {
                id: MetricId::BatPercent,
                value: p as f64,
            });
        }
        if let Some(c) = self.charging {
            out.push(MetricValue {
                id: MetricId::BatCharging,
                value: if c { 1.0 } else { 0.0 },
            });
        }
        out
    }
}

/// The `IOPMPowerSource` entry, parsed and converted to SI units.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BatteryDetail {
    pub voltage_v: Option<f64>,
    /// Signed amps, positive while charging.
    pub current_a: Option<f64>,
    pub temp_c: Option<f64>,
    pub cycles: Option<u64>,
    pub design_mah: Option<u64>,
    pub full_mah: Option<u64>,
    /// Raw mAh in the cell right now (`AppleRawCurrentCapacity`).
    pub current_mah: Option<u64>,
    pub charging: Option<bool>,
    pub external_connected: Option<bool>,
    pub fully_charged: Option<bool>,
    /// Minutes, as the device estimates it (65535 = unknown).
    pub time_remaining_min: Option<u64>,
    pub serial: Option<String>,
}

impl BatteryDetail {
    /// Parse the IORegistry dictionary.
    pub fn parse(d: &plist::Dictionary) -> Self {
        let charging = pbool(d, "IsCharging");
        let external = pbool(d, "ExternalConnected");
        let fully = pbool(d, "FullyCharged");
        let current_a = int(d, "InstantAmperage").map(|ma| {
            let mut a = ma as f64 / 1000.0;
            // Positive while charging; flip when the device disagrees.
            match (charging, fully) {
                (Some(true), _) if a < 0.0 => a = -a,
                (Some(false), Some(false)) if a > 0.0 => a = -a,
                _ => {}
            }
            a
        });
        let design_mah = uint(d, "DesignCapacity").filter(|v| *v > 0);
        let full_mah = uint(d, "AppleRawMaxCapacity")
            .or_else(|| uint(d, "NominalChargeCapacity"))
            .or_else(|| uint(d, "MaxCapacity").filter(|v| *v > 100))
            .filter(|v| *v > 0);
        Self {
            voltage_v: uint(d, "Voltage").map(|mv| mv as f64 / 1000.0),
            current_a,
            temp_c: int(d, "Temperature").map(|t| t as f64 / 100.0),
            cycles: uint(d, "CycleCount"),
            design_mah,
            full_mah,
            current_mah: uint(d, "AppleRawCurrentCapacity")
                .or_else(|| uint(d, "CurrentCapacity").filter(|v| *v > 100)),
            charging,
            external_connected: external,
            fully_charged: fully,
            time_remaining_min: uint(d, "TimeRemaining").filter(|v| *v != 65535),
            serial: string(d, "Serial"),
        }
    }

    /// Full-charge capacity as % of design capacity.
    pub fn health_pct(&self) -> Option<f64> {
        let full = self.full_mah? as f64;
        let design = self.design_mah? as f64;
        (design > 0.0).then(|| (full / design * 100.0).clamp(0.0, 150.0))
    }

    /// V × A, signed like the current.
    pub fn power_w(&self) -> Option<f64> {
        Some(self.voltage_v? * self.current_a?)
    }

    /// Detail metrics: voltage, current, power, temperature (both
    /// `bat.temp` and `temp.battery`), cycles, capacities, health.
    pub fn metrics(&self) -> Vec<MetricValue> {
        let mut out = Vec::with_capacity(10);
        let mut push = |id, v: Option<f64>| {
            if let Some(value) = v {
                out.push(MetricValue { id, value });
            }
        };
        push(MetricId::BatVoltage, self.voltage_v);
        push(MetricId::BatCurrent, self.current_a);
        push(MetricId::BatPower, self.power_w());
        push(MetricId::BatTemp, self.temp_c);
        push(MetricId::TempBattery, self.temp_c);
        push(MetricId::BatCycles, self.cycles.map(|c| c as f64));
        push(
            MetricId::BatDesignCapacity,
            self.design_mah.map(|c| c as f64),
        );
        push(MetricId::BatFullCapacity, self.full_mah.map(|c| c as f64));
        push(MetricId::BatHealth, self.health_pct());
        out
    }
}

/// Query the `IOPMPowerSource` entry.
pub async fn fetch_ioregistry(
    diag: &mut DiagnosticsRelayClient,
) -> Result<plist::Dictionary, IosError> {
    match diag.ioregistry(None, None, Some(IOREG_CLASS)).await {
        Ok(Some(d)) => Ok(d),
        Ok(None) => Err(IosError::Parse(format!(
            "IORegistry answered without a {IOREG_CLASS} entry"
        ))),
        Err(e) => Err(IosError::service("diagnostics_relay/IORegistry", e)),
    }
}

/// True when the error means the device is gone rather than the query
/// being refused.
pub fn is_device_gone(e: &IosError) -> bool {
    match e {
        IosError::DeviceGone(_) => true,
        IosError::Service { source, .. } => matches!(
            source,
            idevice::IdeviceError::Socket(_)
                | idevice::IdeviceError::DeviceNotFound
                | idevice::IdeviceError::NoEstablishedConnection
                | idevice::IdeviceError::Usbmuxd(_)
        ),
        IosError::Io(_) => true,
        _ => false,
    }
}

/// Sample loop: every `period` read the lockdown domain (always) and the
/// IORegistry entry (while `detail` is true; a refused query turns it off
/// after logging once). Ends on cancel, on a closed sink, or when the
/// device is gone (`gone` is set so the caller reports it once).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    provider: std::sync::Arc<dyn IdeviceProvider>,
    pair: PairingFile,
    sink: EventSink,
    device_id: String,
    period: Duration,
    detail: bool,
    cancel: CancellationToken,
    gone: std::sync::Arc<AtomicBool>,
) {
    let mut interval = tokio::time::interval(period.max(Duration::from_millis(500)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut detail = detail;
    let mut failures = 0u32;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {}
        }
        if sink.is_closed() {
            return;
        }
        match sample(&*provider, &pair, detail).await {
            Ok((values, detail_ok)) => {
                failures = 0;
                if detail && !detail_ok {
                    tracing::info!(device = %device_id, "battery: IORegistry refused; basic fields only from now on");
                    detail = false;
                }
                if !values.is_empty() && sink.samples(&device_id, now_ms(), values).await.is_err() {
                    return;
                }
            }
            Err(e) if is_device_gone(&e) => {
                gone.store(true, Ordering::SeqCst);
                cancel.cancel();
                return;
            }
            Err(e) => {
                failures += 1;
                if failures <= 3 {
                    tracing::warn!(device = %device_id, error = %e, "battery tick failed");
                }
            }
        }
    }
}

/// One tick: `(metrics, detail_ok)`. `detail_ok` is false when the
/// IORegistry query was refused (the lockdown part still succeeded).
pub async fn sample(
    provider: &dyn IdeviceProvider,
    pair: &PairingFile,
    detail: bool,
) -> Result<(Vec<MetricValue>, bool), IosError> {
    let mut ld = Lockdown::connect(provider, pair).await?;
    let basic = BatteryBasic::parse(&ld.battery_domain().await?);
    drop(ld);
    let mut values = basic.metrics();
    if !detail {
        return Ok((values, false));
    }
    let mut diag = DiagnosticsRelayClient::connect(provider)
        .await
        .map_err(|e| IosError::service("diagnostics_relay", e))?;
    let detail_ok = match fetch_ioregistry(&mut diag).await {
        Ok(d) => {
            values.extend(BatteryDetail::parse(&d).metrics());
            true
        }
        Err(e) if is_device_gone(&e) => return Err(e),
        Err(e) => {
            tracing::debug!(error = %e, "IORegistry battery query refused");
            false
        }
    };
    let _ = diag.goodbye().await;
    Ok((values, detail_ok))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ioreg() -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        d.insert("Voltage".into(), plist::Value::Integer(3912.into()));
        d.insert(
            "InstantAmperage".into(),
            plist::Value::Integer(18_446_744_073_709_551_016u64.into()),
        );
        d.insert("Temperature".into(), plist::Value::Integer(2963.into()));
        d.insert("CycleCount".into(), plist::Value::Integer(312.into()));
        d.insert("DesignCapacity".into(), plist::Value::Integer(7000.into()));
        d.insert(
            "AppleRawMaxCapacity".into(),
            plist::Value::Integer(6300.into()),
        );
        d.insert("IsCharging".into(), plist::Value::Boolean(false));
        d.insert("ExternalConnected".into(), plist::Value::Boolean(false));
        d.insert("FullyCharged".into(), plist::Value::Boolean(false));
        d.insert("TimeRemaining".into(), plist::Value::Integer(65535.into()));
        d
    }

    #[test]
    fn detail_converts_units_and_signs() {
        let b = BatteryDetail::parse(&ioreg());
        assert_eq!(b.voltage_v, Some(3.912));
        assert_eq!(b.current_a, Some(-0.6));
        assert_eq!(b.temp_c, Some(29.63));
        assert_eq!(b.cycles, Some(312));
        assert_eq!(b.design_mah, Some(7000));
        assert_eq!(b.full_mah, Some(6300));
        assert_eq!(b.health_pct(), Some(90.0));
        assert!((b.power_w().unwrap() - (-2.3472)).abs() < 1e-9);
        assert_eq!(b.time_remaining_min, None);
        let ids: Vec<MetricId> = b.metrics().iter().map(|m| m.id).collect();
        assert!(ids.contains(&MetricId::BatTemp) && ids.contains(&MetricId::TempBattery));
        assert!(ids.contains(&MetricId::BatHealth));
    }

    #[test]
    fn current_sign_follows_the_charging_flag() {
        let mut d = ioreg();
        d.insert("IsCharging".into(), plist::Value::Boolean(true));
        d.insert(
            "InstantAmperage".into(),
            plist::Value::Integer((-900i64).into()),
        );
        assert_eq!(BatteryDetail::parse(&d).current_a, Some(0.9));
        d.insert("IsCharging".into(), plist::Value::Boolean(false));
        d.insert("InstantAmperage".into(), plist::Value::Integer(900.into()));
        assert_eq!(BatteryDetail::parse(&d).current_a, Some(-0.9));
    }

    #[test]
    fn basic_domain() {
        let mut d = plist::Dictionary::new();
        d.insert(
            "BatteryCurrentCapacity".into(),
            plist::Value::Integer(87.into()),
        );
        d.insert("BatteryIsCharging".into(), plist::Value::Boolean(true));
        d.insert("ExternalConnected".into(), plist::Value::Boolean(true));
        let b = BatteryBasic::parse(&d);
        assert_eq!(b.percent, Some(87));
        assert_eq!(b.charging, Some(true));
        let m = b.metrics();
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].value, 1.0);
    }

    #[test]
    fn missing_fields_are_not_emitted() {
        let d = plist::Dictionary::new();
        assert!(BatteryDetail::parse(&d).metrics().is_empty());
        assert!(BatteryBasic::parse(&d).metrics().is_empty());
        let mut only_max = plist::Dictionary::new();
        only_max.insert("MaxCapacity".into(), plist::Value::Integer(100.into()));
        assert_eq!(BatteryDetail::parse(&only_max).full_mah, None);
    }
}
