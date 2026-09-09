//! 2–5 s cadence: process table (`ps` + `/proc/<pid>/stat` deltas),
//! `dumpsys battery` (+ power_supply sysfs when readable), `cmd wifi status`
//! and the resumed activity (step 2.4).

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use statasmon_device::EventSink;
use statasmon_protocol::{EventKind, MetricId, MetricValue, ProcessRow, ProcessState};

use super::parse::battery::{parse_dumpsys_battery, parse_power_supply};
use super::parse::process::{CLK_TCK, parse_pid_stats, parse_ps, parse_resumed_package};
use super::parse::wifi::parse_wifi_status;
use super::parse::{section, split_sections};
use super::{Batch, Cadence, Sampler, Shared, per_second, sink_closed};
use crate::probe::Probe;
use crate::shell::{Shell, ShellError};

const TIMEOUT_MS: u64 = 8_000;

#[derive(Debug)]
pub struct MediumSampler {
    ps: bool,
    pid_stat: bool,
    battery: bool,
    power_supply: bool,
    wifi: bool,
    resumed: bool,
    shared: Shared,
    /// pid → cpu ticks at the previous tick, for per-process CPU %.
    prev: Option<(Instant, HashMap<u32, u64>)>,
    last_wifi: Option<String>,
    cmd: String,
}

impl MediumSampler {
    pub fn new(probe: &Probe, shared: Shared) -> Option<Self> {
        if !(probe.ps || probe.battery_dumpsys || probe.wifi_cmd) {
            return None;
        }
        let mut b = Batch::default();
        if probe.ps {
            b.section("ps", "ps -A -o PID,PPID,USER,%CPU,RSS,VSZ,NAME 2>/dev/null");
            if probe.proc_pid_stat {
                b.section("pidstat", "cat /proc/[0-9]*/stat 2>/dev/null");
            }
            if probe.activity_resumed {
                b.section(
                    "resumed",
                    "dumpsys activity activities 2>/dev/null | grep -E 'topResumedActivity|ResumedActivity'",
                );
            }
        }
        if probe.battery_dumpsys {
            b.section("battery", "dumpsys battery 2>/dev/null");
            if probe.power_supply_sysfs {
                b.section(
                    "psy",
                    "for f in current_now cycle_count charge_full charge_full_design; do echo \"$f=$(cat /sys/class/power_supply/battery/$f 2>/dev/null)\"; done",
                );
            }
        }
        if probe.wifi_cmd {
            b.section("wifi", "cmd wifi status 2>/dev/null");
        }
        Some(Self {
            ps: probe.ps,
            pid_stat: probe.ps && probe.proc_pid_stat,
            battery: probe.battery_dumpsys,
            power_supply: probe.battery_dumpsys && probe.power_supply_sysfs,
            wifi: probe.wifi_cmd,
            resumed: probe.ps && probe.activity_resumed,
            shared,
            prev: None,
            last_wifi: None,
            cmd: b.into_command(),
        })
    }

    pub fn command(&self) -> &str {
        &self.cmd
    }
}

#[async_trait]
impl Sampler for MediumSampler {
    fn name(&self) -> &'static str {
        "android-medium"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Medium
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

        // Processes.
        let mut rows: Vec<ProcessRow> = Vec::new();
        if self.ps
            && let Some(s) = section(&sections, "ps")
        {
            match parse_ps(s) {
                Ok(ps) => {
                    let stats = if self.pid_stat {
                        section(&sections, "pidstat").map(parse_pid_stats)
                    } else {
                        None
                    };
                    let resumed = if self.resumed {
                        section(&sections, "resumed").and_then(parse_resumed_package)
                    } else {
                        None
                    };
                    let dt = self
                        .prev
                        .as_ref()
                        .map(|(t, _)| at.duration_since(*t).as_secs_f64());
                    {
                        let mut table = self.shared.pids.write();
                        table.clear();
                        for r in &ps {
                            table.insert(r.pid, r.name.clone());
                        }
                    }
                    let uid_net = self.shared.uid_net.read();
                    let mut ticks_now = HashMap::with_capacity(ps.len());
                    for r in &ps {
                        let st = stats.as_ref().and_then(|m| m.get(&r.pid));
                        // % of one core (top convention): ticks/CLK_TCK per second.
                        let cpu_pct = match (st, &self.prev, dt) {
                            (Some(st), Some((_, prev)), Some(dt)) => prev
                                .get(&r.pid)
                                .and_then(|p| {
                                    per_second(st.cpu_ticks.saturating_sub(*p) as f64, dt)
                                })
                                .map(|tps| (tps / CLK_TCK * 100.0) as f32),
                            _ => None,
                        };
                        if let Some(st) = st {
                            ticks_now.insert(r.pid, st.cpu_ticks);
                        }
                        let package = r.package().map(str::to_owned);
                        let state = match (&resumed, &package) {
                            (Some(fg), Some(pkg)) if fg == pkg => ProcessState::Foreground,
                            (Some(_), _) => ProcessState::Background,
                            (None, _) => ProcessState::Unknown,
                        };
                        rows.push(ProcessRow {
                            pid: r.pid,
                            name: r.name.clone(),
                            net_bps: r.uid.and_then(|u| uid_net.get(&u).copied()),
                            app_id: package,
                            cpu_pct,
                            mem_bytes: Some(r.rss_kib * 1024),
                            threads: st.map(|s| s.num_threads),
                            state,
                            energy: None,
                        });
                    }
                    drop(uid_net);
                    push(MetricId::ProcCount, rows.len() as f64);
                    if self.pid_stat {
                        self.prev = Some((at, ticks_now));
                    }
                }
                Err(e) => tracing::debug!(target: "android.medium", "ps: {e}"),
            }
        }

        // Battery.
        if self.battery
            && let Some(s) = section(&sections, "battery")
        {
            match parse_dumpsys_battery(s) {
                Ok(b) => {
                    push(MetricId::BatPercent, b.percent());
                    push(MetricId::BatVoltage, b.voltage_v);
                    push(MetricId::BatTemp, b.temp_c);
                    push(MetricId::BatCharging, if b.charging() { 1.0 } else { 0.0 });
                    if self.power_supply
                        && let Some(p) = section(&sections, "psy")
                        && let Ok(ps) = parse_power_supply(p, b.charging())
                    {
                        if let Some(a) = ps.current_a {
                            push(MetricId::BatCurrent, a);
                            push(MetricId::BatPower, a * b.voltage_v);
                        }
                        if let Some(c) = ps.cycle_count {
                            push(MetricId::BatCycles, c as f64);
                        }
                        if let Some(f) = ps.charge_full_mah {
                            push(MetricId::BatFullCapacity, f);
                        }
                        if let Some(d) = ps.charge_full_design_mah {
                            push(MetricId::BatDesignCapacity, d);
                            if let Some(f) = ps.charge_full_mah
                                && d > 0.0
                            {
                                push(MetricId::BatHealth, f / d * 100.0);
                            }
                        }
                    }
                }
                Err(e) => tracing::debug!(target: "android.medium", "battery: {e}"),
            }
        }

        // Wi-Fi.
        let mut connection_event = None;
        if self.wifi
            && let Some(s) = section(&sections, "wifi")
        {
            match parse_wifi_status(s) {
                Ok(w) => {
                    if let Some(r) = w.rssi_dbm {
                        push(MetricId::NetRssi, r as f64);
                    }
                    if let Some(l) = w.link_mbps {
                        push(MetricId::NetLinkMbps, l as f64);
                    }
                    if let Some(r) = w.retried_tx {
                        push(MetricId::NetRetransmits, r as f64);
                    }
                    let summary = w.summary();
                    if self.last_wifi.as_deref() != Some(&summary) {
                        self.last_wifi = Some(summary.clone());
                        connection_event = Some(summary);
                    }
                }
                Err(e) => tracing::debug!(target: "android.medium", "wifi: {e}"),
            }
        }

        if !values.is_empty() {
            sink.samples(device_id, now_ms, values)
                .await
                .map_err(sink_closed)?;
        }
        if !rows.is_empty() {
            sink.processes(device_id, now_ms, rows)
                .await
                .map_err(sink_closed)?;
        }
        if let Some(detail) = connection_event {
            sink.event(device_id, now_ms, EventKind::Connection, detail, None, None)
                .await
                .map_err(sink_closed)?;
        }
        Ok(())
    }
}
