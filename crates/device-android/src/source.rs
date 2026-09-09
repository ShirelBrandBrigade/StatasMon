//! `AndroidSource`: one Android device behind the [`DeviceSource`] trait.
//!
//! Construction runs the probe and reads the info sheet once (so
//! `capabilities()` and `summary()` are ready before the engine sees the
//! source). `start` spawns one tokio task per cadence that ticks the
//! samplers from `samplers::build`, plus the logcat stream; a
//! `DeviceGone` from any tick ends everything with a single
//! `DeviceState Disconnected` + `Error DeviceLost`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use statasmon_device::{ActionResult, DeviceError, DeviceResult, DeviceSource, EventSink, now_ms};
use statasmon_protocol::{
    CapabilitySet, ConnectionState, DeviceAction, DeviceId, DeviceInfo, DeviceSummary, ErrorCode,
    Message, Platform, SamplingConfig, ToolVersion, Transport,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::adb::server::{AdbDevice, AdbServer, AdbState};
use crate::info::{ConnectionFacts, read_info};
use crate::logcat::{self, LogcatConfig};
use crate::probe::Probe;
use crate::samplers::{self, Cadence, Sampler};
use crate::shell::{Shell, ShellError};

/// How long `stop` waits for the sampler tasks before aborting them.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// `adb bugreport` can take minutes.
const BUGREPORT_TIMEOUT_MS: u64 = 10 * 60 * 1000;

/// Everything the discovery knows about the device before the probe.
#[derive(Debug, Clone)]
pub struct SourceSeed {
    pub device: AdbDevice,
    /// The adb binary (needed for bugreport / pair / connect); `None` in
    /// fixture tests.
    pub adb: Option<AdbServer>,
    /// `adb version`, reported under `DeviceInfo::connection.tools`.
    pub adb_version: Option<ToolVersion>,
}

impl SourceSeed {
    /// A seed for an authorized device with just a serial (tests).
    pub fn for_serial(serial: &str) -> Self {
        Self {
            device: AdbDevice {
                serial: serial.to_owned(),
                state: AdbState::Device,
                product: None,
                model: None,
                device: None,
                transport_id: None,
                usb: !serial.contains(':'),
            },
            adb: None,
            adb_version: None,
        }
    }

    fn transport(&self) -> Transport {
        if self.device.is_tcp() {
            Transport::AdbTcp
        } else {
            Transport::Usb
        }
    }
}

struct Running {
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

/// One Android device.
pub struct AndroidSource {
    id: DeviceId,
    seed: SourceSeed,
    shell: Arc<dyn Shell>,
    state: RwLock<ConnectionState>,
    summary: RwLock<DeviceSummary>,
    probe: RwLock<Probe>,
    capabilities: RwLock<CapabilitySet>,
    info: RwLock<Option<DeviceInfo>>,
    sink: Mutex<Option<EventSink>>,
    running: Mutex<Option<Running>>,
}

impl std::fmt::Debug for AndroidSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AndroidSource")
            .field("id", &self.id)
            .field("state", &*self.state.read())
            .finish_non_exhaustive()
    }
}

/// The device id for an adb serial.
pub fn device_id(serial: &str) -> DeviceId {
    format!("android:{serial}")
}

impl AndroidSource {
    /// Probe the device and read its info sheet, then return the source.
    /// Fails only when the shell itself fails (device gone, adb missing).
    pub async fn connect(shell: Arc<dyn Shell>, seed: SourceSeed) -> Result<Self, ShellError> {
        let probe = Probe::run(&*shell).await?;
        let facts = Self::facts(&seed, "authorized");
        let info = read_info(&*shell, &probe, &facts).await?;
        let capabilities = probe.capabilities();
        let summary = Self::make_summary(&seed, Some(&info), ConnectionState::Tier2);
        Ok(Self {
            id: device_id(&seed.device.serial),
            shell,
            state: RwLock::new(ConnectionState::Tier2),
            summary: RwLock::new(summary),
            probe: RwLock::new(probe),
            capabilities: RwLock::new(capabilities),
            info: RwLock::new(Some(info)),
            sink: Mutex::new(None),
            running: Mutex::new(None),
            seed,
        })
    }

    /// A source for a device adb lists but cannot use yet (unauthorized,
    /// offline, no permissions). Every capability is unavailable with
    /// `reason`; the summary state is `NeedsTrust`. The discovery replaces
    /// it once the device becomes authorized.
    pub fn untrusted(shell: Arc<dyn Shell>, seed: SourceSeed, reason: &str) -> Self {
        let summary = Self::make_summary(&seed, None, ConnectionState::NeedsTrust);
        Self {
            id: device_id(&seed.device.serial),
            shell,
            state: RwLock::new(ConnectionState::NeedsTrust),
            summary: RwLock::new(summary),
            probe: RwLock::new(Probe::default()),
            capabilities: RwLock::new(CapabilitySet::none(reason)),
            info: RwLock::new(None),
            sink: Mutex::new(None),
            running: Mutex::new(None),
            seed,
        }
    }

    /// The adb serial.
    pub fn serial(&self) -> &str {
        &self.seed.device.serial
    }

    /// The probe result (what the samplers were built from).
    pub fn probe(&self) -> Probe {
        self.probe.read().clone()
    }

    fn facts(seed: &SourceSeed, pairing: &str) -> ConnectionFacts {
        ConnectionFacts {
            serial: seed.device.serial.clone(),
            transport: Some(seed.transport()),
            tools: seed.adb_version.iter().cloned().collect(),
            pairing: Some(pairing.to_owned()),
        }
    }

    fn make_summary(
        seed: &SourceSeed,
        info: Option<&DeviceInfo>,
        state: ConnectionState,
    ) -> DeviceSummary {
        let model_name = info
            .and_then(|i| i.identity.marketing_name.clone())
            .or_else(|| seed.device.model_name())
            .or_else(|| seed.device.product.clone())
            .unwrap_or_else(|| seed.device.serial.clone());
        let name = info
            .and_then(|i| i.identity.name.clone())
            .unwrap_or_else(|| model_name.clone());
        DeviceSummary {
            id: device_id(&seed.device.serial),
            platform: Platform::Android,
            name,
            model_name,
            transport: seed.transport(),
            state,
        }
    }

    fn adb(&self) -> DeviceResult<AdbServer> {
        self.seed.adb.clone().ok_or_else(|| {
            DeviceError::new(ErrorCode::AdbNotFound, "no adb binary bound to this source")
                .with_hint("Install Android platform-tools and reconnect the device.")
        })
    }

    async fn refresh(&self) -> DeviceResult<()> {
        let probe = Probe::run(&*self.shell)
            .await
            .map_err(|e| shell_error(&self.id, e))?;
        let facts = Self::facts(&self.seed, "authorized");
        let info = read_info(&*self.shell, &probe, &facts)
            .await
            .map_err(|e| shell_error(&self.id, e))?;
        let caps = probe.capabilities();
        *self.summary.write() =
            Self::make_summary(&self.seed, Some(&info), self.state.read().clone());
        *self.probe.write() = probe;
        *self.capabilities.write() = caps.clone();
        *self.info.write() = Some(info.clone());
        let sink = self.sink.lock().clone();
        if let Some(sink) = sink {
            sink.capabilities(&self.id, caps)
                .await
                .map_err(|_| DeviceError::device_lost(&self.id))?;
            sink.info(&self.id, info)
                .await
                .map_err(|_| DeviceError::device_lost(&self.id))?;
        }
        Ok(())
    }

    fn spawn_bugreport(&self, path: String) -> DeviceResult<ActionResult> {
        let adb = self.adb()?;
        let sink = self.sink.lock().clone().ok_or_else(|| {
            DeviceError::new(ErrorCode::Internal, "source is not started")
                .with_hint("Connect the device before requesting a bugreport.")
        })?;
        let task_id = format!("bugreport:{}:{}", self.serial(), now_ms());
        let serial = self.serial().to_owned();
        let id = self.id.clone();
        let tid = task_id.clone();
        tokio::spawn(async move {
            let _ = sink
                .send(Message::Progress {
                    task_id: tid.clone(),
                    pct: 0.0,
                    message: format!("Collecting bugreport from {serial}…"),
                })
                .await;
            let result = adb
                .run(&["-s", &serial, "bugreport", &path], BUGREPORT_TIMEOUT_MS)
                .await;
            match result {
                Ok(out) if !out.to_ascii_lowercase().contains("error") => {
                    let _ = sink
                        .send(Message::Progress {
                            task_id: tid,
                            pct: 100.0,
                            message: format!("Bugreport saved to {path}"),
                        })
                        .await;
                }
                Ok(out) => {
                    let err = DeviceError::new(
                        ErrorCode::Io,
                        format!("bugreport failed: {}", out.trim()),
                    )
                    .with_hint("adb could not write the bugreport; check the path and try again.");
                    let _ = sink.error(Some(&id), &err).await;
                }
                Err(e) => {
                    let err = shell_error(&id, e);
                    let _ = sink.error(Some(&id), &err).await;
                }
            }
        });
        Ok(ActionResult::Started { task_id })
    }

    async fn pair_or_connect(&self, action: &DeviceAction) -> DeviceResult<ActionResult> {
        let adb = self.adb()?;
        let (out, ok_marker) = match action {
            DeviceAction::AdbPair { host, port, code } => {
                (adb.pair(host, *port, code).await, "Successfully paired")
            }
            DeviceAction::AdbConnect { host, port } => {
                (adb.connect(host, *port).await, "connected to")
            }
            _ => return Err(DeviceError::internal("not a pairing action")),
        };
        let out = out.map_err(|e| shell_error(&self.id, e))?;
        if out.contains(ok_marker) {
            Ok(ActionResult::Done)
        } else {
            Err(
                DeviceError::new(ErrorCode::Io, out.trim().to_owned()).with_hint(
                    "Check the pairing code and the IP:port shown under Wireless debugging.",
                ),
            )
        }
    }
}

/// Map a shell failure to a provider error with the right code.
pub fn shell_error(device_id: &str, e: ShellError) -> DeviceError {
    match e {
        ShellError::DeviceGone(_) => DeviceError::device_lost(device_id),
        ShellError::AdbNotFound(p) => {
            DeviceError::new(ErrorCode::AdbNotFound, format!("adb not found at {p}"))
                .with_hint("Install Android platform-tools or set STATASMON_ADB.")
        }
        ShellError::Timeout(ms) => DeviceError::new(
            ErrorCode::Io,
            format!("device command timed out after {ms} ms"),
        )
        .with_hint("The device is slow to answer; reconnect it if this persists."),
        ShellError::Server(s) => DeviceError::new(ErrorCode::AdbConflict, s)
            .with_hint("The adb server returned an error; restart it with `adb kill-server`."),
        ShellError::Io(err) => DeviceError::io("adb", err),
    }
}

#[async_trait]
impl DeviceSource for AndroidSource {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }

    fn platform(&self) -> Platform {
        Platform::Android
    }

    fn summary(&self) -> DeviceSummary {
        self.summary.read().clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities.read().clone()
    }

    async fn info(&self) -> DeviceResult<DeviceInfo> {
        let info = self.info.read().clone();
        if let Some(info) = info {
            return Ok(info);
        }
        Err(DeviceError::new(
            ErrorCode::NotTrusted,
            format!("{} is not authorized for USB debugging", self.id),
        )
        .with_hint("Accept the USB debugging prompt on the device."))
    }

    async fn start(&self, sink: EventSink, cfg: SamplingConfig) -> DeviceResult<()> {
        self.stop().await?;
        *self.sink.lock() = Some(sink.clone());
        let state = self.state.read().clone();
        let lost = |_| DeviceError::device_lost(&self.id);
        sink.state(&self.id, state.clone(), Some("adb shell ready".to_owned()))
            .await
            .map_err(lost)?;
        sink.capabilities(&self.id, self.capabilities())
            .await
            .map_err(lost)?;
        let info = self.info.read().clone();
        if let Some(info) = info {
            sink.info(&self.id, info).await.map_err(lost)?;
        }
        if state == ConnectionState::NeedsTrust {
            return Ok(());
        }

        let probe = self.probe.read().clone();
        let cancel = CancellationToken::new();
        let gone = Arc::new(AtomicBool::new(false));
        let mut tasks = Vec::new();

        let mut by_cadence: Vec<(Cadence, u64, Vec<Box<dyn Sampler>>)> = vec![
            (Cadence::Fast, u64::from(cfg.fast_ms), Vec::new()),
            (Cadence::Medium, u64::from(cfg.medium_ms), Vec::new()),
            (Cadence::Slow, u64::from(cfg.slow_ms), Vec::new()),
            (Cadence::Heavy, u64::from(cfg.heavy_ms), Vec::new()),
        ];
        for s in samplers::build(&probe) {
            if let Some(slot) = by_cadence.iter_mut().find(|(c, _, _)| *c == s.cadence()) {
                slot.2.push(s);
            }
        }
        for (cadence, period_ms, samplers) in by_cadence {
            if samplers.is_empty() || period_ms == 0 {
                continue;
            }
            tasks.push(tokio::spawn(run_cadence(
                cadence,
                period_ms.max(100),
                samplers,
                Arc::clone(&self.shell),
                sink.clone(),
                self.id.clone(),
                cancel.clone(),
                Arc::clone(&gone),
            )));
        }
        if probe.logcat {
            tasks.push(tokio::spawn(logcat::run(
                Arc::clone(&self.shell),
                sink.clone(),
                self.id.clone(),
                LogcatConfig::default(),
                cancel.child_token(),
            )));
        }
        *self.running.lock() = Some(Running { cancel, tasks });
        Ok(())
    }

    async fn stop(&self) -> DeviceResult<()> {
        let running = self.running.lock().take();
        if let Some(Running { cancel, tasks }) = running {
            cancel.cancel();
            for task in tasks {
                if tokio::time::timeout(STOP_GRACE, task).await.is_err() {
                    tracing::debug!(device = %self.id, "sampler task did not stop in time; aborting");
                }
            }
        }
        Ok(())
    }

    async fn run_action(&self, action: DeviceAction) -> DeviceResult<ActionResult> {
        match action {
            DeviceAction::RefreshInfo => {
                self.refresh().await?;
                Ok(ActionResult::Done)
            }
            DeviceAction::Bugreport { path } => self.spawn_bugreport(path),
            DeviceAction::AdbPair { .. } | DeviceAction::AdbConnect { .. } => {
                self.pair_or_connect(&action).await
            }
            DeviceAction::PcapStart { .. } | DeviceAction::PcapStop => {
                Err(DeviceError::unsupported("packet capture")
                    .with_hint("Packet capture needs root on Android"))
            }
            DeviceAction::FetchCrashReports => {
                Err(DeviceError::unsupported("fetching crash reports")
                    .with_hint("Crash reports arrive as events from dropbox (step 2.6)"))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_cadence(
    cadence: Cadence,
    period_ms: u64,
    mut samplers: Vec<Box<dyn Sampler>>,
    shell: Arc<dyn Shell>,
    sink: EventSink,
    id: DeviceId,
    cancel: CancellationToken,
    gone: Arc<AtomicBool>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(period_ms));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {}
        }
        for s in samplers.iter_mut() {
            if cancel.is_cancelled() {
                return;
            }
            match s.tick(&*shell, &sink, &id, now_ms()).await {
                Ok(()) => {}
                Err(ShellError::DeviceGone(_)) => {
                    if !gone.swap(true, Ordering::SeqCst) {
                        let err = DeviceError::device_lost(&id);
                        let _ = sink
                            .state(
                                &id,
                                ConnectionState::Disconnected,
                                Some("adb lost the device".to_owned()),
                            )
                            .await;
                        let _ = sink.error(Some(&id), &err).await;
                        cancel.cancel();
                    }
                    return;
                }
                Err(e) => {
                    tracing::warn!(device = %id, sampler = s.name(), cadence = ?cadence, error = %e, "tick failed");
                }
            }
        }
        if sink.is_closed() {
            return;
        }
    }
}

impl Drop for AndroidSource {
    fn drop(&mut self) {
        if let Some(Running { cancel, tasks }) = self.running.lock().take() {
            cancel.cancel();
            for t in tasks {
                t.abort();
            }
        }
    }
}
