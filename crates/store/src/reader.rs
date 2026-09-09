//! Read side of one session database: history with SQL downsampling,
//! per-core ticks, filtered and paged logs (substring via FTS5 trigram or
//! LIKE, regex via a registered `regexp` function), events, marks and the
//! row iterators the export writers use.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use regex::{Regex, RegexBuilder};
use rusqlite::functions::{Context, FunctionFlags};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, params, params_from_iter};
use statasmon_protocol::{
    CoreSample, EventKind, HistoryPoint, LogEntry, LogFilter, LogPage, Mark, MetricId, ProcessRow,
    StoredEvent,
};

use crate::codec;
use crate::error::{StoreError, StoreResult};
use crate::schema;

/// History queries never return more points than this; the step is
/// raised automatically when the range would.
pub const MAX_HISTORY_POINTS: i64 = 5_000;

/// Row counts of one session database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionCounts {
    /// Sample ticks (distinct timestamps in `samples`).
    pub samples: u64,
    /// Log entries.
    pub logs: u64,
    /// Events.
    pub events: u64,
    /// Marks.
    pub marks: u64,
}

/// Tables of a session database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Samples,
    Cores,
    Processes,
    Logs,
    Events,
    Marks,
}

impl Table {
    fn name(self) -> &'static str {
        match self {
            Table::Samples => "samples",
            Table::Cores => "cores",
            Table::Processes => "processes",
            Table::Logs => "logs",
            Table::Events => "events",
            Table::Marks => "marks",
        }
    }
}

/// A read connection to one session database.
pub struct SessionReader {
    conn: Connection,
    fts: bool,
    session_id: String,
}

impl std::fmt::Debug for SessionReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionReader")
            .field("session_id", &self.session_id)
            .field("fts", &self.fts)
            .finish_non_exhaustive()
    }
}

fn register_regexp(conn: &Connection) -> StoreResult<()> {
    conn.create_scalar_function(
        "regexp",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx: &Context<'_>| -> rusqlite::Result<bool> {
            let regex: Arc<Regex> = ctx.get_or_create_aux(
                0,
                |vr| -> Result<Regex, Box<dyn std::error::Error + Send + Sync + 'static>> {
                    Ok(RegexBuilder::new(vr.as_str()?)
                        .case_insensitive(true)
                        .size_limit(1 << 20)
                        .build()?)
                },
            )?;
            let matched = match ctx.get_raw(1) {
                ValueRef::Text(t) => std::str::from_utf8(t).is_ok_and(|s| regex.is_match(s)),
                _ => false,
            };
            Ok(matched)
        },
    )?;
    Ok(())
}

fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn ceil_div(a: i64, b: i64) -> i64 {
    if b <= 0 {
        return a;
    }
    (a + b - 1) / b
}

fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(", ")
}

struct Where {
    sql: String,
    params: Vec<Value>,
}

fn build_where(filter: &LogFilter, fts: bool) -> StoreResult<Where> {
    let mut clauses: Vec<String> = vec!["1 = 1".to_owned()];
    let mut params: Vec<Value> = Vec::new();
    if !filter.processes.is_empty() {
        clauses.push(format!(
            "lower(process) IN ({})",
            placeholders(filter.processes.len())
        ));
        params.extend(
            filter
                .processes
                .iter()
                .map(|p| Value::Text(p.to_lowercase())),
        );
    }
    if !filter.levels.is_empty() {
        clauses.push(format!("level IN ({})", placeholders(filter.levels.len())));
        params.extend(
            filter
                .levels
                .iter()
                .map(|l| Value::Text(codec::level_name(*l))),
        );
    }
    if let Some(from) = filter.from_ms {
        clauses.push("ts_ms >= ?".to_owned());
        params.push(Value::Integer(from));
    }
    if let Some(to) = filter.to_ms {
        clauses.push("ts_ms <= ?".to_owned());
        params.push(Value::Integer(to));
    }
    if !filter.buffers.is_empty() {
        clauses.push(format!(
            "lower(buffer) IN ({})",
            placeholders(filter.buffers.len())
        ));
        params.extend(filter.buffers.iter().map(|b| Value::Text(b.to_lowercase())));
    }
    match filter.query.as_deref().map(str::trim) {
        None | Some("") => {}
        Some(q) if filter.regex => {
            RegexBuilder::new(q)
                .case_insensitive(true)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| StoreError::InvalidFilter(e.to_string()))?;
            clauses
                .push("(regexp(?, message) OR regexp(?, tag) OR regexp(?, subsystem))".to_owned());
            for _ in 0..3 {
                params.push(Value::Text(q.to_owned()));
            }
        }
        Some(q) => {
            let like = format!("%{}%", escape_like(q));
            if fts && q.chars().count() >= 3 {
                clauses.push(
                    "(seq IN (SELECT rowid FROM logs_fts WHERE logs_fts MATCH ?) \
                     OR tag LIKE ? ESCAPE '\\' OR subsystem LIKE ? ESCAPE '\\')"
                        .to_owned(),
                );
                params.push(Value::Text(format!("\"{}\"", q.replace('"', "\"\""))));
            } else {
                clauses.push(
                    "(message LIKE ? ESCAPE '\\' OR tag LIKE ? ESCAPE '\\' \
                     OR subsystem LIKE ? ESCAPE '\\')"
                        .to_owned(),
                );
                params.push(Value::Text(like.clone()));
            }
            params.push(Value::Text(like.clone()));
            params.push(Value::Text(like));
        }
    }
    Ok(Where {
        sql: clauses.join(" AND "),
        params,
    })
}

const LOG_COLUMNS: &str =
    "seq, ts_ms, pid, process, tag, subsystem, category, buffer, level, message";

fn log_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<LogEntry> {
    Ok(LogEntry {
        seq: r.get::<_, i64>(0)?.max(0) as u64,
        ts_ms: r.get(1)?,
        pid: r.get(2)?,
        process: r.get(3)?,
        tag: r.get(4)?,
        subsystem: r.get(5)?,
        category: r.get(6)?,
        buffer: r.get(7)?,
        level: codec::parse_level(&r.get::<_, String>(8)?),
        message: r.get(9)?,
    })
}

impl SessionReader {
    pub(crate) fn open(path: &Path, session_id: &str) -> StoreResult<Self> {
        if !path.is_file() {
            return Err(StoreError::UnknownSession(session_id.to_owned()));
        }
        let (conn, fts) = schema::open_session(path, false)?;
        register_regexp(&conn)?;
        Ok(Self {
            conn,
            fts,
            session_id: session_id.to_owned(),
        })
    }

    /// The session this reader is on.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// True when substring search uses the FTS5 trigram index.
    pub fn fts_enabled(&self) -> bool {
        self.fts
    }

    /// Every `meta` key.
    pub fn meta(&self) -> StoreResult<BTreeMap<String, String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM meta ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Row counts.
    pub fn counts(&self) -> StoreResult<SessionCounts> {
        let one = |sql: &str| -> StoreResult<u64> {
            Ok(self.conn.query_row(sql, [], |r| r.get::<_, i64>(0))? as u64)
        };
        Ok(SessionCounts {
            samples: one("SELECT COUNT(DISTINCT ts_ms) FROM samples")?,
            logs: one("SELECT COUNT(*) FROM logs")?,
            events: one("SELECT COUNT(*) FROM events")?,
            marks: one("SELECT COUNT(*) FROM marks")?,
        })
    }

    /// Rows of one table with `ts_ms` in `[from_ms, to_ms]` (export progress).
    pub fn row_count(&self, table: Table, from_ms: i64, to_ms: i64) -> StoreResult<u64> {
        let n: i64 = self.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE ts_ms BETWEEN ?1 AND ?2",
                table.name()
            ),
            params![from_ms, to_ms],
            |r| r.get(0),
        )?;
        Ok(n.max(0) as u64)
    }

    /// Metrics with at least one sample, in catalog order.
    pub fn metrics(&self) -> StoreResult<Vec<MetricId>> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT metric FROM samples")?;
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut ids: Vec<MetricId> = names
            .iter()
            .filter_map(|n| codec::parse_metric(n))
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Points of `metric` in `[from_ms, to_ms]`, averaged per `step_ms`
    /// bucket (stamped with the bucket start) when `step_ms > 0`. Never
    /// more than [`MAX_HISTORY_POINTS`]: the step is raised when needed.
    pub fn history(
        &self,
        metric: MetricId,
        from_ms: i64,
        to_ms: i64,
        step_ms: u32,
    ) -> StoreResult<Vec<HistoryPoint>> {
        if to_ms < from_ms {
            return Ok(Vec::new());
        }
        let name = codec::metric_name(metric);
        let span = to_ms - from_ms + 1;
        let mut step = i64::from(step_ms);
        if step == 0 {
            let raw: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM samples WHERE metric = ?1 AND ts_ms BETWEEN ?2 AND ?3",
                params![name, from_ms, to_ms],
                |r| r.get(0),
            )?;
            if raw > MAX_HISTORY_POINTS {
                step = ceil_div(span, MAX_HISTORY_POINTS).max(1);
            }
        } else if ceil_div(span, step) > MAX_HISTORY_POINTS {
            step = ceil_div(span, MAX_HISTORY_POINTS).max(1);
        }
        let mut out = Vec::new();
        if step == 0 {
            let mut stmt = self.conn.prepare_cached(
                "SELECT ts_ms, value FROM samples WHERE metric = ?1 AND ts_ms BETWEEN ?2 AND ?3 \
                 ORDER BY ts_ms",
            )?;
            let rows = stmt.query_map(params![name, from_ms, to_ms], |r| {
                Ok(HistoryPoint {
                    ts_ms: r.get(0)?,
                    value: r.get(1)?,
                })
            })?;
            for row in rows {
                out.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare_cached(
                "SELECT ?1 + ((ts_ms - ?1) / ?2) * ?2 AS bucket, AVG(value) FROM samples \
                 WHERE metric = ?3 AND ts_ms BETWEEN ?1 AND ?4 \
                 GROUP BY bucket ORDER BY bucket",
            )?;
            let rows = stmt.query_map(params![from_ms, step, name, to_ms], |r| {
                Ok(HistoryPoint {
                    ts_ms: r.get(0)?,
                    value: r.get(1)?,
                })
            })?;
            for row in rows {
                out.push(row?);
            }
        }
        Ok(out)
    }

    /// Every sample row in `[from_ms, to_ms]`, oldest first; returns the count.
    pub fn for_each_sample(
        &self,
        from_ms: i64,
        to_ms: i64,
        mut f: impl FnMut(i64, MetricId, f64) -> StoreResult<()>,
    ) -> StoreResult<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts_ms, metric, value FROM samples WHERE ts_ms BETWEEN ?1 AND ?2 \
             ORDER BY ts_ms, metric",
        )?;
        let mut rows = stmt.query(params![from_ms, to_ms])?;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            let name: String = r.get(1)?;
            if let Some(metric) = codec::parse_metric(&name) {
                f(r.get(0)?, metric, r.get(2)?)?;
                n += 1;
            }
        }
        Ok(n)
    }

    /// Per-core ticks in `[from_ms, to_ms]`.
    pub fn cores(&self, from_ms: i64, to_ms: i64) -> StoreResult<Vec<(i64, Vec<CoreSample>)>> {
        let mut out: Vec<(i64, Vec<CoreSample>)> = Vec::new();
        self.for_each_core(from_ms, to_ms, |ts, core| {
            match out.last_mut() {
                Some((t, cores)) if *t == ts => cores.push(core),
                _ => out.push((ts, vec![core])),
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Every core row in `[from_ms, to_ms]`, ordered by time then index.
    pub fn for_each_core(
        &self,
        from_ms: i64,
        to_ms: i64,
        mut f: impl FnMut(i64, CoreSample) -> StoreResult<()>,
    ) -> StoreResult<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts_ms, idx, kind, usage, freq_mhz, online FROM cores \
             WHERE ts_ms BETWEEN ?1 AND ?2 ORDER BY ts_ms, idx",
        )?;
        let mut rows = stmt.query(params![from_ms, to_ms])?;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            let core = CoreSample {
                index: r.get(1)?,
                kind: codec::parse_core_kind(&r.get::<_, String>(2)?),
                usage_pct: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0) as f32,
                freq_mhz: r.get(4)?,
                online: r.get(5)?,
            };
            f(r.get(0)?, core)?;
            n += 1;
        }
        Ok(n)
    }

    /// Every process row in `[from_ms, to_ms]`, ordered by time then pid.
    /// `app_id` restricts to rows of that app (by `app_id` or by name).
    pub fn for_each_process(
        &self,
        from_ms: i64,
        to_ms: i64,
        app_id: Option<&str>,
        mut f: impl FnMut(i64, ProcessRow) -> StoreResult<()>,
    ) -> StoreResult<u64> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT ts_ms, pid, name, app_id, cpu, mem, threads, net, state FROM processes \
             WHERE ts_ms BETWEEN ?1 AND ?2 AND (?3 IS NULL OR app_id = ?3 OR name = ?3) \
             ORDER BY ts_ms, pid",
        )?;
        let mut rows = stmt.query(params![from_ms, to_ms, app_id])?;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            let row = ProcessRow {
                pid: r.get(1)?,
                name: r.get(2)?,
                app_id: r.get(3)?,
                cpu_pct: r.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                mem_bytes: r.get::<_, Option<i64>>(5)?.map(|m| m.max(0) as u64),
                threads: r.get(6)?,
                net_bps: r.get(7)?,
                state: codec::parse_process_state(&r.get::<_, String>(8)?),
                energy: None,
            };
            f(r.get(0)?, row)?;
            n += 1;
        }
        Ok(n)
    }

    /// Process names seen for `app_id` (plus the id itself).
    pub fn processes_for_app(&self, app_id: &str) -> StoreResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT DISTINCT name FROM processes WHERE app_id = ?1 OR name = ?1")?;
        let mut names: Vec<String> = stmt
            .query_map([app_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        if !names.iter().any(|n| n == app_id) {
            names.push(app_id.to_owned());
        }
        Ok(names)
    }

    /// Entries matching `filter` (its `device_id`/`session_id` are ignored).
    pub fn count_logs(&self, filter: &LogFilter) -> StoreResult<u64> {
        let w = build_where(filter, self.fts)?;
        let n: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM logs WHERE {}", w.sql),
            params_from_iter(w.params.iter()),
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Page `limit` matching entries starting at match index `offset`,
    /// oldest first; `total` counts every match.
    pub fn logs(&self, filter: &LogFilter, offset: usize, limit: usize) -> StoreResult<LogPage> {
        let total = self.count_logs(filter)?;
        let w = build_where(filter, self.fts)?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {LOG_COLUMNS} FROM logs WHERE {} ORDER BY seq LIMIT ? OFFSET ?",
            w.sql
        ))?;
        let mut params = w.params;
        params.push(Value::Integer(limit as i64));
        params.push(Value::Integer(offset as i64));
        let entries: Vec<LogEntry> = stmt
            .query_map(params_from_iter(params.iter()), log_from_row)?
            .collect::<Result<_, _>>()?;
        Ok(LogPage {
            total,
            first_seq: entries.first().map_or(0, |e| e.seq),
            entries,
        })
    }

    /// Stream every matching entry, oldest first; returns the count.
    pub fn for_each_log(
        &self,
        filter: &LogFilter,
        mut f: impl FnMut(LogEntry) -> StoreResult<()>,
    ) -> StoreResult<u64> {
        let w = build_where(filter, self.fts)?;
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {LOG_COLUMNS} FROM logs WHERE {} ORDER BY seq",
            w.sql
        ))?;
        let mut rows = stmt.query(params_from_iter(w.params.iter()))?;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            f(log_from_row(r)?)?;
            n += 1;
        }
        Ok(n)
    }

    /// The entries with these sequence numbers, ordered by `seq`.
    pub fn logs_by_seq(&self, seqs: &[u64]) -> StoreResult<Vec<LogEntry>> {
        let mut out = Vec::with_capacity(seqs.len());
        for chunk in seqs.chunks(500) {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {LOG_COLUMNS} FROM logs WHERE seq IN ({}) ORDER BY seq",
                placeholders(chunk.len())
            ))?;
            let params: Vec<Value> = chunk.iter().map(|s| Value::Integer(*s as i64)).collect();
            let rows = stmt.query_map(params_from_iter(params.iter()), log_from_row)?;
            for row in rows {
                out.push(row?);
            }
        }
        out.sort_by_key(|e| e.seq);
        Ok(out)
    }

    /// Every stored event, oldest first.
    pub fn events(&self) -> StoreResult<Vec<StoredEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts_ms, kind, detail, pid, process FROM events ORDER BY ts_ms, rowid",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredEvent {
                ts_ms: r.get(0)?,
                kind: codec::parse_event_kind(&r.get::<_, String>(1)?),
                detail: r.get(2)?,
                pid: r.get(3)?,
                process: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Events of one kind.
    pub fn events_of(&self, kind: EventKind) -> StoreResult<Vec<StoredEvent>> {
        Ok(self
            .events()?
            .into_iter()
            .filter(|e| e.kind == kind)
            .collect())
    }

    /// Every mark, oldest first.
    pub fn marks(&self) -> StoreResult<Vec<Mark>> {
        let mut stmt = self
            .conn
            .prepare("SELECT ts_ms, label FROM marks ORDER BY ts_ms, rowid")?;
        let rows = stmt.query_map([], |r| {
            Ok(Mark {
                ts_ms: r.get(0)?,
                label: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
