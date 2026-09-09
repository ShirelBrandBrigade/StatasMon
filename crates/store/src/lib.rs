//! StatasMon `store` crate: the SQLite session store (docs/PLAN.md §3.9,
//! step 5.1–5.2, docs/DECISIONS.md ADR-012).
//!
//! Layout under the app-data root: `index.sqlite` (one row per session,
//! mirroring [`SessionSummary`] plus the device information document and
//! the file path) and `sessions/<id>.sqlite` (samples, cores, processes,
//! logs with an FTS5 trigram index, events, marks, meta). Both are WAL with
//! `synchronous=NORMAL`; schemas are versioned with `user_version`.
//!
//! Writes go through one [`Writer`](writer) thread over a bounded
//! `std::sync::mpsc` queue: everything that arrives within one second is
//! written in one transaction per session. The engine calls
//! [`Store::write`], which never blocks — a full queue drops the message
//! and counts it ([`Store::dropped`]). Session lifecycle
//! ([`Store::begin`], [`Store::end`], [`Store::mark`]) and the barriers
//! ([`Store::flush`], [`Store::sweep`]) are rare and wait for queue room.
//!
//! Reads open their own connection per query ([`SessionReader`]):
//! history is downsampled in SQL (`GROUP BY` bucket average, never more
//! than 5 000 points), logs are filtered by process, level, time range,
//! buffer and query (substring through FTS5 trigram when available, LIKE
//! otherwise; regex through a registered `regexp` function).
//!
//! Retention: a daily sweep on the writer thread deletes ended sessions
//! older than `keep_days`, then the oldest ended sessions past
//! `max_total_bytes`, and checkpoints (`TRUNCATE`) the WALs. The engine
//! splits a session that exceeds `max_session_ms` into a new one.
//!
//! [`SessionSummary`]: statasmon_protocol::SessionSummary

pub mod codec;
pub mod config;
pub mod error;
pub mod ids;
pub mod reader;
pub mod replay;
pub mod schema;
pub mod store;
mod writer;

pub use config::{DEFAULT_KEEP_DAYS, DEFAULT_MAX_SESSION_MS, DEFAULT_WRITER_CAPACITY, StoreConfig};
pub use error::{StoreError, StoreResult};
pub use ids::{sanitize, session_id_for};
pub use reader::{MAX_HISTORY_POINTS, SessionCounts, SessionReader, Table};
pub use replay::{parse_jsonl, replay, replay_jsonl};
pub use store::{SessionStats, Store, StoreStats};
pub use writer::now_ms;
