//! `com.apple.os_trace_relay`: the structured log stream (step 3.5). The
//! thermal state and memory warnings are derived from it (`thermal.rs`).
//!
//! Mirrors `device-android/src/logcat.rs`: one task per device that opens
//! the relay through the provider, maps every `OsTraceLog` to a
//! [`LogEntry`], batches them (≤ [`BATCH_MAX`] entries or
//! [`BATCH_LATENCY`]), feeds each entry through [`Derive`] and reconnects
//! with backoff while the device is still reachable.

use std::sync::Arc;
use std::time::Duration;

use idevice::IdeviceError;
use idevice::IdeviceService;
use idevice::provider::IdeviceProvider;
use idevice::services::os_trace_relay::{
    LogLevel as RelayLevel, OsTraceLog, OsTraceRelayClient, OsTraceRelayReceiver,
};
use idevice::usbmuxd::errors::UsbmuxdError;
use statasmon_device::EventSink;
use statasmon_protocol::{EventKind, LogEntry, LogLevel, MetricValue};
use tokio_util::sync::CancellationToken;

use crate::thermal::{Derive, Derived};

/// Max entries per `Message::Log` (docs/PLAN.md §3.5).
pub const BATCH_MAX: usize = statasmon_device::MAX_LOG_BATCH;
/// Flush a partial batch after this long.
pub const BATCH_LATENCY: Duration = Duration::from_millis(250);
/// Identical events (same kind and process/pid) within this window are one
/// incident reported by several lines (`Formulating…` + `Saved…`, or a
/// system-wide memory warning hitting every app).
const EVENT_DEDUPE_MS: i64 = 10_000;
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(8);
/// Label handed to lockdown for the relay connection.
const LABEL: &str = "os_trace_relay";

/// Map the relay's level byte to the common scale
/// (Notice/Default → Notice).
pub fn level(l: RelayLevel) -> LogLevel {
    match l {
        RelayLevel::Debug => LogLevel::Debug,
        RelayLevel::Info => LogLevel::Info,
        RelayLevel::Notice => LogLevel::Notice,
        RelayLevel::Error => LogLevel::Error,
        RelayLevel::Fault => LogLevel::Fault,
    }
}

/// The relay's timestamp is the packet's seconds + microseconds, exposed
/// as a naive UTC datetime.
pub fn ts_ms(log: &OsTraceLog) -> i64 {
    log.timestamp.and_utc().timestamp_millis()
}

/// Process name: the executable's basename (`filename` is the sender's
/// path on iOS 17; `image_name` the emitting image), else `pid N`.
pub fn process_name(log: &OsTraceLog) -> String {
    basename(&log.filename)
        .or_else(|| basename(&log.image_name))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("pid {}", log.pid))
}

fn basename(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path).trim();
    (!name.is_empty()).then_some(name)
}

/// One relay record → one entry with the given sequence number.
pub fn to_entry(seq: u64, log: &OsTraceLog) -> LogEntry {
    let (subsystem, category) = match &log.label {
        Some(l) => (
            (!l.subsystem.is_empty()).then(|| l.subsystem.clone()),
            (!l.category.is_empty()).then(|| l.category.clone()),
        ),
        None => (None, None),
    };
    LogEntry {
        seq,
        ts_ms: ts_ms(log),
        pid: Some(log.pid),
        process: process_name(log),
        tag: None,
        subsystem,
        category,
        buffer: None,
        level: level(log.level),
        message: log.message.clone(),
    }
}

/// The provider cannot reach the device any more: stop instead of retrying.
fn is_gone(e: &IdeviceError) -> bool {
    matches!(
        e,
        IdeviceError::DeviceNotFound
            | IdeviceError::Usbmuxd(UsbmuxdError::BadDevice)
            | IdeviceError::Usbmuxd(UsbmuxdError::ConnectionRefused)
    )
}

/// Stream logs until `cancel` fires; reconnects with backoff while the
/// device is still attached. Emits `Message::Log` batches (≤ 500 entries or
/// 250 ms), `thermal.state` samples and `ThermalChange` / `MemoryWarning` /
/// `Jetsam` events via `thermal::Derive`.
pub async fn run(
    provider: Arc<dyn IdeviceProvider>,
    sink: EventSink,
    device_id: String,
    cancel: CancellationToken,
) {
    let mut state = StreamState::default();
    let mut backoff = BACKOFF_MIN;
    loop {
        if cancel.is_cancelled() || sink.is_closed() {
            return;
        }
        let opened = tokio::select! {
            _ = cancel.cancelled() => return,
            r = open(&*provider) => r,
        };
        match opened {
            Ok(receiver) => {
                backoff = BACKOFF_MIN;
                match read_stream(receiver, &sink, &device_id, &mut state, &cancel).await {
                    Outcome::Cancelled | Outcome::SinkClosed => return,
                    Outcome::Ended => {
                        tracing::info!(target: "ios.ostrace", "stream ended; reconnecting in {backoff:?}");
                    }
                }
            }
            Err(e) if is_gone(&e) => {
                tracing::info!(target: "ios.ostrace", "device gone: {e}");
                return;
            }
            Err(e) => {
                tracing::warn!(target: "ios.ostrace", "open failed: {e}; retrying in {backoff:?}");
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

async fn open(provider: &dyn IdeviceProvider) -> Result<OsTraceRelayReceiver, IdeviceError> {
    let client = OsTraceRelayClient::connect(provider).await?;
    tracing::debug!(target: "ios.ostrace", "{LABEL} connected via {}", provider.label());
    client.start_trace(None).await
}

enum Outcome {
    Ended,
    Cancelled,
    SinkClosed,
}

/// Everything that survives a reconnect.
#[derive(Default)]
struct StreamState {
    seq: u64,
    derive: Derive,
    recent_events: Vec<(EventKind, String, i64)>,
}

impl StreamState {
    fn next_entry(&mut self, log: &OsTraceLog) -> LogEntry {
        self.seq += 1;
        to_entry(self.seq, log)
    }

    /// One event per incident: same kind and the same process (or pid
    /// when the line names none) inside [`EVENT_DEDUPE_MS`].
    fn dedupe(
        &mut self,
        kind: EventKind,
        pid: Option<u32>,
        process: &Option<String>,
        ts_ms: i64,
    ) -> bool {
        self.recent_events
            .retain(|(_, _, t)| ts_ms - *t < EVENT_DEDUPE_MS);
        let key = match (kind, process, pid) {
            (EventKind::MemoryWarning, _, _) => String::new(),
            (_, Some(p), _) => p.clone(),
            (_, None, Some(pid)) => pid.to_string(),
            (_, None, None) => String::new(),
        };
        if self
            .recent_events
            .iter()
            .any(|(k, p, _)| *k == kind && *p == key)
        {
            return false;
        }
        self.recent_events.push((kind, key, ts_ms));
        true
    }
}

async fn read_stream(
    mut receiver: OsTraceRelayReceiver,
    sink: &EventSink,
    device_id: &str,
    state: &mut StreamState,
    cancel: &CancellationToken,
) -> Outcome {
    let mut pending: Vec<LogEntry> = Vec::with_capacity(BATCH_MAX);
    let flush_after = tokio::time::sleep(BATCH_LATENCY);
    tokio::pin!(flush_after);
    let mut ended = false;
    loop {
        let flush = tokio::select! {
            _ = cancel.cancelled() => return Outcome::Cancelled,
            _ = sink.closed() => return Outcome::SinkClosed,
            _ = &mut flush_after, if !pending.is_empty() => true,
            next = receiver.next() => {
                match next {
                    Ok(log) => {
                        if pending.is_empty() {
                            flush_after.as_mut().reset(tokio::time::Instant::now() + BATCH_LATENCY);
                        }
                        let entry = state.next_entry(&log);
                        pending.push(entry);
                        pending.len() >= BATCH_MAX
                    }
                    Err(e) => {
                        tracing::debug!(target: "ios.ostrace", "read: {e}");
                        ended = true;
                        true
                    }
                }
            }
        };
        if flush && !pending.is_empty() {
            let batch = std::mem::take(&mut pending);
            if let Outcome::SinkClosed = deliver(sink, device_id, state, batch).await {
                return Outcome::SinkClosed;
            }
        }
        if ended {
            return Outcome::Ended;
        }
    }
}

/// An event derived from a batch, sent after the batch itself.
struct PendingEvent {
    ts_ms: i64,
    kind: EventKind,
    detail: String,
    pid: Option<u32>,
    process: Option<String>,
}

/// Derive first (so the batch order is kept), send the batch, then the
/// samples and events it produced.
async fn deliver(
    sink: &EventSink,
    device_id: &str,
    state: &mut StreamState,
    batch: Vec<LogEntry>,
) -> Outcome {
    let mut samples: Vec<(i64, Vec<MetricValue>)> = Vec::new();
    let mut events: Vec<PendingEvent> = Vec::new();
    for entry in &batch {
        let derived = state.derive.observe(entry);
        if derived.is_empty() {
            continue;
        }
        let mut values = Vec::new();
        for d in derived {
            match d {
                Derived::Sample { id, value } => values.push(MetricValue { id, value }),
                Derived::Event {
                    kind,
                    detail,
                    pid,
                    process,
                } => {
                    if state.dedupe(kind, pid, &process, entry.ts_ms) {
                        events.push(PendingEvent {
                            ts_ms: entry.ts_ms,
                            kind,
                            detail,
                            pid,
                            process,
                        });
                    }
                }
            }
        }
        if !values.is_empty() {
            samples.push((entry.ts_ms, values));
        }
    }
    if sink.log(device_id, batch).await.is_err() {
        return Outcome::SinkClosed;
    }
    for (ts_ms, values) in samples {
        if sink.samples(device_id, ts_ms, values).await.is_err() {
            return Outcome::SinkClosed;
        }
    }
    for e in events {
        if sink
            .event(device_id, e.ts_ms, e.kind, e.detail, e.pid, e.process)
            .await
            .is_err()
        {
            return Outcome::SinkClosed;
        }
    }
    Outcome::Ended
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use idevice::services::os_trace_relay::SyslogLabel;

    fn log(level: RelayLevel, filename: &str, image: &str, message: &str) -> OsTraceLog {
        OsTraceLog {
            pid: 71,
            timestamp: DateTime::from_timestamp(1_757_400_000, 123_456_000)
                .unwrap()
                .naive_utc(),
            level,
            image_name: image.into(),
            filename: filename.into(),
            message: message.into(),
            label: filename.contains("thermalmonitord").then(|| SyslogLabel {
                subsystem: "com.apple.thermalmonitord".into(),
                category: "state".into(),
            }),
        }
    }

    #[test]
    fn maps_levels_timestamp_and_names() {
        let l = log(
            RelayLevel::Error,
            "/usr/libexec/thermalmonitord",
            "/System/Library/PrivateFrameworks/Foo.framework/Foo",
            "hello",
        );
        let e = to_entry(7, &l);
        assert_eq!(e.seq, 7);
        assert_eq!(e.ts_ms, 1_757_400_000_123);
        assert_eq!(e.pid, Some(71));
        assert_eq!(e.process, "thermalmonitord");
        assert_eq!(e.subsystem.as_deref(), Some("com.apple.thermalmonitord"));
        assert_eq!(e.category.as_deref(), Some("state"));
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.message, "hello");
        assert!(e.tag.is_none() && e.buffer.is_none());

        assert_eq!(level(RelayLevel::Debug), LogLevel::Debug);
        assert_eq!(level(RelayLevel::Info), LogLevel::Info);
        assert_eq!(level(RelayLevel::Notice), LogLevel::Notice);
        assert_eq!(level(RelayLevel::Fault), LogLevel::Fault);
    }

    #[test]
    fn process_name_falls_back() {
        let mut l = log(RelayLevel::Info, "", "/usr/lib/libFoo.dylib", "m");
        assert_eq!(process_name(&l), "libFoo.dylib");
        l.image_name.clear();
        assert_eq!(process_name(&l), "pid 71");
        l.filename = "kernel".into();
        assert_eq!(process_name(&l), "kernel");
        let mut l = log(RelayLevel::Info, "x", "y", "m");
        l.label = Some(SyslogLabel {
            subsystem: String::new(),
            category: String::new(),
        });
        let e = to_entry(1, &l);
        assert!(e.subsystem.is_none() && e.category.is_none());
    }

    #[test]
    fn dedupe_collapses_one_incident() {
        let mut s = StreamState::default();
        let p = Some("FieldKit".to_owned());
        assert!(s.dedupe(EventKind::Crash, Some(1), &p, 1_000));
        assert!(!s.dedupe(EventKind::Crash, None, &p, 2_000));
        assert!(s.dedupe(EventKind::Jetsam, None, &p, 2_000));
        assert!(s.dedupe(EventKind::Crash, None, &p, 12_000));
        assert!(s.dedupe(EventKind::MemoryWarning, Some(1), &p, 12_000));
        assert!(!s.dedupe(EventKind::MemoryWarning, Some(2), &None, 13_000));
        assert!(s.dedupe(EventKind::MemoryWarning, Some(3), &None, 23_000));
    }

    #[tokio::test]
    async fn deliver_sends_batch_then_samples_then_events() {
        let (sink, mut rx) = EventSink::channel(16);
        let mut state = StreamState::default();
        let batch = vec![
            to_entry(
                1,
                &log(
                    RelayLevel::Notice,
                    "/usr/libexec/thermalmonitord",
                    "",
                    "thermal state: Fair",
                ),
            ),
            to_entry(
                2,
                &log(
                    RelayLevel::Notice,
                    "/usr/libexec/thermalmonitord",
                    "",
                    "thermal state: Serious",
                ),
            ),
            to_entry(
                3,
                &log(
                    RelayLevel::Notice,
                    "/System/Library/CoreServices/ReportCrash",
                    "",
                    "Formulating fatal 309 report for corpse[9] FieldKit",
                ),
            ),
            to_entry(
                4,
                &log(
                    RelayLevel::Notice,
                    "/System/Library/CoreServices/ReportCrash",
                    "",
                    "Saved type '309' report (1 of max 25) at /x/FieldKit-2026-09-09-1.ips",
                ),
            ),
        ];
        assert!(matches!(
            deliver(&sink, "ios:1", &mut state, batch).await,
            Outcome::Ended
        ));
        let mut kinds = Vec::new();
        while let Ok(m) = rx.try_recv() {
            kinds.push(match m {
                statasmon_protocol::Message::Log { entries, .. } => format!("log{}", entries.len()),
                statasmon_protocol::Message::Samples { values, .. } => {
                    format!("samples{}", values.len())
                }
                statasmon_protocol::Message::Event { kind, detail, .. } => {
                    format!("{kind:?}:{detail}")
                }
                other => panic!("unexpected {other:?}"),
            });
        }
        assert_eq!(
            kinds,
            vec![
                "log4",
                "samples2",
                "samples2",
                "ThermalChange:Fair → Serious",
                "Crash:Crash: FieldKit",
            ]
        );
        assert_eq!(state.derive.last_state(), Some(2));
    }
}
