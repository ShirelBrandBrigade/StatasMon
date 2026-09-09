//! The [`Store`]: index database, session lifecycle, the writer handle and
//! the read API. One `Store` per app-data root; cheap to share as an `Arc`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use statasmon_protocol::{
    CoreSample, DeviceId, DeviceInfo, DeviceSummary, EventKind, HistoryPoint, LogFilter, LogPage,
    Mark, Message, MetricId, SessionDetail, SessionSummary, StoredEvent,
};

use crate::codec;
use crate::config::StoreConfig;
use crate::error::{StoreError, StoreResult};
use crate::ids::sanitize;
use crate::reader::SessionReader;
use crate::schema;
use crate::writer::{self, NewSession, Op, Writer};

/// How long `flush`/`sweep` wait for the writer.
const FLUSH_TIMEOUT: Duration = Duration::from_secs(30);

/// Counters of one session as kept in the index.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStats {
    pub sample_count: u64,
    pub log_count: u64,
    pub event_count: u64,
    pub mark_count: u64,
    pub size_bytes: u64,
}

/// Totals over the whole store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreStats {
    pub root: String,
    pub total_bytes: u64,
    pub session_count: u64,
}

/// The session store.
pub struct Store {
    config: StoreConfig,
    index: Mutex<Connection>,
    tx: SyncSender<Op>,
    /// Device id → the session it is recording into.
    active: Mutex<BTreeMap<DeviceId, String>>,
    /// Ids begun in this process (dedup before the writer has inserted them).
    known: Mutex<BTreeSet<String>>,
    dropped: AtomicU64,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    gate: Arc<Mutex<()>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("root", &self.config.root)
            .field("active", &self.active.lock().len())
            .field("dropped", &self.dropped.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

fn recover(index: &Connection) -> StoreResult<usize> {
    // Sessions left open by a crash: end them at their last sample.
    let n = index.execute(
        "UPDATE sessions SET \
         ended_at_ms = COALESCE(last_ts_ms, started_at_ms), \
         elapsed_ms = MAX(elapsed_ms, COALESCE(last_ts_ms, started_at_ms) - started_at_ms) \
         WHERE ended_at_ms IS NULL",
        [],
    )?;
    Ok(n)
}

const SUMMARY_COLUMNS: &str = "id, device_id, device_name, model_name, platform, started_at_ms, \
     ended_at_ms, elapsed_ms, sample_count, log_count, event_count, mark_count, size_bytes";

fn summary_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionSummary> {
    Ok(SessionSummary {
        id: r.get(0)?,
        device_id: r.get(1)?,
        device_name: r.get(2)?,
        model_name: r.get(3)?,
        platform: codec::parse_platform(&r.get::<_, String>(4)?),
        started_at_ms: r.get(5)?,
        ended_at_ms: r.get(6)?,
        elapsed_ms: r.get(7)?,
        sample_count: r.get::<_, i64>(8)? as u64,
        log_count: r.get::<_, i64>(9)? as u64,
        event_count: r.get::<_, i64>(10)? as u64,
        mark_count: r.get::<_, i64>(11)? as u64,
        size_bytes: r.get::<_, i64>(12)? as u64,
        live: false,
    })
}

impl Store {
    /// Open (creating) the store under `config.root` and start the writer.
    pub fn open(config: StoreConfig) -> StoreResult<Arc<Self>> {
        std::fs::create_dir_all(config.sessions_dir())?;
        let index = schema::open_index(&config.index_path())?;
        let recovered = recover(&index)?;
        if recovered > 0 {
            tracing::info!(
                sessions = recovered,
                "store: closed sessions left open by a previous run"
            );
        }
        let gate = Arc::new(Mutex::new(()));
        let writer = Writer::new(config.clone(), Arc::clone(&gate))?;
        let (tx, rx) = sync_channel(config.writer_capacity.max(8));
        let thread = std::thread::Builder::new()
            .name("statasmon-store".to_owned())
            .spawn(move || writer.run(rx))?;
        Ok(Arc::new(Self {
            config,
            index: Mutex::new(index),
            tx,
            active: Mutex::new(BTreeMap::new()),
            known: Mutex::new(BTreeSet::new()),
            dropped: AtomicU64::new(0),
            thread: Mutex::new(Some(thread)),
            gate,
        }))
    }

    /// The configuration in use.
    pub fn config(&self) -> &StoreConfig {
        &self.config
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.config.root
    }

    /// Messages dropped because the writer queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Test hook: while this mutex is held the writer applies no batch.
    #[doc(hidden)]
    pub fn writer_gate(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.gate)
    }

    fn send(&self, op: Op) -> StoreResult<()> {
        self.tx.send(op).map_err(|_| StoreError::Closed)
    }

    fn exists_in_index(&self, id: &str) -> StoreResult<bool> {
        let index = self.index.lock();
        let found: Option<i64> = index
            .query_row("SELECT 1 FROM sessions WHERE id = ?1", [id], |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    fn unique_id(&self, base: &str) -> StoreResult<String> {
        let base = sanitize(base);
        let mut id = base.clone();
        let mut n = 2;
        loop {
            let taken = self.known.lock().contains(&id)
                || self.exists_in_index(&id)?
                || self.config.session_path(&id).exists();
            if !taken {
                return Ok(id);
            }
            id = format!("{base}-{n}");
            n += 1;
        }
    }

    /// Start recording `device` into a new session named after `base_id`
    /// (made unique with a numeric suffix if needed). Returns the id.
    pub fn begin(
        &self,
        base_id: &str,
        summary: &DeviceSummary,
        info: Option<&DeviceInfo>,
        started_at_ms: i64,
    ) -> StoreResult<String> {
        let id = self.unique_id(base_id)?;
        self.known.lock().insert(id.clone());
        let displaced = self.active.lock().insert(summary.id.clone(), id.clone());
        if let Some(old) = displaced {
            // The engine ends before it begins; keep the index consistent anyway.
            tracing::warn!(session = %old, device = %summary.id, "session begun while another was live; ending it");
            self.send(Op::End {
                session_id: old,
                ended_at_ms: started_at_ms,
                elapsed_ms: -1,
            })?;
        }
        self.send(Op::Begin(Box::new(NewSession {
            id: id.clone(),
            path: self.config.session_path(&id),
            summary: summary.clone(),
            info: info.cloned(),
            started_at_ms,
        })))?;
        Ok(id)
    }

    /// Attach (or replace) the device information document of a session.
    pub fn set_info(&self, session_id: &str, info: &DeviceInfo) -> StoreResult<()> {
        self.send(Op::SetInfo {
            session_id: session_id.to_owned(),
            info: Box::new(info.clone()),
        })
    }

    /// Queue a `Samples`/`Cores`/`Processes`/`Log`/`Event` message for the
    /// session its device is recording into. Never blocks: returns `false`
    /// (and counts a drop) when the queue is full, when the device has no
    /// active session, or for message kinds that are not stored.
    pub fn write(&self, message: &Arc<Message>) -> bool {
        let storable = match &**message {
            Message::Samples { .. }
            | Message::Cores { .. }
            | Message::Processes { .. }
            | Message::Log { .. } => true,
            Message::Event { kind, .. } => *kind != EventKind::Mark,
            _ => false,
        };
        if !storable {
            return false;
        }
        let Some(device_id) = message.device_id() else {
            return false;
        };
        let Some(session_id) = self.active.lock().get(device_id).cloned() else {
            return false;
        };
        match self.tx.try_send(Op::Message {
            session_id,
            message: Arc::clone(message),
        }) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                let n = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                if n == 1 || n.is_multiple_of(1000) {
                    tracing::warn!(dropped = n, "store writer queue full; dropping messages");
                }
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    /// Like [`Store::write`] but waits for room in the queue instead of
    /// dropping (offline imports, replays). Returns `false` only for
    /// messages that are not stored or devices without a session.
    pub fn write_blocking(&self, message: &Arc<Message>) -> StoreResult<bool> {
        let Some(device_id) = message.device_id() else {
            return Ok(false);
        };
        let storable = match &**message {
            Message::Samples { .. }
            | Message::Cores { .. }
            | Message::Processes { .. }
            | Message::Log { .. } => true,
            Message::Event { kind, .. } => *kind != EventKind::Mark,
            _ => false,
        };
        if !storable {
            return Ok(false);
        }
        let Some(session_id) = self.active.lock().get(device_id).cloned() else {
            return Ok(false);
        };
        self.send(Op::Message {
            session_id,
            message: Arc::clone(message),
        })?;
        Ok(true)
    }

    /// Store a mark in `session_id`.
    pub fn mark(&self, session_id: &str, mark: &Mark) -> StoreResult<()> {
        self.send(Op::Mark {
            session_id: session_id.to_owned(),
            mark: mark.clone(),
        })
    }

    /// End a session: `elapsed_ms` is the recording time excluding pauses
    /// (negative keeps the value the writer derived from the last sample).
    pub fn end(&self, session_id: &str, ended_at_ms: i64, elapsed_ms: i64) -> StoreResult<()> {
        self.active.lock().retain(|_, s| s != session_id);
        self.send(Op::End {
            session_id: session_id.to_owned(),
            ended_at_ms,
            elapsed_ms,
        })
    }

    /// The session `device_id` is recording into, if any.
    pub fn active_session(&self, device_id: &str) -> Option<String> {
        self.active.lock().get(device_id).cloned()
    }

    /// True while some device records into `session_id`.
    pub fn is_live(&self, session_id: &str) -> bool {
        self.active.lock().values().any(|s| s == session_id)
    }

    /// Wait until every queued op has been written.
    pub fn flush(&self) -> StoreResult<()> {
        self.barrier(Op::Flush)
    }

    /// Run the retention sweep now and wait for it.
    pub fn sweep(&self) -> StoreResult<()> {
        self.barrier(Op::Sweep)
    }

    fn barrier(&self, op: fn(SyncSender<()>) -> Op) -> StoreResult<()> {
        let (tx, rx) = sync_channel(1);
        self.send(op(tx))?;
        rx.recv_timeout(FLUSH_TIMEOUT)
            .map_err(|_| StoreError::Timeout)
    }

    /// Every stored session, newest first, with the live flag set for the
    /// ones still being recorded.
    pub fn list(&self) -> StoreResult<Vec<SessionSummary>> {
        let rows: Vec<SessionSummary> = {
            let index = self.index.lock();
            let mut stmt = index.prepare(&format!(
                "SELECT {SUMMARY_COLUMNS} FROM sessions ORDER BY started_at_ms DESC, id DESC"
            ))?;
            let rows = stmt.query_map([], summary_from_row)?;
            rows.collect::<Result<_, _>>()?
        };
        let active = self.active.lock();
        Ok(rows
            .into_iter()
            .map(|mut s| {
                s.live = active.values().any(|id| *id == s.id);
                s
            })
            .collect())
    }

    /// One session's summary.
    pub fn get(&self, session_id: &str) -> StoreResult<SessionSummary> {
        let row = {
            let index = self.index.lock();
            index
                .query_row(
                    &format!("SELECT {SUMMARY_COLUMNS} FROM sessions WHERE id = ?1"),
                    [session_id],
                    summary_from_row,
                )
                .optional()?
        };
        let mut s = row.ok_or_else(|| StoreError::UnknownSession(session_id.to_owned()))?;
        s.live = self.is_live(session_id);
        Ok(s)
    }

    /// Counters from the index (fresh after [`Store::flush`]).
    pub fn stats(&self, session_id: &str) -> StoreResult<SessionStats> {
        let s = self.get(session_id)?;
        Ok(SessionStats {
            sample_count: s.sample_count,
            log_count: s.log_count,
            event_count: s.event_count,
            mark_count: s.mark_count,
            size_bytes: s.size_bytes,
        })
    }

    /// Totals over every session.
    pub fn store_stats(&self) -> StoreResult<StoreStats> {
        let (total_bytes, session_count): (i64, i64) = self.index.lock().query_row(
            "SELECT COALESCE(SUM(size_bytes), 0), COUNT(*) FROM sessions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(StoreStats {
            root: self.config.root.to_string_lossy().into_owned(),
            total_bytes: total_bytes.max(0) as u64,
            session_count: session_count.max(0) as u64,
        })
    }

    fn path_of(&self, session_id: &str) -> StoreResult<PathBuf> {
        let path: Option<String> = self
            .index
            .lock()
            .query_row(
                "SELECT path FROM sessions WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .optional()?;
        path.map(PathBuf::from)
            .ok_or_else(|| StoreError::UnknownSession(session_id.to_owned()))
    }

    /// Stored device information of a session, if any.
    pub fn info(&self, session_id: &str) -> StoreResult<Option<DeviceInfo>> {
        let json: Option<String> = self
            .index
            .lock()
            .query_row(
                "SELECT device_info_json FROM sessions WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(match json {
            Some(j) => Some(serde_json::from_str(&j)?),
            None => None,
        })
    }

    /// A read connection to one session.
    pub fn reader(&self, session_id: &str) -> StoreResult<SessionReader> {
        let path = self.path_of(session_id)?;
        SessionReader::open(&path, session_id)
    }

    /// Everything the UI needs to open a session.
    pub fn open_session(&self, session_id: &str) -> StoreResult<SessionDetail> {
        let summary = self.get(session_id)?;
        let info = self.info(session_id)?;
        let reader = self.reader(session_id)?;
        Ok(SessionDetail {
            summary,
            info,
            marks: reader.marks()?,
            events: reader.events()?,
            metrics: reader.metrics()?,
        })
    }

    /// Downsampled history (see [`SessionReader::history`]).
    pub fn history(
        &self,
        session_id: &str,
        metric: MetricId,
        from_ms: i64,
        to_ms: i64,
        step_ms: u32,
    ) -> StoreResult<Vec<HistoryPoint>> {
        self.reader(session_id)?
            .history(metric, from_ms, to_ms, step_ms)
    }

    /// Per-core ticks in `[from_ms, to_ms]`.
    pub fn cores_history(
        &self,
        session_id: &str,
        from_ms: i64,
        to_ms: i64,
    ) -> StoreResult<Vec<(i64, Vec<CoreSample>)>> {
        self.reader(session_id)?.cores(from_ms, to_ms)
    }

    /// A page of stored log entries.
    pub fn logs(
        &self,
        session_id: &str,
        filter: &LogFilter,
        offset: usize,
        limit: usize,
    ) -> StoreResult<LogPage> {
        self.reader(session_id)?.logs(filter, offset, limit)
    }

    /// Matching stored log entries.
    pub fn count_logs(&self, session_id: &str, filter: &LogFilter) -> StoreResult<u64> {
        self.reader(session_id)?.count_logs(filter)
    }

    /// Stored events.
    pub fn events(&self, session_id: &str) -> StoreResult<Vec<StoredEvent>> {
        self.reader(session_id)?.events()
    }

    /// Stored marks.
    pub fn marks(&self, session_id: &str) -> StoreResult<Vec<Mark>> {
        self.reader(session_id)?.marks()
    }

    /// Delete a stored session (index row and file). Live sessions must be
    /// ended first.
    pub fn delete(&self, session_id: &str) -> StoreResult<()> {
        if self.is_live(session_id) {
            return Err(StoreError::Live(session_id.to_owned()));
        }
        let path = self.path_of(session_id)?;
        // Make sure the writer has processed the `End` and closed the file.
        self.flush()?;
        writer::delete_files(&path)?;
        self.index
            .lock()
            .execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
        self.known.lock().remove(session_id);
        Ok(())
    }

    /// Flush, stop the writer thread and checkpoint. Idempotent.
    pub fn close(&self) {
        let Some(thread) = self.thread.lock().take() else {
            return;
        };
        let _ = self.tx.send(Op::Shutdown);
        let _ = thread.join();
        let _ = self
            .index
            .lock()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_serialize_as_plain_numbers() {
        let s = StoreStats {
            root: "/x".into(),
            total_bytes: 12,
            session_count: 1,
        };
        let json = serde_json::to_string(&s).map_err(|e| e.to_string());
        assert_eq!(
            json,
            Ok("{\"root\":\"/x\",\"total_bytes\":12,\"session_count\":1}".to_owned())
        );
    }
}
