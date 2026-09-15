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
//! ([`EngineConfig::auto_record`]). Every provider message is forwarded to
//! the bus and kept in the in-memory rings whatever the session status:
//! a connected device is always shown live, recording or not (ADR-018).
//! While **Recording**, `Samples`, `Cores`, `Processes` and `Log` messages
//! are additionally counted and written to the store; while **Paused** or
//! **Stopped** they are not. `Info`, `Capabilities`, `DeviceState`, `Event`
//! and `Error` messages feed the headline and are stored while a session
//! is open; so are the engine's own `Headline` and `Session` messages.
//!
//! ## The store (Phase 5)
//! With [`EngineConfig::store`] set, the engine opens a
//! [`statasmon_store::Store`] and becomes its producer: every session
//! start (auto-record, `start_session`, a split at the recording cap)
//! begins a store session named `<device>-<yyyyMMdd-HHmmss>`, which is the
//! id the UI sees in `SessionState.id`; while **Recording** every
//! `Samples`/`Cores`/`Processes`/`Log`/`Event` is handed to the store's
//! writer queue (non-blocking; a full queue drops and counts), marks are
//! stored whenever a session is open, `stop`/detach end the session.
//! History and log queries with a `session_id` are answered from the
//! store, without one from the live rings, which stay as they are.
//!
//! The engine never blocks: all provider I/O runs in the sources' own
//! tasks; the engine's ingress task only mutates in-memory state under a
//! short `parking_lot` lock and never awaits while holding it; store
//! writes happen on the store's own thread.
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
pub use statasmon_store::{Store, StoreConfig};
