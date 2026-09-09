//! StatasMon `device` crate: the contract every device provider implements.
//!
//! Role (docs/PLAN.md §3.3, §3.4): this crate owns the [`DeviceSource`]
//! trait, the [`Discovery`] trait that finds devices and hands them to the
//! engine, the [`EventSink`] providers push protocol [`Message`]s into, the
//! provider error type [`DeviceError`] (always carrying a user-facing hint
//! and a protocol [`statasmon_protocol::ErrorCode`]), and small helpers
//! shared by all providers (`parse`, `catalog`).
//!
//! It contains no platform code. iOS, Android and the fake device live in
//! their own crates and depend on this one; `statasmon-core` consumes the
//! traits and never talks to a platform library directly.
//!
//! [`Message`]: statasmon_protocol::Message

pub mod catalog;
pub mod error;
pub mod parse;
pub mod sink;
pub mod source;

pub use catalog::{allowed_metrics, metric_capability};
pub use error::{DeviceError, DeviceResult};
pub use parse::now_ms;
pub use sink::{EventSink, MAX_LOG_BATCH, SinkClosed};
pub use source::{ActionResult, DeviceSource, Discovery, DiscoveryEvent, DiscoverySink};
