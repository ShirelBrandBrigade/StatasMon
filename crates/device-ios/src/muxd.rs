//! `MuxdDiscovery`: usbmuxd device tracking and the pairing / trust flow
//! (step 3.1, docs/PLAN.md §3.7).
//!
//! - Connects to usbmuxd (`/var/run/usbmuxd` on Unix, TCP 127.0.0.1:27015
//!   on Windows; `USBMUXD_SOCKET_ADDRESS` overrides both — the OS branch
//!   lives in `idevice::usbmuxd::UsbmuxdAddr::default`, nowhere else).
//! - Lists devices, then `Listen`s for attach/detach; if `Listen` is
//!   refused it polls `ListDevices` every 2 s.
//! - Per device: read the pair record from usbmuxd and validate it with a
//!   lockdown session. No record (or a revoked one) → attach an
//!   "untrusted" [`IosSource`] (`NeedsTrust`, every capability unavailable
//!   with the reason), send a lockdown `Pair` so the device shows "Trust
//!   This Computer", and re-check every 3 s. Once trusted: save the record
//!   into usbmuxd, detach the untrusted source, attach the tier-1 source.
//! - usbmuxd unreachable → nothing is emitted, retry every 5 s; the Tauri
//!   layer calls [`MuxdDiscovery::probe`] once at startup for the notice.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use idevice::pairing_file::PairingFile;
use idevice::provider::{IdeviceProvider, UsbmuxdProvider};
use idevice::usbmuxd::{
    Connection, UsbmuxdAddr, UsbmuxdConnection, UsbmuxdDevice, UsbmuxdListenEvent,
};
use idevice::{Idevice, IdeviceError};
use parking_lot::RwLock;
use statasmon_device::{DeviceResult, DeviceSource as _, Discovery, DiscoverySink};
use statasmon_protocol::{ConnectionState, DeviceId, Transport};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::error::IosError;
use crate::lockdown::{LABEL, Lockdown};
use crate::source::{IosSeed, IosSource, TrustHandle, device_id};

/// Retry interval while usbmuxd is unreachable.
pub const MUXD_RETRY: Duration = Duration::from_secs(5);
/// `ListDevices` polling interval when `Listen` is unavailable.
pub const POLL: Duration = Duration::from_secs(2);
/// How often an untrusted device is re-checked / re-asked.
pub const TRUST_RECHECK: Duration = Duration::from_secs(3);
/// Reason shown while the device waits for the Trust dialog.
pub const REASON_TRUST: &str = "Accept \"Trust This Computer\" on the device";
/// Reason shown while the device is locked with a passcode.
pub const REASON_LOCKED: &str =
    "Unlock the device, then accept \"Trust This Computer\" when it appears";
/// Reason shown after the user tapped "Don't Trust".
pub const REASON_DENIED: &str =
    "Trust was declined on the device; unplug and reconnect it to be asked again";

/// A usbmuxd provider that serves a cached pair record, so every service
/// open costs one usbmuxd round-trip instead of two. The logs agent
/// receives it as `Arc<dyn IdeviceProvider>`.
#[derive(Debug, Clone)]
pub struct IosProvider {
    inner: UsbmuxdProvider,
    pair: Arc<RwLock<Option<PairingFile>>>,
}

impl IosProvider {
    /// A provider for `device` on `addr`, without a pair record yet.
    pub fn new(device: &UsbmuxdDevice, addr: UsbmuxdAddr) -> Self {
        Self {
            inner: device.to_provider(addr, LABEL),
            pair: Arc::new(RwLock::new(None)),
        }
    }

    /// The device UDID.
    pub fn udid(&self) -> &str {
        &self.inner.udid
    }

    /// usbmuxd's numeric device id.
    pub fn mux_id(&self) -> u32 {
        self.inner.device_id
    }

    /// Cache (or replace) the pair record.
    pub fn set_pair(&self, pair: PairingFile) {
        *self.pair.write() = Some(pair);
    }

    /// The cached pair record.
    pub fn pair(&self) -> Option<PairingFile> {
        self.pair.read().clone()
    }
}

impl IdeviceProvider for IosProvider {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        self.inner.connect(port)
    }

    fn label(&self) -> &str {
        self.inner.label()
    }

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        if let Some(p) = self.pair.read().clone() {
            return Box::pin(async move { Ok(p) });
        }
        self.inner.get_pairing_file()
    }
}

/// Where the pairing flow stands for one device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairState {
    /// A valid record is in usbmuxd (or was just created).
    Paired,
    /// Waiting for the dialog (`reason` is user-facing).
    NeedsTrust { reason: String },
}

/// Finds iOS devices through usbmuxd.
#[derive(Debug)]
pub struct MuxdDiscovery {
    addr: UsbmuxdAddr,
    retry: Duration,
    poll: Duration,
    trust_recheck: Duration,
}

impl Default for MuxdDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl MuxdDiscovery {
    /// The platform default socket (or `USBMUXD_SOCKET_ADDRESS`).
    pub fn new() -> Self {
        Self {
            addr: UsbmuxdAddr::from_env_var().unwrap_or_default(),
            retry: MUXD_RETRY,
            poll: POLL,
            trust_recheck: TRUST_RECHECK,
        }
    }

    /// A specific usbmuxd address.
    #[must_use]
    pub fn with_addr(mut self, addr: UsbmuxdAddr) -> Self {
        self.addr = addr;
        self
    }

    /// Faster re-checks (tests).
    #[must_use]
    pub fn with_intervals(mut self, poll: Duration, trust_recheck: Duration) -> Self {
        self.poll = poll.max(Duration::from_millis(200));
        self.trust_recheck = trust_recheck.max(Duration::from_millis(200));
        self
    }

    /// The address in use.
    pub fn addr(&self) -> &UsbmuxdAddr {
        &self.addr
    }

    /// One round-trip to usbmuxd; `Err(MuxdUnavailable)` carries the hint
    /// for the startup notice.
    pub async fn probe(&self) -> Result<(), IosError> {
        list_devices(&self.addr).await.map(|_| ())
    }

    /// The devices usbmuxd knows right now.
    pub async fn list(&self) -> Result<Vec<UsbmuxdDevice>, IosError> {
        list_devices(&self.addr).await
    }
}

async fn open(addr: &UsbmuxdAddr) -> Result<UsbmuxdConnection, IosError> {
    addr.connect(0)
        .await
        .map_err(|e| IosError::MuxdUnavailable(format!("{addr:?}: {e}")))
}

/// `ListDevices` on a fresh connection.
pub async fn list_devices(addr: &UsbmuxdAddr) -> Result<Vec<UsbmuxdDevice>, IosError> {
    let mut conn = open(addr).await?;
    conn.get_devices()
        .await
        .map_err(|e| IosError::MuxdUnavailable(format!("ListDevices: {e}")))
}

/// The pair record usbmuxd holds for `udid`, if any.
pub async fn read_pair_record(
    addr: &UsbmuxdAddr,
    udid: &str,
) -> Result<Option<PairingFile>, IosError> {
    let mut conn = open(addr).await?;
    match conn.get_pair_record(udid).await {
        Ok(p) => Ok(Some(p)),
        Err(IdeviceError::Socket(e)) => Err(IosError::MuxdUnavailable(e.to_string())),
        Err(e) => {
            tracing::debug!(udid, error = %e, "no pair record in usbmuxd");
            Ok(None)
        }
    }
}

/// Check the record works: open lockdown and start a session.
pub async fn validate_pair_record(
    provider: &IosProvider,
    pair: &PairingFile,
) -> Result<(), IosError> {
    Lockdown::connect(provider, pair).await.map(|_| ())
}

/// Send one lockdown `Pair` request. `Ok(None)` means the dialog is still
/// open (`PairingDialogResponsePending`).
pub async fn pair_once(
    addr: &UsbmuxdAddr,
    provider: &IosProvider,
    host_id: &str,
) -> Result<Option<PairingFile>, IosError> {
    let buid = {
        let mut conn = open(addr).await?;
        conn.get_buid()
            .await
            .map_err(|e| IosError::MuxdUnavailable(format!("ReadBUID: {e}")))?
    };
    let mut ld = Lockdown::connect_unpaired(provider).await?;
    match ld.client_mut().pair_once(host_id, buid, Some(LABEL)).await {
        Ok(p) => Ok(Some(p)),
        Err(IdeviceError::PairingDialogResponsePending) => Ok(None),
        Err(e) => Err(IosError::service("lockdown/Pair", e)),
    }
}

/// Store a record in usbmuxd so other tools (and the next launch) see it.
pub async fn save_pair_record(
    addr: &UsbmuxdAddr,
    udid: &str,
    pair: PairingFile,
) -> Result<(), IosError> {
    let bytes = pair
        .serialize()
        .map_err(|e| IosError::service("pairing_file/serialize", e))?;
    let mut conn = open(addr).await?;
    conn.save_pair_record(udid, bytes)
        .await
        .map_err(|e| IosError::MuxdUnavailable(format!("SavePairRecord: {e}")))
}

/// A fresh host id for a pairing attempt (uppercase UUID, what Apple's
/// tools generate).
pub fn new_host_id() -> String {
    uuid::Uuid::new_v4().to_string().to_ascii_uppercase()
}

/// True when the error says the device left the bus (not "refused").
pub fn is_gone(e: &IosError) -> bool {
    crate::battery::is_device_gone(e)
}

/// The transport for a usbmuxd connection type.
pub fn transport_of(connection: &Connection) -> Transport {
    match connection {
        Connection::Usb => Transport::Usb,
        Connection::Network(_) | Connection::Unknown(_) => Transport::Wifi,
    }
}

/// What the listener thread forwards.
#[derive(Debug)]
enum ListenMsg {
    /// `Listen` was accepted; events follow.
    Ready,
    Event(UsbmuxdListenEvent),
    /// The connection or the request failed (reason).
    Ended(String),
}

/// Run usbmuxd `Listen` on its own thread with a current-thread runtime
/// (the crate's event stream is `!Send`). Ends when the receiver drops.
fn spawn_listener(addr: UsbmuxdAddr) -> tokio::sync::mpsc::Receiver<ListenMsg> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let spawned = std::thread::Builder::new()
        .name("usbmuxd-listen".to_owned())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.blocking_send(ListenMsg::Ended(format!("runtime: {e}")));
                    return;
                }
            };
            rt.block_on(async move {
                let mut conn = match addr.connect(0).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(ListenMsg::Ended(e.to_string())).await;
                        return;
                    }
                };
                let mut stream = match conn.listen().await {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.send(ListenMsg::Ended(e.to_string())).await;
                        return;
                    }
                };
                if tx.send(ListenMsg::Ready).await.is_err() {
                    return;
                }
                loop {
                    tokio::select! {
                        _ = tx.closed() => return,
                        ev = stream.next() => {
                            let msg = match ev {
                                Some(Ok(ev)) => ListenMsg::Event(ev),
                                Some(Err(e)) => ListenMsg::Ended(e.to_string()),
                                None => ListenMsg::Ended("listen stream ended".to_owned()),
                            };
                            let ended = matches!(msg, ListenMsg::Ended(_));
                            if tx.send(msg).await.is_err() || ended {
                                return;
                            }
                        }
                    }
                }
            });
        });
    if let Err(e) = spawned {
        let (tx2, rx2) = tokio::sync::mpsc::channel(1);
        let _ = tx2.try_send(ListenMsg::Ended(format!("thread: {e}")));
        return rx2;
    }
    rx
}

struct Tracked {
    udid: String,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

#[async_trait]
impl Discovery for MuxdDiscovery {
    fn name(&self) -> &'static str {
        "usbmuxd"
    }

    async fn run(self: Box<Self>, sink: DiscoverySink) -> DeviceResult<()> {
        let mut known: HashMap<u32, Tracked> = HashMap::new();
        let mut logged_unreachable = false;
        loop {
            let mut conn = match open(&self.addr).await {
                Ok(c) => {
                    if logged_unreachable {
                        tracing::info!("usbmuxd reachable again");
                        logged_unreachable = false;
                    }
                    c
                }
                Err(e) => {
                    if !logged_unreachable {
                        tracing::warn!(error = %e, "usbmuxd unreachable; retrying every {:?}", self.retry);
                        logged_unreachable = true;
                    }
                    if self.wait(&sink, self.retry).await.is_err() {
                        return Ok(());
                    }
                    continue;
                }
            };
            // Initial listing; `Listen` also replays attached devices, and
            // `on_attach` is idempotent, so both paths are safe.
            match conn.get_devices().await {
                Ok(list) => {
                    for dev in list {
                        if self.on_attach(&sink, &mut known, dev).await.is_err() {
                            return Ok(());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "ListDevices failed; reconnecting");
                    if self.wait(&sink, self.retry).await.is_err() {
                        return Ok(());
                    }
                    continue;
                }
            }
            drop(conn);
            // `Listen` on a dedicated thread: the crate's stream is not
            // `Send`, so it cannot live inside this (spawned) future.
            let mut events = spawn_listener(self.addr.clone());
            let ended = match events.recv().await {
                Some(ListenMsg::Ready) => loop {
                    tokio::select! {
                        _ = sink.closed() => return Ok(()),
                        ev = events.recv() => match ev {
                            Some(ListenMsg::Event(UsbmuxdListenEvent::Connected(dev))) => {
                                if self.on_attach(&sink, &mut known, dev).await.is_err() {
                                    return Ok(());
                                }
                            }
                            Some(ListenMsg::Event(UsbmuxdListenEvent::Disconnected(id))) => {
                                if on_detach(&sink, &mut known, id).await.is_err() {
                                    return Ok(());
                                }
                            }
                            Some(ListenMsg::Ended(reason)) => break Some(reason),
                            Some(ListenMsg::Ready) => {}
                            None => break Some("listener thread ended".to_owned()),
                        },
                    }
                },
                other => {
                    let reason = match other {
                        Some(ListenMsg::Ended(e)) => e,
                        _ => "listener thread ended".to_owned(),
                    };
                    tracing::info!(
                        reason,
                        "usbmuxd Listen unavailable; polling every {:?}",
                        self.poll
                    );
                    drop(events);
                    self.poll_loop(&sink, &mut known).await
                }
            };
            match ended {
                Some(reason) => {
                    tracing::warn!(reason, "usbmuxd connection lost; every device is detached")
                }
                None => return Ok(()),
            }
            for (_, t) in known.drain() {
                t.cancel.cancel();
                t.task.abort();
                if sink.detached(device_id(&t.udid)).await.is_err() {
                    return Ok(());
                }
            }
            if self.wait(&sink, self.retry).await.is_err() {
                return Ok(());
            }
        }
    }
}

impl MuxdDiscovery {
    /// Sleep unless the engine drops the sink (`Err`).
    async fn wait(&self, sink: &DiscoverySink, d: Duration) -> Result<(), ()> {
        tokio::select! {
            _ = tokio::time::sleep(d) => Ok(()),
            _ = sink.closed() => Err(()),
        }
    }

    /// Poll `ListDevices` until usbmuxd fails (`Some(reason)`) or the sink
    /// closes (`None`).
    async fn poll_loop(
        &self,
        sink: &DiscoverySink,
        known: &mut HashMap<u32, Tracked>,
    ) -> Option<String> {
        let mut interval = tokio::time::interval(self.poll);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = sink.closed() => return None,
                _ = interval.tick() => {}
            }
            let list = match list_devices(&self.addr).await {
                Ok(l) => l,
                Err(e) => return Some(e.to_string()),
            };
            let present: std::collections::HashSet<u32> =
                list.iter().map(|d| d.device_id).collect();
            let gone: Vec<u32> = known
                .keys()
                .filter(|id| !present.contains(id))
                .copied()
                .collect();
            for id in gone {
                if on_detach(sink, known, id).await.is_err() {
                    return None;
                }
            }
            for dev in list {
                if self.on_attach(sink, known, dev).await.is_err() {
                    return None;
                }
            }
        }
    }

    /// Start tracking `dev` (no-op when already tracked). `Err` = sink closed.
    async fn on_attach(
        &self,
        sink: &DiscoverySink,
        known: &mut HashMap<u32, Tracked>,
        dev: UsbmuxdDevice,
    ) -> Result<(), ()> {
        if let Some(t) = known.get(&dev.device_id) {
            if t.udid == dev.udid {
                return Ok(());
            }
            if on_detach(sink, known, dev.device_id).await.is_err() {
                return Err(());
            }
        }
        let cancel = CancellationToken::new();
        let transport = transport_of(&dev.connection_type);
        tracing::info!(udid = %dev.udid, mux_id = dev.device_id, ?transport, "ios device detected");
        let task = tokio::spawn(attach_flow(
            self.addr.clone(),
            dev.clone(),
            transport,
            sink.clone(),
            cancel.clone(),
            self.trust_recheck,
        ));
        known.insert(
            dev.device_id,
            Tracked {
                udid: dev.udid,
                cancel,
                task,
            },
        );
        Ok(())
    }
}

async fn on_detach(
    sink: &DiscoverySink,
    known: &mut HashMap<u32, Tracked>,
    id: u32,
) -> Result<(), ()> {
    let Some(t) = known.remove(&id) else {
        return Ok(());
    };
    t.cancel.cancel();
    t.task.abort();
    tracing::info!(udid = %t.udid, mux_id = id, "ios device detached");
    sink.detached(device_id(&t.udid)).await.map_err(|_| ())
}

/// Per-device task: pairing state machine ending in a tier-1 source.
async fn attach_flow(
    addr: UsbmuxdAddr,
    dev: UsbmuxdDevice,
    transport: Transport,
    sink: DiscoverySink,
    cancel: CancellationToken,
    recheck: Duration,
) {
    let provider = Arc::new(IosProvider::new(&dev, addr.clone()));
    let seed = IosSeed::new(&dev.udid, dev.device_id, transport);
    let host_id = new_host_id();
    let mut untrusted: Option<TrustHandle> = None;
    let mut last_reason: Option<String> = None;

    loop {
        if cancel.is_cancelled() || sink.is_closed() {
            return;
        }
        // 1. A usable pair record?
        let pair = match read_pair_record(&addr, &dev.udid).await {
            Ok(Some(p)) => match validate_pair_record(&provider, &p).await {
                Ok(()) => Some(p),
                Err(e) if is_gone(&e) => {
                    tracing::info!(udid = %dev.udid, error = %e, "device gone during pairing check");
                    return;
                }
                Err(e) => {
                    tracing::info!(udid = %dev.udid, error = %e, "pair record rejected; re-pairing");
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(udid = %dev.udid, error = %e, "usbmuxd pair record read failed");
                None
            }
        };
        if let Some(pair) = pair {
            provider.set_pair(pair.clone());
            match IosSource::connect(Arc::clone(&provider), pair, seed.clone()).await {
                Ok(source) => {
                    if untrusted.take().is_some() && sink.detached(source.id()).await.is_err() {
                        return;
                    }
                    tracing::info!(udid = %dev.udid, id = %source.id(), "ios device attached (tier 1)");
                    let _ = sink.attached(Box::new(source)).await;
                    return;
                }
                Err(e) if is_gone(&e) => return,
                Err(e) => {
                    tracing::warn!(udid = %dev.udid, error = %e, "tier-1 connect failed; retrying");
                    if wait(&cancel, recheck).await.is_err() {
                        return;
                    }
                    continue;
                }
            }
        }

        // 2. Not paired: show NeedsTrust and ask the device. After "Don't
        // Trust" the device is asked again only every 30 s.
        let (reason, delay) = match pair_once(&addr, &provider, &host_id).await {
            Ok(Some(pair)) => {
                tracing::info!(udid = %dev.udid, "device trusted this computer");
                if let Err(e) = save_pair_record(&addr, &dev.udid, pair.clone()).await {
                    tracing::warn!(udid = %dev.udid, error = %e, "usbmuxd did not store the pair record; keeping it in memory");
                }
                provider.set_pair(pair);
                continue;
            }
            Ok(None) => (REASON_TRUST.to_owned(), recheck),
            Err(e) if is_gone(&e) => return,
            Err(IosError::Service {
                source: IdeviceError::UserDeniedPairing,
                ..
            }) => (REASON_DENIED.to_owned(), recheck * 10),
            Err(IosError::Service {
                source: IdeviceError::PasswordProtected,
                ..
            }) => (REASON_LOCKED.to_owned(), recheck),
            Err(e) => {
                tracing::warn!(udid = %dev.udid, error = %e, "pairing request failed");
                (format!("{REASON_TRUST} ({e})"), recheck)
            }
        };
        if last_reason.as_deref() != Some(reason.as_str()) {
            match &untrusted {
                Some(handle) => handle.update(ConnectionState::NeedsTrust, &reason).await,
                None => {
                    let source = IosSource::untrusted(Arc::clone(&provider), seed.clone(), &reason);
                    let handle = source.trust_handle();
                    if sink.attached(Box::new(source)).await.is_err() {
                        return;
                    }
                    untrusted = Some(handle);
                }
            }
            last_reason = Some(reason);
        }
        if wait(&cancel, delay).await.is_err() {
            return;
        }
    }
}

async fn wait(cancel: &CancellationToken, d: Duration) -> Result<(), ()> {
    tokio::select! {
        _ = tokio::time::sleep(d) => Ok(()),
        _ = cancel.cancelled() => Err(()),
    }
}

/// The device id for a UDID (re-exported for callers of the discovery).
pub fn ios_device_id(udid: &str) -> DeviceId {
    device_id(udid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transports_and_ids() {
        assert_eq!(transport_of(&Connection::Usb), Transport::Usb);
        assert_eq!(
            transport_of(&Connection::Network(
                "10.0.0.2"
                    .parse()
                    .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
            )),
            Transport::Wifi
        );
        assert_eq!(ios_device_id("abc"), "ios:abc");
        let id = new_host_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id, id.to_ascii_uppercase());
    }

    #[tokio::test]
    async fn probe_reports_unreachable_socket() {
        let disc = MuxdDiscovery::new().with_addr(UsbmuxdAddr::TcpSocket(
            "127.0.0.1:1".parse().unwrap_or_else(|_| unreachable!()),
        ));
        let err = disc.probe().await.err();
        assert!(matches!(err, Some(IosError::MuxdUnavailable(_))));
    }
}
