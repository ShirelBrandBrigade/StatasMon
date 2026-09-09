//! The channel providers push protocol messages into. One sink per engine;
//! providers clone it freely (it is an `mpsc::Sender`).

use statasmon_protocol::{
    CapabilitySet, ConnectionState, CoreSample, DeviceInfo, EventKind, LogEntry, Message,
    MetricValue, ProcessRow,
};
use tokio::sync::mpsc;

use crate::error::DeviceError;

/// Maximum log entries per `Message::Log` (docs/PLAN.md §3.5).
pub const MAX_LOG_BATCH: usize = 500;

/// The receiving side is gone: the engine shut down. Providers treat this
/// as a request to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("event sink closed: the engine is gone")]
pub struct SinkClosed;

/// Cheap-to-clone handle a provider uses to publish messages.
#[derive(Debug, Clone)]
pub struct EventSink {
    tx: mpsc::Sender<Message>,
}

impl EventSink {
    /// Wrap an existing sender.
    pub fn new(tx: mpsc::Sender<Message>) -> Self {
        Self { tx }
    }

    /// A new bounded channel with its sink.
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<Message>) {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        (Self { tx }, rx)
    }

    /// True once the engine dropped the receiver.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Wait until the receiver is dropped (useful in `select!` loops).
    pub async fn closed(&self) {
        self.tx.closed().await;
    }

    /// Publish one message, waiting for space when the engine is behind.
    pub async fn send(&self, message: Message) -> Result<(), SinkClosed> {
        self.tx.send(message).await.map_err(|_| SinkClosed)
    }

    /// One tick of scalar metrics.
    pub async fn samples(
        &self,
        device_id: &str,
        ts_ms: i64,
        values: Vec<MetricValue>,
    ) -> Result<(), SinkClosed> {
        self.send(Message::Samples {
            device_id: device_id.to_owned(),
            ts_ms,
            values,
        })
        .await
    }

    /// One tick of per-core samples.
    pub async fn cores(
        &self,
        device_id: &str,
        ts_ms: i64,
        cores: Vec<CoreSample>,
    ) -> Result<(), SinkClosed> {
        self.send(Message::Cores {
            device_id: device_id.to_owned(),
            ts_ms,
            cores,
        })
        .await
    }

    /// A process table snapshot.
    pub async fn processes(
        &self,
        device_id: &str,
        ts_ms: i64,
        rows: Vec<ProcessRow>,
    ) -> Result<(), SinkClosed> {
        self.send(Message::Processes {
            device_id: device_id.to_owned(),
            ts_ms,
            rows,
        })
        .await
    }

    /// Log entries, split into batches of at most [`MAX_LOG_BATCH`].
    /// An empty vector sends nothing.
    pub async fn log(&self, device_id: &str, entries: Vec<LogEntry>) -> Result<(), SinkClosed> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut entries = entries;
        while entries.len() > MAX_LOG_BATCH {
            let rest = entries.split_off(MAX_LOG_BATCH);
            let batch = std::mem::replace(&mut entries, rest);
            self.send(Message::Log {
                device_id: device_id.to_owned(),
                entries: batch,
            })
            .await?;
        }
        self.send(Message::Log {
            device_id: device_id.to_owned(),
            entries,
        })
        .await
    }

    /// A discrete event (thermal change, crash, memory warning, ...).
    pub async fn event(
        &self,
        device_id: &str,
        ts_ms: i64,
        kind: EventKind,
        detail: impl Into<String>,
        pid: Option<u32>,
        process: Option<String>,
    ) -> Result<(), SinkClosed> {
        self.send(Message::Event {
            device_id: device_id.to_owned(),
            ts_ms,
            kind,
            detail: detail.into(),
            pid,
            process,
        })
        .await
    }

    /// The full device information document (re-sent whenever it changes).
    pub async fn info(&self, device_id: &str, info: DeviceInfo) -> Result<(), SinkClosed> {
        self.send(Message::Info {
            device_id: device_id.to_owned(),
            info: Box::new(info),
        })
        .await
    }

    /// The capability set (re-sent when it is re-evaluated).
    pub async fn capabilities(
        &self,
        device_id: &str,
        set: CapabilitySet,
    ) -> Result<(), SinkClosed> {
        self.send(Message::Capabilities {
            device_id: device_id.to_owned(),
            set,
        })
        .await
    }

    /// A connection-state transition.
    pub async fn state(
        &self,
        device_id: &str,
        state: ConnectionState,
        detail: Option<String>,
    ) -> Result<(), SinkClosed> {
        self.send(Message::DeviceState {
            device_id: device_id.to_owned(),
            state,
            detail,
        })
        .await
    }

    /// A provider error surfaced to the UI.
    pub async fn error(
        &self,
        device_id: Option<&str>,
        err: &DeviceError,
    ) -> Result<(), SinkClosed> {
        self.send(err.to_message(device_id.map(str::to_owned)))
            .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use statasmon_protocol::{LogLevel, MetricId};

    fn entry(seq: u64) -> LogEntry {
        LogEntry {
            seq,
            ts_ms: 1,
            pid: Some(1),
            process: "p".into(),
            tag: None,
            subsystem: None,
            category: None,
            buffer: None,
            level: LogLevel::Info,
            message: "m".into(),
        }
    }

    #[tokio::test]
    async fn samples_helper_builds_the_message() {
        let (sink, mut rx) = EventSink::channel(4);
        sink.samples(
            "fake:1",
            10,
            vec![MetricValue {
                id: MetricId::CpuTotal,
                value: 1.0,
            }],
        )
        .await
        .unwrap();
        match rx.recv().await.unwrap() {
            Message::Samples {
                device_id,
                ts_ms,
                values,
            } => {
                assert_eq!(device_id, "fake:1");
                assert_eq!(ts_ms, 10);
                assert_eq!(values.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn log_helper_splits_batches_and_skips_empty() {
        let (sink, mut rx) = EventSink::channel(8);
        sink.log("fake:1", vec![]).await.unwrap();
        let entries: Vec<LogEntry> = (0..1200).map(entry).collect();
        sink.log("fake:1", entries).await.unwrap();
        let mut sizes = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::Log { entries, .. } => sizes.push(entries.len()),
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(sizes, vec![500, 500, 200]);
    }

    #[tokio::test]
    async fn event_and_error_helpers() {
        let (sink, mut rx) = EventSink::channel(4);
        sink.event(
            "fake:1",
            5,
            EventKind::Crash,
            "boom",
            Some(42),
            Some("FieldKit".into()),
        )
        .await
        .unwrap();
        let err = DeviceError::unsupported("pcap");
        sink.error(Some("fake:1"), &err).await.unwrap();
        assert!(matches!(
            rx.recv().await.unwrap(),
            Message::Event {
                kind: EventKind::Crash,
                pid: Some(42),
                ..
            }
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            Message::Error {
                code: statasmon_protocol::ErrorCode::Unsupported,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn closed_receiver_reports_sink_closed() {
        let (sink, rx) = EventSink::channel(1);
        drop(rx);
        assert!(sink.is_closed());
        assert_eq!(sink.samples("fake:1", 0, vec![]).await, Err(SinkClosed));
    }
}
