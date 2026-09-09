//! Messages from the core to the UI (docs/PLAN.md §3.5). One enum, tagged
//! by `type`, so the UI can switch on it exhaustively.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::capabilities::CapabilitySet;
use crate::device::{ConnectionState, DeviceId, DeviceInfo, DeviceSummary};
use crate::logs::LogEntry;
use crate::metrics::MetricId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricValue {
    pub id: MetricId,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoreKind {
    Performance,
    Efficiency,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoreSample {
    pub index: u16,
    pub kind: CoreKind,
    pub usage_pct: f32,
    pub freq_mhz: Option<u32>,
    /// Offline cores report `false` and no usage.
    pub online: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ProcessState {
    Foreground,
    Background,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    /// Bundle id (iOS) or package (Android) when the process belongs to an app.
    pub app_id: Option<String>,
    pub cpu_pct: Option<f32>,
    pub mem_bytes: Option<u64>,
    pub threads: Option<u32>,
    pub net_bps: Option<f64>,
    pub state: ProcessState,
    /// Free-form energy impact label where the platform provides one.
    pub energy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EventKind {
    ThermalChange,
    MemoryWarning,
    LmkKill,
    Jetsam,
    Crash,
    Anr,
    AppLaunch,
    AppExit,
    Mark,
    Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCode {
    DriverMissing,
    NotTrusted,
    DeveloperModeOff,
    TunnelFailed,
    AdbNotFound,
    AdbConflict,
    DeviceLost,
    Io,
    Parse,
    Unsupported,
    Internal,
}

/// Everything the core sends to the UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Message {
    DeviceAttached {
        device: DeviceSummary,
    },
    DeviceDetached {
        device_id: DeviceId,
    },
    DeviceState {
        device_id: DeviceId,
        state: ConnectionState,
        detail: Option<String>,
    },
    Capabilities {
        device_id: DeviceId,
        set: CapabilitySet,
    },
    Info {
        device_id: DeviceId,
        info: Box<DeviceInfo>,
    },
    /// One tick of scalar metrics.
    Samples {
        device_id: DeviceId,
        ts_ms: i64,
        values: Vec<MetricValue>,
    },
    Cores {
        device_id: DeviceId,
        ts_ms: i64,
        cores: Vec<CoreSample>,
    },
    Processes {
        device_id: DeviceId,
        ts_ms: i64,
        rows: Vec<ProcessRow>,
    },
    /// Batched log lines, at most 500 per message.
    Log {
        device_id: DeviceId,
        entries: Vec<LogEntry>,
    },
    Event {
        device_id: DeviceId,
        ts_ms: i64,
        kind: EventKind,
        detail: String,
        pid: Option<u32>,
        process: Option<String>,
    },
    Progress {
        task_id: String,
        pct: f32,
        message: String,
    },
    Error {
        device_id: Option<DeviceId>,
        code: ErrorCode,
        message: String,
        hint: Option<String>,
    },
}

impl Message {
    pub fn device_id(&self) -> Option<&str> {
        match self {
            Message::DeviceAttached { device } => Some(&device.id),
            Message::DeviceDetached { device_id }
            | Message::DeviceState { device_id, .. }
            | Message::Capabilities { device_id, .. }
            | Message::Info { device_id, .. }
            | Message::Samples { device_id, .. }
            | Message::Cores { device_id, .. }
            | Message::Processes { device_id, .. }
            | Message::Log { device_id, .. }
            | Message::Event { device_id, .. } => Some(device_id),
            Message::Progress { .. } => None,
            Message::Error { device_id, .. } => device_id.as_deref(),
        }
    }
}
