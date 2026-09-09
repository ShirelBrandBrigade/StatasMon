//! Tier 2: the Instruments (DVT) services over the developer tunnel
//! (steps 4.3–4.6). Each service runs in its own loop on its own DVT hub
//! connection; the loops share [`Tier2Shared`] so per-process network
//! rates and energy labels can be joined into the process table that
//! sysmontap emits. Every parser is a pure function over the decoded plist
//! value and is tested against `fixtures/ios/dvt/**`.

pub mod deviceinfo;
pub mod energy;
pub mod network;
pub mod parse;
pub mod sysmontap;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use statasmon_idmap::apple::CoreLayout;

use crate::tunnel::{Tunnel, is_transport_error};

/// State the tier-2 loops share (small; read once per tick).
#[derive(Debug, Default, Clone)]
pub struct Tier2Shared {
    /// Per-pid network throughput (bits/s, rx + tx) from the networking
    /// service, refreshed at the medium cadence.
    pub pid_net_bps: HashMap<u32, f64>,
    /// Energy labels per pid from the energy gauge (when enabled).
    pub energy: HashMap<u32, String>,
    /// RAM in bytes from `hardwareInformation` or sysmontap `physMemSize`.
    pub ram_total: Option<u64>,
    /// Core layout (from idmap) used to label per-core samples.
    pub layout: Option<CoreLayout>,
    /// Top pids by CPU from the last sysmontap tick (energy sampling set).
    pub top_pids: Vec<u32>,
}

/// Shared handle.
pub type SharedState = Arc<RwLock<Tier2Shared>>;

/// Why a tier-2 loop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopEnd {
    Cancelled,
    SinkClosed,
    /// The tunnel (or the device) is gone; the supervisor degrades to tier 1.
    TunnelDead(String),
    /// The service refused or misbehaved while the tunnel stayed up; the
    /// loop is not restarted, the capability gap is reported.
    ServiceFailed(String),
}

impl LoopEnd {
    /// Classify a service error: transport errors kill the tunnel.
    pub fn from_error(tunnel: &Tunnel, stage: &str, e: &idevice::IdeviceError) -> Self {
        tunnel.note_error(stage, e);
        if is_transport_error(e) || !tunnel.is_alive() {
            LoopEnd::TunnelDead(format!("{stage}: {e}"))
        } else {
            LoopEnd::ServiceFailed(format!("{stage}: {e}"))
        }
    }

    pub fn is_tunnel_dead(&self) -> bool {
        matches!(self, LoopEnd::TunnelDead(_))
    }
}
