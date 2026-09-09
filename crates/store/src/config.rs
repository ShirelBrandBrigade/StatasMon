//! Store configuration: where the files live and the retention policy
//! (docs/PLAN.md §3.9, docs/DECISIONS.md ADR-012).

use std::path::{Path, PathBuf};

/// A session is closed and a new one started past this recording time.
pub const DEFAULT_MAX_SESSION_MS: i64 = 4 * 60 * 60 * 1000;
/// Sessions that ended more than this many days ago are deleted.
pub const DEFAULT_KEEP_DAYS: u32 = 30;
/// Writer queue depth (messages); the engine drops when it is full.
pub const DEFAULT_WRITER_CAPACITY: usize = 4096;

/// Everything the store needs at open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreConfig {
    /// Directory holding `index.sqlite` and `sessions/<id>.sqlite`.
    pub root: PathBuf,
    /// Recording-time cap per session; 0 disables the split.
    pub max_session_ms: i64,
    /// Days to keep ended sessions; 0 keeps them forever.
    pub keep_days: u32,
    /// Optional cap on the total size of all session files; the oldest
    /// ended sessions are deleted first.
    pub max_total_bytes: Option<u64>,
    /// Writer queue depth.
    pub writer_capacity: usize,
}

impl StoreConfig {
    /// Defaults under `root`.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            max_session_ms: DEFAULT_MAX_SESSION_MS,
            keep_days: DEFAULT_KEEP_DAYS,
            max_total_bytes: None,
            writer_capacity: DEFAULT_WRITER_CAPACITY,
        }
    }

    /// Replace the per-session recording cap.
    #[must_use]
    pub fn with_max_session_ms(mut self, ms: i64) -> Self {
        self.max_session_ms = ms.max(0);
        self
    }

    /// Replace the days-to-keep.
    #[must_use]
    pub fn with_keep_days(mut self, days: u32) -> Self {
        self.keep_days = days;
        self
    }

    /// Replace the total-size cap.
    #[must_use]
    pub fn with_max_total_bytes(mut self, bytes: Option<u64>) -> Self {
        self.max_total_bytes = bytes;
        self
    }

    /// Replace the writer queue depth (at least 8).
    #[must_use]
    pub fn with_writer_capacity(mut self, capacity: usize) -> Self {
        self.writer_capacity = capacity.max(8);
        self
    }

    /// Path of the index database.
    pub fn index_path(&self) -> PathBuf {
        self.root.join("index.sqlite")
    }

    /// Directory of the per-session databases.
    pub fn sessions_dir(&self) -> PathBuf {
        self.root.join("sessions")
    }

    /// Path of one session database.
    pub fn session_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(format!("{session_id}.sqlite"))
    }
}
