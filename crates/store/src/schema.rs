//! Schemas and migrations (`PRAGMA user_version`) for the index database
//! and the per-session databases.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::error::StoreResult;

/// Schema version of `index.sqlite`.
pub const INDEX_VERSION: i32 = 1;
/// Schema version of `sessions/<id>.sqlite`.
pub const SESSION_VERSION: i32 = 1;

const INDEX_V1: &str = "
CREATE TABLE IF NOT EXISTS sessions (
    id               TEXT PRIMARY KEY,
    device_id        TEXT NOT NULL,
    device_name      TEXT NOT NULL,
    model_name       TEXT NOT NULL,
    platform         TEXT NOT NULL,
    started_at_ms    INTEGER NOT NULL,
    ended_at_ms      INTEGER,
    elapsed_ms       INTEGER NOT NULL DEFAULT 0,
    last_ts_ms       INTEGER,
    sample_count     INTEGER NOT NULL DEFAULT 0,
    log_count        INTEGER NOT NULL DEFAULT 0,
    event_count      INTEGER NOT NULL DEFAULT 0,
    mark_count       INTEGER NOT NULL DEFAULT 0,
    size_bytes       INTEGER NOT NULL DEFAULT 0,
    device_info_json TEXT,
    path             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_device ON sessions(device_id, started_at_ms);
CREATE INDEX IF NOT EXISTS sessions_started ON sessions(started_at_ms);
";

const SESSION_V1: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS samples (
    ts_ms  INTEGER NOT NULL,
    metric TEXT NOT NULL,
    value  REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS samples_metric_ts ON samples(metric, ts_ms);
CREATE TABLE IF NOT EXISTS cores (
    ts_ms    INTEGER NOT NULL,
    idx      INTEGER NOT NULL,
    kind     TEXT NOT NULL,
    usage    REAL,
    freq_mhz INTEGER,
    online   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS cores_ts ON cores(ts_ms);
CREATE TABLE IF NOT EXISTS processes (
    ts_ms   INTEGER NOT NULL,
    pid     INTEGER NOT NULL,
    name    TEXT NOT NULL,
    app_id  TEXT,
    cpu     REAL,
    mem     INTEGER,
    threads INTEGER,
    net     REAL,
    state   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS processes_ts ON processes(ts_ms);
CREATE INDEX IF NOT EXISTS processes_app ON processes(app_id);
CREATE TABLE IF NOT EXISTS logs (
    seq       INTEGER PRIMARY KEY,
    ts_ms     INTEGER NOT NULL,
    pid       INTEGER,
    process   TEXT NOT NULL,
    tag       TEXT,
    subsystem TEXT,
    category  TEXT,
    buffer    TEXT,
    level     TEXT NOT NULL,
    message   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS logs_ts ON logs(ts_ms);
CREATE TABLE IF NOT EXISTS events (
    ts_ms   INTEGER NOT NULL,
    kind    TEXT NOT NULL,
    detail  TEXT NOT NULL,
    pid     INTEGER,
    process TEXT
);
CREATE TABLE IF NOT EXISTS marks (ts_ms INTEGER NOT NULL, label TEXT NOT NULL);
";

/// External-content FTS5 index on `logs.message` with the trigram
/// tokenizer (case-insensitive substring search for queries of three or
/// more characters). Created once; `meta.fts` records whether it exists.
const SESSION_FTS: &str = "
CREATE VIRTUAL TABLE logs_fts USING fts5(
    message, content='logs', content_rowid='seq', tokenize='trigram'
);
CREATE TRIGGER logs_ai AFTER INSERT ON logs BEGIN
    INSERT INTO logs_fts(rowid, message) VALUES (new.seq, new.message);
END;
CREATE TRIGGER logs_ad AFTER DELETE ON logs BEGIN
    INSERT INTO logs_fts(logs_fts, rowid, message) VALUES ('delete', old.seq, old.message);
END;
";

fn pragmas(conn: &Connection) -> StoreResult<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}

fn user_version(conn: &Connection) -> StoreResult<i32> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

/// Apply every step past the current `user_version`.
fn migrate(conn: &Connection, steps: &[&str]) -> StoreResult<()> {
    let current = user_version(conn)?;
    for (i, sql) in steps.iter().enumerate() {
        let version = i as i32 + 1;
        if version <= current {
            continue;
        }
        conn.execute_batch("BEGIN")?;
        let applied = conn
            .execute_batch(sql)
            .and_then(|()| conn.pragma_update(None, "user_version", version));
        match applied {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e.into());
            }
        }
    }
    Ok(())
}

/// Open (creating and migrating) the index database.
pub fn open_index(path: &Path) -> StoreResult<Connection> {
    let conn = Connection::open(path)?;
    pragmas(&conn)?;
    migrate(&conn, &[INDEX_V1])?;
    Ok(conn)
}

/// Open (creating and migrating) one session database. Returns the
/// connection and whether the FTS5 index is available in it.
pub fn open_session(path: &Path, create: bool) -> StoreResult<(Connection, bool)> {
    let flags = if create {
        OpenFlags::default()
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX
    };
    let conn = Connection::open_with_flags(path, flags)?;
    pragmas(&conn)?;
    if create {
        migrate(&conn, &[SESSION_V1])?;
    }
    let fts = ensure_fts(&conn, create)?;
    Ok((conn, fts))
}

/// Read `meta.fts`; when absent and `create`, try to build the FTS index
/// and record the outcome.
fn ensure_fts(conn: &Connection, create: bool) -> StoreResult<bool> {
    let stored = meta_get(conn, "fts")?;
    match stored.as_deref() {
        Some("1") => return Ok(true),
        Some(_) => return Ok(false),
        None if !create => return Ok(false),
        None => {}
    }
    let ok = match conn.execute_batch(SESSION_FTS) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, "FTS5 (trigram) unavailable; log search uses LIKE");
            false
        }
    };
    meta_set(conn, "fts", if ok { "1" } else { "0" })?;
    Ok(ok)
}

/// One `meta` value.
pub fn meta_get(conn: &Connection, key: &str) -> StoreResult<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

/// Set one `meta` value.
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> StoreResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta(key, value) VALUES (?1, ?2)",
        [key, value],
    )?;
    Ok(())
}
