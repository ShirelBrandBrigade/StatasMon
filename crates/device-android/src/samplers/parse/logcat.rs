//! One line of `logcat -v threadtime -v epoch -v uid -v usec`:
//! `<epoch>.<usec> <uid> <pid> <tid> <L> <tag>: <message>` (the tag is
//! padded with spaces before the colon; `uid` is a number or a name).

use statasmon_protocol::LogLevel;

/// A parsed header line. Continuation lines (no header) belong to the
/// previous entry's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub ts_ms: i64,
    pub uid: String,
    pub pid: u32,
    pub tid: u32,
    pub level: LogLevel,
    pub tag: String,
    pub message: String,
}

/// What one raw line is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// `--------- beginning of main`
    Buffer(String),
    Entry(LogLine),
    Continuation(String),
}

pub fn level_from_char(c: char) -> LogLevel {
    match c {
        'V' | 'D' => LogLevel::Debug,
        'I' => LogLevel::Info,
        'W' => LogLevel::Warning,
        'E' => LogLevel::Error,
        'F' | 'A' => LogLevel::Fault,
        _ => LogLevel::Info,
    }
}

pub fn classify(raw: &str) -> Line {
    let line = raw.trim_end_matches('\r');
    if let Some(rest) = line.trim().strip_prefix("--------- beginning of ") {
        return Line::Buffer(rest.trim().to_owned());
    }
    match parse_header(line) {
        Some(e) => Line::Entry(e),
        None => Line::Continuation(line.to_owned()),
    }
}

fn parse_header(line: &str) -> Option<LogLine> {
    let s = line.trim_start();
    let (ts, s) = take_token(s)?;
    let (secs, usec) = ts.split_once('.')?;
    let secs: i64 = secs.parse().ok()?;
    let usec: i64 = usec.parse().ok()?;
    let ts_ms = secs.checked_mul(1000)?.checked_add(usec / 1000)?;
    let (uid, s) = take_token(s)?;
    let (pid, s) = take_token(s)?;
    let pid: u32 = pid.parse().ok()?;
    let (tid, s) = take_token(s)?;
    let tid: u32 = tid.parse().ok()?;
    let (lvl, s) = take_token(s)?;
    let mut lvl_chars = lvl.chars();
    let level_char = lvl_chars.next()?;
    if lvl_chars.next().is_some() || !"VDIWEFAS".contains(level_char) {
        return None;
    }
    let s = s.trim_start();
    // Tag runs to the first ": " (or ":" at end of line); it may contain
    // spaces because logcat pads short tags before the colon.
    let colon = s.find(':')?;
    let tag = s[..colon].trim_end();
    let message = s[colon + 1..].strip_prefix(' ').unwrap_or(&s[colon + 1..]);
    Some(LogLine {
        ts_ms,
        uid: uid.to_owned(),
        pid,
        tid,
        level: level_from_char(level_char),
        tag: tag.to_owned(),
        message: message.to_owned(),
    })
}

fn take_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((&s[..end], &s[end..]))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn classifies_fixture_lines() {
        let text = fixture("logcat_dump");
        let mut buffers = Vec::new();
        let mut entries = 0usize;
        let mut continuations = 0usize;
        for raw in text.lines() {
            match classify(raw) {
                Line::Buffer(b) => buffers.push(b),
                Line::Entry(_) => entries += 1,
                Line::Continuation(_) => continuations += 1,
            }
        }
        assert_eq!(buffers, ["main", "system", "events"]);
        assert!(entries > 2500, "{entries} entries");
        assert_eq!(continuations, 0, "this capture has no multi-line messages");
    }

    #[test]
    fn header_fields() {
        let l = "         1788954114.561704  1000  1621  1634 E HeatmapThread: ECT Thread time in milliseconds: 1788954114";
        let Line::Entry(e) = classify(l) else {
            panic!("expected entry");
        };
        assert_eq!(e.ts_ms, 1_788_954_114_561);
        assert_eq!(e.uid, "1000");
        assert_eq!((e.pid, e.tid), (1621, 1634));
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.tag, "HeatmapThread");
        assert_eq!(e.message, "ECT Thread time in milliseconds: 1788954114");

        let padded = "         1788954210.869967 shell  3898  3898 I adbd    : adbd service requested 'shell,v2,raw:dumpsys diskstats'";
        let Line::Entry(e) = classify(padded) else {
            panic!("expected entry");
        };
        assert_eq!(e.uid, "shell");
        assert_eq!(e.tag, "adbd");
        assert!(e.message.starts_with("adbd service requested"));

        assert_eq!(
            classify("--------- beginning of crash"),
            Line::Buffer("crash".into())
        );
        assert_eq!(
            classify("\tat com.example.Foo.bar(Foo.java:12)"),
            Line::Continuation("\tat com.example.Foo.bar(Foo.java:12)".into())
        );
    }

    #[test]
    fn level_map() {
        assert_eq!(level_from_char('V'), LogLevel::Debug);
        assert_eq!(level_from_char('D'), LogLevel::Debug);
        assert_eq!(level_from_char('I'), LogLevel::Info);
        assert_eq!(level_from_char('W'), LogLevel::Warning);
        assert_eq!(level_from_char('E'), LogLevel::Error);
        assert_eq!(level_from_char('F'), LogLevel::Fault);
        assert_eq!(level_from_char('A'), LogLevel::Fault);
    }
}
