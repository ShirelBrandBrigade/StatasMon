//! The engine: owns sources, drives discovery, updates per-device state
//! and republishes every message on the bus.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use statasmon_device::{
    ActionResult, DeviceSource, Discovery, DiscoveryEvent, DiscoverySink, EventSink, now_ms,
};
use statasmon_protocol::{
    CapabilitySet, CoreSample, DeviceAction, DeviceId, DeviceInfo, DeviceSummary, EventKind,
    Headline, HistoryPoint, HistoryQuery, LogFilter, LogPage, Message, MetricId, MetricValue,
    ProcessRow, SessionState, SessionStatus,
};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::logs::LogRing;
use crate::ring::{CoreRing, RingBuffer};
use crate::rules::{self, RecentEvent, RulesInput};
use crate::session::Session;
use crate::snapshot::DeviceSnapshot;

/// Provider → engine channel depth.
const INGRESS_CAPACITY: usize = 2048;
/// Discovery → engine channel depth.
const DISCOVERY_CAPACITY: usize = 32;
/// Events remembered per device (pruned to the rules window on evaluate).
const MAX_EVENTS: usize = 256;
/// How often `Message::Session` is re-sent while recording.
const SESSION_TICK: std::time::Duration = std::time::Duration::from_secs(5);

struct DeviceRecord {
    source: Arc<dyn DeviceSource>,
    summary: DeviceSummary,
    info: Option<DeviceInfo>,
    capabilities: CapabilitySet,
    metrics: RingBuffer,
    cores: CoreRing,
    logs: LogRing,
    latest: BTreeMap<MetricId, f64>,
    latest_ts_ms: Option<i64>,
    latest_cores: Vec<CoreSample>,
    processes: Vec<ProcessRow>,
    events: VecDeque<RecentEvent>,
    session: Session,
    headline: Headline,
}

impl fmt::Debug for DeviceRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceRecord")
            .field("summary", &self.summary)
            .field("session", &self.session.status())
            .finish_non_exhaustive()
    }
}

impl DeviceRecord {
    fn new(source: Arc<dyn DeviceSource>, config: &EngineConfig) -> Self {
        let summary = source.summary();
        let capabilities = source.capabilities();
        let mut record = Self {
            source,
            summary,
            info: None,
            capabilities,
            metrics: RingBuffer::new(config.ring_capacity),
            cores: CoreRing::new(config.core_capacity),
            logs: LogRing::new(config.log_capacity),
            latest: BTreeMap::new(),
            latest_ts_ms: None,
            latest_cores: Vec::new(),
            processes: Vec::new(),
            events: VecDeque::new(),
            session: Session::new(),
            headline: Headline::default(),
        };
        record.evaluate(config.event_window_ms, now_ms());
        record
    }

    fn display_name(&self) -> &str {
        self.info
            .as_ref()
            .and_then(|i| i.identity.marketing_name.as_deref())
            .unwrap_or(&self.summary.model_name)
    }

    fn latest_values(&self) -> Vec<MetricValue> {
        self.latest
            .iter()
            .map(|(id, value)| MetricValue {
                id: *id,
                value: *value,
            })
            .collect()
    }

    /// Re-run the rules; returns the new headline when it changed.
    fn evaluate(&mut self, window_ms: i64, now_ms: i64) -> Option<Headline> {
        while self
            .events
            .front()
            .is_some_and(|e| e.ts_ms < now_ms - window_ms)
        {
            self.events.pop_front();
        }
        let latest = self.latest_values();
        let events: Vec<RecentEvent> = self.events.iter().cloned().collect();
        let retransmits = self
            .metrics
            .delta_over(MetricId::NetRetransmits, window_ms, now_ms);
        let dropped = self
            .metrics
            .delta_over(MetricId::NetDropped, window_ms, now_ms);
        let headline = rules::evaluate(&RulesInput {
            device_name: self.display_name(),
            platform: self.platform_hint(),
            latest: &latest,
            events: &events,
            capabilities: &self.capabilities,
            now_ms,
            retransmits_per_min: retransmits,
            dropped_per_min: dropped,
        });
        if headline == self.headline {
            None
        } else {
            self.headline = headline.clone();
            Some(headline)
        }
    }

    /// The platform whose rules apply: fake devices imitate the platform
    /// named by their OS.
    fn platform_hint(&self) -> statasmon_protocol::Platform {
        use statasmon_protocol::Platform;
        match self.summary.platform {
            Platform::Fake => match self
                .info
                .as_ref()
                .and_then(|i| i.software.os_name.as_deref())
            {
                Some("Android") => Platform::Android,
                Some(_) => Platform::Ios,
                None => Platform::Fake,
            },
            other => other,
        }
    }

    fn snapshot(&self, now_ms: i64) -> DeviceSnapshot {
        DeviceSnapshot {
            summary: self.summary.clone(),
            info: self.info.clone(),
            capabilities: self.capabilities.clone(),
            latest: self.latest_values(),
            latest_ts_ms: self.latest_ts_ms,
            cores: self.latest_cores.clone(),
            processes: self.processes.clone(),
            headline: self.headline.clone(),
            session: self.session.state(now_ms),
        }
    }
}

type Records = BTreeMap<DeviceId, Arc<Mutex<DeviceRecord>>>;

struct Inner {
    config: EngineConfig,
    bus: broadcast::Sender<Arc<Message>>,
    devices: RwLock<Records>,
    ingress: EventSink,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    shut_down: parking_lot::Mutex<bool>,
}

/// The session engine. Cheap to clone; all clones share one state.
///
/// Must be created inside a tokio runtime (it spawns its ingress task).
#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("devices", &self.inner.devices.read().len())
            .field("subscribers", &self.inner.bus.receiver_count())
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// Create the engine and start its ingress task.
    pub fn new(config: EngineConfig) -> Self {
        let (bus, _) = broadcast::channel(config.bus_capacity.max(4096));
        let (ingress, rx) = EventSink::channel(INGRESS_CAPACITY);
        let inner = Arc::new(Inner {
            config,
            bus,
            devices: RwLock::new(BTreeMap::new()),
            ingress,
            tasks: Mutex::new(Vec::new()),
            shut_down: Mutex::new(false),
        });
        let engine = Self { inner };
        let ingress = tokio::spawn(engine.clone().ingress_loop(rx));
        let ticker = tokio::spawn(engine.clone().session_ticker());
        let mut tasks = engine.inner.tasks.lock();
        tasks.push(ingress);
        tasks.push(ticker);
        drop(tasks);
        engine
    }

    /// The configuration in use.
    pub fn config(&self) -> &EngineConfig {
        &self.inner.config
    }

    /// Subscribe to every message after the engine processed it. A
    /// receiver that falls more than the bus capacity behind gets
    /// `RecvError::Lagged` and continues; the engine never blocks on it.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Message>> {
        self.inner.bus.subscribe()
    }

    /// Run a discovery; every device it reports is registered and started.
    pub fn attach_discovery(&self, discovery: Box<dyn Discovery>) {
        let name = discovery.name();
        let (sink, mut rx) = DiscoverySink::channel(DISCOVERY_CAPACITY);
        let engine = self.clone();
        let consumer = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                match event {
                    DiscoveryEvent::Attached(source) => {
                        if let Err(e) = engine.attach_source(source).await {
                            tracing::warn!(discovery = name, error = %e, "attach failed");
                        }
                    }
                    DiscoveryEvent::Detached(id) => {
                        if let Err(e) = engine.detach(&id).await {
                            tracing::debug!(discovery = name, device = %id, error = %e, "detach");
                        }
                    }
                }
            }
        });
        let runner = tokio::spawn(async move {
            if let Err(e) = discovery.run(sink).await {
                tracing::warn!(discovery = name, error = %e, "discovery ended with error");
            }
        });
        let mut tasks = self.inner.tasks.lock();
        tasks.push(consumer);
        tasks.push(runner);
    }

    /// Register one source directly (tests, fixture replay), publish
    /// `DeviceAttached`, `Capabilities` and `Info`, then start it.
    pub async fn attach_source(&self, source: Box<dyn DeviceSource>) -> Result<(), EngineError> {
        if *self.inner.shut_down.lock() {
            return Err(EngineError::ShutDown);
        }
        let source: Arc<dyn DeviceSource> = Arc::from(source);
        let id = source.id();
        if self.inner.devices.read().contains_key(&id) {
            self.detach(&id).await?;
        }
        let mut record = DeviceRecord::new(Arc::clone(&source), &self.inner.config);
        let summary = record.summary.clone();
        let capabilities = record.capabilities.clone();
        let now = now_ms();
        if self.inner.config.auto_record {
            let _ = record.session.start(now);
        }
        let started_session = record.session.status() == SessionStatus::Recording;
        let session = record.session.state(now);
        let headline = record.headline.clone();
        self.inner
            .devices
            .write()
            .insert(id.clone(), Arc::new(Mutex::new(record)));
        self.publish(Message::DeviceAttached { device: summary });
        self.publish(Message::Capabilities {
            device_id: id.clone(),
            set: capabilities,
        });
        self.publish(Message::Headline {
            device_id: id.clone(),
            headline,
        });
        if started_session {
            self.publish_session_event(&id, now, "Session started");
        }
        self.publish(Message::Session {
            device_id: id.clone(),
            session,
        });
        match source.info().await {
            Ok(info) => self.handle(Message::Info {
                device_id: id.clone(),
                info: Box::new(info),
            }),
            Err(e) => {
                tracing::warn!(device = %id, error = %e, "info failed");
                self.publish(e.to_message(Some(id.clone())));
            }
        }
        if let Err(e) = source
            .start(
                self.inner.ingress.clone(),
                self.inner.config.sampling.clone(),
            )
            .await
        {
            self.publish(e.to_message(Some(id.clone())));
            return Err(e.into());
        }
        Ok(())
    }

    /// Stop and forget a device, publishing `DeviceDetached`.
    pub async fn detach(&self, device_id: &str) -> Result<(), EngineError> {
        let record = self
            .inner
            .devices
            .write()
            .remove(device_id)
            .ok_or_else(|| EngineError::UnknownDevice(device_id.to_owned()))?;
        let source = Arc::clone(&record.lock().source);
        if let Err(e) = source.stop().await {
            tracing::warn!(device = device_id, error = %e, "stop failed");
        }
        self.publish(Message::DeviceDetached {
            device_id: device_id.to_owned(),
        });
        Ok(())
    }

    /// Summaries of every attached device, in id order.
    pub fn devices(&self) -> Vec<DeviceSummary> {
        self.inner
            .devices
            .read()
            .values()
            .map(|r| r.lock().summary.clone())
            .collect()
    }

    /// The current state of one device.
    pub fn snapshot(&self, device_id: &str) -> Option<DeviceSnapshot> {
        let record = self.record(device_id).ok()?;
        let snap = record.lock().snapshot(now_ms());
        Some(snap)
    }

    /// The headline of one device.
    pub fn headline(&self, device_id: &str) -> Option<Headline> {
        let record = self.record(device_id).ok()?;
        let h = record.lock().headline.clone();
        Some(h)
    }

    /// Metric history from the live ring (downsampled when `step_ms > 0`).
    pub fn history(&self, query: &HistoryQuery) -> Result<Vec<HistoryPoint>, EngineError> {
        let record = self.record(&query.device_id)?;
        let points =
            record
                .lock()
                .metrics
                .history(query.metric, query.from_ms, query.to_ms, query.step_ms);
        Ok(points)
    }

    /// Per-core ticks in `[from_ms, to_ms]`.
    pub fn cores_history(
        &self,
        device_id: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<(i64, Vec<CoreSample>)>, EngineError> {
        let record = self.record(device_id)?;
        let ticks = record.lock().cores.history(from_ms, to_ms);
        Ok(ticks)
    }

    /// A page of log entries matching `filter` (its `device_id` picks the device).
    pub fn query_logs(
        &self,
        filter: &LogFilter,
        offset: usize,
        limit: usize,
    ) -> Result<LogPage, EngineError> {
        let record = self.record(&filter.device_id)?;
        let page = record.lock().logs.filter(filter, offset, limit)?;
        Ok(page)
    }

    /// Number of stored log entries matching `filter`.
    pub fn count_logs(&self, filter: &LogFilter) -> Result<u64, EngineError> {
        let record = self.record(&filter.device_id)?;
        let n = record.lock().logs.count(filter)?;
        Ok(n)
    }

    /// The session state of one device.
    pub fn session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        let record = self.record(device_id)?;
        let s = record.lock().session.state(now_ms());
        Ok(s)
    }

    /// Start a new recording session.
    pub fn start_session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "start", "Session started", |s, now| s.start(now))
    }

    /// Pause: keep the connection, stop storing.
    pub fn pause(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "pause", "Session paused", |s, now| s.pause(now))
    }

    /// Resume storing.
    pub fn resume(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "resume", "Session resumed", |s, now| {
            s.resume(now)
        })
    }

    /// Stop the session (the device stays connected).
    pub fn stop_session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "stop", "Session stopped", |s, now| s.stop(now))
    }

    /// Add a mark; publishes an `Event { kind: Mark }`.
    pub fn mark(&self, device_id: &str, label: &str) -> Result<SessionState, EngineError> {
        let record = self.record(device_id)?;
        let now = now_ms();
        let (state, mark) = {
            let mut r = record.lock();
            let mark =
                r.session
                    .mark(now, label)
                    .map_err(|status| EngineError::InvalidTransition {
                        action: "mark",
                        status,
                    })?;
            (r.session.state(now), mark)
        };
        self.publish(Message::Event {
            device_id: device_id.to_owned(),
            ts_ms: mark.ts_ms,
            kind: EventKind::Mark,
            detail: mark.label,
            pid: None,
            process: None,
        });
        self.publish(Message::Session {
            device_id: device_id.to_owned(),
            session: state.clone(),
        });
        Ok(state)
    }

    /// Run a device action on the provider.
    pub async fn run_action(
        &self,
        device_id: &str,
        action: DeviceAction,
    ) -> Result<ActionResult, EngineError> {
        let source = Arc::clone(&self.record(device_id)?.lock().source);
        let result = source.run_action(action).await;
        if let Err(e) = &result {
            self.publish(e.to_message(Some(device_id.to_owned())));
        }
        Ok(result?)
    }

    /// Stop every source and background task.
    pub async fn shutdown(&self) {
        *self.inner.shut_down.lock() = true;
        let ids: Vec<DeviceId> = self.inner.devices.read().keys().cloned().collect();
        for id in ids {
            if let Err(e) = self.detach(&id).await {
                tracing::debug!(device = %id, error = %e, "detach on shutdown");
            }
        }
        for task in self.inner.tasks.lock().drain(..) {
            task.abort();
        }
    }

    fn record(&self, device_id: &str) -> Result<Arc<Mutex<DeviceRecord>>, EngineError> {
        self.inner
            .devices
            .read()
            .get(device_id)
            .cloned()
            .ok_or_else(|| EngineError::UnknownDevice(device_id.to_owned()))
    }

    fn session_command(
        &self,
        device_id: &str,
        action: &'static str,
        detail: &str,
        f: impl FnOnce(&mut Session, i64) -> Result<(), SessionStatus>,
    ) -> Result<SessionState, EngineError> {
        let record = self.record(device_id)?;
        let now = now_ms();
        let state = {
            let mut r = record.lock();
            f(&mut r.session, now)
                .map_err(|status| EngineError::InvalidTransition { action, status })?;
            r.session.state(now)
        };
        self.publish_session_event(device_id, now, detail);
        self.publish(Message::Session {
            device_id: device_id.to_owned(),
            session: state.clone(),
        });
        Ok(state)
    }

    fn publish_session_event(&self, device_id: &str, ts_ms: i64, detail: &str) {
        self.publish(Message::Event {
            device_id: device_id.to_owned(),
            ts_ms,
            kind: EventKind::Connection,
            detail: detail.to_owned(),
            pid: None,
            process: None,
        });
    }

    fn publish(&self, message: Message) {
        // `send` only fails when there is no subscriber; that is fine.
        let _ = self.inner.bus.send(Arc::new(message));
    }

    async fn ingress_loop(self, mut rx: mpsc::Receiver<Message>) {
        while let Some(message) = rx.recv().await {
            self.handle(message);
        }
    }

    /// Re-send `Message::Session` for every recording device every 5 s so
    /// the elapsed time and counters stay fresh without polling.
    async fn session_ticker(self) {
        let mut tick = tokio::time::interval(SESSION_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let records: Vec<(DeviceId, Arc<Mutex<DeviceRecord>>)> = self
                .inner
                .devices
                .read()
                .iter()
                .map(|(id, r)| (id.clone(), Arc::clone(r)))
                .collect();
            let now = now_ms();
            for (id, record) in records {
                let state = {
                    let r = record.lock();
                    r.session.is_recording().then(|| r.session.state(now))
                };
                if let Some(session) = state {
                    self.publish(Message::Session {
                        device_id: id,
                        session,
                    });
                }
            }
        }
    }

    /// Update state for one provider message and forward it when the
    /// session allows (see the crate docs for the storage policy).
    fn handle(&self, message: Message) {
        let Some(device_id) = message.device_id().map(str::to_owned) else {
            self.publish(message);
            return;
        };
        let Some(record) = self.inner.devices.read().get(&device_id).cloned() else {
            tracing::debug!(device = %device_id, "message for unknown device dropped");
            return;
        };
        let window = self.inner.config.event_window_ms;
        let mut headline = None;
        let forward = {
            let mut r = record.lock();
            let recording = r.session.is_recording();
            match &message {
                Message::Samples { ts_ms, values, .. } => {
                    for v in values {
                        r.latest.insert(v.id, v.value);
                    }
                    r.latest_ts_ms = Some(*ts_ms);
                    if recording {
                        r.metrics.push(*ts_ms, values);
                        r.session.count_samples(1);
                    }
                    headline = r.evaluate(window, *ts_ms);
                    recording
                }
                Message::Cores { ts_ms, cores, .. } => {
                    r.latest_cores = cores.clone();
                    if recording {
                        r.cores.push(*ts_ms, cores.clone());
                    }
                    recording
                }
                Message::Processes { rows, .. } => {
                    r.processes = rows.clone();
                    recording
                }
                Message::Log { entries, .. } => {
                    if recording {
                        r.logs.push(entries.iter().cloned());
                        r.session.count_logs(entries.len() as u64);
                    }
                    recording
                }
                Message::Event {
                    ts_ms,
                    kind,
                    detail,
                    process,
                    ..
                } => {
                    if !matches!(kind, EventKind::Mark | EventKind::Connection) {
                        if r.events.len() >= MAX_EVENTS {
                            r.events.pop_front();
                        }
                        r.events.push_back(RecentEvent {
                            ts_ms: *ts_ms,
                            kind: *kind,
                            detail: detail.clone(),
                            process: process.clone(),
                        });
                        headline = r.evaluate(window, *ts_ms);
                    }
                    true
                }
                Message::Info { info, .. } => {
                    r.info = Some((**info).clone());
                    headline = r.evaluate(window, now_ms());
                    true
                }
                Message::Capabilities { set, .. } => {
                    r.capabilities = set.clone();
                    headline = r.evaluate(window, now_ms());
                    true
                }
                Message::DeviceState { state, .. } => {
                    r.summary.state = state.clone();
                    true
                }
                Message::DeviceAttached { .. }
                | Message::DeviceDetached { .. }
                | Message::Headline { .. }
                | Message::Session { .. }
                | Message::Progress { .. }
                | Message::Error { .. } => true,
            }
        };
        if forward {
            self.publish(message);
        }
        if let Some(headline) = headline {
            self.publish(Message::Headline {
                device_id,
                headline,
            });
        }
    }
}
