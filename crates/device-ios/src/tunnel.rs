//! The iOS 17.4+ developer tunnel (step 4.1; docs/PLAN.md §3.7, ADR-013).
//!
//! `CoreDeviceProxy` is a plain lockdown service (`StartService` with the
//! pair record, TLS like every other service) that carries raw IPv6
//! packets. The `idevice` crate runs a userspace TCP stack (`jktcp`) over
//! it, so no TUN device, no root and no OS permission are needed on macOS
//! or Windows. Through that stack we reach the device's RSD port, read the
//! service table with one RemoteXPC handshake, and open the DVT hub
//! (`com.apple.instruments.dtservicehub`) once per DVT loop.
//!
//! iOS 18.2+ replaced QUIC with TCP + TLS-PSK for the *network* transport
//! (remoted); the USB `CoreDeviceProxy` path used here is the same CDTunnel
//! framing on every version from 17.4 on, so nothing changes per major.
//!
//! Death detection: every service open that fails with a transport error
//! marks the tunnel dead, and a heartbeat opens (and immediately closes) a
//! TCP connection to the RSD port every [`HEARTBEAT`] to catch a silent
//! unplug. `is_alive()` is what the source's supervisor polls.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use idevice::dvt::remote_server::RemoteServerClient;
use idevice::services::core_device_proxy::CoreDeviceProxy;
use idevice::services::rsd::{RsdHandshake, RsdService};
use idevice::tcp::handle::{AdapterHandle, StreamHandle};
use idevice::tunnel::TunnelInfo;
use idevice::{IdeviceError, IdeviceService, ReadWrite};
use parking_lot::Mutex;
use tokio::task::JoinHandle;

use crate::error::IosError;
use crate::muxd::IosProvider;

/// A DVT hub connection (`com.apple.instruments.dtservicehub`).
pub type DvtHub = RemoteServerClient<Box<dyn ReadWrite + 'static>>;

/// The DVT hub every Instruments service is a channel of.
pub const DVT_HUB: &str = "com.apple.instruments.dtservicehub";
/// Heartbeat period (one TCP handshake to the RSD port over the tunnel).
pub const HEARTBEAT: Duration = Duration::from_secs(15);
/// How long one service open may take before the tunnel is considered hung.
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(20);

/// Hints attached to `IosError::Tunnel` by [`Tunnel::open`].
pub mod hints {
    pub const DEV_MODE: &str = "The device lists no developer services: turn on Settings → Privacy & Security → Developer Mode and restart it.";
    pub const RETRY: &str = "The developer tunnel could not be opened; unplug and reconnect the device. Tier 1 keeps working.";
}

/// The open tunnel.
pub struct Tunnel {
    handle: AdapterHandle,
    info: TunnelInfo,
    services: HashMap<String, RsdService>,
    rsd_uuid: String,
    protocol_version: usize,
    dead: Arc<AtomicBool>,
    death_reason: Arc<Mutex<Option<String>>>,
    heartbeat: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tunnel")
            .field("client", &self.info.client_address)
            .field("server", &self.info.server_address)
            .field("rsd_port", &self.info.server_rsd_port)
            .field("services", &self.services.len())
            .field("alive", &self.is_alive())
            .finish()
    }
}

/// True when the error means the tunnel (or the device) is gone rather
/// than one service refusing us.
pub fn is_transport_error(e: &IdeviceError) -> bool {
    matches!(
        e,
        IdeviceError::Socket(_)
            | IdeviceError::NoEstablishedConnection
            | IdeviceError::DeviceNotFound
            | IdeviceError::Usbmuxd(_)
            | IdeviceError::Timeout
    )
}

fn tunnel_error(stage: &str, e: &IdeviceError) -> IosError {
    IosError::Tunnel(format!("{stage}: {e}"))
}

impl Tunnel {
    /// Open the tunnel: CoreDeviceProxy → userspace TCP stack → RSD
    /// handshake. Fails with `IosError::DeveloperModeOff` when RSD lists no
    /// DVT hub (Developer Mode is off) and `IosError::Tunnel` otherwise.
    pub async fn open(provider: Arc<IosProvider>) -> Result<Self, IosError> {
        let proxy = tokio::time::timeout(OPEN_TIMEOUT, CoreDeviceProxy::connect(&*provider))
            .await
            .map_err(|_| IosError::Tunnel("CoreDeviceProxy: handshake timed out".to_owned()))?
            .map_err(|e| tunnel_error("CoreDeviceProxy", &e))?;
        let info = proxy.tunnel_info().clone();
        tracing::info!(
            udid = provider.udid(),
            client = %info.client_address,
            server = %info.server_address,
            mtu = info.mtu,
            rsd_port = info.server_rsd_port,
            "CoreDeviceProxy tunnel established"
        );
        if info.server_rsd_port == 0 {
            return Err(IosError::Tunnel(
                "CoreDeviceProxy handshake carried no RSD port".to_owned(),
            ));
        }
        let adapter = proxy
            .create_software_tunnel()
            .map_err(|e| tunnel_error("userspace TCP stack", &e))?;
        let mut handle = adapter.to_async_handle();

        let rsd_stream = tokio::time::timeout(OPEN_TIMEOUT, handle.connect(info.server_rsd_port))
            .await
            .map_err(|_| IosError::Tunnel("RSD: connect timed out".to_owned()))?
            .map_err(|e| IosError::Tunnel(format!("RSD connect: {e}")))?;
        let rsd = tokio::time::timeout(OPEN_TIMEOUT, RsdHandshake::new(rsd_stream))
            .await
            .map_err(|_| IosError::Tunnel("RSD: handshake timed out".to_owned()))?
            .map_err(|e| tunnel_error("RSD handshake", &e))?;
        tracing::info!(
            udid = provider.udid(),
            services = rsd.services.len(),
            protocol = rsd.protocol_version,
            "RSD handshake done"
        );

        let dead = Arc::new(AtomicBool::new(false));
        let death_reason = Arc::new(Mutex::new(None));
        let heartbeat = tokio::spawn(heartbeat(
            handle.clone(),
            info.server_rsd_port,
            Arc::clone(&dead),
            Arc::clone(&death_reason),
        ));
        let tunnel = Self {
            handle,
            info,
            services: rsd.services,
            rsd_uuid: rsd.uuid,
            protocol_version: rsd.protocol_version,
            dead,
            death_reason,
            heartbeat: Some(heartbeat),
        };
        if !tunnel.has_service(DVT_HUB) {
            tunnel.close().await;
            return Err(IosError::DeveloperModeOff);
        }
        Ok(tunnel)
    }

    /// Addresses, MTU and RSD port from the CDTunnel handshake.
    pub fn info(&self) -> &TunnelInfo {
        &self.info
    }

    /// The RSD service table.
    pub fn services(&self) -> &HashMap<String, RsdService> {
        &self.services
    }

    /// Whether RSD advertises `name`.
    pub fn has_service(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }

    /// The RSD instance id.
    pub fn rsd_uuid(&self) -> &str {
        &self.rsd_uuid
    }

    /// One line for `DeviceInfo.connection.tunnel`.
    pub fn describe(&self) -> String {
        format!(
            "Userspace tunnel · RSD port {} · {} services",
            self.info.server_rsd_port,
            self.services.len()
        )
    }

    /// True until a service open, a read, or the heartbeat saw the
    /// transport fail.
    pub fn is_alive(&self) -> bool {
        !self.dead.load(Ordering::SeqCst)
    }

    /// Why the tunnel died, once it has.
    pub fn death_reason(&self) -> Option<String> {
        self.death_reason.lock().clone()
    }

    /// Record that the transport failed (`reason` is kept for the error).
    pub fn mark_dead(&self, reason: impl Into<String>) {
        if !self.dead.swap(true, Ordering::SeqCst) {
            let reason = reason.into();
            tracing::warn!(reason = %reason, "developer tunnel died");
            *self.death_reason.lock() = Some(reason);
        }
    }

    /// Note a service error: transport errors mark the tunnel dead.
    pub fn note_error(&self, stage: &str, e: &IdeviceError) {
        if is_transport_error(e) {
            self.mark_dead(format!("{stage}: {e}"));
        }
    }

    /// A raw TCP connection to `port` on the device through the tunnel.
    pub async fn open_port(&self, port: u16) -> Result<StreamHandle, IosError> {
        if !self.is_alive() {
            return Err(IosError::Tunnel(
                self.death_reason()
                    .unwrap_or_else(|| "tunnel is closed".to_owned()),
            ));
        }
        let mut handle = self.handle.clone();
        match tokio::time::timeout(OPEN_TIMEOUT, handle.connect(port)).await {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(e)) => {
                self.mark_dead(format!("connect to port {port}: {e}"));
                Err(IosError::Tunnel(format!("connect to port {port}: {e}")))
            }
            Err(_) => {
                self.mark_dead(format!("connect to port {port} timed out"));
                Err(IosError::Tunnel(format!(
                    "connect to port {port} timed out"
                )))
            }
        }
    }

    /// Open an RSD service by its advertised name.
    pub async fn open_service<T: RsdServiceExt>(&self) -> Result<T, IosError> {
        let name = T::rsd_service_name();
        let Some(service) = self.services.get(name.as_ref()) else {
            return Err(IosError::Tunnel(format!("RSD does not advertise {name}")));
        };
        let stream = self.open_port(service.port).await?;
        match tokio::time::timeout(OPEN_TIMEOUT, T::from_stream(Box::new(stream))).await {
            Ok(Ok(t)) => Ok(t),
            Ok(Err(e)) => {
                self.note_error(&name, &e);
                Err(IosError::Tunnel(format!("{name}: {e}")))
            }
            Err(_) => {
                self.mark_dead(format!("{name}: handshake timed out"));
                Err(IosError::Tunnel(format!("{name}: handshake timed out")))
            }
        }
    }

    /// A fresh DVT hub connection (each DVT loop owns one, so a hung
    /// service never blocks another).
    pub async fn dvt(&self) -> Result<DvtHub, IosError> {
        // Built directly rather than through `open_service`: the trait's
        // `from_stream` future carries a higher-ranked lifetime that keeps a
        // spawned loop from being `Send`.
        let Some(service) = self.services.get(DVT_HUB) else {
            return Err(IosError::DeveloperModeOff);
        };
        let stream = self.open_port(service.port).await?;
        Ok(RemoteServerClient::new(stream))
    }

    /// Tear the tunnel down (the TCP stack task ends; open streams fail).
    pub async fn close(mut self) {
        self.shutdown().await;
    }

    async fn shutdown(&mut self) {
        if let Some(h) = self.heartbeat.take() {
            h.abort();
        }
        self.dead.store(true, Ordering::SeqCst);
        let mut handle = self.handle.clone();
        let _ = handle.close().await;
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Some(h) = self.heartbeat.take() {
            h.abort();
        }
        self.dead.store(true, Ordering::SeqCst);
        // `AdapterHandle::close` is async; dropping the last clone also ends
        // the stack task, and the heartbeat clone is aborted above.
    }
}

/// The crate's `RsdService` trait re-exported under a local name so
/// `open_service` can be generic without naming the crate path everywhere.
pub trait RsdServiceExt: idevice::RsdService {}
impl<T: idevice::RsdService> RsdServiceExt for T {}

async fn heartbeat(
    handle: AdapterHandle,
    rsd_port: u16,
    dead: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<String>>>,
) {
    let mut interval = tokio::time::interval(HEARTBEAT);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        if dead.load(Ordering::SeqCst) {
            return;
        }
        let mut h = handle.clone();
        let outcome = tokio::time::timeout(OPEN_TIMEOUT, h.connect(rsd_port)).await;
        let failure = match outcome {
            Ok(Ok(mut stream)) => {
                stream.close();
                None
            }
            Ok(Err(e)) => Some(format!("heartbeat: {e}")),
            Err(_) => Some("heartbeat: RSD connect timed out".to_owned()),
        };
        if let Some(why) = failure {
            if !dead.swap(true, Ordering::SeqCst) {
                tracing::warn!(reason = %why, "developer tunnel died");
                *reason.lock() = Some(why);
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_are_recognised() {
        assert!(is_transport_error(&IdeviceError::NoEstablishedConnection));
        assert!(is_transport_error(&IdeviceError::Socket(
            std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed")
        )));
        assert!(is_transport_error(&IdeviceError::Timeout));
        assert!(!is_transport_error(&IdeviceError::ServiceNotFound));
        assert!(!is_transport_error(&IdeviceError::UnexpectedResponse(
            "x".into()
        )));
    }

    #[test]
    fn tunnel_errors_carry_the_stage() {
        let e = tunnel_error("RSD handshake", &IdeviceError::ServiceNotFound);
        assert!(matches!(e, IosError::Tunnel(ref s) if s.starts_with("RSD handshake: ")));
    }
}
