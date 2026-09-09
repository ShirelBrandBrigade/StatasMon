//! Android provider over ADB (docs/PLAN.md §3.8, docs/research/android.md).
//!
//! Layout (two agents work here in parallel; keep modules disjoint):
//! - `adb/`: finding adb, the server, the host protocol, the [`Shell`]
//!   implementation, device tracking (steps 2.1–2.2).
//! - `probe`, `info`: one-time capability probe and the `DeviceInfo` sheet (2.2).
//! - `samplers/`: pure parsers + cadence samplers for CPU/memory/net (fast),
//!   battery/processes/wifi (medium), thermal/netstats/disk (slow) (2.3–2.5).
//! - `logcat`, `events`: the log stream, pid filters, LMK/crash/ANR events (2.6).
//! - `source`: `AndroidSource`, the `DeviceSource` that ties it together.
//!
//! Everything talks to the device through the [`Shell`] trait so parsers and
//! samplers are tested against `fixtures/android/**` without hardware.

pub mod adb;
pub mod events;
pub mod info;
pub mod logcat;
pub mod probe;
pub mod samplers;
pub mod shell;
pub mod source;

pub use shell::fixture::FixtureShell;
pub use shell::process::ProcessShell;
pub use shell::{Shell, ShellError, ShellStream};
