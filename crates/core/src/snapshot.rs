//! What the UI reads when it polls a device (≤ 2 Hz): the latest of
//! everything plus the headline and the session.

use serde::{Deserialize, Serialize};
use statasmon_protocol::{
    CapabilitySet, CoreSample, DeviceInfo, DeviceSummary, Headline, MetricValue, ProcessRow,
    SessionState,
};

/// The current state of one device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceSnapshot {
    /// Sidebar summary.
    pub summary: DeviceSummary,
    /// Full information sheet, once received.
    pub info: Option<DeviceInfo>,
    /// Capabilities with reasons for gaps.
    pub capabilities: CapabilitySet,
    /// Newest value per metric (updates even while paused).
    pub latest: Vec<MetricValue>,
    /// Timestamp of the newest sample tick, for showing the age of values.
    pub latest_ts_ms: Option<i64>,
    /// Newest per-core tick.
    pub cores: Vec<CoreSample>,
    /// Newest process table.
    pub processes: Vec<ProcessRow>,
    /// Headline and pills from the rules.
    pub headline: Headline,
    /// Recording state.
    pub session: SessionState,
}
