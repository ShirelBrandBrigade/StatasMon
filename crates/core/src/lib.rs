//! StatasMon `core` crate: the session engine.
//!
//! Role (docs/PLAN.md §3.3, §3.6, §3.11, step 0.4). The [`Engine`] owns
//! every connected [`DeviceSource`], drives discovery, and turns the
//! provider stream into three things the rest of the app consumes:
//!
//! 1. a **bus** (`tokio::sync::broadcast`) carrying every protocol
//!    [`Message`] after the engine has updated its own state — the Tauri
//!    layer subscribes and pushes to the webview; lagging subscribers are
//!    skipped, never block the engine;
//! 2. **per-device ring buffers** for the live UI: 3,600 samples per
//!    metric, 600 per-core ticks, 50,000 log lines with Rust-side
//!    filtering and paging, the latest process table, the latest values;
//! 3. the **rules engine** ([`rules`]) that produces the plain-language
//!    [`Headline`] and status pills from the latest values, the last 60 s
//!    of events and the capability set; a `Message::Headline` is published
//!    whenever the verdict changes, and a `Message::Session` on every
//!    session state change and every 5 s while recording.
//!
//! [`Headline`]: statasmon_protocol::Headline
//!
//! ## Sessions and what is stored
//! Each device has a [`Session`] (`Idle | Recording | Paused | Stopped`). A
//! session starts recording automatically when a device attaches
//! ([`EngineConfig::auto_record`]). While **Recording**, `Samples`, `Cores`,
//! `Processes` and `Log` messages are stored in the ring buffers, counted,
//! and forwarded to the bus. While **Paused** or **Stopped** the connection
//! stays alive and the snapshot's `latest`, `cores` and `processes` still
//! update (so the live view stays truthful), but nothing is stored,
//! counted or forwarded to the bus. `Info`, `Capabilities`, `DeviceState`,
//! `Event` and `Error` messages are always stored (events feed the
//! headline) and always forwarded; so are the engine's own `Headline` and
//! `Session` messages.
//!
//! The engine never blocks: all provider I/O runs in the sources' own
//! tasks; the engine's ingress task only mutates in-memory state under a
//! short `parking_lot` lock and never awaits while holding it.
//!
//! [`DeviceSource`]: statasmon_device::DeviceSource
//! [`Message`]: statasmon_protocol::Message

pub mod config;
pub mod engine;
pub mod error;
pub mod logs;
pub mod ring;
pub mod rules;
pub mod session;
pub mod snapshot;

pub use config::EngineConfig;
pub use engine::Engine;
pub use error::EngineError;
pub use logs::LogRing;
pub use ring::{CoreRing, RingBuffer};
pub use rules::{RecentEvent, RulesInput};
pub use session::Session;
pub use snapshot::DeviceSnapshot;
