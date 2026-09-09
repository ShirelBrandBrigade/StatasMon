//! 1 s cadence: `/proc/stat` per core, cpufreq, `/proc/meminfo`, PSI (or
//! the vmstat fallback), `/proc/loadavg`, `/proc/net/dev` (step 2.3).

use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use statasmon_device::EventSink;
use statasmon_protocol::{CoreKind, CoreSample, MetricId, MetricValue};

use super::parse::cpu::{
    ProcStat, parse_cpu_online, parse_cpufreq_khz, parse_loadavg, parse_proc_stat,
};
use super::parse::mem::{
    breakdown, parse_meminfo, parse_psi_memory, parse_vmstat_field, pressure_from_majfault_rate,
    pressure_from_psi,
};
use super::parse::net::{NetTotals, parse_net_dev, totals};
use super::parse::{section, split_sections};
use super::{Batch, Cadence, Sampler, per_second, sink_closed};
use crate::probe::Probe;
use crate::shell::{Shell, ShellError};

const TIMEOUT_MS: u64 = 3_000;
const CPUFREQ_GLOB: &str = "/sys/devices/system/cpu/cpu[0-9]*/cpufreq";

#[derive(Debug)]
struct Prev {
    at: Instant,
    stat: Option<ProcStat>,
    net: Option<NetTotals>,
    majfault: Option<u64>,
}

#[derive(Debug)]
pub struct FastSampler {
    proc_stat: bool,
    cpufreq: bool,
    loadavg: bool,
    meminfo: bool,
    psi: bool,
    net_dev: bool,
    cpu_count: u16,
    /// `cpuinfo_max_freq` per core, read once (first tick) to label cores.
    max_khz: Option<BTreeMap<u16, u64>>,
    /// Highest max frequency among cores → the "performance" tier.
    top_khz: u64,
    prev: Option<Prev>,
    /// Counters at the first tick; `net.rx_total`/`tx_total` are relative to it.
    net_base: Option<NetTotals>,
    cmd: String,
    first_cmd: String,
}

impl FastSampler {
    /// `None` when nothing in this cadence is readable.
    pub fn new(probe: &Probe) -> Option<Self> {
        if !(probe.proc_stat || probe.meminfo || probe.loadavg || probe.proc_net_dev) {
            return None;
        }
        let build = |first: bool| {
            let mut b = Batch::default();
            if probe.proc_stat {
                b.section("stat", "cat /proc/stat 2>/dev/null");
                b.section("online", "cat /sys/devices/system/cpu/online 2>/dev/null");
            }
            if probe.cpufreq {
                b.section(
                    "freq",
                    &format!("grep -H . {CPUFREQ_GLOB}/scaling_cur_freq 2>/dev/null"),
                );
                if first {
                    b.section(
                        "maxfreq",
                        &format!("grep -H . {CPUFREQ_GLOB}/cpuinfo_max_freq 2>/dev/null"),
                    );
                }
            }
            if probe.meminfo {
                b.section("meminfo", "cat /proc/meminfo 2>/dev/null");
                if probe.psi_memory {
                    b.section("psi", "cat /proc/pressure/memory 2>/dev/null");
                } else {
                    b.section("vmstat", "grep -E '^pgmajfault ' /proc/vmstat 2>/dev/null");
                }
            }
            if probe.loadavg {
                b.section("loadavg", "cat /proc/loadavg 2>/dev/null");
            }
            if probe.proc_net_dev {
                b.section("netdev", "cat /proc/net/dev 2>/dev/null");
            }
            b.into_command()
        };
        Some(Self {
            proc_stat: probe.proc_stat,
            cpufreq: probe.cpufreq,
            loadavg: probe.loadavg,
            meminfo: probe.meminfo,
            psi: probe.psi_memory,
            net_dev: probe.proc_net_dev,
            cpu_count: probe.cpu_count,
            max_khz: None,
            top_khz: 0,
            prev: None,
            net_base: None,
            cmd: build(false),
            first_cmd: build(true),
        })
    }

    /// The command this sampler runs on a steady-state tick.
    pub fn command(&self) -> &str {
        &self.cmd
    }

    fn core_kind(&self, idx: u16) -> CoreKind {
        match self.max_khz.as_ref().and_then(|m| m.get(&idx)) {
            Some(max) if *max == self.top_khz => CoreKind::Performance,
            Some(_) => CoreKind::Efficiency,
            None => CoreKind::Unknown,
        }
    }
}

#[async_trait]
impl Sampler for FastSampler {
    fn name(&self) -> &'static str {
        "android-fast"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Fast
    }

    async fn tick(
        &mut self,
        shell: &dyn Shell,
        sink: &EventSink,
        device_id: &str,
        now_ms: i64,
    ) -> Result<(), ShellError> {
        let cmd = if self.max_khz.is_none() && self.cpufreq {
            self.first_cmd.clone()
        } else {
            self.cmd.clone()
        };
        let out = shell.exec(&cmd, TIMEOUT_MS).await?;
        let at = Instant::now();
        let sections = split_sections(&out);
        let dt = self
            .prev
            .as_ref()
            .map(|p| at.duration_since(p.at).as_secs_f64());
        let mut values: Vec<MetricValue> = Vec::with_capacity(24);
        let mut push = |id: MetricId, v: f64| {
            if v.is_finite() {
                values.push(MetricValue { id, value: v });
            }
        };

        // Core labels (once).
        if self.cpufreq
            && self.max_khz.is_none()
            && let Some(s) = section(&sections, "maxfreq")
            && let Ok(m) = parse_cpufreq_khz(s)
        {
            self.top_khz = m.values().copied().max().unwrap_or(0);
            self.max_khz = Some(m);
        }

        // CPU.
        let mut stat_now = None;
        let mut cores: Vec<CoreSample> = Vec::new();
        if self.proc_stat
            && let Some(s) = section(&sections, "stat")
        {
            match parse_proc_stat(s) {
                Ok(stat) => {
                    if let Some(prev) = self.prev.as_ref().and_then(|p| p.stat.as_ref()) {
                        let d = stat.total.delta(&prev.total);
                        let tot = d.total() as f64;
                        if tot > 0.0 {
                            push(MetricId::CpuTotal, d.busy() as f64 / tot * 100.0);
                            push(MetricId::CpuUser, d.user_all() as f64 / tot * 100.0);
                            push(MetricId::CpuSystem, d.system_all() as f64 / tot * 100.0);
                        }
                        let online =
                            section(&sections, "online").and_then(|o| parse_cpu_online(o).ok());
                        let cur_khz = section(&sections, "freq")
                            .and_then(|f| parse_cpufreq_khz(f).ok())
                            .unwrap_or_default();
                        let n = stat
                            .cores
                            .keys()
                            .next_back()
                            .map_or(0, |k| k + 1)
                            .max(self.cpu_count);
                        for idx in 0..n {
                            let is_online = online.as_ref().is_none_or(|o| o.contains(&idx));
                            let usage = match (stat.cores.get(&idx), prev.cores.get(&idx)) {
                                (Some(c), Some(p)) if is_online => {
                                    let d = c.delta(p);
                                    let t = d.total() as f64;
                                    if t > 0.0 {
                                        (d.busy() as f64 / t * 100.0) as f32
                                    } else {
                                        0.0
                                    }
                                }
                                _ => 0.0,
                            };
                            let present = stat.cores.contains_key(&idx) && is_online;
                            cores.push(CoreSample {
                                index: idx,
                                kind: self.core_kind(idx),
                                usage_pct: usage,
                                freq_mhz: present
                                    .then(|| cur_khz.get(&idx).map(|k| (*k / 1000) as u32))
                                    .flatten(),
                                online: present,
                            });
                        }
                    }
                    stat_now = Some(stat);
                }
                Err(e) => tracing::debug!(target: "android.fast", "proc/stat: {e}"),
            }
        }

        // Memory.
        let mut majfault_now = None;
        if self.meminfo
            && let Some(s) = section(&sections, "meminfo")
        {
            match parse_meminfo(s).and_then(|m| breakdown(&m)) {
                Ok(b) => {
                    push(MetricId::MemTotal, b.total as f64);
                    push(MetricId::MemUsed, b.used as f64);
                    push(MetricId::MemFree, b.free as f64);
                    push(MetricId::MemCached, b.cached as f64);
                    if let Some(v) = b.app {
                        push(MetricId::MemApp, v as f64);
                    }
                    if let Some(v) = b.wired {
                        push(MetricId::MemWired, v as f64);
                    }
                    if let Some(v) = b.swap {
                        push(MetricId::MemSwap, v as f64);
                    }
                    if let Some(v) = b.compressed {
                        push(MetricId::MemCompressed, v as f64);
                    }
                }
                Err(e) => tracing::debug!(target: "android.fast", "meminfo: {e}"),
            }
            if self.psi {
                if let Some(p) = section(&sections, "psi") {
                    match parse_psi_memory(p) {
                        Ok(psi) => push(MetricId::MemPressure, pressure_from_psi(&psi)),
                        Err(e) => tracing::debug!(target: "android.fast", "psi: {e}"),
                    }
                }
            } else if let Some(v) = section(&sections, "vmstat")
                && let Ok(faults) = parse_vmstat_field(v, "pgmajfault")
            {
                majfault_now = Some(faults);
                if let (Some(prev), Some(dt)) = (self.prev.as_ref().and_then(|p| p.majfault), dt)
                    && let Some(rate) = per_second(faults.saturating_sub(prev) as f64, dt)
                {
                    push(MetricId::MemPressure, pressure_from_majfault_rate(rate));
                }
            }
        }

        // Load.
        if self.loadavg
            && let Some(s) = section(&sections, "loadavg")
        {
            match parse_loadavg(s) {
                Ok(l) => {
                    push(MetricId::CpuLoad1, l.one);
                    push(MetricId::CpuLoad5, l.five);
                }
                Err(e) => tracing::debug!(target: "android.fast", "loadavg: {e}"),
            }
        }

        // Network counters.
        let mut net_now = None;
        if self.net_dev
            && let Some(s) = section(&sections, "netdev")
        {
            match parse_net_dev(s) {
                Ok(ifaces) => {
                    let t = totals(&ifaces);
                    let base = *self.net_base.get_or_insert(t);
                    push(
                        MetricId::NetRxTotal,
                        t.rx_bytes.saturating_sub(base.rx_bytes) as f64,
                    );
                    push(
                        MetricId::NetTxTotal,
                        t.tx_bytes.saturating_sub(base.tx_bytes) as f64,
                    );
                    push(MetricId::NetDropped, t.dropped as f64);
                    if let (Some(prev), Some(dt)) = (self.prev.as_ref().and_then(|p| p.net), dt) {
                        let d = |a: u64, b: u64| a.saturating_sub(b) as f64;
                        if let Some(r) = per_second(d(t.rx_bytes, prev.rx_bytes) * 8.0, dt) {
                            push(MetricId::NetRxBps, r);
                        }
                        if let Some(r) = per_second(d(t.tx_bytes, prev.tx_bytes) * 8.0, dt) {
                            push(MetricId::NetTxBps, r);
                        }
                        if let Some(r) = per_second(d(t.rx_packets, prev.rx_packets), dt) {
                            push(MetricId::NetRxPps, r);
                        }
                        if let Some(r) = per_second(d(t.tx_packets, prev.tx_packets), dt) {
                            push(MetricId::NetTxPps, r);
                        }
                    }
                    net_now = Some(t);
                }
                Err(e) => tracing::debug!(target: "android.fast", "net/dev: {e}"),
            }
        }

        self.prev = Some(Prev {
            at,
            stat: stat_now,
            net: net_now,
            majfault: majfault_now,
        });

        if !values.is_empty() {
            sink.samples(device_id, now_ms, values)
                .await
                .map_err(sink_closed)?;
        }
        if !cores.is_empty() {
            sink.cores(device_id, now_ms, cores)
                .await
                .map_err(sink_closed)?;
        }
        Ok(())
    }
}
