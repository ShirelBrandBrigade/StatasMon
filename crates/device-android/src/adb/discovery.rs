//! `AdbDiscovery`: polls `adb devices -l` and reports attach/detach to the
//! engine (docs/PLAN.md §3.8, step 2.2).
//!
//! - A serial in `device` state is probed (in its own task, so a slow
//!   device never delays the others) and attached as a full
//!   [`AndroidSource`].
//! - A serial in any other state (`unauthorized`, `offline`, `no
//!   permissions`, recovery…) is attached immediately as an "untrusted"
//!   source: every capability unavailable with the reason, summary state
//!   `NeedsTrust`. When its state changes (the user taps *Allow*) it is
//!   detached and re-attached as a full source.
//! - adb errors are logged once, not every poll.
//!
//! When built with [`AdbDiscovery::from_locator`] and no adb is installed,
//! the discovery keeps re-running the locator every few seconds so the
//! device appears as soon as the user installs platform-tools.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use statasmon_device::{DeviceResult, DeviceSource as _, Discovery, DiscoverySink};
use statasmon_protocol::ToolVersion;
use tokio::sync::mpsc;

use super::locator::{AdbLocation, locate};
use super::server::{AdbDevice, AdbServer, AdbState};
use crate::shell::{Shell, ShellError};
use crate::source::{AndroidSource, SourceSeed, device_id};

/// Default `adb devices -l` polling interval.
pub const DEFAULT_POLL: Duration = Duration::from_secs(2);
/// How often the locator is retried while adb is missing.
pub const LOCATOR_RETRY: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Shared {
    server: Mutex<Option<AdbServer>>,
}

/// Wireless pairing / connect actions for the Add-device dialog (step 2.7).
/// Clone freely; works as soon as the discovery has located adb.
#[derive(Debug, Clone)]
pub struct AdbHandle {
    shared: Arc<Shared>,
}

impl AdbHandle {
    fn server(&self) -> Result<AdbServer, ShellError> {
        self.shared
            .server
            .lock()
            .clone()
            .ok_or_else(|| ShellError::AdbNotFound("adb has not been located yet".to_owned()))
    }

    /// The adb binary in use, once located.
    pub fn adb_path(&self) -> Option<std::path::PathBuf> {
        self.shared
            .server
            .lock()
            .as_ref()
            .map(|s| s.adb_path().to_path_buf())
    }

    /// `adb pair host:port code`; returns adb's output text.
    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, ShellError> {
        self.server()?.pair(host, port, code).await
    }

    /// `adb connect host:port`; returns adb's output text. The device then
    /// shows up in the next poll as `host:port`.
    pub async fn connect(&self, host: &str, port: u16) -> Result<String, ShellError> {
        self.server()?.connect(host, port).await
    }
}

/// Polls the adb server and reports devices.
#[derive(Debug)]
pub struct AdbDiscovery {
    server: Option<AdbServer>,
    poll: Duration,
    locator_retry: Duration,
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Known {
    state: AdbState,
    /// A probe/attach task is in flight for this serial.
    attaching: bool,
}

#[derive(Debug)]
struct AttachOutcome {
    serial: String,
    ok: bool,
}

impl AdbDiscovery {
    /// Use an already located adb.
    pub fn new(server: AdbServer) -> Self {
        Self {
            server: Some(server),
            poll: DEFAULT_POLL,
            locator_retry: LOCATOR_RETRY,
            shared: Arc::new(Shared {
                server: Mutex::new(None),
            }),
        }
    }

    /// Locate adb when `run` starts (and keep retrying while it is missing).
    pub fn from_locator() -> Self {
        Self {
            server: None,
            poll: DEFAULT_POLL,
            locator_retry: LOCATOR_RETRY,
            shared: Arc::new(Shared {
                server: Mutex::new(None),
            }),
        }
    }

    /// Change the polling interval (minimum 200 ms).
    #[must_use]
    pub fn with_poll(mut self, poll: Duration) -> Self {
        self.poll = poll.max(Duration::from_millis(200));
        self
    }

    /// Pairing / connect handle.
    pub fn handle(&self) -> AdbHandle {
        AdbHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    async fn resolve_server(&self, sink: &DiscoverySink) -> Option<AdbServer> {
        if let Some(s) = &self.server {
            return Some(s.clone());
        }
        let mut logged = false;
        loop {
            match locate().await {
                AdbLocation::Found {
                    path,
                    source,
                    version,
                } => {
                    tracing::info!(
                        adb = %path.display(),
                        ?source,
                        version = version.as_ref().map(|v| v.version.as_str()).unwrap_or("?"),
                        "adb located"
                    );
                    return Some(AdbServer::new(path));
                }
                AdbLocation::Missing { hint } => {
                    if !logged {
                        tracing::error!(hint = %hint, "adb not found; retrying every {:?}", self.locator_retry);
                        logged = true;
                    }
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(self.locator_retry) => {}
                _ = sink.closed() => return None,
            }
        }
    }
}

#[async_trait]
impl Discovery for AdbDiscovery {
    fn name(&self) -> &'static str {
        "adb"
    }

    async fn run(self: Box<Self>, sink: DiscoverySink) -> DeviceResult<()> {
        let Some(server) = self.resolve_server(&sink).await else {
            return Ok(());
        };
        *self.shared.server.lock() = Some(server.clone());
        if let Err(e) = server.start().await {
            tracing::warn!(error = %e, "adb start-server failed; polling anyway");
        }
        let version: Option<ToolVersion> = match server.version().await {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(error = %e, "adb version failed");
                None
            }
        };

        let mut known: HashMap<String, Known> = HashMap::new();
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<AttachOutcome>();
        let mut interval = tokio::time::interval(self.poll);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut error_logged = false;

        loop {
            tokio::select! {
                _ = sink.closed() => return Ok(()),
                outcome = done_rx.recv() => {
                    if let Some(AttachOutcome { serial, ok }) = outcome {
                        if ok {
                            if let Some(k) = known.get_mut(&serial) {
                                k.attaching = false;
                            }
                        } else {
                            known.remove(&serial);
                        }
                    }
                }
                _ = interval.tick() => {
                    let list = match server.devices().await {
                        Ok(list) => {
                            if error_logged {
                                tracing::info!("adb devices recovered");
                                error_logged = false;
                            }
                            list
                        }
                        Err(e) => {
                            if !error_logged {
                                tracing::warn!(error = %e, "adb devices failed (logged once)");
                                error_logged = true;
                            }
                            continue;
                        }
                    };
                    if reconcile(&server, &version, &sink, &mut known, list, &done_tx).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Apply one listing: attach new/changed devices, detach vanished ones.
/// `Err` means the engine dropped the sink.
async fn reconcile(
    server: &AdbServer,
    version: &Option<ToolVersion>,
    sink: &DiscoverySink,
    known: &mut HashMap<String, Known>,
    list: Vec<AdbDevice>,
    done_tx: &mpsc::UnboundedSender<AttachOutcome>,
) -> Result<(), ()> {
    let present: std::collections::HashSet<String> =
        list.iter().map(|d| d.serial.clone()).collect();

    // Vanished devices first, so a serial that comes back gets a fresh source.
    let gone: Vec<String> = known
        .iter()
        .filter(|(serial, k)| !k.attaching && !present.contains(*serial))
        .map(|(s, _)| s.clone())
        .collect();
    for serial in gone {
        known.remove(&serial);
        tracing::info!(serial = %serial, "android device detached");
        sink.detached(device_id(&serial)).await.map_err(|_| ())?;
    }

    for dev in list {
        let serial = dev.serial.clone();
        match known.get(&serial) {
            Some(k) if k.attaching => continue,
            Some(k) if k.state == dev.state => continue,
            Some(_) => {
                // State changed (e.g. unauthorized → device): replace the source.
                known.remove(&serial);
                tracing::info!(serial = %serial, state = ?dev.state, "android device state changed");
                sink.detached(device_id(&serial)).await.map_err(|_| ())?;
            }
            None => {}
        }
        let shell: Arc<dyn Shell> = Arc::new(server.shell(&serial));
        let seed = SourceSeed {
            device: dev.clone(),
            adb: Some(server.clone()),
            adb_version: version.clone(),
        };
        if dev.state == AdbState::Device {
            known.insert(
                serial.clone(),
                Known {
                    state: dev.state.clone(),
                    attaching: true,
                },
            );
            let sink = sink.clone();
            let done = done_tx.clone();
            tokio::spawn(async move {
                let ok = match AndroidSource::connect(shell, seed).await {
                    Ok(source) => {
                        tracing::info!(serial = %serial, id = %source.id(), "android device attached");
                        sink.attached(Box::new(source)).await.is_ok()
                    }
                    Err(e) => {
                        tracing::warn!(serial = %serial, error = %e, "probe failed; will retry on next poll");
                        false
                    }
                };
                let _ = done.send(AttachOutcome { serial, ok });
            });
        } else {
            let reason = dev.state.reason();
            tracing::info!(serial = %serial, state = ?dev.state, "android device needs attention");
            known.insert(
                serial.clone(),
                Known {
                    state: dev.state.clone(),
                    attaching: false,
                },
            );
            let source = AndroidSource::untrusted(shell, seed, &reason);
            sink.attached(Box::new(source)).await.map_err(|_| ())?;
        }
    }
    Ok(())
}
