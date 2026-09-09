//! `FakeSource`: a live seeded generator or a fixture replay, behind the
//! same [`DeviceSource`] trait real providers implement.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::Mutex;
use statasmon_device::{
    ActionResult, DeviceError, DeviceResult, DeviceSource, EventSink, SinkClosed, now_ms,
};
use statasmon_protocol::{
    CapabilitySet, ConnectionState, DeviceAction, DeviceId, DeviceInfo, DeviceSummary, EventKind,
    Message, Platform, SamplingConfig,
};
use tokio::task::JoinHandle;

use crate::config::{FakeDevice, Scenario};
use crate::profiles::Profile;
use crate::replay::{Fixture, message_ts};
use crate::sim::Simulator;

/// Interval between log batches.
const LOG_BATCH_MS: u64 = 200;

#[derive(Debug, Clone)]
struct Live {
    profile: Profile,
    scenario: Scenario,
    seed: u64,
    time_scale: f64,
    log_lines_per_sec: f64,
}

#[derive(Debug)]
enum Mode {
    Live(Live),
    Replay { fixture: Arc<Fixture>, speed: f64 },
}

/// One fake device.
#[derive(Debug)]
pub struct FakeSource {
    id: DeviceId,
    summary: DeviceSummary,
    capabilities: CapabilitySet,
    info: Option<DeviceInfo>,
    mode: Mode,
    sink: Mutex<Option<EventSink>>,
    task: Mutex<Option<JoinHandle<()>>>,
    started_at: Mutex<Option<Instant>>,
}

impl FakeSource {
    /// A live generator for `kind`.
    pub fn new(
        kind: FakeDevice,
        scenario: Scenario,
        seed: u64,
        time_scale: f64,
        log_lines_per_sec: f64,
    ) -> Self {
        let profile = Profile::new(kind);
        Self {
            id: profile.id.clone(),
            summary: profile.summary(),
            capabilities: profile.capabilities(),
            info: None,
            mode: Mode::Live(Live {
                profile,
                scenario,
                seed,
                time_scale: if time_scale > 0.0 { time_scale } else { 1.0 },
                log_lines_per_sec: log_lines_per_sec.max(0.0),
            }),
            sink: Mutex::new(None),
            task: Mutex::new(None),
            started_at: Mutex::new(None),
        }
    }

    /// A replay of `path` (JSONL of protocol messages). `speed` 2.0 plays
    /// twice as fast; timestamps are re-based to the wall clock at start.
    pub fn from_fixture(path: impl AsRef<Path>, speed: f64) -> DeviceResult<Self> {
        Self::from_fixture_data(Fixture::load(path)?, speed)
    }

    /// A replay of an already parsed fixture.
    pub fn from_fixture_data(fixture: Fixture, speed: f64) -> DeviceResult<Self> {
        if speed.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(DeviceError::new(
                statasmon_protocol::ErrorCode::Internal,
                format!("replay speed must be positive, got {speed}"),
            )
            .with_hint("Use a speed such as 1.0 (real time) or 10.0."));
        }
        Ok(Self {
            id: fixture.summary.id.clone(),
            summary: fixture.summary.clone(),
            capabilities: fixture.capabilities.clone(),
            info: fixture.info.clone(),
            mode: Mode::Replay {
                fixture: Arc::new(fixture),
                speed,
            },
            sink: Mutex::new(None),
            task: Mutex::new(None),
            started_at: Mutex::new(None),
        })
    }

    /// The scenario a live source follows (None for replays).
    pub fn scenario(&self) -> Option<Scenario> {
        match &self.mode {
            Mode::Live(l) => Some(l.scenario),
            Mode::Replay { .. } => None,
        }
    }

    fn uptime_secs(&self) -> u64 {
        let base = 86_400 * 3 + 4_120;
        let since = self
            .started_at
            .lock()
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0);
        base + since
    }

    fn build_info(&self) -> DeviceInfo {
        match (&self.mode, &self.info) {
            (Mode::Live(l), _) => l.profile.info(self.uptime_secs(), now_ms()),
            (Mode::Replay { .. }, Some(info)) => info.clone(),
            (Mode::Replay { .. }, None) => DeviceInfo::default(),
        }
    }
}

#[async_trait]
impl DeviceSource for FakeSource {
    fn id(&self) -> DeviceId {
        self.id.clone()
    }

    fn platform(&self) -> Platform {
        Platform::Fake
    }

    fn summary(&self) -> DeviceSummary {
        self.summary.clone()
    }

    fn capabilities(&self) -> CapabilitySet {
        self.capabilities.clone()
    }

    async fn info(&self) -> DeviceResult<DeviceInfo> {
        Ok(self.build_info())
    }

    async fn start(&self, sink: EventSink, cfg: SamplingConfig) -> DeviceResult<()> {
        self.stop().await?;
        *self.started_at.lock() = Some(Instant::now());
        *self.sink.lock() = Some(sink.clone());
        let id = self.id.clone();
        let tier = self.summary.state.clone();
        let caps = self.capabilities.clone();
        let handle = match &self.mode {
            Mode::Live(live) => {
                let live = live.clone();
                tokio::spawn(async move {
                    let _ = sink
                        .state(&id, tier, Some("fake device started".into()))
                        .await;
                    let _ = sink.capabilities(&id, caps).await;
                    if let Err(SinkClosed) = run_live(live, sink, cfg).await {
                        tracing::debug!(device = %id, "sink closed; fake source stopping");
                    }
                })
            }
            Mode::Replay { fixture, speed } => {
                let fixture = Arc::clone(fixture);
                let speed = *speed;
                tokio::spawn(async move {
                    let _ = sink.state(&id, tier, Some("replay started".into())).await;
                    if let Err(SinkClosed) = run_replay(&id, fixture, speed, sink).await {
                        tracing::debug!(device = %id, "sink closed; replay stopping");
                    }
                })
            }
        };
        *self.task.lock() = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> DeviceResult<()> {
        if let Some(handle) = self.task.lock().take() {
            handle.abort();
        }
        Ok(())
    }

    async fn run_action(&self, action: DeviceAction) -> DeviceResult<ActionResult> {
        match action {
            DeviceAction::RefreshInfo => {
                let info = self.build_info();
                let sink = self.sink.lock().clone();
                match sink {
                    Some(sink) => {
                        sink.info(&self.id, info)
                            .await
                            .map_err(|_| DeviceError::device_lost(&self.id))?;
                        Ok(ActionResult::Done)
                    }
                    None => Err(DeviceError::new(
                        statasmon_protocol::ErrorCode::Internal,
                        "fake source is not started",
                    )
                    .with_hint("Connect the device before refreshing its information.")),
                }
            }
            other => Err(DeviceError::unsupported(format!(
                "{other:?} on the fake device"
            ))),
        }
    }
}

async fn run_live(live: Live, sink: EventSink, cfg: SamplingConfig) -> Result<(), SinkClosed> {
    let id = live.profile.id.clone();
    let mut sim = Simulator::new(live.profile.clone(), live.scenario, live.seed);
    let caps = live.profile.capabilities();
    let emit_cores = caps.is_available(statasmon_protocol::Capability::CpuPerCore);
    let emit_procs = caps.is_available(statasmon_protocol::Capability::Processes);
    let emit_logs =
        caps.is_available(statasmon_protocol::Capability::Logs) && live.log_lines_per_sec > 0.0;

    let fast_ms = u64::from(cfg.fast_ms.max(10));
    let medium_ms = u64::from(cfg.medium_ms.max(10));
    let mut fast = tokio::time::interval(Duration::from_millis(fast_ms));
    let mut medium = tokio::time::interval(Duration::from_millis(medium_ms));
    let mut logs = tokio::time::interval(Duration::from_millis(LOG_BATCH_MS));
    let lines_per_batch = (live.log_lines_per_sec * LOG_BATCH_MS as f64 / 1000.0).ceil() as usize;
    for i in [&mut fast, &mut medium, &mut logs] {
        i.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    }
    let start = Instant::now();
    let dt = fast_ms as f64 / 1000.0;
    loop {
        tokio::select! {
            _ = fast.tick() => {
                let t = start.elapsed().as_secs_f64() * live.time_scale;
                let ts = now_ms();
                for msg in sim.scenario_step(t, ts) {
                    sink.send(msg).await?;
                }
                let values = sim.samples(t, dt * live.time_scale);
                if !values.is_empty() {
                    sink.samples(&id, ts, values).await?;
                }
                if emit_cores {
                    sink.cores(&id, ts, sim.cores(t)).await?;
                }
            }
            _ = medium.tick() => {
                if emit_procs {
                    sink.processes(&id, now_ms(), sim.processes()).await?;
                }
            }
            _ = logs.tick() => {
                if emit_logs && lines_per_batch > 0 {
                    let entries = sim.log_lines(now_ms(), LOG_BATCH_MS as i64, lines_per_batch);
                    sink.log(&id, entries).await?;
                }
            }
            _ = sink.closed() => return Err(SinkClosed),
        }
    }
}

async fn run_replay(
    id: &str,
    fixture: Arc<Fixture>,
    speed: f64,
    sink: EventSink,
) -> Result<(), SinkClosed> {
    let wall0 = Instant::now();
    let wall0_ms = now_ms();
    let base = fixture.messages.iter().find_map(message_ts);
    for msg in &fixture.messages {
        let mut msg = msg.clone();
        if let (Some(ts), Some(base)) = (message_ts(&msg), base) {
            let offset_ms = (ts - base).max(0);
            let due = Duration::from_millis((offset_ms as f64 / speed) as u64);
            let elapsed = wall0.elapsed();
            if due > elapsed {
                tokio::select! {
                    _ = tokio::time::sleep(due - elapsed) => {}
                    _ = sink.closed() => return Err(SinkClosed),
                }
            }
            let new_ts = wall0_ms + (offset_ms as f64 / speed) as i64;
            rebase(&mut msg, ts - new_ts);
        }
        sink.send(msg).await?;
    }
    sink.event(
        id,
        now_ms(),
        EventKind::Connection,
        "Replay finished",
        None,
        None,
    )
    .await?;
    sink.state(
        id,
        ConnectionState::Disconnected,
        Some("replay finished".into()),
    )
    .await
}

/// Shift every timestamp in `msg` back by `delta_ms`.
fn rebase(msg: &mut Message, delta_ms: i64) {
    match msg {
        Message::Samples { ts_ms, .. }
        | Message::Cores { ts_ms, .. }
        | Message::Processes { ts_ms, .. }
        | Message::Event { ts_ms, .. } => *ts_ms -= delta_ms,
        Message::Log { entries, .. } => {
            for e in entries {
                e.ts_ms -= delta_ms;
            }
        }
        _ => {}
    }
}

impl Drop for FakeSource {
    fn drop(&mut self) {
        if let Some(handle) = self.task.lock().take() {
            handle.abort();
        }
    }
}
