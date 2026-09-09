//! 10–30 s cadence: `dumpsys thermalservice`, per-UID `dumpsys netstats`,
//! `dumpsys diskstats` + `df`, established TCP count (step 2.5).

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use statasmon_device::EventSink;
use statasmon_protocol::{EventKind, MetricId, MetricValue};

use super::parse::disk::{parse_df, parse_diskstats};
use super::parse::net::count_established;
use super::parse::netstats::{UidBytes, parse_uid_bytes};
use super::parse::thermal::{
    TYPE_BATTERY, TYPE_CPU, TYPE_DISPLAY, TYPE_SKIN, common_state, parse_thermalservice,
};
use super::parse::{section, split_sections};
use super::{Batch, Cadence, Sampler, Shared, per_second, sink_closed};
use crate::events::thermal_change_detail;
use crate::probe::Probe;
use crate::shell::{Shell, ShellError};

const TIMEOUT_MS: u64 = 20_000;

#[derive(Debug)]
pub struct SlowSampler {
    thermal: bool,
    /// Emit `temp.*` only when the probe saw real sensors (capability gate).
    thermal_sensors: bool,
    netstats: bool,
    diskstats: bool,
    df: bool,
    shared: Shared,
    prev_thermal_raw: Option<u32>,
    prev_uid: Option<(Instant, HashMap<u32, UidBytes>)>,
    cmd: String,
}

impl SlowSampler {
    pub fn new(probe: &Probe, shared: Shared) -> Option<Self> {
        if !(probe.thermalservice || probe.netstats || probe.diskstats || probe.df) {
            return None;
        }
        let mut b = Batch::default();
        if probe.thermalservice {
            b.section("thermal", "dumpsys thermalservice 2>/dev/null");
        }
        if probe.netstats {
            // --poll forces the eBPF counters into the current bucket
            // (otherwise they refresh about every 30 min).
            b.section(
                "netstats",
                "dumpsys netstats --poll >/dev/null 2>&1; dumpsys netstats detail 2>/dev/null",
            );
        }
        if probe.diskstats {
            b.section("diskstats", "dumpsys diskstats 2>/dev/null");
        }
        if probe.df {
            b.section("df", "df -k /data 2>/dev/null");
        }
        // Cheap and harmless when unreadable (empty section → no metric).
        b.section("tcp", "cat /proc/net/tcp /proc/net/tcp6 2>/dev/null");
        Some(Self {
            thermal: probe.thermalservice,
            thermal_sensors: probe.thermalservice && probe.thermal_sensors,
            netstats: probe.netstats,
            diskstats: probe.diskstats,
            df: probe.df,
            shared,
            prev_thermal_raw: None,
            prev_uid: None,
            cmd: b.into_command(),
        })
    }

    pub fn command(&self) -> &str {
        &self.cmd
    }
}

#[async_trait]
impl Sampler for SlowSampler {
    fn name(&self) -> &'static str {
        "android-slow"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Slow
    }

    async fn tick(
        &mut self,
        shell: &dyn Shell,
        sink: &EventSink,
        device_id: &str,
        now_ms: i64,
    ) -> Result<(), ShellError> {
        let out = shell.exec(&self.cmd, TIMEOUT_MS).await?;
        let at = Instant::now();
        let sections = split_sections(&out);
        let mut values: Vec<MetricValue> = Vec::with_capacity(16);
        let mut push = |id: MetricId, v: f64| {
            if v.is_finite() {
                values.push(MetricValue { id, value: v });
            }
        };
        let mut thermal_event = None;

        if self.thermal
            && let Some(s) = section(&sections, "thermal")
        {
            match parse_thermalservice(s) {
                Ok(t) => {
                    push(MetricId::ThermalRaw, t.status as f64);
                    push(MetricId::ThermalState, common_state(t.status));
                    if self.thermal_sensors {
                        if let Some(v) = t.max_of(TYPE_CPU) {
                            push(MetricId::TempSoc, v);
                        }
                        if let Some(v) = t.max_of(TYPE_BATTERY) {
                            push(MetricId::TempBattery, v);
                        }
                        if let Some(v) = t.max_of(TYPE_SKIN) {
                            push(MetricId::TempSkin, v);
                        }
                        if let Some(v) = t.max_of(TYPE_DISPLAY) {
                            push(MetricId::TempDisplay, v);
                        }
                    }
                    if let Some(prev) = self.prev_thermal_raw
                        && prev != t.status
                    {
                        thermal_event = Some(thermal_change_detail(prev, t.status));
                    }
                    self.prev_thermal_raw = Some(t.status);
                }
                Err(e) => tracing::debug!(target: "android.slow", "thermalservice: {e}"),
            }
        }

        if self.netstats
            && let Some(s) = section(&sections, "netstats")
        {
            match parse_uid_bytes(s) {
                Ok(now) => {
                    if let Some((t0, prev)) = &self.prev_uid {
                        let dt = at.duration_since(*t0).as_secs_f64();
                        let mut rates: HashMap<u32, f64> = HashMap::with_capacity(now.len());
                        for (uid, b) in &now {
                            let p = prev.get(uid).copied().unwrap_or_default();
                            let bytes = b.rx.saturating_sub(p.rx) + b.tx.saturating_sub(p.tx);
                            if let Some(bps) = per_second(bytes as f64 * 8.0, dt) {
                                rates.insert(*uid, bps);
                            }
                        }
                        *self.shared.uid_net.write() = rates;
                    }
                    self.prev_uid = Some((at, now));
                }
                Err(e) => tracing::debug!(target: "android.slow", "netstats: {e}"),
            }
        }

        let mut have_totals = false;
        if self.df
            && let Some(s) = section(&sections, "df")
        {
            match parse_df(s, "/data") {
                Ok(r) => {
                    push(MetricId::DiskTotal, r.total as f64);
                    push(MetricId::DiskUsed, r.used as f64);
                    push(MetricId::DiskFree, r.free as f64);
                    have_totals = true;
                }
                Err(e) => tracing::debug!(target: "android.slow", "df: {e}"),
            }
        }
        if self.diskstats
            && let Some(s) = section(&sections, "diskstats")
        {
            match parse_diskstats(s) {
                Ok(d) => {
                    // Category sizes come from a periodic job and can be
                    // hours stale; emitted as measured, the UI shows age.
                    if let Some(v) = d.apps() {
                        push(MetricId::DiskApps, v as f64);
                    }
                    if let Some(v) = d.media() {
                        push(MetricId::DiskMedia, v as f64);
                    }
                    if let Some(v) = d.system {
                        push(MetricId::DiskSystem, v as f64);
                    }
                    if let Some(v) = d.other {
                        push(MetricId::DiskOther, v as f64);
                    }
                    if !have_totals && let (Some(total), Some(free)) = (d.data_total, d.data_free) {
                        push(MetricId::DiskTotal, total as f64);
                        push(MetricId::DiskFree, free as f64);
                        push(MetricId::DiskUsed, total.saturating_sub(free) as f64);
                    }
                }
                Err(e) => tracing::debug!(target: "android.slow", "diskstats: {e}"),
            }
        }

        if let Some(s) = section(&sections, "tcp")
            && let Some(n) = count_established(s)
        {
            push(MetricId::NetConnections, n as f64);
        }

        if !values.is_empty() {
            sink.samples(device_id, now_ms, values)
                .await
                .map_err(sink_closed)?;
        }
        if let Some(detail) = thermal_event {
            sink.event(
                device_id,
                now_ms,
                EventKind::ThermalChange,
                detail,
                None,
                None,
            )
            .await
            .map_err(sink_closed)?;
        }
        Ok(())
    }
}
