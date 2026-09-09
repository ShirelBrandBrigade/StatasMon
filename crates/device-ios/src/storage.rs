//! Storage at the slow cadence (step 3.4): lockdown `com.apple.disk_usage`
//! for the totals and installation_proxy `Browse` for per-app sizes.
//!
//! Mapping (docs/research/ios.md §2): `disk.total` = `TotalDiskCapacity`,
//! `disk.free` = `TotalDataAvailable`, `disk.used` = `TotalDataCapacity` −
//! `TotalDataAvailable`, `disk.system` = `TotalDiskCapacity` −
//! `TotalDataCapacity`, `disk.apps` = Σ(`StaticDiskUsage` +
//! `DynamicDiskUsage`). iOS exposes no media/other split to the host, so
//! `disk.media` and `disk.other` are never emitted.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use idevice::IdeviceService;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use idevice::services::installation_proxy::InstallationProxyClient;
use parking_lot::RwLock;
use statasmon_device::{EventSink, now_ms};
use statasmon_protocol::{MetricId, MetricValue};
use tokio_util::sync::CancellationToken;

use crate::battery::is_device_gone;
use crate::error::IosError;
use crate::lockdown::{Lockdown, string, uint};

/// The per-app browse runs on every Nth slow tick (first tick included).
pub const APPS_EVERY_N_TICKS: u32 = 5;
/// Attributes requested from installation_proxy.
pub const APP_ATTRIBUTES: [&str; 4] = [
    "CFBundleIdentifier",
    "CFBundleDisplayName",
    "StaticDiskUsage",
    "DynamicDiskUsage",
];

/// `com.apple.disk_usage`, parsed (bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskUsage {
    pub total: Option<u64>,
    pub data_capacity: Option<u64>,
    pub data_available: Option<u64>,
    pub reserved: Option<u64>,
    pub system_available: Option<u64>,
    pub system_capacity: Option<u64>,
}

impl DiskUsage {
    /// Parse the lockdown domain.
    pub fn parse(d: &plist::Dictionary) -> Self {
        Self {
            total: uint(d, "TotalDiskCapacity"),
            data_capacity: uint(d, "TotalDataCapacity"),
            data_available: uint(d, "TotalDataAvailable"),
            reserved: uint(d, "AmountDataReserved"),
            system_available: uint(d, "TotalSystemAvailable"),
            system_capacity: uint(d, "TotalSystemCapacity"),
        }
    }

    pub fn used(&self) -> Option<u64> {
        Some(self.data_capacity?.saturating_sub(self.data_available?))
    }

    pub fn system(&self) -> Option<u64> {
        Some(self.total?.saturating_sub(self.data_capacity?))
    }

    /// `disk.total`, `disk.used`, `disk.free`, `disk.system`.
    pub fn metrics(&self) -> Vec<MetricValue> {
        let mut out = Vec::with_capacity(4);
        let mut push = |id, v: Option<u64>| {
            if let Some(v) = v {
                out.push(MetricValue {
                    id,
                    value: v as f64,
                });
            }
        };
        push(MetricId::DiskTotal, self.total);
        push(MetricId::DiskUsed, self.used());
        push(MetricId::DiskFree, self.data_available);
        push(MetricId::DiskSystem, self.system());
        out
    }
}

/// One installed app's footprint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppSize {
    pub bundle_id: String,
    pub name: Option<String>,
    pub static_bytes: u64,
    pub dynamic_bytes: u64,
}

impl AppSize {
    pub fn total(&self) -> u64 {
        self.static_bytes.saturating_add(self.dynamic_bytes)
    }
}

/// Parse a `Browse` result list; apps without a bundle id are skipped,
/// the rest are sorted by total size, largest first.
pub fn parse_app_sizes(list: &[plist::Value]) -> Vec<AppSize> {
    let mut apps: Vec<AppSize> = list
        .iter()
        .filter_map(|v| {
            let d = v.as_dictionary()?;
            Some(AppSize {
                bundle_id: string(d, "CFBundleIdentifier")?,
                name: string(d, "CFBundleDisplayName"),
                static_bytes: uint(d, "StaticDiskUsage").unwrap_or(0),
                dynamic_bytes: uint(d, "DynamicDiskUsage").unwrap_or(0),
            })
        })
        .collect();
    apps.sort_by(|a, b| {
        b.total()
            .cmp(&a.total())
            .then(a.bundle_id.cmp(&b.bundle_id))
    });
    apps
}

/// Σ static + dynamic over every app.
pub fn apps_total(apps: &[AppSize]) -> u64 {
    apps.iter()
        .fold(0u64, |acc, a| acc.saturating_add(a.total()))
}

/// The browse options plist.
pub fn browse_options() -> plist::Value {
    let mut d = plist::Dictionary::new();
    d.insert(
        "ReturnAttributes".into(),
        plist::Value::Array(APP_ATTRIBUTES.iter().map(|a| (*a).into()).collect()),
    );
    plist::Value::Dictionary(d)
}

/// Read the disk_usage domain.
pub async fn fetch_disk_usage(
    provider: &dyn IdeviceProvider,
    pair: &PairingFile,
) -> Result<DiskUsage, IosError> {
    let mut ld = Lockdown::connect(provider, pair).await?;
    Ok(DiskUsage::parse(&ld.disk_usage().await?))
}

/// Browse installed apps with the size attributes.
pub async fn fetch_app_sizes(provider: &dyn IdeviceProvider) -> Result<Vec<AppSize>, IosError> {
    let mut client = InstallationProxyClient::connect(provider)
        .await
        .map_err(|e| IosError::service("installation_proxy", e))?;
    let list = client
        .browse(Some(browse_options()))
        .await
        .map_err(|e| IosError::service("installation_proxy/Browse", e))?;
    Ok(parse_app_sizes(&list))
}

/// Latest per-app sizes, shared with the source for `IosSource::app_sizes`.
pub type AppTable = Arc<RwLock<Vec<AppSize>>>;

/// Sample loop at the slow cadence; the per-app browse runs on tick 1 and
/// then every [`APPS_EVERY_N_TICKS`]. `per_app` false skips it entirely.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    provider: Arc<dyn IdeviceProvider>,
    pair: PairingFile,
    sink: EventSink,
    device_id: String,
    period: Duration,
    per_app: bool,
    apps: AppTable,
    cancel: CancellationToken,
    gone: Arc<AtomicBool>,
) {
    let mut interval = tokio::time::interval(period.max(Duration::from_secs(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut tick = 0u32;
    let mut apps_failures = 0u32;
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {}
        }
        if sink.is_closed() {
            return;
        }
        let mut values = match fetch_disk_usage(&*provider, &pair).await {
            Ok(usage) => usage.metrics(),
            Err(e) if is_device_gone(&e) => {
                gone.store(true, Ordering::SeqCst);
                cancel.cancel();
                return;
            }
            Err(e) => {
                tracing::warn!(device = %device_id, error = %e, "disk_usage failed");
                Vec::new()
            }
        };
        if per_app && tick.is_multiple_of(APPS_EVERY_N_TICKS) {
            match fetch_app_sizes(&*provider).await {
                Ok(list) => {
                    apps_failures = 0;
                    values.push(MetricValue {
                        id: MetricId::DiskApps,
                        value: apps_total(&list) as f64,
                    });
                    *apps.write() = list;
                }
                Err(e) if is_device_gone(&e) => {
                    gone.store(true, Ordering::SeqCst);
                    cancel.cancel();
                    return;
                }
                Err(e) => {
                    apps_failures += 1;
                    if apps_failures <= 2 {
                        tracing::warn!(device = %device_id, error = %e, "installation_proxy browse failed");
                    }
                }
            }
        }
        tick = tick.wrapping_add(1);
        if !values.is_empty() && sink.samples(&device_id, now_ms(), values).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn usage() -> plist::Dictionary {
        let mut d = plist::Dictionary::new();
        let put = |d: &mut plist::Dictionary, k: &str, v: u64| {
            d.insert(k.into(), plist::Value::Integer(v.into()));
        };
        put(&mut d, "TotalDiskCapacity", 64_000_000_000);
        put(&mut d, "TotalDataCapacity", 57_000_000_000);
        put(&mut d, "TotalDataAvailable", 20_000_000_000);
        put(&mut d, "AmountDataReserved", 200_000_000);
        d
    }

    #[test]
    fn disk_usage_mapping() {
        let u = DiskUsage::parse(&usage());
        assert_eq!(u.total, Some(64_000_000_000));
        assert_eq!(u.used(), Some(37_000_000_000));
        assert_eq!(u.system(), Some(7_000_000_000));
        assert_eq!(u.reserved, Some(200_000_000));
        let m = u.metrics();
        let ids: Vec<MetricId> = m.iter().map(|x| x.id).collect();
        assert_eq!(
            ids,
            vec![
                MetricId::DiskTotal,
                MetricId::DiskUsed,
                MetricId::DiskFree,
                MetricId::DiskSystem
            ]
        );
        assert!(!ids.contains(&MetricId::DiskMedia));
    }

    #[test]
    fn app_sizes_sum_and_sort() {
        let app = |id: &str, s: u64, d: u64| {
            let mut m = plist::Dictionary::new();
            m.insert("CFBundleIdentifier".into(), id.into());
            m.insert("CFBundleDisplayName".into(), id.to_uppercase().into());
            m.insert("StaticDiskUsage".into(), plist::Value::Integer(s.into()));
            m.insert("DynamicDiskUsage".into(), plist::Value::Integer(d.into()));
            plist::Value::Dictionary(m)
        };
        let mut nameless = plist::Dictionary::new();
        nameless.insert("StaticDiskUsage".into(), plist::Value::Integer(5.into()));
        let list = vec![
            app("a.small", 10, 5),
            app("b.big", 1000, 500),
            plist::Value::Dictionary(nameless),
        ];
        let apps = parse_app_sizes(&list);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].bundle_id, "b.big");
        assert_eq!(apps[0].name.as_deref(), Some("B.BIG"));
        assert_eq!(apps_total(&apps), 1515);
        let opts = browse_options();
        let attrs = opts
            .as_dictionary()
            .unwrap()
            .get("ReturnAttributes")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(attrs.len(), 4);
    }

    #[test]
    fn missing_totals_emit_nothing() {
        assert!(
            DiskUsage::parse(&plist::Dictionary::new())
                .metrics()
                .is_empty()
        );
    }
}
