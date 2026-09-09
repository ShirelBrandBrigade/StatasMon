//! The `DeviceSource` and `Discovery` traits (docs/PLAN.md §3.4).
//!
//! A [`Discovery`] watches a transport (usbmuxd, adb, the fake config) and
//! hands each device it finds to the engine as a boxed [`DeviceSource`].
//! The engine owns the sources: it calls `info`, `start` and `stop`, and
//! forwards everything the source pushes into its [`EventSink`].

use std::fmt;

use async_trait::async_trait;
use statasmon_protocol::{
    CapabilitySet, DeviceAction, DeviceId, DeviceInfo, DeviceSummary, Platform, SamplingConfig,
};
use tokio::sync::mpsc;

use crate::error::DeviceResult;
use crate::sink::{EventSink, SinkClosed};

/// Outcome of a [`DeviceAction`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    /// Completed; any data was sent over the sink (e.g. a fresh `Info`).
    Done,
    /// Completed and produced a file at `path` (pcap, bugreport, crash log).
    File { path: String },
    /// A long-running task began; progress arrives as `Message::Progress`.
    Started { task_id: String },
}

/// One connected device, whatever the platform.
///
/// Implementations must never block: all I/O happens inside the tokio
/// tasks they spawn in `start`, and `stop` cancels those tasks.
#[async_trait]
pub trait DeviceSource: Send + Sync {
    /// Stable id: `ios:<udid>`, `android:<serial>`, `fake:<name>`.
    fn id(&self) -> DeviceId;

    /// Which platform this is.
    fn platform(&self) -> Platform;

    /// Sidebar-level summary (name, model, transport, connection state).
    fn summary(&self) -> DeviceSummary;

    /// What this device and tier can provide, with reasons for each gap.
    fn capabilities(&self) -> CapabilitySet;

    /// The full device information document.
    async fn info(&self) -> DeviceResult<DeviceInfo>;

    /// Start sampling on the given cadence, pushing into `sink`. Returns
    /// once the background tasks are running.
    async fn start(&self, sink: EventSink, cfg: SamplingConfig) -> DeviceResult<()>;

    /// Cancel sampling. Idempotent.
    async fn stop(&self) -> DeviceResult<()>;

    /// Run a one-off action (pcap, crash fetch, bugreport, refresh info).
    async fn run_action(&self, action: DeviceAction) -> DeviceResult<ActionResult>;
}

/// What a discovery reports to the engine.
pub enum DiscoveryEvent {
    /// A device appeared; the engine now owns the source.
    Attached(Box<dyn DeviceSource>),
    /// A device went away.
    Detached(DeviceId),
}

impl fmt::Debug for DiscoveryEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoveryEvent::Attached(source) => {
                f.debug_tuple("Attached").field(&source.summary()).finish()
            }
            DiscoveryEvent::Detached(id) => f.debug_tuple("Detached").field(id).finish(),
        }
    }
}

/// Where a [`Discovery`] reports attach/detach.
#[derive(Debug, Clone)]
pub struct DiscoverySink {
    tx: mpsc::Sender<DiscoveryEvent>,
}

impl DiscoverySink {
    /// A new bounded channel with its sink.
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<DiscoveryEvent>) {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        (Self { tx }, rx)
    }

    /// Report a new device.
    pub async fn attached(&self, source: Box<dyn DeviceSource>) -> Result<(), SinkClosed> {
        self.tx
            .send(DiscoveryEvent::Attached(source))
            .await
            .map_err(|_| SinkClosed)
    }

    /// Report that a device went away.
    pub async fn detached(&self, device_id: DeviceId) -> Result<(), SinkClosed> {
        self.tx
            .send(DiscoveryEvent::Detached(device_id))
            .await
            .map_err(|_| SinkClosed)
    }

    /// True once the engine dropped the receiver.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Wait until the engine drops the receiver.
    pub async fn closed(&self) {
        self.tx.closed().await;
    }
}

/// Finds devices on one transport and reports them until the sink closes.
#[async_trait]
pub trait Discovery: Send + Sync {
    /// Short name for logs ("usbmuxd", "adb", "fake").
    fn name(&self) -> &'static str;

    /// Run until the transport ends or the engine drops the sink.
    async fn run(self: Box<Self>, sink: DiscoverySink) -> DeviceResult<()>;
}
