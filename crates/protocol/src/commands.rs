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

/// Retention policy of the session store (docs/PLAN.md §3.9, ADR-012).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RetentionConfig {
    /// A session is split past this many hours of recording: default 4.
    pub max_session_hours: u32,
    /// Ended sessions older than this many days are deleted: default 30.
    pub keep_days: u32,
    /// Optional cap on the total size of all sessions, in GB.
    pub max_total_gb: Option<f64>,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_session_hours: 4,
            keep_days: 30,
            max_total_gb: None,
        }
    }
}

/// User settings persisted by the app (`settings.json` in the app data
/// folder, docs/PLAN.md 6.6, ADR-014).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    /// Sampling cadence handed to devices when they start.
    pub sampling: SamplingConfig,
    pub retention: RetentionConfig,
    /// Explicit adb binary; `None` lets the locator search.
    pub adb_path: Option<String>,
    /// Opt-in update check on launch (off by default; no network otherwise).
    pub update_check: bool,
    /// Reserved: only "dark" exists today.
    pub theme: String,
    /// Settings file format version.
    pub version: u32,
}

/// Current settings file format version.
pub const SETTINGS_VERSION: u32 = 1;

impl Default for Settings {
    fn default() -> Self {
        Self {
            sampling: SamplingConfig::default(),
            retention: RetentionConfig::default(),
            adb_path: None,
            update_check: false,
            theme: "dark".to_owned(),
            version: SETTINGS_VERSION,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogFilter {
    pub device_id: DeviceId,
    /// A stored session to query instead of the live device.
    pub session_id: Option<String>,
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
    /// A stored session to query instead of the live ring buffer.
    pub session_id: Option<String>,
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

/// One row of the Sessions screen (docs/FEATURES.md §0, §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionSummary {
    pub id: String,
    pub device_id: DeviceId,
    pub device_name: String,
    pub model_name: String,
    pub platform: crate::device::Platform,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub elapsed_ms: i64,
    pub sample_count: u64,
    pub log_count: u64,
    pub event_count: u64,
    pub mark_count: u64,
    pub size_bytes: u64,
    /// True while the device is still recording into this session.
    pub live: bool,
}

/// A stored event row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StoredEvent {
    pub ts_ms: i64,
    pub kind: crate::message::EventKind,
    pub detail: String,
    pub pid: Option<u32>,
    pub process: Option<String>,
}

/// Everything needed to open a stored session in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub info: Option<crate::device::DeviceInfo>,
    pub marks: Vec<crate::status::Mark>,
    pub events: Vec<StoredEvent>,
    /// Metrics that have at least one sample in this session.
    pub metrics: Vec<MetricId>,
}

/// Result of an export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExportResult {
    pub path: String,
    pub bytes: u64,
    pub entries: u64,
}
