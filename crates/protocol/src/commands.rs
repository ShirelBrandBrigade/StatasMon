//! Requests from the UI to the core (Tauri command payloads).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::device::DeviceId;
use crate::logs::LogLevel;
use crate::metrics::MetricId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SamplingConfig {
    /// Cheap sources (proc, sysmontap): default 1000.
    pub fast_ms: u32,
    /// Battery, processes, Wi-Fi: default 3000.
    pub medium_ms: u32,
    /// Thermal service, netstats, disk stats: default 15000.
    pub slow_ms: u32,
    /// Opt-in expensive snapshots (dumpsys meminfo): 0 disables.
    pub heavy_ms: u32,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            fast_ms: 1000,
            medium_ms: 3000,
            slow_ms: 15000,
            heavy_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogFilter {
    pub device_id: DeviceId,
    /// Process names or app ids; empty = all.
    pub processes: Vec<String>,
    /// Empty = all levels.
    pub levels: Vec<LogLevel>,
    pub query: Option<String>,
    pub regex: bool,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    /// Android buffers; empty = all.
    pub buffers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogPage {
    pub total: u64,
    /// Sequence number of the first entry returned.
    pub first_seq: u64,
    pub entries: Vec<crate::logs::LogEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HistoryQuery {
    pub device_id: DeviceId,
    pub metric: MetricId,
    pub from_ms: i64,
    pub to_ms: i64,
    /// Downsample step; 0 = raw.
    pub step_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HistoryPoint {
    pub ts_ms: i64,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExportFormat {
    Log,
    Jsonl,
    Csv,
    Bundle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "scope", rename_all = "snake_case")]
#[ts(export)]
pub enum ExportScope {
    Session,
    Range { from_ms: i64, to_ms: i64 },
    Filter { filter: LogFilter },
    App { app_id: String },
    Selection { seqs: Vec<u64> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportRequest {
    pub device_id: DeviceId,
    pub session_id: Option<String>,
    pub format: ExportFormat,
    pub scope: ExportScope,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum DeviceAction {
    PcapStart {
        path: String,
    },
    PcapStop,
    FetchCrashReports,
    Bugreport {
        path: String,
    },
    RefreshInfo,
    /// Wireless ADB pairing (Android 11+).
    AdbPair {
        host: String,
        port: u16,
        code: String,
    },
    AdbConnect {
        host: String,
        port: u16,
    },
}
