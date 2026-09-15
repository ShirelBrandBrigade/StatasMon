//! The engine: owns sources, drives discovery, updates per-device state,
//! feeds the session store and republishes every message on the bus.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use statasmon_device::{
    ActionResult, DeviceError, DeviceSource, Discovery, DiscoveryEvent, DiscoverySink, EventSink,
    now_ms,
};
use statasmon_protocol::{
    CapabilitySet, CoreSample, DeviceAction, DeviceId, DeviceInfo, DeviceSummary, ErrorCode,
    EventKind, Headline, HistoryPoint, HistoryQuery, LogFilter, LogPage, Message, MetricId,
    MetricValue, ProcessRow, SessionDetail, SessionState, SessionStatus, SessionSummary,
};
use statasmon_store::{Store, StoreError, session_id_for};
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
    /// The store's id of the current (or last) session; this is the id the
    /// UI sees in `SessionState.id` when a store is configured.
    store_session: Option<String>,
    headline: Headline,
}

impl fmt::Debug for DeviceRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceRecord")
            .field("summary", &self.summary)
            .field("session", &self.session.status())
            .field("store_session", &self.store_session)
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
            store_session: None,
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

    /// The session state for the UI, carrying the store's session id.
    fn session_state(&self, now_ms: i64) -> SessionState {
        let mut state = self.session.state(now_ms);
        if state.id.is_some()
            && let Some(id) = &self.store_session
        {
            state.id = Some(id.clone());
        }
        state
    }

    /// True while the session is recording or paused (has a live session).
    fn session_open(&self) -> bool {
        matches!(
            self.session.status(),
            SessionStatus::Recording | SessionStatus::Paused
        )
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
            session: self.session_state(now_ms),
        }
    }

    /// A summary row for a session that only exists in memory (no store).
    fn live_summary(&self, state: &SessionState, now_ms: i64) -> Option<SessionSummary> {
        let id = state.id.clone()?;
        Some(SessionSummary {
            id,
            device_id: self.summary.id.clone(),
            device_name: self.summary.name.clone(),
            model_name: self.display_name().to_owned(),
            platform: self.summary.platform,
            started_at_ms: state.started_at_ms.unwrap_or(now_ms),
            ended_at_ms: (state.status == SessionStatus::Stopped).then_some(now_ms),
            elapsed_ms: state.elapsed_ms,
            sample_count: state.sample_count,
            log_count: state.log_count,
            event_count: 0,
            mark_count: state.marks.len() as u64,
            size_bytes: 0,
            live: self.session_open(),
        })
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
    /// Startup / global notices (e.g. adb not found) replayed to late subscribers.
    notices: parking_lot::Mutex<Vec<Message>>,
    /// The session store, when configured and opened.
    store: Option<Arc<Store>>,
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
            .field("store", &self.inner.store.is_some())
            .finish_non_exhaustive()
    }
}

/// Map a store error onto the engine's error type (a store problem is
/// reported like a provider I/O problem, with a hint).
fn store_err(e: StoreError) -> EngineError {
    match e {
        StoreError::InvalidFilter(m) => EngineError::InvalidFilter(m),
        StoreError::UnknownSession(id) => EngineError::Device(
            DeviceError::new(ErrorCode::Unsupported, format!("unknown session {id}"))
                .with_hint("The session was deleted or never stored. Refresh the Sessions list."),
        ),
        StoreError::Live(id) => EngineError::Device(
            DeviceError::new(
                ErrorCode::Unsupported,
                format!("session {id} is still recording"),
            )
            .with_hint("Stop the recording before deleting or exporting the whole session."),
        ),
        other => EngineError::Device(
            DeviceError::new(ErrorCode::Io, format!("session store: {other}"))
                .with_hint("Check free disk space and that the app data folder is writable."),
        ),
    }
}

impl Engine {
    /// Create the engine and start its ingress task. When `config.store` is
    /// set the store is opened under its root; if that fails the engine
    /// runs without storage and publishes an `Error` notice saying so.
    pub fn new(config: EngineConfig) -> Self {
        let (bus, _) = broadcast::channel(config.bus_capacity.max(4096));
        let (ingress, rx) = EventSink::channel(INGRESS_CAPACITY);
        let mut store_error = None;
        let store = match &config.store {
            Some(store_config) => match Store::open(store_config.clone()) {
                Ok(store) => {
                    tracing::info!(root = %store_config.root.display(), "session store opened");
                    Some(store)
                }
                Err(e) => {
                    tracing::error!(root = %store_config.root.display(), error = %e, "session store failed to open");
                    store_error = Some(format!(
                        "Session storage is off: {e} ({})",
                        store_config.root.display()
                    ));
                    None
                }
            },
            None => None,
        };
        let inner = Arc::new(Inner {
            config,
            bus,
            devices: RwLock::new(BTreeMap::new()),
            ingress,
            tasks: Mutex::new(Vec::new()),
            shut_down: Mutex::new(false),
            notices: parking_lot::Mutex::new(Vec::new()),
            store,
        });
        let engine = Self { inner };
        if let Some(message) = store_error {
            engine.notice(Message::Error {
                device_id: None,
                code: ErrorCode::Io,
                message,
                hint: Some(
                    "Sessions are not being saved. Check the app data folder is writable and restart."
                        .to_owned(),
                ),
            });
        }
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

    /// The session store, when one is configured and open.
    pub fn store(&self) -> Option<Arc<Store>> {
        self.inner.store.clone()
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
        if self.inner.config.auto_record && record.session.start(now).is_ok() {
            self.begin_store_session(&mut record, now);
        }
        let started_session = record.session.status() == SessionStatus::Recording;
        let session = record.session_state(now);
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

    /// Stop and forget a device, publishing `DeviceDetached`. An open
    /// session is ended in the store.
    pub async fn detach(&self, device_id: &str) -> Result<(), EngineError> {
        let record = self
            .inner
            .devices
            .write()
            .remove(device_id)
            .ok_or_else(|| EngineError::UnknownDevice(device_id.to_owned()))?;
        let source = {
            let mut r = record.lock();
            if r.session_open() {
                let now = now_ms();
                self.end_store_session(&mut r, now);
                let _ = r.session.stop(now);
            }
            Arc::clone(&r.source)
        };
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

    /// Metric history: from the store when `query.session_id` is set,
    /// otherwise from the live ring (downsampled when `step_ms > 0`).
    pub fn history(&self, query: &HistoryQuery) -> Result<Vec<HistoryPoint>, EngineError> {
        if let Some(session_id) = query.session_id.as_deref() {
            let store = self.store_or_err()?;
            return store
                .history(
                    session_id,
                    query.metric,
                    query.from_ms,
                    query.to_ms,
                    query.step_ms,
                )
                .map_err(store_err);
        }
        let record = self.record(&query.device_id)?;
        let points =
            record
                .lock()
                .metrics
                .history(query.metric, query.from_ms, query.to_ms, query.step_ms);
        Ok(points)
    }

    /// Per-core ticks in `[from_ms, to_ms]` from the live ring.
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

    /// Per-core ticks of a stored session.
    pub fn stored_cores_history(
        &self,
        session_id: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<(i64, Vec<CoreSample>)>, EngineError> {
        self.store_or_err()?
            .cores_history(session_id, from_ms, to_ms)
            .map_err(store_err)
    }

    /// A page of log entries matching `filter`: from the store when
    /// `filter.session_id` is set, otherwise from the live ring of
    /// `filter.device_id`.
    pub fn query_logs(
        &self,
        filter: &LogFilter,
        offset: usize,
        limit: usize,
    ) -> Result<LogPage, EngineError> {
        if let Some(session_id) = filter.session_id.as_deref() {
            return self
                .store_or_err()?
                .logs(session_id, filter, offset, limit)
                .map_err(store_err);
        }
        let record = self.record(&filter.device_id)?;
        let page = record.lock().logs.filter(filter, offset, limit)?;
        Ok(page)
    }

    /// Number of log entries matching `filter` (store or live ring, as
    /// [`Engine::query_logs`]).
    pub fn count_logs(&self, filter: &LogFilter) -> Result<u64, EngineError> {
        if let Some(session_id) = filter.session_id.as_deref() {
            return self
                .store_or_err()?
                .count_logs(session_id, filter)
                .map_err(store_err);
        }
        let record = self.record(&filter.device_id)?;
        let n = record.lock().logs.count(filter)?;
        Ok(n)
    }

    /// The session state of one device.
    pub fn session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        let record = self.record(device_id)?;
        let s = record.lock().session_state(now_ms());
        Ok(s)
    }

    /// The store id of the device's current (or last) session.
    pub fn current_session_id(&self, device_id: &str) -> Result<Option<String>, EngineError> {
        let record = self.record(device_id)?;
        let r = record.lock();
        Ok(r.session_state(now_ms()).id)
    }

    /// Every session: stored ones (newest first) with the live ones
    /// overlaid with their in-memory counters; without a store, only the
    /// in-memory sessions of attached devices.
    pub fn session_summaries(&self) -> Result<Vec<SessionSummary>, EngineError> {
        let mut out = match &self.inner.store {
            Some(store) => store.list().map_err(store_err)?,
            None => Vec::new(),
        };
        let now = now_ms();
        let records: Vec<Arc<Mutex<DeviceRecord>>> =
            self.inner.devices.read().values().cloned().collect();
        for record in records {
            let r = record.lock();
            let state = r.session_state(now);
            let Some(id) = state.id.clone() else {
                continue;
            };
            if let Some(row) = out.iter_mut().find(|s| s.id == id) {
                if r.session_open() {
                    overlay_live(row, &state);
                }
            } else if self.inner.store.is_none()
                && let Some(row) = r.live_summary(&state, now)
            {
                out.push(row);
            }
        }
        out.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms).then(b.id.cmp(&a.id)));
        Ok(out)
    }

    /// Everything the UI needs to open a stored session (live counters
    /// overlaid while it is still recording).
    pub fn session_detail(&self, session_id: &str) -> Result<SessionDetail, EngineError> {
        let mut detail = self
            .store_or_err()?
            .open_session(session_id)
            .map_err(store_err)?;
        let now = now_ms();
        let records: Vec<Arc<Mutex<DeviceRecord>>> =
            self.inner.devices.read().values().cloned().collect();
        for record in records {
            let r = record.lock();
            if r.session_open() && r.store_session.as_deref() == Some(session_id) {
                let state = r.session_state(now);
                overlay_live(&mut detail.summary, &state);
                detail.marks = state.marks;
            }
        }
        Ok(detail)
    }

    /// Delete a stored session. A session still being recorded is
    /// stopped first. Runs the file deletion on a blocking thread.
    pub async fn delete_session(&self, session_id: &str) -> Result<(), EngineError> {
        let store = self.store_or_err()?;
        let recording: Vec<DeviceId> = self
            .inner
            .devices
            .read()
            .iter()
            .filter(|(_, r)| {
                let r = r.lock();
                r.session_open() && r.store_session.as_deref() == Some(session_id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for device_id in recording {
            self.stop_session(&device_id)?;
        }
        let id = session_id.to_owned();
        tokio::task::spawn_blocking(move || store.delete(&id))
            .await
            .map_err(|e| EngineError::Device(DeviceError::new(ErrorCode::Internal, e.to_string())))?
            .map_err(store_err)
    }

    /// Start a new recording session (and a new store session).
    pub fn start_session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "start", "Session started", |engine, r, now| {
            r.session.start(now)?;
            engine.begin_store_session(r, now);
            Ok(())
        })
    }

    /// Pause: keep the connection, stop storing.
    pub fn pause(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "pause", "Session paused", |_, r, now| {
            r.session.pause(now)
        })
    }

    /// Resume storing.
    pub fn resume(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "resume", "Session resumed", |_, r, now| {
            r.session.resume(now)
        })
    }

    /// Stop the session (the device stays connected); the store session ends.
    pub fn stop_session(&self, device_id: &str) -> Result<SessionState, EngineError> {
        self.session_command(device_id, "stop", "Session stopped", |engine, r, now| {
            r.session.stop(now)?;
            engine.end_store_session(r, now);
            Ok(())
        })
    }

    /// Add a mark; publishes an `Event { kind: Mark }` and stores the mark.
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
            if let (Some(store), Some(sid)) = (&self.inner.store, &r.store_session)
                && let Err(e) = store.mark(sid, &mark)
            {
                tracing::warn!(session = %sid, error = %e, "store mark");
            }
            (r.session_state(now), mark)
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

    /// Stop every source and background task, end open sessions and flush
    /// the store.
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
        if let Some(store) = self.inner.store.clone() {
            let flushed = tokio::task::spawn_blocking(move || store.flush()).await;
            if let Ok(Err(e)) = flushed {
                tracing::warn!(error = %e, "store flush on shutdown");
            }
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

    fn store_or_err(&self) -> Result<Arc<Store>, EngineError> {
        self.inner.store.clone().ok_or_else(|| {
            EngineError::Device(
                DeviceError::new(
                    ErrorCode::Unsupported,
                    "session storage is not available in this run",
                )
                .with_hint(
                    "Sessions are not being saved; restart the app to retry opening the store.",
                ),
            )
        })
    }

    /// Begin a store session for a record whose `Session` just started.
    fn begin_store_session(&self, r: &mut DeviceRecord, now_ms: i64) {
        let Some(store) = &self.inner.store else {
            return;
        };
        let base = session_id_for(&r.summary.id, now_ms);
        match store.begin(&base, &r.summary, r.info.as_ref(), now_ms) {
            Ok(id) => r.store_session = Some(id),
            Err(e) => {
                tracing::error!(device = %r.summary.id, error = %e, "store begin failed");
                r.store_session = None;
            }
        }
    }

    /// End the store session of a record (the id stays for display).
    fn end_store_session(&self, r: &mut DeviceRecord, now_ms: i64) {
        if let (Some(store), Some(id)) = (&self.inner.store, &r.store_session)
            && let Err(e) = store.end(id, now_ms, r.session.elapsed_ms(now_ms))
        {
            tracing::warn!(session = %id, error = %e, "store end failed");
        }
    }

    /// Split the session when it reaches the store's recording cap.
    /// Returns the new session state when a split happened.
    fn maybe_split(&self, r: &mut DeviceRecord, now_ms: i64) -> Option<SessionState> {
        let cap = self.inner.config.store.as_ref()?.max_session_ms;
        if cap <= 0 || r.session.elapsed_ms(now_ms) < cap {
            return None;
        }
        r.session.stop(now_ms).ok()?;
        self.end_store_session(r, now_ms);
        r.session.start(now_ms).ok()?;
        self.begin_store_session(r, now_ms);
        Some(r.session_state(now_ms))
    }

    fn session_command(
        &self,
        device_id: &str,
        action: &'static str,
        detail: &str,
        f: impl FnOnce(&Engine, &mut DeviceRecord, i64) -> Result<(), SessionStatus>,
    ) -> Result<SessionState, EngineError> {
        let record = self.record(device_id)?;
        let now = now_ms();
        let state = {
            let mut r = record.lock();
            f(self, &mut r, now)
                .map_err(|status| EngineError::InvalidTransition { action, status })?;
            r.session_state(now)
        };
        self.publish_session_event(device_id, now, detail);
        self.publish(Message::Session {
            device_id: device_id.to_owned(),
            session: state.clone(),
        });
        Ok(state)
    }

    /// Publish a session transition as a `Connection` event; stored too
    /// while the device has an open store session (so "Session started",
    /// "paused", "resumed" and splits appear on the stored timeline).
    fn publish_session_event(&self, device_id: &str, ts_ms: i64, detail: &str) {
        let message = Arc::new(Message::Event {
            device_id: device_id.to_owned(),
            ts_ms,
            kind: EventKind::Connection,
            detail: detail.to_owned(),
            pid: None,
            process: None,
        });
        self.store_write(&message);
        self.publish_arc(message);
    }

    /// Publish a message that is not tied to a device source (startup errors,
    /// tool problems). Kept (last 20) so a UI that subscribes later still
    /// sees it; see [`Engine::notices`].
    pub fn notice(&self, message: Message) {
        {
            let mut n = self.inner.notices.lock();
            n.push(message.clone());
            if n.len() > 20 {
                let drop = n.len() - 20;
                n.drain(..drop);
            }
        }
        self.publish(message);
    }

    /// Publish a `Message::Progress` for a long task (export). Not kept
    /// as a notice.
    pub fn progress(&self, task_id: &str, pct: f32, message: &str) {
        self.publish(Message::Progress {
            task_id: task_id.to_owned(),
            pct: pct.clamp(0.0, 100.0),
            message: message.to_owned(),
        });
    }

    /// Notices published so far with [`Engine::notice`], oldest first.
    pub fn notices(&self) -> Vec<Message> {
        self.inner.notices.lock().clone()
    }

    fn publish(&self, message: Message) {
        self.publish_arc(Arc::new(message));
    }

    fn publish_arc(&self, message: Arc<Message>) {
        // `send` only fails when there is no subscriber; that is fine.
        let _ = self.inner.bus.send(message);
    }

    /// Hand a message to the store writer (never blocks; drops are counted
    /// by the store).
    fn store_write(&self, message: &Arc<Message>) {
        if let Some(store) = &self.inner.store {
            store.write(message);
        }
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
                    r.session.is_recording().then(|| r.session_state(now))
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

    /// Update state for one provider message, feed the store and forward
    /// it when the session allows (see the crate docs for the policy).
    fn handle(&self, message: Message) {
        let Some(device_id) = message.device_id().map(str::to_owned) else {
            self.publish(message);
            return;
        };
        let Some(record) = self.inner.devices.read().get(&device_id).cloned() else {
            tracing::debug!(device = %device_id, "message for unknown device dropped");
            return;
        };
        let message = Arc::new(message);
        let window = self.inner.config.event_window_ms;
        let mut headline = None;
        let mut split = None;
        let forward = {
            let mut r = record.lock();
            let recording = r.session.is_recording();
            match &*message {
                // Live data always reaches the screen and the in-memory
                // rings; recording only decides what is counted and stored.
                Message::Samples { ts_ms, values, .. } => {
                    for v in values {
                        r.latest.insert(v.id, v.value);
                    }
                    r.latest_ts_ms = Some(*ts_ms);
                    r.metrics.push(*ts_ms, values);
                    if recording {
                        r.session.count_samples(1);
                        self.store_write(&message);
                        split = self.maybe_split(&mut r, *ts_ms);
                    }
                    headline = r.evaluate(window, *ts_ms);
                    true
                }
                Message::Cores { ts_ms, cores, .. } => {
                    r.latest_cores = cores.clone();
                    r.cores.push(*ts_ms, cores.clone());
                    if recording {
                        self.store_write(&message);
                    }
                    true
                }
                Message::Processes { rows, .. } => {
                    r.processes = rows.clone();
                    if recording {
                        self.store_write(&message);
                    }
                    true
                }
                Message::Log { entries, .. } => {
                    r.logs.push(entries.iter().cloned());
                    if recording {
                        r.session.count_logs(entries.len() as u64);
                        self.store_write(&message);
                    }
                    true
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
                    if recording {
                        self.store_write(&message);
                    }
                    true
                }
                Message::Info { info, .. } => {
                    r.info = Some((**info).clone());
                    if let (Some(store), Some(sid)) = (&self.inner.store, &r.store_session)
                        && r.session_open()
                        && let Err(e) = store.set_info(sid, info)
                    {
                        tracing::warn!(session = %sid, error = %e, "store info");
                    }
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
            self.publish_arc(message);
        }
        if let Some(headline) = headline {
            self.publish(Message::Headline {
                device_id: device_id.clone(),
                headline,
            });
        }
        if let Some(session) = split {
            let cap_h = self
                .inner
                .config
                .store
                .as_ref()
                .map_or(0.0, |s| s.max_session_ms as f64 / 3_600_000.0);
            self.publish_session_event(
                &device_id,
                session.started_at_ms.unwrap_or_else(now_ms),
                &format!("Session split: {cap_h:.2} h recording cap reached"),
            );
            self.publish(Message::Session { device_id, session });
        }
    }
}

/// Overlay the in-memory counters of a live session onto a stored row.
fn overlay_live(row: &mut SessionSummary, state: &SessionState) {
    row.live = true;
    row.ended_at_ms = None;
    row.elapsed_ms = state.elapsed_ms;
    row.sample_count = row.sample_count.max(state.sample_count);
    row.log_count = row.log_count.max(state.log_count);
    row.mark_count = row.mark_count.max(state.marks.len() as u64);
}
