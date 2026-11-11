//! The ADB transport (docs/PLAN.md §3.8, steps 2.1–2.2, ADR-009):
//! locating the binary, the server, device tracking and wireless pairing.
//!
//! - [`locator`]: where adb is (`STATASMON_ADB`, managed copy, PATH,
//!   `~/Downloads/platform-tools`, SDK) and its version.
//! - [`server`]: `start-server`, `devices -l` parsing, `pair` / `connect`.
//! - [`discovery`]: [`AdbDiscovery`] polling the server for the engine.

pub mod discovery;
pub mod locator;
pub mod process;
pub mod server;

pub use discovery::{AdbDiscovery, AdbHandle, DEFAULT_POLL};
pub use locator::{AdbLocation, AdbSource, HostOs, LocatorEnv, locate, locate_with};
pub use server::{AdbDevice, AdbServer, AdbState, parse_devices, parse_version};
