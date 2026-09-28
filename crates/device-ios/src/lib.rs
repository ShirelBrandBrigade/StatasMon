//! iOS / iPadOS provider (docs/PLAN.md §3.7, docs/research/ios.md).
//!
//! Tier 1 (no tunnel, no Developer Mode): lockdown values, battery via
//! diagnostics_relay, storage via lockdown + installation_proxy, structured
//! logs via os_trace_relay, crash reports, packet capture via pcapd.
//! Tier 2 (Phase 4): CoreDeviceProxy tunnel + RSD + DVT (sysmontap,
//! networking, deviceinfo).
//!
//! Layout (two agents work here in parallel; keep modules disjoint):
//! - `muxd`, `lockdown`, `info`, `battery`, `storage`, `source`: discovery,
//!   pairing/trust, the DeviceInfo sheet, tier-1 metrics, the DeviceSource
//!   (steps 3.1–3.4).
//! - `ostrace`, `thermal`, `crash`, `pcap`: the log stream and what is
//!   derived from it, crash reports, capture (steps 3.5–3.7).
//! - `tunnel`, `dvt/*`: the developer tunnel and the DVT loops (steps
//!   4.1–4.6); `source` upgrades to tier 2 when Developer Mode is on.
//!
//! Every parser is a pure function tested against `fixtures/ios/**`.

pub mod battery;
pub mod crash;
pub mod dvt;
pub mod error;
pub mod info;
pub mod lockdown;
pub mod muxd;
pub mod ostrace;
pub mod pcap;
pub mod setup;
pub mod source;
pub mod storage;
pub mod thermal;
pub mod tunnel;

pub use error::IosError;
pub use info::{ConnectionFacts, InfoInputs, build_info, load_inputs, read_info};
pub use muxd::{IosProvider, MuxdDiscovery};
pub use source::{
    IosSeed, IosSource, TrustHandle, app_data_dir, device_id, tier1_capabilities,
    tier2_capabilities,
};
pub use tunnel::Tunnel;
