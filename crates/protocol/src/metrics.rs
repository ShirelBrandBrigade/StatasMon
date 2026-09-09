//! The closed metric catalog (docs/PLAN.md §3.5). Every sampled number has
//! a `MetricId`; the catalog carries its unit, label and kind so the UI
//! formats every value the same way.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum MetricId {
    #[serde(rename = "cpu.total")]
    CpuTotal,
    #[serde(rename = "cpu.user")]
    CpuUser,
    #[serde(rename = "cpu.system")]
    CpuSystem,
    #[serde(rename = "cpu.load1")]
    CpuLoad1,
    #[serde(rename = "cpu.load5")]
    CpuLoad5,
    #[serde(rename = "mem.total")]
    MemTotal,
    #[serde(rename = "mem.used")]
    MemUsed,
    #[serde(rename = "mem.free")]
    MemFree,
    #[serde(rename = "mem.app")]
    MemApp,
    #[serde(rename = "mem.wired")]
    MemWired,
    #[serde(rename = "mem.compressed")]
    MemCompressed,
    #[serde(rename = "mem.cached")]
    MemCached,
    #[serde(rename = "mem.swap")]
    MemSwap,
    /// 0 normal · 1 warning · 2 urgent · 3 critical (platform raw kept in events).
    #[serde(rename = "mem.pressure")]
    MemPressure,
    /// Common severity 0 nominal · 1 fair/light · 2 serious/moderate-severe · 3 critical.
    #[serde(rename = "thermal.state")]
    ThermalState,
    /// Platform raw level (iOS 0–3, Android 0–6).
    #[serde(rename = "thermal.raw")]
    ThermalRaw,
    #[serde(rename = "temp.battery")]
    TempBattery,
    #[serde(rename = "temp.soc")]
    TempSoc,
    #[serde(rename = "temp.skin")]
    TempSkin,
    #[serde(rename = "temp.pmu")]
    TempPmu,
    #[serde(rename = "temp.display")]
    TempDisplay,
    #[serde(rename = "bat.percent")]
    BatPercent,
    #[serde(rename = "bat.voltage")]
    BatVoltage,
    /// Signed: positive while charging.
    #[serde(rename = "bat.current")]
    BatCurrent,
    #[serde(rename = "bat.power")]
    BatPower,
    #[serde(rename = "bat.temp")]
    BatTemp,
    #[serde(rename = "bat.cycles")]
    BatCycles,
    /// Full-charge capacity as % of design capacity.
    #[serde(rename = "bat.health")]
    BatHealth,
    #[serde(rename = "bat.charging")]
    BatCharging,
    #[serde(rename = "bat.design_capacity")]
    BatDesignCapacity,
    #[serde(rename = "bat.full_capacity")]
    BatFullCapacity,
    #[serde(rename = "disk.total")]
    DiskTotal,
    #[serde(rename = "disk.used")]
    DiskUsed,
    #[serde(rename = "disk.free")]
    DiskFree,
    #[serde(rename = "disk.apps")]
    DiskApps,
    #[serde(rename = "disk.media")]
    DiskMedia,
    #[serde(rename = "disk.system")]
    DiskSystem,
    #[serde(rename = "disk.other")]
    DiskOther,
    #[serde(rename = "disk.write_bps")]
    DiskWriteBps,
    #[serde(rename = "net.rx_bps")]
    NetRxBps,
    #[serde(rename = "net.tx_bps")]
    NetTxBps,
    #[serde(rename = "net.rx_pps")]
    NetRxPps,
    #[serde(rename = "net.tx_pps")]
    NetTxPps,
    #[serde(rename = "net.rx_total")]
    NetRxTotal,
    #[serde(rename = "net.tx_total")]
    NetTxTotal,
    #[serde(rename = "net.rssi")]
    NetRssi,
    #[serde(rename = "net.link_mbps")]
    NetLinkMbps,
    #[serde(rename = "net.connections")]
    NetConnections,
    #[serde(rename = "net.retransmits")]
    NetRetransmits,
    #[serde(rename = "net.dropped")]
    NetDropped,
    #[serde(rename = "net.latency_ms")]
    NetLatencyMs,
    #[serde(rename = "proc.count")]
    ProcCount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Unit {
    Percent,
    Bytes,
    BytesPerSec,
    BitsPerSec,
    PacketsPerSec,
    Count,
    Celsius,
    Volts,
    Amps,
    Watts,
    MilliampHours,
    Seconds,
    Milliseconds,
    Dbm,
    Mbps,
    /// Enumerated state; see the metric's doc for the mapping.
    State,
    Load,
    Bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MetricKind {
    Gauge,
    Rate,
    Counter,
    State,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricMeta {
    pub id: MetricId,
    pub label: &'static str,
    pub unit: Unit,
    pub kind: MetricKind,
    /// Group used by the UI: cpu, mem, thermal, battery, disk, net, proc.
    pub group: &'static str,
}

impl MetricId {
    pub const ALL: [MetricId; 52] = [
        MetricId::CpuTotal,
        MetricId::CpuUser,
        MetricId::CpuSystem,
        MetricId::CpuLoad1,
        MetricId::CpuLoad5,
        MetricId::MemTotal,
        MetricId::MemUsed,
        MetricId::MemFree,
        MetricId::MemApp,
        MetricId::MemWired,
        MetricId::MemCompressed,
        MetricId::MemCached,
        MetricId::MemSwap,
        MetricId::MemPressure,
        MetricId::ThermalState,
        MetricId::ThermalRaw,
        MetricId::TempBattery,
        MetricId::TempSoc,
        MetricId::TempSkin,
        MetricId::TempPmu,
        MetricId::TempDisplay,
        MetricId::BatPercent,
        MetricId::BatVoltage,
        MetricId::BatCurrent,
        MetricId::BatPower,
        MetricId::BatTemp,
        MetricId::BatCycles,
        MetricId::BatHealth,
        MetricId::BatCharging,
        MetricId::BatDesignCapacity,
        MetricId::BatFullCapacity,
        MetricId::DiskTotal,
        MetricId::DiskUsed,
        MetricId::DiskFree,
        MetricId::DiskApps,
        MetricId::DiskMedia,
        MetricId::DiskSystem,
        MetricId::DiskOther,
        MetricId::DiskWriteBps,
        MetricId::NetRxBps,
        MetricId::NetTxBps,
        MetricId::NetRxPps,
        MetricId::NetTxPps,
        MetricId::NetRxTotal,
        MetricId::NetTxTotal,
        MetricId::NetRssi,
        MetricId::NetLinkMbps,
        MetricId::NetConnections,
        MetricId::NetRetransmits,
        MetricId::NetDropped,
        MetricId::NetLatencyMs,
        MetricId::ProcCount,
    ];

    /// Wire name ("cpu.total"). Matches the serde rename.
    pub fn as_str(self) -> &'static str {
        self.meta().wire
    }

    pub fn meta(self) -> Meta {
        use MetricId as M;
        use MetricKind as K;
        use Unit as U;
        let m = |wire, label, unit, kind, group| Meta {
            wire,
            label,
            unit,
            kind,
            group,
        };
        match self {
            M::CpuTotal => m("cpu.total", "CPU", U::Percent, K::Gauge, "cpu"),
            M::CpuUser => m("cpu.user", "CPU user", U::Percent, K::Gauge, "cpu"),
            M::CpuSystem => m("cpu.system", "CPU system", U::Percent, K::Gauge, "cpu"),
            M::CpuLoad1 => m("cpu.load1", "Load 1 min", U::Load, K::Gauge, "cpu"),
            M::CpuLoad5 => m("cpu.load5", "Load 5 min", U::Load, K::Gauge, "cpu"),
            M::MemTotal => m("mem.total", "Memory total", U::Bytes, K::Gauge, "mem"),
            M::MemUsed => m("mem.used", "Memory used", U::Bytes, K::Gauge, "mem"),
            M::MemFree => m("mem.free", "Memory free", U::Bytes, K::Gauge, "mem"),
            M::MemApp => m("mem.app", "App memory", U::Bytes, K::Gauge, "mem"),
            M::MemWired => m("mem.wired", "Wired", U::Bytes, K::Gauge, "mem"),
            M::MemCompressed => m("mem.compressed", "Compressed", U::Bytes, K::Gauge, "mem"),
            M::MemCached => m("mem.cached", "Cached", U::Bytes, K::Gauge, "mem"),
            M::MemSwap => m("mem.swap", "Swap / ZRAM", U::Bytes, K::Gauge, "mem"),
            M::MemPressure => m("mem.pressure", "Memory pressure", U::State, K::State, "mem"),
            M::ThermalState => m(
                "thermal.state",
                "Thermal state",
                U::State,
                K::State,
                "thermal",
            ),
            M::ThermalRaw => m(
                "thermal.raw",
                "Thermal level (raw)",
                U::State,
                K::State,
                "thermal",
            ),
            M::TempBattery => m(
                "temp.battery",
                "Battery temperature",
                U::Celsius,
                K::Gauge,
                "thermal",
            ),
            M::TempSoc => m(
                "temp.soc",
                "SoC temperature",
                U::Celsius,
                K::Gauge,
                "thermal",
            ),
            M::TempSkin => m(
                "temp.skin",
                "Skin temperature",
                U::Celsius,
                K::Gauge,
                "thermal",
            ),
            M::TempPmu => m(
                "temp.pmu",
                "Power management temperature",
                U::Celsius,
                K::Gauge,
                "thermal",
            ),
            M::TempDisplay => m(
                "temp.display",
                "Display temperature",
                U::Celsius,
                K::Gauge,
                "thermal",
            ),
            M::BatPercent => m("bat.percent", "Battery", U::Percent, K::Gauge, "battery"),
            M::BatVoltage => m("bat.voltage", "Voltage", U::Volts, K::Gauge, "battery"),
            M::BatCurrent => m("bat.current", "Current", U::Amps, K::Gauge, "battery"),
            M::BatPower => m("bat.power", "Power", U::Watts, K::Gauge, "battery"),
            M::BatTemp => m(
                "bat.temp",
                "Battery temperature",
                U::Celsius,
                K::Gauge,
                "battery",
            ),
            M::BatCycles => m("bat.cycles", "Cycle count", U::Count, K::Gauge, "battery"),
            M::BatHealth => m(
                "bat.health",
                "Battery health",
                U::Percent,
                K::Gauge,
                "battery",
            ),
            M::BatCharging => m("bat.charging", "Charging", U::Bool, K::State, "battery"),
            M::BatDesignCapacity => m(
                "bat.design_capacity",
                "Design capacity",
                U::MilliampHours,
                K::Gauge,
                "battery",
            ),
            M::BatFullCapacity => m(
                "bat.full_capacity",
                "Full-charge capacity",
                U::MilliampHours,
                K::Gauge,
                "battery",
            ),
            M::DiskTotal => m("disk.total", "Storage total", U::Bytes, K::Gauge, "disk"),
            M::DiskUsed => m("disk.used", "Storage used", U::Bytes, K::Gauge, "disk"),
            M::DiskFree => m("disk.free", "Storage free", U::Bytes, K::Gauge, "disk"),
            M::DiskApps => m("disk.apps", "Apps", U::Bytes, K::Gauge, "disk"),
            M::DiskMedia => m("disk.media", "Media", U::Bytes, K::Gauge, "disk"),
            M::DiskSystem => m("disk.system", "System", U::Bytes, K::Gauge, "disk"),
            M::DiskOther => m("disk.other", "Other", U::Bytes, K::Gauge, "disk"),
            M::DiskWriteBps => m(
                "disk.write_bps",
                "Write rate",
                U::BytesPerSec,
                K::Rate,
                "disk",
            ),
            M::NetRxBps => m("net.rx_bps", "Download", U::BitsPerSec, K::Rate, "net"),
            M::NetTxBps => m("net.tx_bps", "Upload", U::BitsPerSec, K::Rate, "net"),
            M::NetRxPps => m("net.rx_pps", "Packets in", U::PacketsPerSec, K::Rate, "net"),
            M::NetTxPps => m(
                "net.tx_pps",
                "Packets out",
                U::PacketsPerSec,
                K::Rate,
                "net",
            ),
            M::NetRxTotal => m("net.rx_total", "Downloaded", U::Bytes, K::Counter, "net"),
            M::NetTxTotal => m("net.tx_total", "Uploaded", U::Bytes, K::Counter, "net"),
            M::NetRssi => m("net.rssi", "Signal", U::Dbm, K::Gauge, "net"),
            M::NetLinkMbps => m("net.link_mbps", "Link speed", U::Mbps, K::Gauge, "net"),
            M::NetConnections => m("net.connections", "Connections", U::Count, K::Gauge, "net"),
            M::NetRetransmits => m(
                "net.retransmits",
                "Retransmits",
                U::Count,
                K::Counter,
                "net",
            ),
            M::NetDropped => m("net.dropped", "Dropped", U::Count, K::Counter, "net"),
            M::NetLatencyMs => m(
                "net.latency_ms",
                "Latency",
                U::Milliseconds,
                K::Gauge,
                "net",
            ),
            M::ProcCount => m("proc.count", "Processes", U::Count, K::Gauge, "proc"),
        }
    }

    /// The catalog as sent to the UI once at startup.
    pub fn catalog() -> Vec<MetricMeta> {
        Self::ALL
            .iter()
            .map(|id| {
                let m = id.meta();
                MetricMeta {
                    id: *id,
                    label: m.label,
                    unit: m.unit,
                    kind: m.kind,
                    group: m.group,
                }
            })
            .collect()
    }
}

/// Static metadata (not serialized directly; see [`MetricMeta`]).
#[derive(Debug, Clone, Copy)]
pub struct Meta {
    pub wire: &'static str,
    pub label: &'static str,
    pub unit: Unit,
    pub kind: MetricKind,
    pub group: &'static str,
}
