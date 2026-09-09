//! `IosSource`: one iOS / iPadOS device behind the [`DeviceSource`] trait
//! (steps 3.1–3.4 for tier 1, 4.1–4.6 for tier 2; docs/PLAN.md §3.7).
//!
//! Construction reads the info sheet once (`connect`) so `summary()`,
//! `capabilities()` and `info()` are ready before the engine sees the
//! source. `start` emits `DeviceState Tier1`, `Capabilities`, `Info` and
//! spawns the battery loop (medium cadence), the storage loop (slow), the
//! os_trace stream (`ostrace::run`) and a crash-report poll; any loop that
//! finds the device gone ends everything with one `DeviceState
//! Disconnected` + `Error DeviceLost`. An `untrusted` source is what the
//! discovery attaches while the Trust dialog is pending.
//!
//! Tier 2 ([`supervise`]): when Developer Mode is on the supervisor opens
//! the developer tunnel ([`Tunnel`]), reads `deviceinfo`, emits
//! `DeviceState Tier2` with the tier-2 capabilities and info, and runs the
//! sysmontap (fast), networking (medium) and optional energy loops plus a
//! slow `deviceinfo` refresh. When the tunnel dies the source degrades to
//! tier 1 with one `Error TunnelFailed` and retries with backoff
//! ([`TUNNEL_BACKOFF_MIN`] → [`TUNNEL_BACKOFF_MAX`]); while Developer Mode
//! is off it is re-read every [`DEV_MODE_RECHECK`] so turning it on is
//! picked up without replugging.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use idevice::pairing_file::PairingFile;
use idevice::provider::IdeviceProvider;
use parking_lot::{Mutex, RwLock};
use statasmon_device::{ActionResult, DeviceError, DeviceResult, DeviceSource, EventSink, now_ms};
use statasmon_protocol::{
    Availability, Capability, CapabilitySet, ConnectionState, DeviceAction, DeviceId, DeviceInfo,
    DeviceSummary, ErrorCode, EventKind, Message, Platform, SamplingConfig, Transport,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::crash::{self, CrashReport};
use crate::dvt::deviceinfo::{self as dvt_info, DeviceFacts};
use crate::dvt::{self, LoopEnd, SharedState, Tier2Shared};
use crate::error::IosError;
use crate::info::{self, ConnectionFacts, InfoInputs};
use crate::muxd::IosProvider;
use crate::storage::{AppSize, AppTable};
use crate::tunnel::{self, Tunnel};
use crate::{battery, lockdown, ostrace, pcap, storage};

/// How long `stop` waits for the loops before aborting them.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// Crash-report poll interval.
pub const CRASH_POLL: Duration = Duration::from_secs(60);
/// Tier label shown under `DeviceInfo::connection.tier`.
pub const TIER1: &str = "tier 1 (no tunnel)";
/// Tier-2 label (`dvt::deviceinfo::TIER2`).
pub const TIER2: &str = dvt_info::TIER2;
/// While tier 1 because Developer Mode is off, re-read it this often.
pub const DEV_MODE_RECHECK: Duration = Duration::from_secs(30);
/// Retry delay after a tunnel failure, doubled up to the maximum.
pub const TUNNEL_BACKOFF_MIN: Duration = Duration::from_secs(5);
pub const TUNNEL_BACKOFF_MAX: Duration = Duration::from_secs(60);

/// Reasons for the tier-1 capability gaps (user-facing, docs/PLAN.md §2).
pub mod reasons {
    pub const DEV_MODE_OFF: &str =
        "Developer Mode is off on this device (Settings → Privacy & Security → Developer Mode)";
    pub const NEEDS_TUNNEL: &str = "Needs the developer tunnel (Phase 4)";
    pub const NO_FREQUENCY: &str = "iOS does not report core frequencies to the host";
    pub const BATTERY_TEMP_ONLY: &str = "Only the battery temperature is readable on iOS";
    pub const WIFI_DETAILS: &str = "Not readable without the developer tunnel";
    pub const STORAGE_CATEGORIES: &str =
        "iOS exposes only per-app and system sizes to the host (no media/other split)";
    pub const BATTERY_DETAIL_REFUSED: &str =
        "The device refused the IORegistry battery query (diagnostics_relay)";
    pub const THERMAL_FROM_LOG: &str = "Derived from the device log (thermalmonitord)";
    pub const TUNNEL_FAILED: &str = "The developer tunnel could not be opened; retrying (unplug and reconnect the device if it persists)";
    pub const TUNNEL_LOST: &str = "The developer tunnel dropped; reconnecting";
    pub const NO_FRONT_APP: &str = "The developer services do not report the front-most app";
    pub const WIFI_DETAILS_TIER2: &str = "Not exposed by the developer services";
    pub const ENERGY_OFF: &str = "The energy gauge is off by default (it breaks on some iOS 18 builds); enable it in the device settings";
    pub const ENERGY_FAILED: &str = "The energy gauge failed on this device; labels stay off";
    pub const NO_LOAD_AVERAGE: &str = "iOS does not report a load average to the host";
    pub const NO_MEMORY_PRESSURE: &str =
        "sysmontap reports no memory-pressure level; memory warnings come from the device log";
    pub const NETWORKING_FAILED: &str =
        "The DVT networking service failed on this device; system totals still come from sysmontap";
}

/// The device id for a UDID.
pub fn device_id(udid: &str) -> DeviceId {
    format!("ios:{udid}")
}

/// The app-data root: `STATASMON_DATA_DIR`, else the platform folder
/// (`~/Library/Application Support`, `%LOCALAPPDATA%`, `$XDG_DATA_HOME` /
/// `~/.local/share`), else the temp dir.
pub fn app_data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("STATASMON_DATA_DIR") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library").join("Application Support"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".local").join("share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("StatasMon")
}

/// What the discovery knows before pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosSeed {
    pub udid: String,
    pub mux_id: u32,
    pub transport: Transport,
    /// Where crash reports are copied (`<app-data>/StatasMon/crashes/<udid>`).
    pub crash_dir: PathBuf,
}

impl IosSeed {
    /// A seed with the default crash directory.
    pub fn new(udid: &str, mux_id: u32, transport: Transport) -> Self {
        Self {
            udid: udid.to_owned(),
            mux_id,
            transport,
            crash_dir: app_data_dir().join("crashes").join(udid),
        }
    }
}

/// The tier-1 capability set. `developer_mode` decides the reason for the
/// DVT gaps; `battery_detail` is whether IORegistry answered.
pub fn tier1_capabilities(developer_mode: Option<bool>, battery_detail: bool) -> CapabilitySet {
    let dvt_reason = match developer_mode {
        Some(false) => reasons::DEV_MODE_OFF,
        _ => reasons::NEEDS_TUNNEL,
    };
    capabilities_for(dvt_reason, battery_detail)
}

/// The tier-2 capability set: everything sysmontap, networking and
/// deviceinfo provide; the gaps carry what the developer services still
/// do not report. `energy` is whether the gauge is enabled.
pub fn tier2_capabilities(battery_detail: bool, energy: bool) -> CapabilitySet {
    use Capability as C;
    let mut set = capabilities_for(reasons::NEEDS_TUNNEL, battery_detail);
    for cap in [
        C::CpuTotal,
        C::CpuPerCore,
        C::MemoryBreakdown,
        C::Processes,
        C::ProcessThreads,
        C::NetworkThroughput,
        C::NetworkPackets,
        C::NetworkPerProcess,
    ] {
        set.set(cap, Availability::Available);
    }
    let unavailable = |reason: &str| Availability::Unavailable {
        reason: reason.to_owned(),
    };
    set.set(C::LoadAverage, unavailable(reasons::NO_LOAD_AVERAGE));
    set.set(C::MemoryPressure, unavailable(reasons::NO_MEMORY_PRESSURE));
    set.set(C::ForegroundApp, unavailable(reasons::NO_FRONT_APP));
    set.set(C::WifiDetails, unavailable(reasons::WIFI_DETAILS_TIER2));
    set.set(
        C::Energy,
        if energy {
            Availability::Available
        } else {
            unavailable(reasons::ENERGY_OFF)
        },
    );
    set
}

/// The tier-1 shape with `dvt_reason` on every developer-service gap.
pub fn capabilities_for(dvt_reason: &str, battery_detail: bool) -> CapabilitySet {
    use Capability as C;
    let mut set = CapabilitySet::default();
    let mut put = |cap: Capability, ok: bool, reason: &str| {
        set.set(
            cap,
            if ok {
                Availability::Available
            } else {
                Availability::Unavailable {
                    reason: reason.to_owned(),
                }
            },
        );
    };
    put(C::DeviceInfo, true, "");
    put(C::BatteryBasic, true, "");
    put(
        C::BatteryDetail,
        battery_detail,
        reasons::BATTERY_DETAIL_REFUSED,
    );
    put(C::StorageTotals, true, "");
    put(C::StoragePerApp, true, "");
    put(C::StorageCategories, false, reasons::STORAGE_CATEGORIES);
    put(C::Logs, true, "");
    put(C::PerAppLogs, true, "");
    put(C::CrashReports, true, "");
    put(C::PacketCapture, true, "");
    put(C::ThermalState, true, reasons::THERMAL_FROM_LOG);
    put(C::MemoryWarnings, true, reasons::THERMAL_FROM_LOG);
    for cap in [
        C::CpuTotal,
        C::CpuPerCore,
        C::LoadAverage,
        C::MemoryBreakdown,
        C::MemoryPressure,
        C::Processes,
        C::ProcessThreads,
        C::NetworkThroughput,
        C::NetworkPackets,
        C::NetworkPerProcess,
        C::ForegroundApp,
        C::Energy,
    ] {
        put(cap, false, dvt_reason);
    }
    put(C::CpuFrequency, false, reasons::NO_FREQUENCY);
    put(C::TemperatureSensors, false, reasons::BATTERY_TEMP_ONLY);
    put(C::WifiDetails, false, reasons::WIFI_DETAILS);
    set
}

/// State shared between the source, the discovery's [`TrustHandle`] and
/// the tier-2 supervisor.
#[derive(Debug)]
struct Shared {
    id: DeviceId,
    state: RwLock<ConnectionState>,
    summary: RwLock<DeviceSummary>,
    capabilities: RwLock<CapabilitySet>,
    sink: Mutex<Option<EventSink>>,
    info: RwLock<Option<DeviceInfo>>,
    developer_mode: RwLock<Option<bool>>,
    battery_detail: RwLock<bool>,
    /// Energy gauge opt-in (off by default).
    energy: AtomicBool,
    /// The last deviceinfo facts and tunnel description while tier 2 is up.
    tier2: Mutex<Option<Tier2Facts>>,
}

#[derive(Debug, Clone)]
struct Tier2Facts {
    facts: DeviceFacts,
    ram_fallback: Option<u64>,
    tunnel: String,
}

impl Shared {
    fn set_state(&self, state: ConnectionState) {
        *self.state.write() = state.clone();
        self.summary.write().state = state;
    }
}

/// Lets the discovery update an untrusted source's state and reason while
/// the Trust dialog is pending (pushed through the source's sink once the
/// engine started it).
#[derive(Debug, Clone)]
pub struct TrustHandle {
    shared: Arc<Shared>,
}

impl TrustHandle {
    /// Set the connection state and the reason on every capability.
    pub async fn update(&self, state: ConnectionState, reason: &str) {
        *self.shared.state.write() = state.clone();
        self.shared.summary.write().state = state.clone();
        let caps = CapabilitySet::none(reason);
        *self.shared.capabilities.write() = caps.clone();
        let sink = self.shared.sink.lock().clone();
        if let Some(sink) = sink {
            let _ = sink
                .state(&self.shared.id, state, Some(reason.to_owned()))
                .await;
            let _ = sink.capabilities(&self.shared.id, caps).await;
        }
    }
}

struct Running {
    cancel: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

struct PcapRun {
    cancel: CancellationToken,
    task_id: String,
}

/// One iOS device.
pub struct IosSource {
    seed: IosSeed,
    provider: Arc<IosProvider>,
    pair: Option<PairingFile>,
    shared: Arc<Shared>,
    apps: AppTable,
    running: Mutex<Option<Running>>,
    pcap: Mutex<Option<PcapRun>>,
}

impl std::fmt::Debug for IosSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IosSource")
            .field("id", &self.shared.id)
            .field("state", &*self.shared.state.read())
            .finish_non_exhaustive()
    }
}

impl IosSource {
    /// Read the info sheet and build the tier-1 source. Fails when lockdown
    /// itself fails (device gone, record rejected).
    pub async fn connect(
        provider: Arc<IosProvider>,
        pair: PairingFile,
        seed: IosSeed,
    ) -> Result<Self, IosError> {
        let inputs = info::read_inputs(&*provider, &pair).await?;
        Ok(Self::from_inputs(provider, Some(pair), seed, &inputs))
    }

    /// Build a source from already-read inputs (fixture tests).
    pub fn from_inputs(
        provider: Arc<IosProvider>,
        pair: Option<PairingFile>,
        seed: IosSeed,
        inputs: &InfoInputs,
    ) -> Self {
        let sheet = info::build_info(inputs, &Self::facts(&seed, "paired"));
        let battery_detail = inputs.ioregistry_battery.is_some();
        let caps = tier1_capabilities(inputs.developer_mode, battery_detail);
        let summary = Self::make_summary(&seed, Some(&sheet), ConnectionState::Tier1);
        Self {
            shared: Arc::new(Shared {
                id: device_id(&seed.udid),
                state: RwLock::new(ConnectionState::Tier1),
                summary: RwLock::new(summary),
                capabilities: RwLock::new(caps),
                sink: Mutex::new(None),
                info: RwLock::new(Some(sheet)),
                developer_mode: RwLock::new(inputs.developer_mode),
                battery_detail: RwLock::new(battery_detail),
                energy: AtomicBool::new(false),
                tier2: Mutex::new(None),
            }),
            provider,
            pair,
            apps: Arc::new(RwLock::new(Vec::new())),
            running: Mutex::new(None),
            pcap: Mutex::new(None),
            seed,
        }
    }

    /// A source for a device that has not trusted this computer: state
    /// `NeedsTrust`, every capability unavailable with `reason`.
    pub fn untrusted(provider: Arc<IosProvider>, seed: IosSeed, reason: &str) -> Self {
        let summary = Self::make_summary(&seed, None, ConnectionState::NeedsTrust);
        Self {
            shared: Arc::new(Shared {
                id: device_id(&seed.udid),
                state: RwLock::new(ConnectionState::NeedsTrust),
                summary: RwLock::new(summary),
                capabilities: RwLock::new(CapabilitySet::none(reason)),
                sink: Mutex::new(None),
                info: RwLock::new(None),
                developer_mode: RwLock::new(None),
                battery_detail: RwLock::new(false),
                energy: AtomicBool::new(false),
                tier2: Mutex::new(None),
            }),
            provider,
            pair: None,
            apps: Arc::new(RwLock::new(Vec::new())),
            running: Mutex::new(None),
            pcap: Mutex::new(None),
            seed,
        }
    }

    /// The handle the discovery keeps for an untrusted source.
    pub fn trust_handle(&self) -> TrustHandle {
        TrustHandle {
            shared: Arc::clone(&self.shared),
        }
    }

    /// The UDID.
    pub fn udid(&self) -> &str {
        &self.seed.udid
    }

    /// The provider (what `ostrace::run` / `crash::fetch` / `pcap::capture`
    /// receive).
    pub fn provider(&self) -> Arc<dyn IdeviceProvider> {
        Arc::clone(&self.provider) as Arc<dyn IdeviceProvider>
    }

    /// Latest per-app sizes from installation_proxy, largest first.
    pub fn app_sizes(&self) -> Vec<AppSize> {
        self.apps.read().clone()
    }

    /// Developer Mode as last read.
    pub fn developer_mode(&self) -> Option<bool> {
        *self.shared.developer_mode.read()
    }

    /// Turn the optional energy gauge on or off (off by default; takes
    /// effect at the next tunnel open, or at once if the source is at tier
    /// 1). See `dvt::energy`.
    pub fn set_energy_enabled(&self, on: bool) {
        self.shared.energy.store(on, Ordering::SeqCst);
    }

    /// Whether the energy gauge is enabled.
    pub fn energy_enabled(&self) -> bool {
        self.shared.energy.load(Ordering::SeqCst)
    }

    /// The last `deviceinfo` facts while tier 2 is up.
    pub fn tier2_facts(&self) -> Option<DeviceFacts> {
        self.shared.tier2.lock().as_ref().map(|t| t.facts.clone())
    }

    fn facts(seed: &IosSeed, pairing: &str) -> ConnectionFacts {
        ConnectionFacts {
            udid: seed.udid.clone(),
            transport: Some(seed.transport),
            pairing: Some(pairing.to_owned()),
            tier: Some(TIER1.to_owned()),
            tunnel: Some("not attempted at tier 1".to_owned()),
        }
    }

    fn make_summary(
        seed: &IosSeed,
        info: Option<&DeviceInfo>,
        state: ConnectionState,
    ) -> DeviceSummary {
        let model_name = info
            .and_then(|i| {
                i.identity
                    .marketing_name
                    .clone()
                    .or_else(|| i.identity.model_identifier.clone())
            })
            .unwrap_or_else(|| "Apple device".to_owned());
        let name = info
            .and_then(|i| i.identity.name.clone())
            .unwrap_or_else(|| model_name.clone());
        DeviceSummary {
            id: device_id(&seed.udid),
            platform: Platform::Ios,
            name,
            model_name,
            transport: seed.transport,
            state,
        }
    }

    fn id_str(&self) -> &str {
        &self.shared.id
    }

    fn pair(&self) -> DeviceResult<PairingFile> {
        self.pair.clone().ok_or_else(|| {
            DeviceError::new(
                ErrorCode::NotTrusted,
                format!("{} has not trusted this computer", self.shared.id),
            )
            .with_hint("Accept \"Trust This Computer\" on the device.")
        })
    }

    fn sink(&self) -> DeviceResult<EventSink> {
        self.shared.sink.lock().clone().ok_or_else(|| {
            DeviceError::new(ErrorCode::Internal, "source is not started")
                .with_hint("Connect the device before running actions.")
        })
    }

    async fn refresh(&self) -> DeviceResult<()> {
        let pair = self.pair()?;
        let inputs = info::read_inputs(&*self.provider, &pair)
            .await
            .map_err(|e| to_device_error(self.id_str(), e))?;
        let mut sheet = info::build_info(&inputs, &Self::facts(&self.seed, "paired"));
        let detail = inputs.ioregistry_battery.is_some() || *self.shared.battery_detail.read();
        let tier2 = self.shared.tier2.lock().clone();
        let caps = match (&*self.shared.state.read(), &tier2) {
            (ConnectionState::Tier2, Some(t)) => {
                dvt_info::apply(&mut sheet, &t.facts, t.ram_fallback, &t.tunnel);
                tier2_capabilities(detail, self.energy_enabled())
            }
            _ => tier1_capabilities(inputs.developer_mode, detail),
        };
        *self.shared.developer_mode.write() = inputs.developer_mode;
        *self.shared.battery_detail.write() = detail;
        *self.shared.summary.write() =
            Self::make_summary(&self.seed, Some(&sheet), self.shared.state.read().clone());
        *self.shared.capabilities.write() = caps.clone();
        *self.shared.info.write() = Some(sheet.clone());
        let sink = self.shared.sink.lock().clone();
        if let Some(sink) = sink {
            let lost = |_| DeviceError::device_lost(self.id_str());
            sink.capabilities(self.id_str(), caps).await.map_err(lost)?;
            sink.info(self.id_str(), sheet).await.map_err(lost)?;
        }
        Ok(())
    }

    /// Fetch crash reports now and emit one `Crash` event per new report.
    async fn fetch_crashes(&self, since: &mut Option<i64>) -> DeviceResult<usize> {
        let sink = self.sink()?;
        let reports = crash::fetch(&*self.provider, &self.seed.crash_dir, *since)
            .await
            .map_err(|e| to_device_error(self.id_str(), e))?;
        let n = reports.len();
        for r in &reports {
            if let Some(ts) = r.ts_ms {
                *since = Some(since.map_or(ts, |s| s.max(ts)));
            }
            sink.event(
                self.id_str(),
                r.ts_ms.unwrap_or_else(now_ms),
                EventKind::Crash,
                crash_detail(r),
                r.pid,
                r.process.clone(),
            )
            .await
            .map_err(|_| DeviceError::device_lost(self.id_str()))?;
        }
        Ok(n)
    }

    fn start_pcap(&self, path: String) -> DeviceResult<ActionResult> {
        let sink = self.sink()?;
        if self.pcap.lock().is_some() {
            return Err(
                DeviceError::new(ErrorCode::Io, "a packet capture is already running")
                    .with_hint("Stop the current capture before starting another."),
            );
        }
        let cancel = CancellationToken::new();
        let task_id = format!("pcap:{}:{}", self.seed.udid, now_ms());
        let provider = self.provider();
        let id = self.shared.id.clone();
        let tid = task_id.clone();
        let child = cancel.clone();
        *self.pcap.lock() = Some(PcapRun {
            cancel,
            task_id: task_id.clone(),
        });
        tokio::spawn(async move {
            let _ = sink
                .send(Message::Progress {
                    task_id: tid.clone(),
                    pct: 0.0,
                    message: format!("Capturing packets to {path}…"),
                })
                .await;
            match pcap::capture(provider, PathBuf::from(&path), child).await {
                Ok(packets) => {
                    let _ = sink
                        .send(Message::Progress {
                            task_id: tid,
                            pct: 100.0,
                            message: format!("Capture saved to {path} ({packets} packets)"),
                        })
                        .await;
                }
                Err(e) => {
                    let _ = sink.error(Some(&id), &to_device_error(&id, e)).await;
                }
            }
        });
        Ok(ActionResult::Started { task_id })
    }

    fn stop_pcap(&self) -> ActionResult {
        if let Some(run) = self.pcap.lock().take() {
            run.cancel.cancel();
            tracing::info!(task = %run.task_id, "pcap stopped");
        }
        ActionResult::Done
    }
}

/// "<process> — <reason>" (falls back to the file name).
pub fn crash_detail(r: &CrashReport) -> String {
    detail_text(r.process.as_deref(), r.reason.as_deref(), &r.name)
}

fn detail_text(process: Option<&str>, reason: Option<&str>, name: &str) -> String {
    match (process, reason) {
        (Some(p), Some(reason)) => format!("{p} — {reason}"),
        (Some(p), None) => p.to_owned(),
        (None, Some(reason)) => reason.to_owned(),
        (None, None) => name.to_owned(),
    }
}

/// Map a provider error to the bus error with the right code and hint.
pub fn to_device_error(device_id: &str, e: IosError) -> DeviceError {
    if battery::is_device_gone(&e) {
        return DeviceError::device_lost(device_id);
    }
    let code = e.code();
    let hint = e.hint();
    let err = DeviceError::new(code, e.to_string());
    match hint {
        Some(h) => err.with_hint(h),
        None => err.with_hint(match code {
            ErrorCode::Parse => "The device sent data we do not understand yet; please report it.",
            _ => "The device service failed; reconnect the device if this persists.",
        }),
    }
}

#[async_trait]
impl DeviceSource for IosSource {
    fn id(&self) -> DeviceId {
        self.shared.id.clone()
    }

    fn platform(&self) -> Platform {
        Platform::Ios
    }

    fn summary(&self) -> DeviceSummary {
        self.shared.summary.read().clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.shared.capabilities.read().clone()
    }

    async fn info(&self) -> DeviceResult<DeviceInfo> {
        if let Some(info) = self.shared.info.read().clone() {
            return Ok(info);
        }
        Err(self
            .pair()
            .err()
            .unwrap_or_else(|| DeviceError::internal("info sheet missing on a paired source")))
    }

    #[allow(clippy::vec_init_then_push)]
    async fn start(&self, sink: EventSink, cfg: SamplingConfig) -> DeviceResult<()> {
        self.stop().await?;
        *self.shared.sink.lock() = Some(sink.clone());
        let id = self.shared.id.clone();
        let state = self.shared.state.read().clone();
        let lost = |_| DeviceError::device_lost(&id);
        let detail = match &state {
            ConnectionState::NeedsTrust => self
                .capabilities()
                .reason(Capability::DeviceInfo)
                .map(str::to_owned),
            _ => Some("lockdown session ready".to_owned()),
        };
        sink.state(&id, state.clone(), detail).await.map_err(lost)?;
        sink.capabilities(&id, self.capabilities())
            .await
            .map_err(lost)?;
        let info = self.shared.info.read().clone();
        if let Some(info) = info {
            sink.info(&id, info).await.map_err(lost)?;
        }
        let Some(pair) = self.pair.clone() else {
            return Ok(());
        };

        let cancel = CancellationToken::new();
        let gone = Arc::new(AtomicBool::new(false));
        let provider = self.provider();
        let mut tasks = Vec::new();
        tasks.push(tokio::spawn(battery::run(
            Arc::clone(&provider),
            pair.clone(),
            sink.clone(),
            id.clone(),
            Duration::from_millis(u64::from(cfg.medium_ms)),
            *self.shared.battery_detail.read(),
            cancel.clone(),
            Arc::clone(&gone),
        )));
        tasks.push(tokio::spawn(storage::run(
            Arc::clone(&provider),
            pair.clone(),
            sink.clone(),
            id.clone(),
            Duration::from_millis(u64::from(cfg.slow_ms)),
            true,
            Arc::clone(&self.apps),
            cancel.clone(),
            Arc::clone(&gone),
        )));
        tasks.push(tokio::spawn(ostrace::run(
            Arc::clone(&provider),
            sink.clone(),
            id.clone(),
            cancel.child_token(),
        )));
        tasks.push(tokio::spawn(crash_poll(
            Arc::clone(&provider),
            self.seed.crash_dir.clone(),
            sink.clone(),
            id.clone(),
            cancel.child_token(),
        )));
        tasks.push(tokio::spawn(supervise(Tier2Ctx {
            provider: Arc::clone(&self.provider),
            shared: Arc::clone(&self.shared),
            sink: sink.clone(),
            cfg,
            cancel: cancel.child_token(),
            gone: Arc::clone(&gone),
            root: cancel.clone(),
        })));
        // Watchdog: one Disconnected + DeviceLost when a loop saw the device go.
        {
            let sink = sink.clone();
            let id = id.clone();
            let cancel = cancel.clone();
            let gone = Arc::clone(&gone);
            let shared = Arc::clone(&self.shared);
            tasks.push(tokio::spawn(async move {
                cancel.cancelled().await;
                if gone.load(Ordering::SeqCst) {
                    shared.set_state(ConnectionState::Disconnected);
                    *shared.tier2.lock() = None;
                    let _ = sink
                        .state(
                            &id,
                            ConnectionState::Disconnected,
                            Some("usbmuxd lost the device".to_owned()),
                        )
                        .await;
                    let _ = sink.error(Some(&id), &DeviceError::device_lost(&id)).await;
                }
            }));
        }
        *self.running.lock() = Some(Running { cancel, tasks });
        Ok(())
    }

    async fn stop(&self) -> DeviceResult<()> {
        self.stop_pcap();
        let running = self.running.lock().take();
        if let Some(Running { cancel, tasks }) = running {
            cancel.cancel();
            for task in tasks {
                if tokio::time::timeout(STOP_GRACE, task).await.is_err() {
                    tracing::debug!(device = %self.shared.id, "task did not stop in time; aborting");
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
            DeviceAction::FetchCrashReports => {
                self.pair()?;
                let mut since = None;
                let n = self.fetch_crashes(&mut since).await?;
                tracing::info!(device = %self.shared.id, reports = n, "crash reports fetched");
                Ok(ActionResult::Done)
            }
            DeviceAction::PcapStart { path } => {
                self.pair()?;
                self.start_pcap(path)
            }
            DeviceAction::PcapStop => Ok(self.stop_pcap()),
            DeviceAction::Bugreport { .. } => Err(DeviceError::unsupported("bugreport")
                .with_hint("Use FetchCrashReports on iOS; there is no bugreport equivalent.")),
            DeviceAction::AdbPair { .. } | DeviceAction::AdbConnect { .. } => {
                Err(DeviceError::unsupported("adb pairing")
                    .with_hint("iOS devices pair through usbmuxd; there is nothing to enter."))
            }
        }
    }
}

/// Poll crash reports every [`CRASH_POLL`], emitting one event per report.
async fn crash_poll(
    provider: Arc<dyn IdeviceProvider>,
    dir: PathBuf,
    sink: EventSink,
    device_id: DeviceId,
    cancel: CancellationToken,
) {
    let mut since: Option<i64> = Some(now_ms());
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut interval = tokio::time::interval(CRASH_POLL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = interval.tick() => {}
        }
        if sink.is_closed() {
            return;
        }
        match crash::fetch(&*provider, &dir, since).await {
            Ok(reports) => {
                for r in reports {
                    if !seen.insert(r.name.clone()) {
                        continue;
                    }
                    if let Some(ts) = r.ts_ms {
                        since = Some(since.map_or(ts, |s| s.max(ts)));
                    }
                    let detail = crash_detail(&r);
                    if sink
                        .event(
                            &device_id,
                            r.ts_ms.unwrap_or_else(now_ms),
                            EventKind::Crash,
                            detail,
                            r.pid,
                            r.process,
                        )
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Err(e) if battery::is_device_gone(&e) => return,
            Err(e) => tracing::debug!(device = %device_id, error = %e, "crash poll failed"),
        }
    }
}

/// What the tier-2 supervisor needs.
struct Tier2Ctx {
    provider: Arc<IosProvider>,
    shared: Arc<Shared>,
    sink: EventSink,
    cfg: SamplingConfig,
    /// Cancelled by `stop` (child of the source's token).
    cancel: CancellationToken,
    gone: Arc<AtomicBool>,
    /// The source's token, cancelled when the device is gone.
    root: CancellationToken,
}

impl Tier2Ctx {
    fn id(&self) -> &str {
        &self.shared.id
    }

    async fn wait(&self, d: Duration) -> bool {
        tokio::select! {
            _ = self.cancel.cancelled() => false,
            _ = tokio::time::sleep(d) => !self.sink.is_closed(),
        }
    }

    fn device_gone(&self) {
        self.gone.store(true, Ordering::SeqCst);
        self.root.cancel();
    }

    /// Store and emit a capability set.
    async fn set_capabilities(&self, caps: CapabilitySet) {
        *self.shared.capabilities.write() = caps.clone();
        let _ = self.sink.capabilities(self.id(), caps).await;
    }

    /// Mark some capabilities unavailable with `reason` and emit.
    async fn withdraw(&self, caps: &[Capability], reason: &str) {
        let set = {
            let mut set = self.shared.capabilities.write();
            for cap in caps {
                set.set(
                    *cap,
                    Availability::Unavailable {
                        reason: reason.to_owned(),
                    },
                );
            }
            set.clone()
        };
        let _ = self.sink.capabilities(self.id(), set).await;
    }

    async fn set_state(&self, state: ConnectionState, detail: String) {
        self.shared.set_state(state.clone());
        let _ = self.sink.state(self.id(), state, Some(detail)).await;
    }

    async fn emit_info(&self) {
        let info = self.shared.info.read().clone();
        if let Some(info) = info {
            let _ = self.sink.info(self.id(), info).await;
        }
    }

    /// Developer Mode changed (or was learned): update the sheet and, at
    /// tier 1, the capability reasons.
    async fn set_developer_mode(&self, on: Option<bool>) {
        *self.shared.developer_mode.write() = on;
        if let Some(info) = self.shared.info.write().as_mut() {
            info.state.developer_mode = on;
        }
        if *self.shared.state.read() != ConnectionState::Tier2 {
            let caps = tier1_capabilities(on, *self.shared.battery_detail.read());
            self.set_capabilities(caps).await;
        }
        self.emit_info().await;
    }

    /// Overlay the deviceinfo facts on the sheet, remember them, emit.
    async fn apply_tier2_info(&self, facts: &DeviceFacts, ram_fallback: Option<u64>, tunnel: &str) {
        {
            let mut guard = self.shared.info.write();
            if let Some(info) = guard.as_mut() {
                dvt_info::apply(info, facts, ram_fallback, tunnel);
            }
        }
        *self.shared.tier2.lock() = Some(Tier2Facts {
            facts: facts.clone(),
            ram_fallback,
            tunnel: tunnel.to_owned(),
        });
        self.emit_info().await;
    }

    /// Back to tier 1 after the tunnel died: state, capabilities, one error.
    async fn degrade(&self, why: &str, retry_in: Duration) {
        *self.shared.tier2.lock() = None;
        {
            let mut guard = self.shared.info.write();
            if let Some(info) = guard.as_mut() {
                info.connection.tier = Some(TIER1.to_owned());
                info.connection.tunnel = Some(format!("lost: {why}"));
            }
        }
        let reason = format!("{} ({why})", reasons::TUNNEL_LOST);
        let caps = capabilities_for(&reason, *self.shared.battery_detail.read());
        self.set_state(
            ConnectionState::Tier1,
            format!(
                "developer tunnel lost: {why}; retrying in {} s",
                retry_in.as_secs()
            ),
        )
        .await;
        self.set_capabilities(caps).await;
        self.emit_info().await;
        let err = DeviceError::new(
            ErrorCode::TunnelFailed,
            format!("the developer tunnel dropped: {why}"),
        )
        .with_hint(tunnel::hints::RETRY);
        let _ = self.sink.error(Some(self.id()), &err).await;
    }
}

/// The tier-2 supervisor (see the module docs).
async fn supervise(ctx: Tier2Ctx) {
    let mut backoff = TUNNEL_BACKOFF_MIN;
    let mut failure_reported = false;
    loop {
        if ctx.cancel.is_cancelled() || ctx.sink.is_closed() {
            return;
        }
        let dev = *ctx.shared.developer_mode.read();
        if dev != Some(true) {
            if !ctx.wait(DEV_MODE_RECHECK).await {
                return;
            }
            match lockdown::developer_mode_status(&*ctx.provider).await {
                Ok(on) => {
                    if Some(on) != dev {
                        tracing::info!(device = %ctx.id(), on, "Developer Mode changed");
                        ctx.set_developer_mode(Some(on)).await;
                    }
                }
                Err(e) if battery::is_device_gone(&e) => {
                    ctx.device_gone();
                    return;
                }
                Err(e) => {
                    tracing::debug!(device = %ctx.id(), error = %e, "Developer Mode re-check failed")
                }
            }
            continue;
        }

        tracing::info!(device = %ctx.id(), "opening the developer tunnel");
        match Tunnel::open(Arc::clone(&ctx.provider)).await {
            Ok(t) => {
                backoff = TUNNEL_BACKOFF_MIN;
                failure_reported = false;
                let tunnel = Arc::new(t);
                let end = run_tier2(&ctx, Arc::clone(&tunnel)).await;
                match Arc::try_unwrap(tunnel) {
                    Ok(t) => t.close().await,
                    Err(t) => t.mark_dead("closed by the supervisor"),
                }
                match end {
                    LoopEnd::Cancelled | LoopEnd::SinkClosed => return,
                    LoopEnd::TunnelDead(why) | LoopEnd::ServiceFailed(why) => {
                        if ctx.cancel.is_cancelled() {
                            return;
                        }
                        ctx.degrade(&why, backoff).await;
                    }
                }
            }
            Err(IosError::DeveloperModeOff) => {
                tracing::info!(device = %ctx.id(), "RSD lists no developer services: Developer Mode is off");
                ctx.set_developer_mode(Some(false)).await;
                continue;
            }
            Err(e) if battery::is_device_gone(&e) => {
                ctx.device_gone();
                return;
            }
            Err(e) => {
                tracing::warn!(device = %ctx.id(), error = %e, retry_s = backoff.as_secs(), "developer tunnel failed");
                if !failure_reported {
                    failure_reported = true;
                    let err = to_device_error(ctx.id(), e).with_hint(tunnel::hints::RETRY);
                    let _ = ctx.sink.error(Some(ctx.id()), &err).await;
                    let caps =
                        capabilities_for(reasons::TUNNEL_FAILED, *ctx.shared.battery_detail.read());
                    ctx.set_capabilities(caps).await;
                    ctx.set_state(
                        ConnectionState::Tier1,
                        format!("developer tunnel failed: {}", err.message()),
                    )
                    .await;
                }
            }
        }
        if !ctx.wait(backoff).await {
            return;
        }
        backoff = (backoff * 2).min(TUNNEL_BACKOFF_MAX);
    }
}

/// Tier 2 on an open tunnel until it (or the sysmontap service) fails.
async fn run_tier2(ctx: &Tier2Ctx, tunnel: Arc<Tunnel>) -> LoopEnd {
    let id = ctx.shared.id.clone();
    let facts = match dvt_info::read(&tunnel).await {
        Ok(f) => f,
        Err(e) => {
            return if tunnel.is_alive() {
                LoopEnd::ServiceFailed(e.to_string())
            } else {
                LoopEnd::TunnelDead(e.to_string())
            };
        }
    };
    let (ram_fallback, layout) = {
        let info = ctx.shared.info.read();
        let hw = info.as_ref().map(|i| &i.hardware);
        (
            hw.and_then(|h| h.ram_bytes),
            hw.and_then(|h| {
                Some(statasmon_idmap::apple::CoreLayout {
                    total: h.cpu_cores?,
                    performance: h.performance_cores?,
                    efficiency: h.efficiency_cores?,
                })
            }),
        )
    };
    let shared2: SharedState = Arc::new(RwLock::new(Tier2Shared {
        ram_total: facts.hardware.ram_bytes.or(ram_fallback),
        layout,
        ..Tier2Shared::default()
    }));
    let description = tunnel.describe();
    ctx.apply_tier2_info(&facts, ram_fallback, &description)
        .await;
    let energy = ctx.shared.energy.load(Ordering::SeqCst);
    let battery_detail = *ctx.shared.battery_detail.read();
    ctx.set_capabilities(tier2_capabilities(battery_detail, energy))
        .await;
    ctx.set_state(ConnectionState::Tier2, description.clone())
        .await;
    tracing::info!(device = %id, cores = ?facts.hardware.cpu_cores, ram = ?facts.hardware.ram_bytes, uptime_s = ?facts.uptime_secs(), "tier 2 ready");

    let child = ctx.cancel.child_token();
    let fast = Duration::from_millis(u64::from(ctx.cfg.fast_ms));
    let medium = Duration::from_millis(u64::from(ctx.cfg.medium_ms));
    let slow = Duration::from_millis(u64::from(ctx.cfg.slow_ms)).max(Duration::from_secs(5));
    let mut sys_task = tokio::spawn(dvt::sysmontap::run(
        Arc::clone(&tunnel),
        ctx.sink.clone(),
        id.clone(),
        fast,
        medium,
        Arc::clone(&shared2),
        child.clone(),
    ));
    let mut net_task = tokio::spawn(dvt::network::run(
        Arc::clone(&tunnel),
        ctx.sink.clone(),
        id.clone(),
        medium,
        Arc::clone(&shared2),
        child.clone(),
    ));
    let mut energy_task = energy.then(|| {
        tokio::spawn(dvt::energy::run(
            Arc::clone(&tunnel),
            medium,
            Arc::clone(&shared2),
            child.clone(),
        ))
    });
    let mut net_running = true;
    let mut refresh = tokio::time::interval(slow);
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    refresh.tick().await;

    let end = loop {
        tokio::select! {
            _ = child.cancelled() => break LoopEnd::Cancelled,
            r = &mut sys_task => {
                break r.unwrap_or_else(|e| LoopEnd::ServiceFailed(format!("sysmontap task: {e}")));
            }
            r = &mut net_task, if net_running => {
                net_running = false;
                let outcome = r.unwrap_or_else(|e| LoopEnd::ServiceFailed(format!("networking task: {e}")));
                match outcome {
                    LoopEnd::TunnelDead(why) => break LoopEnd::TunnelDead(why),
                    LoopEnd::ServiceFailed(why) => {
                        tracing::warn!(device = %id, why, "networking service failed; per-process network is off");
                        ctx.withdraw(&[Capability::NetworkPerProcess], reasons::NETWORKING_FAILED).await;
                    }
                    LoopEnd::Cancelled | LoopEnd::SinkClosed => break outcome,
                }
            }
            r = async { match energy_task.as_mut() { Some(t) => t.await, None => std::future::pending().await } }, if energy_task.is_some() => {
                energy_task = None;
                let outcome = r.unwrap_or_else(|e| LoopEnd::ServiceFailed(format!("energy task: {e}")));
                match outcome {
                    LoopEnd::TunnelDead(why) => break LoopEnd::TunnelDead(why),
                    LoopEnd::ServiceFailed(_) => {
                        ctx.withdraw(&[Capability::Energy], reasons::ENERGY_FAILED).await;
                    }
                    LoopEnd::Cancelled | LoopEnd::SinkClosed => break outcome,
                }
            }
            _ = refresh.tick() => {
                if !tunnel.is_alive() {
                    break LoopEnd::TunnelDead(
                        tunnel.death_reason().unwrap_or_else(|| "heartbeat failed".to_owned()),
                    );
                }
                match dvt_info::read(&tunnel).await {
                    Ok(facts) => {
                        let ram = { shared2.read().ram_total };
                        ctx.apply_tier2_info(&facts, ram.or(ram_fallback), &description).await;
                    }
                    Err(e) if !tunnel.is_alive() => break LoopEnd::TunnelDead(e.to_string()),
                    Err(e) => tracing::debug!(device = %id, error = %e, "deviceinfo refresh failed"),
                }
            }
        }
    };
    child.cancel();
    for task in [Some(sys_task), net_running.then_some(net_task), energy_task]
        .into_iter()
        .flatten()
    {
        if tokio::time::timeout(STOP_GRACE, task).await.is_err() {
            tracing::debug!(device = %id, "a tier-2 loop did not stop in time");
        }
    }
    end
}

impl Drop for IosSource {
    fn drop(&mut self) {
        if let Some(run) = self.pcap.lock().take() {
            run.cancel.cancel();
        }
        if let Some(Running { cancel, tasks }) = self.running.lock().take() {
            cancel.cancel();
            for t in tasks {
                t.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_reasons_follow_developer_mode() {
        let off = tier1_capabilities(Some(false), true);
        assert_eq!(
            off.reason(Capability::CpuTotal),
            Some(reasons::DEV_MODE_OFF)
        );
        assert_eq!(
            off.reason(Capability::Processes),
            Some(reasons::DEV_MODE_OFF)
        );
        assert!(off.is_available(Capability::BatteryDetail));
        assert!(off.is_available(Capability::Logs));
        assert!(off.is_available(Capability::PacketCapture));
        assert!(off.is_available(Capability::ThermalState));
        assert_eq!(
            off.reason(Capability::StorageCategories),
            Some(reasons::STORAGE_CATEGORIES)
        );
        assert_eq!(
            off.reason(Capability::CpuFrequency),
            Some(reasons::NO_FREQUENCY)
        );
        assert_eq!(
            off.reason(Capability::TemperatureSensors),
            Some(reasons::BATTERY_TEMP_ONLY)
        );
        assert_eq!(
            off.reason(Capability::WifiDetails),
            Some(reasons::WIFI_DETAILS)
        );

        let on = tier1_capabilities(Some(true), false);
        assert_eq!(
            on.reason(Capability::CpuPerCore),
            Some(reasons::NEEDS_TUNNEL)
        );
        assert_eq!(
            on.reason(Capability::BatteryDetail),
            Some(reasons::BATTERY_DETAIL_REFUSED)
        );
        let unknown = tier1_capabilities(None, true);
        assert_eq!(
            unknown.reason(Capability::Energy),
            Some(reasons::NEEDS_TUNNEL)
        );
        // Every capability has an entry.
        assert_eq!(off.entries.len(), Capability::ALL.len());
    }

    #[test]
    fn tier2_capabilities_cover_the_developer_services() {
        let on = tier2_capabilities(true, false);
        for cap in [
            Capability::CpuTotal,
            Capability::CpuPerCore,
            Capability::MemoryBreakdown,
            Capability::Processes,
            Capability::ProcessThreads,
            Capability::NetworkThroughput,
            Capability::NetworkPackets,
            Capability::NetworkPerProcess,
            Capability::BatteryDetail,
            Capability::Logs,
        ] {
            assert!(on.is_available(cap), "{cap:?}");
        }
        assert_eq!(on.reason(Capability::Energy), Some(reasons::ENERGY_OFF));
        assert_eq!(
            on.reason(Capability::ForegroundApp),
            Some(reasons::NO_FRONT_APP)
        );
        assert_eq!(
            on.reason(Capability::WifiDetails),
            Some(reasons::WIFI_DETAILS_TIER2)
        );
        assert_eq!(
            on.reason(Capability::LoadAverage),
            Some(reasons::NO_LOAD_AVERAGE)
        );
        assert_eq!(
            on.reason(Capability::MemoryPressure),
            Some(reasons::NO_MEMORY_PRESSURE)
        );
        assert_eq!(
            on.reason(Capability::CpuFrequency),
            Some(reasons::NO_FREQUENCY)
        );
        assert!(tier2_capabilities(false, true).is_available(Capability::Energy));
        assert_eq!(
            tier2_capabilities(false, true).reason(Capability::BatteryDetail),
            Some(reasons::BATTERY_DETAIL_REFUSED)
        );
        assert_eq!(on.entries.len(), Capability::ALL.len());

        // Tunnel failed: tier-1 shape with the tunnel reason on every DVT gap.
        let failed = capabilities_for(reasons::TUNNEL_FAILED, true);
        assert_eq!(
            failed.reason(Capability::CpuPerCore),
            Some(reasons::TUNNEL_FAILED)
        );
        assert_eq!(
            failed.reason(Capability::Energy),
            Some(reasons::TUNNEL_FAILED)
        );
        assert!(failed.is_available(Capability::Logs));
    }

    #[test]
    fn ids_and_paths() {
        assert_eq!(device_id("UDID"), "ios:UDID");
        let seed = IosSeed::new("UDID", 3, Transport::Usb);
        assert!(seed.crash_dir.ends_with("crashes/UDID"));
        assert!(app_data_dir().ends_with("StatasMon"));
        assert_eq!(
            detail_text(Some("Safari"), Some("SIGSEGV"), "x.ips"),
            "Safari — SIGSEGV"
        );
        assert_eq!(detail_text(None, None, "x.ips"), "x.ips");
    }
}
