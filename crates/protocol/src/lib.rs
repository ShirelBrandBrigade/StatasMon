//! StatasMon protocol: every value that crosses the core ↔ UI boundary.
//!
//! Rules (docs/PLAN.md §3.5): adding a field or metric means changing this
//! crate, regenerating `packages/protocol/generated` (`pnpm protocol:gen`),
//! updating the fake device, the tests and the docs. JSON is the wire
//! format; TypeScript types are generated with `ts-rs` and must never be
//! hand-edited.

pub mod capabilities;
pub mod commands;
pub mod device;
pub mod logs;
pub mod message;
pub mod metrics;
pub mod status;

pub use capabilities::{Availability, Capability, CapabilityEntry, CapabilitySet};
pub use commands::{
    DeviceAction, ExportFormat, ExportRequest, ExportResult, ExportScope, HistoryPoint,
    HistoryQuery, LogFilter, LogPage, SamplingConfig, SessionDetail, SessionSummary, StoredEvent,
};
pub use device::{
    ConnectionInfo, ConnectionState, DeviceHardware, DeviceId, DeviceIdentity, DeviceInfo,
    DeviceSoftware, DeviceState, DeviceSummary, Platform, ToolVersion, Transport,
};
pub use logs::{LogEntry, LogLevel};
pub use message::{
    CoreKind, CoreSample, ErrorCode, EventKind, Message, MetricValue, ProcessRow, ProcessState,
};
pub use metrics::{MetricId, MetricKind, MetricMeta, Unit};
pub use status::{Headline, Mark, Pill, SessionState, SessionStatus, Severity, Tone};

/// Wire-format version. Bump on any breaking change to a serialized shape.
pub const PROTOCOL_VERSION: u32 = 1;
