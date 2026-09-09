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
//! - `tunnel`, `dvt`: tier 2 (Phase 4).
//!
//! Every parser is a pure function tested against `fixtures/ios/**`.

pub mod battery;
pub mod crash;
pub mod info;
pub mod lockdown;
pub mod muxd;
pub mod ostrace;
pub mod pcap;
pub mod source;
pub mod storage;
pub mod thermal;
