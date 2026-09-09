//! StatasMon `device-fake` crate: deterministic fake devices and replay.
//!
//! Role (docs/PLAN.md §3.3, step 0.3): every feature is built against this
//! source before real hardware. It provides
//!
//! * [`FakeDiscovery`] — reports N scripted devices (an iPhone, an iPad and
//!   an Android tablet by default) and lets a [`FakeHandle`] unplug and
//!   re-plug them so the engine and UI can be tested for detach/reattach;
//! * [`FakeSource`] — a seeded generator that emits every metric in the
//!   catalog the device's capability set allows, per-core samples,
//!   process tables, log lines and scenario events on the requested
//!   [`SamplingConfig`] cadence; or a replay of a JSONL fixture of
//!   protocol messages with the original timing (scaled by `speed`);
//! * [`Scenario`] — `quiet`, `warm`, `stress`, `crash` (also selectable
//!   through `STATASMON_FAKE_SCENARIO`).
//!
//! Everything is deterministic for a given seed and cadence; there is no
//! external randomness and no dependency on the wall clock beyond the
//! timestamps stamped on messages.
//!
//! [`SamplingConfig`]: statasmon_protocol::SamplingConfig

mod config;
mod discovery;
mod profiles;
mod replay;
mod rng;
mod sim;
mod source;

pub use config::{FakeConfig, FakeDevice, SCENARIO_ENV, Scenario};
pub use discovery::{DiscoveryGone, FakeDiscovery, FakeHandle};
pub use profiles::Profile;
pub use replay::{Fixture, message_ts};
pub use sim::Simulator;
pub use source::FakeSource;
