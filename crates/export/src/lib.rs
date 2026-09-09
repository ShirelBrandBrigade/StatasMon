//! StatasMon `export` crate (docs/PLAN.md §3.10, step 5.4, ADR-012):
//! `.log`, `.jsonl`, `.csv` and the session `.zip` bundle, written from a
//! stored session with the scopes of [`ExportScope`].
//!
//! Every export writes `<path>.part` first and renames it into place when
//! complete, reports progress through a callback (about every 2 %), and
//! returns the path, size and entry count as an [`ExportResult`].
//!
//! - `.log`: one line per entry —
//!   `2026-09-09 14:32:07.412  FieldKit[4412]  ERROR  com.example.fieldkit  message`
//!   (local time; the origin column is the iOS subsystem or the Android tag).
//! - `.jsonl`: a header line `{"statasmon":1,"session":…}` then one
//!   [`LogEntry`] JSON object per line.
//! - `.csv`: metric samples — **wide** (`ts_ms,cpu.total,mem.used,…`) for
//!   the `Session` scope with at most 64 metrics, **long**
//!   (`ts_ms,metric,value`) otherwise; the `App` scope exports the app's
//!   process rows instead (the only per-app metrics stored).
//! - `.zip`: `device.json`, `session.json`, `metrics.csv` (wide),
//!   `cores.csv`, `processes.csv`, `logs.jsonl`, `events.json`,
//!   `marks.json`, `crashes/*` copied from `<app-data>/crashes/<device>/`
//!   and `capture.pcap` from `<app-data>/pcap/<session>.pcap` when present.
//!
//! [`LogEntry`]: statasmon_protocol::LogEntry

mod bundle;
mod progress;
mod writers;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use statasmon_protocol::{
    ErrorCode, ExportFormat, ExportRequest, ExportResult, ExportScope, LogFilter,
};
use statasmon_store::{SessionReader, Store, StoreError, Table};

pub use progress::Progress;

/// Metrics beyond which the `.csv` falls back to the long format (the
/// whole catalog fits, so a session export is wide unless a future
/// catalog outgrows a spreadsheet).
pub const WIDE_MAX_METRICS: usize = 64;

/// What can go wrong.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// The request names no session and the device has none.
    #[error("no session to export")]
    NoSession,
    /// The request has no destination path.
    #[error("no export path given")]
    NoPath,
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Writing the file failed.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON encoding failed.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// CSV encoding failed.
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    /// Zip encoding failed.
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
}

impl ExportError {
    /// The protocol error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            ExportError::NoSession | ExportError::NoPath => ErrorCode::Unsupported,
            ExportError::Store(StoreError::InvalidFilter(_)) => ErrorCode::Unsupported,
            ExportError::Store(StoreError::UnknownSession(_)) => ErrorCode::Unsupported,
            _ => ErrorCode::Io,
        }
    }

    /// What the user can do about it.
    pub fn hint(&self) -> Option<String> {
        Some(
            match self {
                ExportError::NoSession => "Start a recording or pick a stored session first.",
                ExportError::NoPath => "Choose where to save the export.",
                ExportError::Store(StoreError::InvalidFilter(_)) => {
                    "Fix the regular expression or turn regex mode off."
                }
                ExportError::Store(StoreError::UnknownSession(_)) => {
                    "The session was deleted. Refresh the Sessions list."
                }
                _ => "Check free disk space and that the destination folder is writable.",
            }
            .to_owned(),
        )
    }
}

/// Result alias.
pub type ExportOutcome = Result<ExportResult, ExportError>;

/// Where the log entries of an export come from.
#[derive(Debug, Clone)]
pub(crate) enum LogSource {
    Filter(LogFilter),
    Seqs(Vec<u64>),
}

/// Everything a writer needs about the request.
pub(crate) struct Job<'a> {
    pub store: &'a Store,
    pub reader: &'a SessionReader,
    pub request: &'a ExportRequest,
    pub session_id: &'a str,
    /// Time range of metric-like tables.
    pub from_ms: i64,
    pub to_ms: i64,
    pub logs: LogSource,
    pub app_data_dir: Option<&'a Path>,
}

impl Job<'_> {
    pub(crate) fn log_total(&self) -> Result<u64, ExportError> {
        Ok(match &self.logs {
            LogSource::Filter(f) => self.reader.count_logs(f)?,
            LogSource::Seqs(s) => s.len() as u64,
        })
    }

    pub(crate) fn rows(&self, table: Table) -> Result<u64, ExportError> {
        Ok(self.reader.row_count(table, self.from_ms, self.to_ms)?)
    }
}

/// Resolve the scope into a time range and a log source.
fn resolve_scope(
    reader: &SessionReader,
    scope: &ExportScope,
) -> Result<(i64, i64, LogSource), ExportError> {
    let whole = (i64::MIN, i64::MAX);
    Ok(match scope {
        ExportScope::Session => (whole.0, whole.1, LogSource::Filter(LogFilter::default())),
        ExportScope::Range { from_ms, to_ms } => (
            *from_ms,
            *to_ms,
            LogSource::Filter(LogFilter {
                from_ms: Some(*from_ms),
                to_ms: Some(*to_ms),
                ..LogFilter::default()
            }),
        ),
        ExportScope::Filter { filter } => {
            let mut f = filter.clone();
            f.device_id = String::new();
            f.session_id = None;
            (
                f.from_ms.unwrap_or(whole.0),
                f.to_ms.unwrap_or(whole.1),
                LogSource::Filter(f),
            )
        }
        ExportScope::App { app_id } => (
            whole.0,
            whole.1,
            LogSource::Filter(LogFilter {
                processes: reader.processes_for_app(app_id)?,
                ..LogFilter::default()
            }),
        ),
        ExportScope::Selection { seqs } => {
            let mut seqs = seqs.clone();
            seqs.sort_unstable();
            seqs.dedup();
            (whole.0, whole.1, LogSource::Seqs(seqs))
        }
    })
}

fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

/// Export `request` from `store`. `app_data_dir` locates crash reports and
/// packet captures for the bundle. `progress(pct, message)` is called at
/// the start, about every 2 %, and at 100.
pub fn export(
    store: &Store,
    app_data_dir: Option<&Path>,
    request: &ExportRequest,
    mut progress: impl FnMut(f32, &str),
) -> ExportOutcome {
    if request.path.trim().is_empty() {
        return Err(ExportError::NoPath);
    }
    let session_id = request
        .session_id
        .as_deref()
        .ok_or(ExportError::NoSession)?;
    let path = PathBuf::from(&request.path);
    let reader = store.reader(session_id)?;
    let (from_ms, to_ms, logs) = resolve_scope(&reader, &request.scope)?;
    let job = Job {
        store,
        reader: &reader,
        request,
        session_id,
        from_ms,
        to_ms,
        logs,
        app_data_dir,
    };
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = part_path(&path);
    let mut cb = |pct: f32, msg: &str| progress(pct, msg);
    let mut tracker = Progress::new(&mut cb);
    tracker.report(0.0, "Starting export");
    let written = (|| -> Result<u64, ExportError> {
        let file = File::create(&tmp)?;
        let mut out = BufWriter::new(file);
        let entries = match request.format {
            ExportFormat::Log => writers::write_log(&job, &mut out, &mut tracker)?,
            ExportFormat::Jsonl => writers::write_jsonl(&job, &mut out, &mut tracker)?,
            ExportFormat::Csv => writers::write_csv(&job, &mut out, &mut tracker)?,
            ExportFormat::Bundle => bundle::write_bundle(&job, &mut out, &mut tracker)?,
        };
        out.flush()?;
        let file = out.into_inner().map_err(|e| e.into_error())?;
        file.sync_all()?;
        Ok(entries)
    })();
    let entries = match written {
        Ok(n) => n,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    if path.exists() {
        // Windows cannot rename over an existing file.
        std::fs::remove_file(&path)?;
    }
    std::fs::rename(&tmp, &path)?;
    let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    tracker.report(100.0, "Export complete");
    Ok(ExportResult {
        path: path.to_string_lossy().into_owned(),
        bytes,
        entries,
    })
}
