//! `com.apple.instruments.server.services.sysmontap` (step 4.3): total
//! and per-core CPU, the memory breakdown, network and disk counters and
//! the process table, one message per second.
//!
//! Attribute names vary by iOS version, so the loop first asks
//! `deviceinfo.sysmonSystemAttributes` / `sysmonProcessAttributes` and
//! keeps the intersection with [`WANTED_SYSTEM`] / [`WANTED_PROCESS`]
//! ([`choose`]). Every message is an array of row dictionaries; the rows of
//! one message are merged ([`collect`]) and turned into `Samples`, `Cores`
//! and `Processes` by the pure [`parse_tick`].
//!
//! Units (verify on each iOS major; see ADR-013):
//! - `SystemCPUUsage.CPU_TotalLoad` and every `PerCPUUsage[i].CPU_TotalLoad`
//!   are percentages 0–100; `CPU_UserLoad`/`CPU_SystemLoad` are −1 when the
//!   build does not split them.
//! - `vm*Count` attributes are pages (`vmPageSize`, else 16 KiB on Apple
//!   silicon); `physMemSize` and `net*`/`disk*` are bytes/packets, cumulative
//!   since boot.
//! - process `cpuUsage` is a fraction of one core (1.0 = one core busy),
//!   reported ×100 in the `top` convention of ADR-010; `physFootprint` and
//!   `memResidentSize` are bytes.

use std::collections::HashMap;
use std::time::Duration;

use idevice::IdeviceError;
use idevice::dvt::message::AuxValue;
use plist::{Dictionary, Value};
use statasmon_device::{EventSink, now_ms};
use statasmon_idmap::apple::CoreLayout;
use statasmon_protocol::{CoreKind, CoreSample, MetricId, MetricValue, ProcessRow, ProcessState};
use tokio_util::sync::CancellationToken;

use super::parse::{delta, get_num, get_uint, num, uint};
use super::{LoopEnd, SharedState};
use crate::tunnel::{DvtHub, Tunnel};

/// The DVT channel identifier.
pub const SERVICE: &str = "com.apple.instruments.server.services.sysmontap";

/// Apple silicon numbers its efficiency cores first and the performance
/// cores last (`powermetrics` on M1: E-Cluster = CPU 0–3, P-Cluster = CPU
/// 4–7); sysmontap's `PerCPUUsage` follows the kernel's CPU ids. Flip this
/// constant if a chip proves otherwise (ADR-013).
pub const PERFORMANCE_CORES_LAST: bool = true;

/// Page size when `vmPageSize` is not advertised (arm64 Apple silicon).
pub const DEFAULT_PAGE_SIZE: u64 = 16_384;

/// The process table is sent at most every this often, and only the top
/// [`MAX_PROCESS_ROWS`] rows by CPU.
pub const MAX_PROCESS_ROWS: usize = 250;

/// System attributes we want, both spellings (the `vm*` vm_statistics
/// names iOS 16–17 advertise, and the Instruments display names some
/// builds use), in the order we prefer them.
pub const WANTED_SYSTEM: &[&str] = &[
    "physMemSize",
    "vmPageSize",
    "vmFreeCount",
    "vmWireCount",
    "vmIntPageCount",
    "vmExtPageCount",
    "vmCompressorPageCount",
    "vmPurgeableCount",
    "vmSpeculativeCount",
    "vmUsedCount",
    "vmSwapUsedCount",
    "vmCompressions",
    "vmDecompressions",
    "vmSwapIns",
    "vmSwapOuts",
    "vmPageIns",
    "vmPageOuts",
    "netBytesIn",
    "netBytesOut",
    "netPacketsIn",
    "netPacketsOut",
    "diskBytesRead",
    "diskBytesWritten",
    "diskReadOps",
    "diskWriteOps",
    "CPUCount",
    "EnabledCPUs",
    "PerCPUUsage",
    "CPU_TotalLoad",
    "CPU_UserLoad",
    "CPU_SystemLoad",
    "PhysicalMemoryFree",
    "PhysicalMemoryActive",
    "PhysicalMemoryInactive",
    "PhysicalMemoryWired",
    "PhysicalMemorySpeculative",
    "PhysicalMemoryCompressed",
    "Compressions",
    "Decompressions",
    "SwapIns",
    "SwapOuts",
    "NetBytesIn",
    "NetBytesOut",
    "NetPacketsIn",
    "NetPacketsOut",
    "DiskBytesRead",
    "DiskBytesWritten",
];

/// Process attributes we want.
pub const WANTED_PROCESS: &[&str] = &[
    "pid",
    "name",
    "cpuUsage",
    "physFootprint",
    "memResidentSize",
    "memAnon",
    "threadCount",
    "ctxSwitch",
    "diskBytesRead",
    "diskBytesWritten",
    "bundleID",
    "bundleIdentifier",
    "appSleep",
];

/// What every tool (pymobiledevice3, go-ios) requests and every iOS since
/// 13 accepts; used when the attribute query fails.
pub const FALLBACK_SYSTEM: &[&str] = &[
    "vmExtPageCount",
    "vmFreeCount",
    "vmPurgeableCount",
    "vmSpeculativeCount",
    "physMemSize",
];
pub const FALLBACK_PROCESS: &[&str] = &[
    "pid",
    "name",
    "cpuUsage",
    "physFootprint",
    "memResidentSize",
    "memAnon",
    "ctxSwitch",
];

/// `wanted ∩ available` in `wanted` order; the fallback list when
/// `available` is empty (the query failed or returned nothing).
pub fn choose(available: &[String], wanted: &[&str], fallback: &[&str]) -> Vec<String> {
    if available.is_empty() {
        return fallback.iter().map(|s| (*s).to_owned()).collect();
    }
    wanted
        .iter()
        .filter(|w| available.iter().any(|a| a == *w))
        .map(|w| (*w).to_owned())
        .collect()
}

/// The attribute lists in effect for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attrs {
    pub system: Vec<String>,
    pub process: Vec<String>,
}

impl Attrs {
    /// Pick from what the device advertises.
    pub fn select(system_available: &[String], process_available: &[String]) -> Self {
        Self {
            system: choose(system_available, WANTED_SYSTEM, FALLBACK_SYSTEM),
            process: choose(process_available, WANTED_PROCESS, FALLBACK_PROCESS),
        }
    }

    /// The `setConfig:` dictionary.
    pub fn config(&self, interval: Duration) -> Dictionary {
        let ms = i64::try_from(interval.as_millis()).unwrap_or(1000).max(100);
        let mut cfg = Dictionary::new();
        cfg.insert("ur".into(), Value::Integer(ms.into()));
        cfg.insert("bm".into(), Value::Integer(0i64.into()));
        cfg.insert(
            "procAttrs".into(),
            Value::Array(self.process.iter().cloned().map(Value::String).collect()),
        );
        cfg.insert(
            "sysAttrs".into(),
            Value::Array(self.system.iter().cloned().map(Value::String).collect()),
        );
        cfg.insert("cpuUsage".into(), Value::Boolean(true));
        cfg.insert("physFootprint".into(), Value::Boolean(true));
        cfg.insert(
            "sampleInterval".into(),
            Value::Integer((ms * 1_000_000).into()),
        );
        cfg
    }
}

/// The rows of one sysmontap message, merged.
#[derive(Debug, Default, Clone)]
pub struct SysmonMessage {
    /// `System`: values aligned with `Attrs::system`.
    pub system: Option<Vec<Value>>,
    /// `SystemCPUUsage`: `CPU_TotalLoad`, `CPU_UserLoad`, `CPU_SystemLoad`, `CPU_NiceLoad`.
    pub system_cpu: Option<Dictionary>,
    /// `PerCPUUsage`: one dictionary per core.
    pub per_cpu: Option<Vec<Value>>,
    pub cpu_count: Option<u64>,
    pub enabled_cpus: Option<u64>,
    /// `Processes`: pid (as a string) → values aligned with `Attrs::process`.
    pub processes: Option<Dictionary>,
    pub end_mach_abs_time: Option<u64>,
}

impl SysmonMessage {
    pub fn is_empty(&self) -> bool {
        self.system.is_none()
            && self.system_cpu.is_none()
            && self.per_cpu.is_none()
            && self.processes.is_none()
    }
}

/// Merge every row dictionary of one message.
pub fn collect(data: &Value) -> SysmonMessage {
    let mut out = SysmonMessage::default();
    let rows: Vec<&Dictionary> = match data {
        Value::Array(a) => a.iter().filter_map(Value::as_dictionary).collect(),
        Value::Dictionary(d) => vec![d],
        _ => Vec::new(),
    };
    for row in rows {
        if let Some(v) = row.get("System").and_then(Value::as_array) {
            out.system = Some(v.clone());
        }
        if let Some(v) = row.get("SystemCPUUsage").and_then(Value::as_dictionary) {
            out.system_cpu = Some(v.clone());
        }
        if let Some(v) = row.get("PerCPUUsage").and_then(Value::as_array) {
            out.per_cpu = Some(v.clone());
        }
        if let Some(v) = get_uint(row, "CPUCount") {
            out.cpu_count = Some(v);
        }
        if let Some(v) = get_uint(row, "EnabledCPUs") {
            out.enabled_cpus = Some(v);
        }
        if let Some(v) = row.get("Processes").and_then(Value::as_dictionary) {
            out.processes = Some(v.clone());
        }
        if let Some(v) = get_uint(row, "EndMachAbsTime") {
            out.end_mach_abs_time = Some(v);
        }
    }
    out
}

/// The system attributes of one message by name.
pub fn system_map<'a>(attrs: &'a Attrs, msg: &'a SysmonMessage) -> HashMap<&'a str, &'a Value> {
    let mut map = HashMap::new();
    if let Some(values) = &msg.system {
        for (name, value) in attrs.system.iter().zip(values.iter()) {
            map.insert(name.as_str(), value);
        }
    }
    map
}

/// Cumulative counters kept between ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Counters {
    ts_ms: i64,
    net_rx: Option<u64>,
    net_tx: Option<u64>,
    pkt_rx: Option<u64>,
    pkt_tx: Option<u64>,
    disk_w: Option<u64>,
}

/// Delta state across ticks (one per session).
#[derive(Debug, Default)]
pub struct SysmonState {
    prev: Option<Counters>,
    base: Option<Counters>,
    last_processes_ms: Option<i64>,
}

/// What the other loops contribute to one tick.
#[derive(Debug, Default, Clone)]
pub struct TickContext {
    pub ram_total: Option<u64>,
    pub layout: Option<CoreLayout>,
    pub pid_net_bps: HashMap<u32, f64>,
    pub energy: HashMap<u32, String>,
    /// Emit the process table when at least this long has passed.
    pub process_period: Duration,
}

/// One tick's output.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SysmonTick {
    pub samples: Vec<MetricValue>,
    pub cores: Vec<CoreSample>,
    /// `None` when the table is not due this tick.
    pub processes: Option<Vec<ProcessRow>>,
    /// RAM from `physMemSize`, for the shared state.
    pub ram_total: Option<u64>,
}

/// Core kind for `index` of `total` cores under `layout`.
pub fn core_kind(index: usize, total: usize, layout: Option<CoreLayout>) -> CoreKind {
    let Some(l) = layout else {
        return CoreKind::Unknown;
    };
    let perf = l.performance as usize;
    if perf == 0 || perf > total {
        return CoreKind::Unknown;
    }
    let is_perf = if PERFORMANCE_CORES_LAST {
        index >= total - perf
    } else {
        index < perf
    };
    if is_perf {
        CoreKind::Performance
    } else {
        CoreKind::Efficiency
    }
}

fn pct(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite() && *x >= 0.0)
        .map(|x| x.clamp(0.0, 100.0))
}

fn first_num(map: &HashMap<&str, &Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| map.get(k).and_then(|v| num(v)))
}

fn first_uint(map: &HashMap<&str, &Value>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| map.get(k).and_then(|v| uint(v)))
}

/// Turn one message into metrics, cores and (when due) the process table.
pub fn parse_tick(
    msg: &SysmonMessage,
    attrs: &Attrs,
    state: &mut SysmonState,
    ts_ms: i64,
    ctx: &TickContext,
) -> SysmonTick {
    let sys = system_map(attrs, msg);
    let mut samples = Vec::new();
    let mut push = |id: MetricId, value: f64| {
        if value.is_finite() {
            samples.push(MetricValue { id, value });
        }
    };

    // CPU.
    let per_cpu: Vec<f64> = msg
        .per_cpu
        .as_deref()
        .or_else(|| {
            sys.get("PerCPUUsage")
                .and_then(|v| v.as_array())
                .map(Vec::as_slice)
        })
        .map(|cores| {
            cores
                .iter()
                .filter_map(|c| c.as_dictionary())
                .filter_map(|d| pct(get_num(d, "CPU_TotalLoad")))
                .collect()
        })
        .unwrap_or_default();
    let cpu = msg.system_cpu.as_ref();
    let total = cpu
        .and_then(|d| pct(get_num(d, "CPU_TotalLoad")))
        .or_else(|| pct(first_num(&sys, &["CPU_TotalLoad"])))
        .or_else(|| {
            (!per_cpu.is_empty()).then(|| per_cpu.iter().sum::<f64>() / per_cpu.len() as f64)
        });
    if let Some(t) = total {
        push(MetricId::CpuTotal, t);
    }
    if let Some(u) = cpu
        .and_then(|d| pct(get_num(d, "CPU_UserLoad")))
        .or_else(|| pct(first_num(&sys, &["CPU_UserLoad"])))
    {
        push(MetricId::CpuUser, u);
    }
    if let Some(s) = cpu
        .and_then(|d| pct(get_num(d, "CPU_SystemLoad")))
        .or_else(|| pct(first_num(&sys, &["CPU_SystemLoad"])))
    {
        push(MetricId::CpuSystem, s);
    }
    let n = per_cpu.len();
    let cores: Vec<CoreSample> = per_cpu
        .iter()
        .enumerate()
        .map(|(i, usage)| CoreSample {
            index: u16::try_from(i).unwrap_or(u16::MAX),
            kind: core_kind(i, n, ctx.layout),
            usage_pct: *usage as f32,
            freq_mhz: None,
            online: true,
        })
        .collect();

    // Memory.
    let page = first_uint(&sys, &["vmPageSize"])
        .filter(|p| *p >= 4096)
        .unwrap_or(DEFAULT_PAGE_SIZE);
    let pages = |keys: &[&str]| first_uint(&sys, keys).map(|p| p.saturating_mul(page));
    let ram_total = first_uint(&sys, &["physMemSize"]).filter(|b| *b >= (1 << 28));
    let total_mem = ctx.ram_total.or(ram_total);
    let free = pages(&["vmFreeCount"]).or_else(|| first_uint(&sys, &["PhysicalMemoryFree"]));
    let wired = pages(&["vmWireCount"]).or_else(|| first_uint(&sys, &["PhysicalMemoryWired"]));
    let app = pages(&["vmIntPageCount"]).or_else(|| first_uint(&sys, &["PhysicalMemoryActive"]));
    let cached =
        pages(&["vmExtPageCount"]).or_else(|| first_uint(&sys, &["PhysicalMemoryInactive"]));
    let compressed = pages(&["vmCompressorPageCount"])
        .or_else(|| first_uint(&sys, &["PhysicalMemoryCompressed"]));
    let swap = pages(&["vmSwapUsedCount"]);
    let used = match (app, wired, compressed) {
        (Some(a), Some(w), Some(c)) => Some(a + w + c),
        _ => match (total_mem, free) {
            (Some(t), Some(f)) => Some(t.saturating_sub(f)),
            _ => pages(&["vmUsedCount"]),
        },
    };
    if let Some(t) = total_mem {
        push(MetricId::MemTotal, t as f64);
    }
    for (id, v) in [
        (MetricId::MemUsed, used),
        (MetricId::MemFree, free),
        (MetricId::MemApp, app),
        (MetricId::MemWired, wired),
        (MetricId::MemCached, cached),
        (MetricId::MemCompressed, compressed),
        (MetricId::MemSwap, swap),
    ] {
        if let Some(v) = v {
            push(id, v as f64);
        }
    }

    // Counters → rates.
    let now = Counters {
        ts_ms,
        net_rx: first_uint(&sys, &["netBytesIn", "NetBytesIn"]),
        net_tx: first_uint(&sys, &["netBytesOut", "NetBytesOut"]),
        pkt_rx: first_uint(&sys, &["netPacketsIn", "NetPacketsIn"]),
        pkt_tx: first_uint(&sys, &["netPacketsOut", "NetPacketsOut"]),
        disk_w: first_uint(&sys, &["diskBytesWritten", "DiskBytesWritten"]),
    };
    if let Some(prev) = state.prev {
        let dt = (ts_ms - prev.ts_ms) as f64 / 1000.0;
        if dt > 0.05 {
            let rate =
                |p: Option<u64>, n: Option<u64>| n.and_then(|n| delta(p, n)).map(|d| d as f64 / dt);
            if let Some(r) = rate(prev.net_rx, now.net_rx) {
                push(MetricId::NetRxBps, r * 8.0);
            }
            if let Some(r) = rate(prev.net_tx, now.net_tx) {
                push(MetricId::NetTxBps, r * 8.0);
            }
            if let Some(r) = rate(prev.pkt_rx, now.pkt_rx) {
                push(MetricId::NetRxPps, r);
            }
            if let Some(r) = rate(prev.pkt_tx, now.pkt_tx) {
                push(MetricId::NetTxPps, r);
            }
            if let Some(r) = rate(prev.disk_w, now.disk_w) {
                push(MetricId::DiskWriteBps, r);
            }
        }
    }
    let base = *state.base.get_or_insert(now);
    if let Some(d) = now.net_rx.and_then(|n| delta(base.net_rx, n)) {
        push(MetricId::NetRxTotal, d as f64);
    }
    if let Some(d) = now.net_tx.and_then(|n| delta(base.net_tx, n)) {
        push(MetricId::NetTxTotal, d as f64);
    }
    state.prev = Some(now);

    // Processes.
    let mut rows = msg
        .processes
        .as_ref()
        .map(|p| parse_processes(p, attrs, ctx))
        .unwrap_or_default();
    if msg.processes.is_some() {
        push(MetricId::ProcCount, rows.len() as f64);
    }
    let due = state.last_processes_ms.is_none_or(|last| {
        ts_ms - last >= i64::try_from(ctx.process_period.as_millis()).unwrap_or(0)
    });
    let processes = if !rows.is_empty() && due {
        state.last_processes_ms = Some(ts_ms);
        rows.sort_by(|a, b| {
            b.cpu_pct
                .unwrap_or(0.0)
                .partial_cmp(&a.cpu_pct.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.mem_bytes.unwrap_or(0).cmp(&a.mem_bytes.unwrap_or(0)))
        });
        rows.truncate(MAX_PROCESS_ROWS);
        Some(rows)
    } else {
        None
    };

    SysmonTick {
        samples,
        cores,
        processes,
        ram_total,
    }
}

/// The process table of one message (unsorted).
pub fn parse_processes(
    processes: &Dictionary,
    attrs: &Attrs,
    ctx: &TickContext,
) -> Vec<ProcessRow> {
    let idx = |name: &str| attrs.process.iter().position(|a| a == name);
    let i_pid = idx("pid");
    let i_name = idx("name");
    let i_cpu = idx("cpuUsage");
    let i_foot = idx("physFootprint");
    let i_res = idx("memResidentSize");
    let i_thr = idx("threadCount");
    let i_bundle = idx("bundleID").or_else(|| idx("bundleIdentifier"));
    let mut rows = Vec::with_capacity(processes.len());
    for (key, value) in processes {
        let values: Vec<Value> = match value {
            Value::Array(a) => a.clone(),
            Value::Dictionary(d) => attrs
                .process
                .iter()
                .map(|a| d.get(a).cloned().unwrap_or(Value::String(String::new())))
                .collect(),
            _ => continue,
        };
        let at = |i: Option<usize>| i.and_then(|i| values.get(i));
        let pid = at(i_pid)
            .and_then(uint)
            .or_else(|| key.trim().parse().ok())
            .and_then(|p| u32::try_from(p).ok());
        let Some(pid) = pid else { continue };
        let name = at(i_name)
            .and_then(|v| v.as_string())
            .map(str::to_owned)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("pid {pid}"));
        let cpu_pct = at(i_cpu)
            .and_then(num)
            .filter(|c| c.is_finite() && *c >= 0.0)
            .map(|c| (c * 100.0) as f32);
        let mem_bytes = at(i_foot)
            .and_then(uint)
            .filter(|b| *b > 0)
            .or_else(|| at(i_res).and_then(uint));
        let threads = at(i_thr).and_then(uint).and_then(|t| u32::try_from(t).ok());
        let app_id = at(i_bundle)
            .and_then(|v| v.as_string())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        rows.push(ProcessRow {
            pid,
            name,
            app_id,
            cpu_pct,
            mem_bytes,
            threads,
            net_bps: ctx.pid_net_bps.get(&pid).copied(),
            state: ProcessState::Unknown,
            energy: ctx.energy.get(&pid).cloned(),
        });
    }
    rows
}

/// Ask deviceinfo for the attribute lists (empty on failure, never fatal).
pub async fn query_attributes(
    hub: &mut DvtHub,
) -> Result<(Vec<String>, Vec<String>), IdeviceError> {
    let mut di = idevice::dvt::device_info::DeviceInfoClient::new(hub).await?;
    let system = di.sysmon_system_attributes().await.unwrap_or_else(|e| {
        tracing::debug!(error = %e, "sysmonSystemAttributes failed; using the fallback list");
        Vec::new()
    });
    let process = di.sysmon_process_attributes().await.unwrap_or_else(|e| {
        tracing::debug!(error = %e, "sysmonProcessAttributes failed; using the fallback list");
        Vec::new()
    });
    Ok((system, process))
}

/// The sysmontap loop: one hub connection, attribute discovery, `setConfig:`,
/// `start`, then one tick per message until cancel, sink close or the
/// tunnel dies. `medium` is the process-table cadence.
pub async fn run(
    tunnel: std::sync::Arc<Tunnel>,
    sink: EventSink,
    device_id: String,
    fast: Duration,
    medium: Duration,
    shared: SharedState,
    cancel: CancellationToken,
) -> LoopEnd {
    let mut hub = match tunnel.dvt().await {
        Ok(h) => h,
        Err(e) => return LoopEnd::TunnelDead(e.to_string()),
    };
    let (sys_avail, proc_avail) = match query_attributes(&mut hub).await {
        Ok(lists) => lists,
        Err(e) => return LoopEnd::from_error(&tunnel, "deviceinfo", &e),
    };
    let attrs = Attrs::select(&sys_avail, &proc_avail);
    tracing::info!(
        device = %device_id,
        system = ?attrs.system,
        process = ?attrs.process,
        advertised_system = sys_avail.len(),
        advertised_process = proc_avail.len(),
        "sysmontap attributes selected"
    );
    let mut channel = match hub.make_channel(SERVICE).await {
        Ok(c) => c,
        Err(e) => return LoopEnd::from_error(&tunnel, "sysmontap channel", &e),
    };
    let interval = fast.max(Duration::from_millis(250));
    if let Err(e) = channel
        .call_method(
            Some(Value::String("setConfig:".into())),
            Some(vec![AuxValue::archived_value(Value::Dictionary(
                attrs.config(interval),
            ))]),
            false,
        )
        .await
    {
        return LoopEnd::from_error(&tunnel, "sysmontap setConfig", &e);
    }
    if let Err(e) = channel
        .call_method(Some(Value::String("start".into())), None, false)
        .await
    {
        return LoopEnd::from_error(&tunnel, "sysmontap start", &e);
    }

    let mut state = SysmonState::default();
    let idle = interval.saturating_mul(10).max(Duration::from_secs(15));
    loop {
        let msg = tokio::select! {
            _ = cancel.cancelled() => {
                let _ = channel.call_method(Some(Value::String("stop".into())), None, false).await;
                return LoopEnd::Cancelled;
            }
            m = tokio::time::timeout(idle, channel.read_message()) => match m {
                Ok(Ok(m)) => m,
                Ok(Err(e)) => return LoopEnd::from_error(&tunnel, "sysmontap read", &e),
                Err(_) => {
                    tunnel.mark_dead("sysmontap: no message for a long time");
                    return LoopEnd::TunnelDead("sysmontap: no message for a long time".to_owned());
                }
            }
        };
        if sink.is_closed() {
            return LoopEnd::SinkClosed;
        }
        let Some(data) = msg.data.as_ref() else {
            continue;
        };
        let parsed = collect(data);
        if parsed.is_empty() {
            continue;
        }
        let ts = now_ms();
        let ctx = {
            let s = shared.read();
            TickContext {
                ram_total: s.ram_total,
                layout: s.layout,
                pid_net_bps: s.pid_net_bps.clone(),
                energy: s.energy.clone(),
                process_period: medium,
            }
        };
        let tick = parse_tick(&parsed, &attrs, &mut state, ts, &ctx);
        {
            let mut s = shared.write();
            if s.ram_total.is_none() && tick.ram_total.is_some() {
                s.ram_total = tick.ram_total;
            }
            if let Some(rows) = &tick.processes {
                s.top_pids = rows.iter().take(5).map(|r| r.pid).collect();
            }
        }
        if !tick.samples.is_empty() && sink.samples(&device_id, ts, tick.samples).await.is_err() {
            return LoopEnd::SinkClosed;
        }
        if !tick.cores.is_empty() && sink.cores(&device_id, ts, tick.cores).await.is_err() {
            return LoopEnd::SinkClosed;
        }
        if let Some(rows) = tick.processes
            && sink.processes(&device_id, ts, rows).await.is_err()
        {
            return LoopEnd::SinkClosed;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn s(v: &str) -> String {
        v.to_owned()
    }

    #[test]
    fn choose_keeps_wanted_order_and_falls_back() {
        let avail = vec![
            s("vmFreeCount"),
            s("physMemSize"),
            s("bogus"),
            s("netBytesIn"),
        ];
        assert_eq!(
            choose(&avail, WANTED_SYSTEM, FALLBACK_SYSTEM),
            vec![s("physMemSize"), s("vmFreeCount"), s("netBytesIn")]
        );
        assert_eq!(
            choose(&[], WANTED_SYSTEM, FALLBACK_SYSTEM),
            FALLBACK_SYSTEM.iter().map(|x| s(x)).collect::<Vec<_>>()
        );
        let a = Attrs::select(&avail, &[s("pid"), s("cpuUsage"), s("name")]);
        assert_eq!(a.process, vec![s("pid"), s("name"), s("cpuUsage")]);
        let cfg = a.config(Duration::from_secs(1));
        assert_eq!(cfg.get("ur").and_then(uint), Some(1000));
        assert_eq!(
            cfg.get("sampleInterval").and_then(uint),
            Some(1_000_000_000)
        );
        assert_eq!(
            cfg.get("sysAttrs")
                .and_then(|v| v.as_array())
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn core_kinds_put_performance_last() {
        let a14 = Some(CoreLayout {
            total: 6,
            performance: 2,
            efficiency: 4,
        });
        let kinds: Vec<CoreKind> = (0..6).map(|i| core_kind(i, 6, a14)).collect();
        assert_eq!(
            kinds,
            vec![
                CoreKind::Efficiency,
                CoreKind::Efficiency,
                CoreKind::Efficiency,
                CoreKind::Efficiency,
                CoreKind::Performance,
                CoreKind::Performance
            ]
        );
        assert_eq!(core_kind(0, 6, None), CoreKind::Unknown);
        // A layout that does not fit the reported core count is not trusted.
        assert_eq!(
            core_kind(
                0,
                2,
                Some(CoreLayout {
                    total: 6,
                    performance: 4,
                    efficiency: 2
                })
            ),
            CoreKind::Unknown
        );
    }
}
