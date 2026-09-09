//! The single writer thread: receives ops over a bounded `std::sync::mpsc`
//! channel, batches everything that arrives within one second into one
//! transaction per session, keeps the index counters current, and runs the
//! retention sweep once a day.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use statasmon_protocol::{DeviceInfo, DeviceSummary, Mark, Message};

use crate::codec;
use crate::config::StoreConfig;
use crate::error::StoreResult;
use crate::schema::{self, SESSION_VERSION};

/// Everything that arrived within this window goes into one transaction.
const BATCH_WINDOW: Duration = Duration::from_secs(1);
/// Upper bound on ops per batch.
const MAX_BATCH: usize = 4096;
/// How often the idle writer checks whether a sweep is due.
const SWEEP_POLL: Duration = Duration::from_secs(60);
/// Sweep interval.
const SWEEP_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// A session the writer opens.
pub(crate) struct NewSession {
    pub id: String,
    pub path: PathBuf,
    pub summary: DeviceSummary,
    pub info: Option<DeviceInfo>,
    pub started_at_ms: i64,
}

/// What the store sends to the writer thread.
pub(crate) enum Op {
    Begin(Box<NewSession>),
    SetInfo {
        session_id: String,
        info: Box<DeviceInfo>,
    },
    Message {
        session_id: String,
        message: Arc<Message>,
    },
    Mark {
        session_id: String,
        mark: Mark,
    },
    End {
        session_id: String,
        ended_at_ms: i64,
        elapsed_ms: i64,
    },
    /// Apply everything queued so far, then answer.
    Flush(SyncSender<()>),
    /// Run the retention sweep now, then answer.
    Sweep(SyncSender<()>),
    Shutdown,
}

impl Op {
    /// Ops after which the batch is applied immediately.
    fn is_barrier(&self) -> bool {
        matches!(self, Op::Flush(_) | Op::Sweep(_) | Op::Shutdown)
    }
}

#[derive(Default)]
struct Pending {
    samples: u64,
    logs: u64,
    events: u64,
    marks: u64,
    last_ts_ms: Option<i64>,
}

struct OpenSession {
    conn: Connection,
    path: PathBuf,
    in_txn: bool,
    pending: Pending,
}

pub(crate) struct Writer {
    config: StoreConfig,
    index: Connection,
    open: HashMap<String, OpenSession>,
    gate: Arc<Mutex<()>>,
    last_sweep: Option<Instant>,
}

/// Wall clock in ms.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Size of a session database including its WAL.
pub(crate) fn db_size(path: &Path) -> u64 {
    let main = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let wal = std::fs::metadata(wal_path(path))
        .map(|m| m.len())
        .unwrap_or(0);
    main + wal
}

fn wal_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push("-wal");
    PathBuf::from(s)
}

fn shm_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push("-shm");
    PathBuf::from(s)
}

/// Remove a session database and its side files.
pub(crate) fn delete_files(path: &Path) -> StoreResult<()> {
    for p in [path.to_path_buf(), wal_path(path), shm_path(path)] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

impl Writer {
    pub(crate) fn new(config: StoreConfig, gate: Arc<Mutex<()>>) -> StoreResult<Self> {
        let index = schema::open_index(&config.index_path())?;
        Ok(Self {
            config,
            index,
            open: HashMap::new(),
            gate,
            last_sweep: None,
        })
    }

    /// The thread body.
    pub(crate) fn run(mut self, rx: Receiver<Op>) {
        self.maybe_sweep();
        loop {
            let first = match rx.recv_timeout(SWEEP_POLL) {
                Ok(op) => op,
                Err(RecvTimeoutError::Timeout) => {
                    self.maybe_sweep();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let mut disconnected = false;
            let mut batch = Vec::with_capacity(64);
            let barrier = first.is_barrier();
            batch.push(first);
            if !barrier {
                let deadline = Instant::now() + BATCH_WINDOW;
                loop {
                    let now = Instant::now();
                    if now >= deadline || batch.len() >= MAX_BATCH {
                        break;
                    }
                    match rx.recv_timeout(deadline - now) {
                        Ok(op) => {
                            let barrier = op.is_barrier();
                            batch.push(op);
                            if barrier {
                                break;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
            }
            let shutdown = {
                let gate = Arc::clone(&self.gate);
                let _gate = gate.lock();
                self.apply(batch)
            };
            if shutdown || disconnected {
                break;
            }
            self.maybe_sweep();
        }
        self.close_all();
    }

    /// Apply one batch. Returns true when a shutdown was requested.
    fn apply(&mut self, batch: Vec<Op>) -> bool {
        let mut flushes = Vec::new();
        let mut sweeps = Vec::new();
        let mut shutdown = false;
        for op in batch {
            let result = match op {
                Op::Begin(s) => self.begin(*s),
                Op::SetInfo { session_id, info } => self.set_info(&session_id, &info),
                Op::Message {
                    session_id,
                    message,
                } => self.write_message(&session_id, &message),
                Op::Mark { session_id, mark } => self.write_mark(&session_id, &mark),
                Op::End {
                    session_id,
                    ended_at_ms,
                    elapsed_ms,
                } => self.end(&session_id, ended_at_ms, elapsed_ms),
                Op::Flush(tx) => {
                    flushes.push(tx);
                    Ok(())
                }
                Op::Sweep(tx) => {
                    sweeps.push(tx);
                    Ok(())
                }
                Op::Shutdown => {
                    shutdown = true;
                    Ok(())
                }
            };
            if let Err(e) = result {
                tracing::error!(error = %e, "store write failed");
            }
        }
        self.commit_all();
        for tx in sweeps {
            self.sweep();
            let _ = tx.send(());
        }
        for tx in flushes {
            let _ = tx.send(());
        }
        shutdown
    }

    fn begin(&mut self, s: NewSession) -> StoreResult<()> {
        if let Some(parent) = s.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let (conn, _fts) = schema::open_session(&s.path, true)?;
        schema::meta_set(&conn, "schema_version", &SESSION_VERSION.to_string())?;
        schema::meta_set(
            &conn,
            "protocol_version",
            &statasmon_protocol::PROTOCOL_VERSION.to_string(),
        )?;
        schema::meta_set(&conn, "session_id", &s.id)?;
        schema::meta_set(&conn, "device_id", &s.summary.id)?;
        schema::meta_set(&conn, "started_at_ms", &s.started_at_ms.to_string())?;
        let info_json = match &s.info {
            Some(info) => Some(serde_json::to_string(info)?),
            None => None,
        };
        self.index.execute(
            "INSERT OR REPLACE INTO sessions(id, device_id, device_name, model_name, platform, \
             started_at_ms, device_info_json, path, size_bytes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                s.id,
                s.summary.id,
                s.summary.name,
                s.summary.model_name,
                codec::platform_name(s.summary.platform),
                s.started_at_ms,
                info_json,
                s.path.to_string_lossy(),
                db_size(&s.path) as i64,
            ],
        )?;
        self.open.insert(
            s.id,
            OpenSession {
                conn,
                path: s.path,
                in_txn: false,
                pending: Pending::default(),
            },
        );
        Ok(())
    }

    fn set_info(&mut self, session_id: &str, info: &DeviceInfo) -> StoreResult<()> {
        let json = serde_json::to_string(info)?;
        self.index.execute(
            "UPDATE sessions SET device_info_json = ?1 WHERE id = ?2",
            params![json, session_id],
        )?;
        Ok(())
    }

    fn session(&mut self, session_id: &str) -> Option<&mut OpenSession> {
        let s = self.open.get_mut(session_id)?;
        if !s.in_txn {
            if let Err(e) = s.conn.execute_batch("BEGIN IMMEDIATE") {
                tracing::error!(session = session_id, error = %e, "begin transaction");
                return None;
            }
            s.in_txn = true;
        }
        Some(s)
    }

    fn write_message(&mut self, session_id: &str, message: &Message) -> StoreResult<()> {
        let Some(s) = self.session(session_id) else {
            tracing::debug!(
                session = session_id,
                "message for a session that is not open"
            );
            return Ok(());
        };
        let conn = &s.conn;
        let pending = &mut s.pending;
        match message {
            Message::Samples { ts_ms, values, .. } => {
                let mut stmt = conn.prepare_cached(
                    "INSERT INTO samples(ts_ms, metric, value) VALUES (?1, ?2, ?3)",
                )?;
                for v in values {
                    stmt.execute(params![ts_ms, codec::metric_name(v.id), v.value])?;
                }
                pending.samples += 1;
                pending.last_ts_ms = Some(pending.last_ts_ms.map_or(*ts_ms, |t| t.max(*ts_ms)));
            }
            Message::Cores { ts_ms, cores, .. } => {
                let mut stmt = conn.prepare_cached(
                    "INSERT INTO cores(ts_ms, idx, kind, usage, freq_mhz, online) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )?;
                for c in cores {
                    stmt.execute(params![
                        ts_ms,
                        c.index,
                        codec::core_kind_name(c.kind),
                        f64::from(c.usage_pct),
                        c.freq_mhz,
                        c.online
                    ])?;
                }
            }
            Message::Processes { ts_ms, rows, .. } => {
                let mut stmt = conn.prepare_cached(
                    "INSERT INTO processes(ts_ms, pid, name, app_id, cpu, mem, threads, net, state) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )?;
                for r in rows {
                    stmt.execute(params![
                        ts_ms,
                        r.pid,
                        r.name,
                        r.app_id,
                        r.cpu_pct.map(f64::from),
                        r.mem_bytes.map(|m| m.min(i64::MAX as u64) as i64),
                        r.threads,
                        r.net_bps,
                        codec::process_state_name(r.state)
                    ])?;
                }
            }
            Message::Log { entries, .. } => {
                let mut stmt = conn.prepare_cached(
                    "INSERT OR IGNORE INTO logs(seq, ts_ms, pid, process, tag, subsystem, \
                     category, buffer, level, message) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for e in entries {
                    let n = stmt.execute(params![
                        e.seq as i64,
                        e.ts_ms,
                        e.pid,
                        e.process,
                        e.tag,
                        e.subsystem,
                        e.category,
                        e.buffer,
                        codec::level_name(e.level),
                        e.message
                    ])?;
                    pending.logs += n as u64;
                    pending.last_ts_ms =
                        Some(pending.last_ts_ms.map_or(e.ts_ms, |t| t.max(e.ts_ms)));
                }
            }
            Message::Event {
                ts_ms,
                kind,
                detail,
                pid,
                process,
                ..
            } => {
                conn.prepare_cached(
                    "INSERT INTO events(ts_ms, kind, detail, pid, process) VALUES (?1, ?2, ?3, ?4, ?5)",
                )?
                .execute(params![
                    ts_ms,
                    codec::event_kind_name(*kind),
                    detail,
                    pid,
                    process
                ])?;
                pending.events += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn write_mark(&mut self, session_id: &str, mark: &Mark) -> StoreResult<()> {
        let Some(s) = self.session(session_id) else {
            return Ok(());
        };
        s.conn
            .prepare_cached("INSERT INTO marks(ts_ms, label) VALUES (?1, ?2)")?
            .execute(params![mark.ts_ms, mark.label])?;
        s.pending.marks += 1;
        Ok(())
    }

    /// Commit the open transaction of one session and fold its pending
    /// counters into the index.
    fn commit_one(&mut self, session_id: &str) -> StoreResult<()> {
        let Some(s) = self.open.get_mut(session_id) else {
            return Ok(());
        };
        if s.in_txn {
            s.in_txn = false;
            if let Err(e) = s.conn.execute_batch("COMMIT") {
                let _ = s.conn.execute_batch("ROLLBACK");
                return Err(e.into());
            }
        }
        let pending = std::mem::take(&mut s.pending);
        let size = db_size(&s.path) as i64;
        self.index.execute(
            "UPDATE sessions SET \
             sample_count = sample_count + ?1, log_count = log_count + ?2, \
             event_count = event_count + ?3, mark_count = mark_count + ?4, \
             last_ts_ms = NULLIF(MAX(COALESCE(last_ts_ms, 0), ?5), 0), \
             elapsed_ms = CASE WHEN ended_at_ms IS NULL AND ?5 > 0 \
                 THEN MAX(elapsed_ms, ?5 - started_at_ms) ELSE elapsed_ms END, \
             size_bytes = ?6 \
             WHERE id = ?7",
            params![
                pending.samples as i64,
                pending.logs as i64,
                pending.events as i64,
                pending.marks as i64,
                pending.last_ts_ms.unwrap_or(0),
                size,
                session_id
            ],
        )?;
        Ok(())
    }

    fn commit_all(&mut self) {
        let ids: Vec<String> = self
            .open
            .iter()
            .filter(|(_, s)| {
                s.in_txn
                    || s.pending.samples + s.pending.logs + s.pending.events + s.pending.marks > 0
            })
            .map(|(id, _)| id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        let _ = self.index.execute_batch("BEGIN IMMEDIATE");
        for id in &ids {
            if let Err(e) = self.commit_one(id) {
                tracing::error!(session = %id, error = %e, "commit failed");
            }
        }
        if let Err(e) = self.index.execute_batch("COMMIT") {
            tracing::error!(error = %e, "index commit failed");
            let _ = self.index.execute_batch("ROLLBACK");
        }
    }

    fn end(&mut self, session_id: &str, ended_at_ms: i64, elapsed_ms: i64) -> StoreResult<()> {
        self.commit_one(session_id)?;
        let size = match self.open.remove(session_id) {
            Some(s) => {
                schema::meta_set(&s.conn, "ended_at_ms", &ended_at_ms.to_string())?;
                if let Err(e) = s.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)") {
                    tracing::debug!(session = session_id, error = %e, "checkpoint");
                }
                drop(s.conn);
                db_size(&s.path) as i64
            }
            None => self
                .index
                .query_row(
                    "SELECT size_bytes FROM sessions WHERE id = ?1",
                    [session_id],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0),
        };
        self.index.execute(
            "UPDATE sessions SET ended_at_ms = ?1,              elapsed_ms = CASE WHEN ?2 < 0 THEN elapsed_ms ELSE ?2 END,              size_bytes = ?3 WHERE id = ?4",
            params![ended_at_ms, elapsed_ms, size, session_id],
        )?;
        Ok(())
    }

    fn close_all(&mut self) {
        self.commit_all();
        let now = now_ms();
        for (id, s) in self.open.drain() {
            let _ = s.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            drop(s.conn);
            let size = db_size(&s.path) as i64;
            // Left live on purpose: `Store::open` recovers it if the app
            // did not end it (a crash); an orderly shutdown ends it first.
            if let Err(e) = self.index.execute(
                "UPDATE sessions SET size_bytes = ?1, last_ts_ms = COALESCE(last_ts_ms, ?2) WHERE id = ?3",
                params![size, now, id],
            ) {
                tracing::debug!(session = %id, error = %e, "close");
            }
        }
        let _ = self.index.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
    }

    fn maybe_sweep(&mut self) {
        let due = self.last_sweep.is_none_or(|t| t.elapsed() >= SWEEP_EVERY);
        if due {
            self.sweep();
        }
    }

    /// Delete ended sessions older than `keep_days`, then the oldest ended
    /// sessions past `max_total_bytes`; checkpoint everything.
    fn sweep(&mut self) {
        self.last_sweep = Some(Instant::now());
        if let Err(e) = self.sweep_inner() {
            tracing::warn!(error = %e, "retention sweep failed");
        }
        let _ = self.index.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
        for s in self.open.values() {
            let _ = s.conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE)");
        }
    }

    fn sweep_inner(&mut self) -> StoreResult<()> {
        struct Row {
            id: String,
            path: PathBuf,
            ended_at_ms: Option<i64>,
            size_bytes: i64,
        }
        let rows: Vec<Row> = {
            let mut stmt = self.index.prepare(
                "SELECT id, path, ended_at_ms, size_bytes FROM sessions ORDER BY started_at_ms ASC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(Row {
                    id: r.get(0)?,
                    path: PathBuf::from(r.get::<_, String>(1)?),
                    ended_at_ms: r.get(2)?,
                    size_bytes: r.get(3)?,
                })
            })?;
            rows.collect::<Result<_, _>>()?
        };
        let now = now_ms();
        let cutoff = if self.config.keep_days > 0 {
            Some(now - i64::from(self.config.keep_days) * 86_400_000)
        } else {
            None
        };
        let mut total: u64 = rows.iter().map(|r| r.size_bytes.max(0) as u64).sum();
        let mut doomed: Vec<&Row> = Vec::new();
        for row in &rows {
            let Some(ended) = row.ended_at_ms else {
                continue;
            };
            if self.open.contains_key(&row.id) {
                continue;
            }
            if cutoff.is_some_and(|c| ended < c) {
                doomed.push(row);
                total = total.saturating_sub(row.size_bytes.max(0) as u64);
            }
        }
        if let Some(cap) = self.config.max_total_bytes {
            for row in &rows {
                if total <= cap {
                    break;
                }
                if row.ended_at_ms.is_none()
                    || self.open.contains_key(&row.id)
                    || doomed.iter().any(|d| d.id == row.id)
                {
                    continue;
                }
                doomed.push(row);
                total = total.saturating_sub(row.size_bytes.max(0) as u64);
            }
        }
        for row in doomed {
            tracing::info!(session = %row.id, "retention: deleting session");
            delete_files(&row.path)?;
            self.index
                .execute("DELETE FROM sessions WHERE id = ?1", [&row.id])?;
        }
        Ok(())
    }
}
