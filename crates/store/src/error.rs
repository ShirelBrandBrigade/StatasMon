//! Store errors.

/// What can go wrong in the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite failed.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// File system failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A stored JSON document could not be (de)serialized.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// No session with that id in the index.
    #[error("unknown session {0}")]
    UnknownSession(String),
    /// The session is still recording; stop it first.
    #[error("session {0} is still recording")]
    Live(String),
    /// A log filter with an invalid regular expression.
    #[error("invalid log filter: {0}")]
    InvalidFilter(String),
    /// The writer thread has stopped.
    #[error("store writer has stopped")]
    Closed,
    /// The writer did not drain within the timeout.
    #[error("store writer did not flush in time")]
    Timeout,
}

/// Result alias.
pub type StoreResult<T> = Result<T, StoreError>;
