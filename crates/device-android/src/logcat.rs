//! Continuous `logcat` stream → `LogEntry` batches and derived events
//! (step 2.6).
//!
//! Line shape (`-v threadtime -v epoch -v uid -v usec`):
//! `<epoch.usec> <uid> <pid> <tid> <L> <tag>: <message>`; lines that do not
//! match the header continue the previous message; `--------- beginning
//! of <buffer>` markers name the buffer for the lines that follow.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use statasmon_device::EventSink;
use statasmon_protocol::{EventKind, LogEntry};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::sync::CancellationToken;

use crate::events::{Derived, derive};
use crate::samplers::PidTable;
use crate::samplers::parse::logcat::{Line, LogLine, classify};
use crate::shell::{Shell, ShellError};

/// Flush a partial batch after this long.
pub const BATCH_LATENCY: Duration = Duration::from_millis(250);
/// Do not re-run `ps` for unknown pids more often than this.
const RESOLVE_INTERVAL: Duration = Duration::from_secs(2);
/// A second Crash/ANR for the same pid within this window is the same
/// incident reported by another source (`am_crash` + `FATAL EXCEPTION`).
const EVENT_DEDUPE_MS: i64 = 10_000;
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(8);

/// How the stream is opened.
#[derive(Debug, Clone)]
pub struct LogcatConfig {
    /// Buffers to read (default main, system, crash, events).
    pub buffers: Vec<String>,
    /// Restrict to one pid (per-app log) when set.
    pub pid: Option<u32>,
    /// Max entries per `Message::Log`.
    pub batch: usize,
    /// pid → process name, shared with the medium sampler (`Shared::pids`).
    /// Unknown pids are resolved here with `ps`, rate-limited.
    pub pids: PidTable,
}

impl Default for LogcatConfig {
    fn default() -> Self {
        Self {
            buffers: ["main", "system", "crash", "events"]
                .map(String::from)
                .to_vec(),
            pid: None,
            batch: 200,
            pids: PidTable::default(),
        }
    }
}

impl LogcatConfig {
    /// The `logcat` invocation for this configuration.
    pub fn command(&self) -> String {
        let mut cmd = String::from("logcat -v threadtime -v epoch -v uid -v usec");
        if !self.buffers.is_empty() {
            cmd.push_str(" -b ");
            cmd.push_str(&self.buffers.join(","));
        }
        cmd.push_str(" -T 1");
        if let Some(pid) = self.pid {
            cmd.push_str(&format!(" --pid={pid}"));
        }
        cmd
    }
}

/// Run until `cancel` fires. Reconnects with backoff if the stream ends
/// while the device is still attached.
pub async fn run(
    shell: Arc<dyn Shell>,
    sink: EventSink,
    device_id: String,
    cfg: LogcatConfig,
    cancel: CancellationToken,
) {
    let cmd = cfg.command();
    let mut state = StreamState::new(cfg);
    let mut backoff = BACKOFF_MIN;
    loop {
        if cancel.is_cancelled() || sink.is_closed() {
            return;
        }
        let stream = tokio::select! {
            _ = cancel.cancelled() => return,
            r = shell.stream(&cmd) => r,
        };
        match stream {
            Ok(stream) => {
                backoff = BACKOFF_MIN;
                let outcome =
                    read_stream(stream, &*shell, &sink, &device_id, &mut state, &cancel).await;
                match outcome {
                    Outcome::Cancelled | Outcome::SinkClosed => return,
                    Outcome::Ended => {}
                }
            }
            Err(ShellError::DeviceGone(_)) => return,
            Err(e) => tracing::warn!(target: "android.logcat", "open failed: {e}"),
        }
        // The stream ended: is the device still there?
        match shell.exec("echo ok", 3_000).await {
            Ok(out) if out.contains("ok") => {
                tracing::info!(target: "android.logcat", "stream ended; reconnecting in {backoff:?}");
            }
            Err(ShellError::DeviceGone(_)) => return,
            Ok(_) | Err(_) => {
                tracing::warn!(target: "android.logcat", "device not answering; retrying in {backoff:?}");
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

enum Outcome {
    Ended,
    Cancelled,
    SinkClosed,
}

/// Everything that survives a reconnect: the sequence counter, the last
/// header (for continuation lines) and the event de-duplication window.
struct StreamState {
    cfg: LogcatConfig,
    seq: u64,
    buffer: Option<String>,
    last: Option<LogLine>,
    recent_events: Vec<(EventKind, Option<u32>, i64)>,
    last_resolve: Option<tokio::time::Instant>,
}

impl StreamState {
    fn new(cfg: LogcatConfig) -> Self {
        Self {
            cfg,
            seq: 0,
            buffer: None,
            last: None,
            recent_events: Vec::new(),
            last_resolve: None,
        }
    }

    fn entry(&mut self, line: LogLine) -> LogEntry {
        self.seq += 1;
        let process = self
            .cfg
            .pids
            .read()
            .get(&line.pid)
            .cloned()
            .unwrap_or_else(|| format!("pid {}", line.pid));
        let e = LogEntry {
            seq: self.seq,
            ts_ms: line.ts_ms,
            pid: Some(line.pid),
            process,
            tag: Some(line.tag.clone()),
            subsystem: None,
            category: None,
            buffer: self.buffer.clone(),
            level: line.level,
            message: line.message.clone(),
        };
        self.last = Some(line);
        e
    }

    fn dedupe(&mut self, d: &Derived, ts_ms: i64) -> bool {
        self.recent_events
            .retain(|(_, _, t)| ts_ms - *t < EVENT_DEDUPE_MS);
        if matches!(d.kind, EventKind::Crash | EventKind::Anr)
            && d.pid.is_some()
            && self
                .recent_events
                .iter()
                .any(|(k, p, _)| *k == d.kind && *p == d.pid)
        {
            return false;
        }
        self.recent_events.push((d.kind, d.pid, ts_ms));
        true
    }
}

async fn read_stream(
    stream: crate::shell::ShellStream,
    shell: &dyn Shell,
    sink: &EventSink,
    device_id: &str,
    state: &mut StreamState,
    cancel: &CancellationToken,
) -> Outcome {
    let mut lines = BufReader::new(stream).lines();
    let mut pending: Vec<LogEntry> = Vec::with_capacity(state.cfg.batch);
    let flush_after = tokio::time::sleep(BATCH_LATENCY);
    tokio::pin!(flush_after);
    let mut ended = false;
    loop {
        let flush = tokio::select! {
            _ = cancel.cancelled() => return Outcome::Cancelled,
            _ = sink.closed() => return Outcome::SinkClosed,
            _ = &mut flush_after, if !pending.is_empty() => true,
            next = lines.next_line() => {
                match next {
                    Ok(Some(raw)) => {
                        let first = pending.is_empty();
                        push_line(state, &mut pending, raw);
                        if first && !pending.is_empty() {
                            flush_after.as_mut().reset(tokio::time::Instant::now() + BATCH_LATENCY);
                        }
                        pending.len() >= state.cfg.batch
                    }
                    Ok(None) => { ended = true; true }
                    Err(e) => {
                        tracing::debug!(target: "android.logcat", "read: {e}");
                        ended = true;
                        true
                    }
                }
            }
        };
        if flush && !pending.is_empty() {
            let batch = std::mem::take(&mut pending);
            if let Outcome::SinkClosed = deliver(shell, sink, device_id, state, batch).await {
                return Outcome::SinkClosed;
            }
        }
        if ended {
            return Outcome::Ended;
        }
    }
}

fn push_line(state: &mut StreamState, pending: &mut Vec<LogEntry>, raw: String) {
    match classify(&raw) {
        Line::Buffer(name) => state.buffer = Some(name),
        Line::Entry(line) => {
            if state.cfg.pid.is_some_and(|p| p != line.pid) {
                return; // logcat --pid already filters; belt and braces
            }
            let e = state.entry(line);
            pending.push(e);
        }
        Line::Continuation(text) => {
            if text.trim().is_empty() {
                return;
            }
            if let Some(last) = pending.last_mut() {
                last.message.push('\n');
                last.message.push_str(&text);
            } else if let Some(prev) = state.last.clone() {
                // The header entry was already flushed: re-emit it as a
                // continuation entry with the same metadata.
                let mut e = state.entry(prev);
                e.message = text;
                pending.push(e);
            }
        }
    }
}

/// Resolve unknown pids (rate-limited), send the batch, then its events.
async fn deliver(
    shell: &dyn Shell,
    sink: &EventSink,
    device_id: &str,
    state: &mut StreamState,
    mut batch: Vec<LogEntry>,
) -> Outcome {
    let unknown: HashSet<u32> = batch
        .iter()
        .filter(|e| e.process.starts_with("pid "))
        .filter_map(|e| e.pid)
        .collect();
    if !unknown.is_empty()
        && state
            .last_resolve
            .is_none_or(|t| t.elapsed() >= RESOLVE_INTERVAL)
    {
        state.last_resolve = Some(tokio::time::Instant::now());
        if let Ok(out) = shell.exec("ps -A -o PID,NAME 2>/dev/null", 3_000).await {
            let names = parse_pid_names(&out);
            if !names.is_empty() {
                let mut table = state.cfg.pids.write();
                for (pid, name) in &names {
                    table.entry(*pid).or_insert_with(|| name.clone());
                }
            }
            for e in batch.iter_mut() {
                if let Some(pid) = e.pid
                    && unknown.contains(&pid)
                    && let Some(name) = names.get(&pid)
                {
                    e.process = name.clone();
                }
            }
        }
    }
    let events: Vec<(Derived, i64)> = batch
        .iter()
        .filter_map(|e| derive(e).map(|d| (d, e.ts_ms)))
        .filter(|(d, ts)| state.dedupe(d, *ts))
        .collect();
    if sink.log(device_id, batch).await.is_err() {
        return Outcome::SinkClosed;
    }
    for (d, ts_ms) in events {
        if sink
            .event(device_id, ts_ms, d.kind, d.detail, d.pid, d.process)
            .await
            .is_err()
        {
            return Outcome::SinkClosed;
        }
    }
    Outcome::Ended
}

/// `ps -A -o PID,NAME` → pid → name.
pub fn parse_pid_names(text: &str) -> HashMap<u32, String> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let pid = it.next()?.parse().ok()?;
            let name = it.collect::<Vec<_>>().join(" ");
            (!name.is_empty()).then_some((pid, name))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn command_shape() {
        let cfg = LogcatConfig::default();
        assert_eq!(
            cfg.command(),
            "logcat -v threadtime -v epoch -v uid -v usec -b main,system,crash,events -T 1"
        );
        let cfg = LogcatConfig {
            pid: Some(4242),
            ..Default::default()
        };
        assert!(cfg.command().ends_with(" --pid=4242"));
    }

    #[test]
    fn pid_names() {
        let m = parse_pid_names("  PID NAME\n    1 init\n 3133 com.android.systemui\n");
        assert_eq!(m[&1], "init");
        assert_eq!(m[&3133], "com.android.systemui");
    }
}
