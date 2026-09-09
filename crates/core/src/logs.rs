//! The per-device log ring with Rust-side filtering and paging
//! (docs/FEATURES.md §9).

use std::collections::VecDeque;

use regex::{Regex, RegexBuilder};
use statasmon_protocol::{LogEntry, LogFilter, LogLevel, LogPage};

use crate::error::EngineError;

/// Fixed-capacity ring of log entries.
#[derive(Debug, Clone)]
pub struct LogRing {
    capacity: usize,
    entries: VecDeque<LogEntry>,
    total: u64,
}

enum Matcher {
    None,
    Substring(String),
    Regex(Regex),
}

impl Matcher {
    fn new(filter: &LogFilter) -> Result<Self, EngineError> {
        match filter.query.as_deref().map(str::trim) {
            None | Some("") => Ok(Matcher::None),
            Some(q) if filter.regex => RegexBuilder::new(q)
                .case_insensitive(true)
                .size_limit(1 << 20)
                .build()
                .map(Matcher::Regex)
                .map_err(|e| EngineError::InvalidFilter(e.to_string())),
            Some(q) => Ok(Matcher::Substring(q.to_lowercase())),
        }
    }

    fn matches(&self, entry: &LogEntry) -> bool {
        match self {
            Matcher::None => true,
            Matcher::Substring(q) => {
                entry.message.to_lowercase().contains(q)
                    || entry
                        .tag
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains(q))
                    || entry
                        .subsystem
                        .as_deref()
                        .is_some_and(|s| s.to_lowercase().contains(q))
            }
            Matcher::Regex(re) => {
                re.is_match(&entry.message)
                    || entry.tag.as_deref().is_some_and(|t| re.is_match(t))
                    || entry.subsystem.as_deref().is_some_and(|s| re.is_match(s))
            }
        }
    }
}

impl LogRing {
    /// A ring keeping `capacity` entries.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: VecDeque::new(),
            total: 0,
        }
    }

    /// Entries kept.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Store entries (oldest are dropped past capacity).
    pub fn push(&mut self, entries: impl IntoIterator<Item = LogEntry>) {
        for e in entries {
            while self.entries.len() >= self.capacity {
                self.entries.pop_front();
            }
            self.entries.push_back(e);
            self.total += 1;
        }
    }

    /// Entries currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries ever stored (not bounded by capacity).
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Entries matching `filter` (ignores its `device_id`).
    pub fn count(&self, filter: &LogFilter) -> Result<u64, EngineError> {
        let m = Matcher::new(filter)?;
        Ok(self
            .entries
            .iter()
            .filter(|e| passes(e, filter, &m))
            .count() as u64)
    }

    /// Page `limit` matching entries starting at match index `offset`
    /// (oldest first). `total` is the number of matches in the whole ring.
    pub fn filter(
        &self,
        filter: &LogFilter,
        offset: usize,
        limit: usize,
    ) -> Result<LogPage, EngineError> {
        let m = Matcher::new(filter)?;
        let mut total = 0u64;
        let mut page = Vec::with_capacity(limit.min(1024));
        for e in self.entries.iter().filter(|e| passes(e, filter, &m)) {
            if total >= offset as u64 && page.len() < limit {
                page.push(e.clone());
            }
            total += 1;
        }
        Ok(LogPage {
            total,
            first_seq: page.first().map_or(0, |e| e.seq),
            entries: page,
        })
    }

    /// The newest `n` entries in chronological order (no filter).
    pub fn tail(&self, n: usize) -> Vec<LogEntry> {
        self.entries
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

fn passes(e: &LogEntry, f: &LogFilter, m: &Matcher) -> bool {
    if !f.processes.is_empty()
        && !f
            .processes
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&e.process))
    {
        return false;
    }
    if !f.levels.is_empty() && !f.levels.contains(&e.level) {
        return false;
    }
    if f.from_ms.is_some_and(|from| e.ts_ms < from) || f.to_ms.is_some_and(|to| e.ts_ms > to) {
        return false;
    }
    if !f.buffers.is_empty()
        && !e
            .buffer
            .as_deref()
            .is_some_and(|b| f.buffers.iter().any(|x| x.eq_ignore_ascii_case(b)))
    {
        return false;
    }
    m.matches(e)
}

/// Sort helper for level ordering in the UI.
pub fn level_rank(level: LogLevel) -> u8 {
    match level {
        LogLevel::Debug => 0,
        LogLevel::Info => 1,
        LogLevel::Notice => 2,
        LogLevel::Warning => 3,
        LogLevel::Error => 4,
        LogLevel::Fault => 5,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn entry(
        seq: u64,
        ts: i64,
        process: &str,
        level: LogLevel,
        buffer: Option<&str>,
        msg: &str,
    ) -> LogEntry {
        LogEntry {
            seq,
            ts_ms: ts,
            pid: Some(1),
            process: process.into(),
            tag: buffer.map(|_| "Tag".to_owned()),
            subsystem: None,
            category: None,
            buffer: buffer.map(str::to_owned),
            level,
            message: msg.into(),
        }
    }

    fn ring() -> LogRing {
        let mut r = LogRing::new(100);
        r.push(vec![
            entry(
                1,
                100,
                "FieldKit",
                LogLevel::Debug,
                None,
                "Sync: 18 pending records",
            ),
            entry(
                2,
                200,
                "Safari",
                LogLevel::Info,
                None,
                "Resource load completed",
            ),
            entry(
                3,
                300,
                "FieldKit",
                LogLevel::Error,
                None,
                "URLSession task failed: NSURLErrorTimedOut (-1001)",
            ),
            entry(
                4,
                400,
                "system_server",
                LogLevel::Warning,
                Some("system"),
                "Low on memory",
            ),
            entry(
                5,
                500,
                "com.example.fieldkit",
                LogLevel::Fault,
                Some("crash"),
                "FATAL EXCEPTION: main",
            ),
        ]);
        r
    }

    fn f() -> LogFilter {
        LogFilter::default()
    }

    #[test]
    fn capacity_and_totals() {
        let mut r = LogRing::new(2);
        r.push((0..5).map(|i| entry(i, i as i64, "p", LogLevel::Info, None, "m")));
        assert_eq!(r.len(), 2);
        assert_eq!(r.total(), 5);
        assert_eq!(
            r.tail(5).iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![3, 4]
        );
    }

    #[test]
    fn filters_by_process_case_insensitively() {
        let r = ring();
        let page = r
            .filter(
                &LogFilter {
                    processes: vec!["fieldkit".into()],
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.first_seq, 1);
    }

    #[test]
    fn filters_by_level() {
        let r = ring();
        let page = r
            .filter(
                &LogFilter {
                    levels: vec![LogLevel::Error, LogLevel::Fault],
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(
            page.entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![3, 5]
        );
    }

    #[test]
    fn filters_by_substring_and_regex() {
        let r = ring();
        let sub = r
            .filter(
                &LogFilter {
                    query: Some("timedout".into()),
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(sub.total, 1);
        let re = r
            .filter(
                &LogFilter {
                    query: Some(r"^(Sync|FATAL)".into()),
                    regex: true,
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(
            re.entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![1, 5]
        );
        let bad = r.filter(
            &LogFilter {
                query: Some("(".into()),
                regex: true,
                ..f()
            },
            0,
            10,
        );
        assert!(matches!(bad, Err(EngineError::InvalidFilter(_))));
        let blank = r
            .filter(
                &LogFilter {
                    query: Some("   ".into()),
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(blank.total, 5);
    }

    #[test]
    fn filters_by_time_range_and_buffer() {
        let r = ring();
        let range = r
            .filter(
                &LogFilter {
                    from_ms: Some(200),
                    to_ms: Some(400),
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(
            range.entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        let buf = r
            .filter(
                &LogFilter {
                    buffers: vec!["CRASH".into()],
                    ..f()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(buf.entries.len(), 1);
        assert_eq!(buf.entries[0].seq, 5);
    }

    #[test]
    fn pages_with_offset_and_limit() {
        let r = ring();
        let page = r.filter(&f(), 3, 10).unwrap();
        assert_eq!(page.total, 5);
        assert_eq!(page.first_seq, 4);
        assert_eq!(page.entries.len(), 2);
        let page = r.filter(&f(), 1, 2).unwrap();
        assert_eq!(
            page.entries.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(r.count(&f()).unwrap(), 5);
        let empty = r.filter(&f(), 99, 2).unwrap();
        assert_eq!(empty.first_seq, 0);
        assert!(empty.entries.is_empty());
    }

    #[test]
    fn level_rank_is_ordered() {
        assert!(level_rank(LogLevel::Fault) > level_rank(LogLevel::Debug));
    }
}
