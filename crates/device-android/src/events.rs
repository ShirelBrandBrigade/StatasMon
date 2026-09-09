//! Derived events (step 2.6): LMK kills, crashes and ANRs from the log
//! stream; thermal status changes from the slow sampler.

use statasmon_protocol::{EventKind, LogEntry, LogLevel};

use crate::samplers::parse::thermal::status_name;

/// An event derived from one log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derived {
    pub kind: EventKind,
    pub detail: String,
    pub pid: Option<u32>,
    pub process: Option<String>,
}

/// `"None → Light"` using the Android thermal status names.
pub fn thermal_change_detail(prev_raw: u32, now_raw: u32) -> String {
    format!("{} → {}", status_name(prev_raw), status_name(now_raw))
}

/// Inspect one (possibly multi-line) entry. Sources:
/// - `lowmemorykiller`: `Kill 'com.foo' (1234), uid 10123, oom_score_adj 900 to free 51200kB …`
/// - `AndroidRuntime` E `FATAL EXCEPTION: main` + `Process: com.foo, PID: 1234`
/// - events buffer `am_crash: [pid,uid,process,flags,exception,message,file,line]`
/// - events buffer `am_anr: [user,pid,package,flags,reason]`
/// - `ActivityManager` E `ANR in com.foo (com.foo/.Main)` + `PID: 1234`
/// - events buffer `am_low_memory: [num_processes]` → MemoryWarning
pub fn derive(entry: &LogEntry) -> Option<Derived> {
    let tag = entry.tag.as_deref()?;
    let msg = entry.message.as_str();
    let first = msg.lines().next().unwrap_or("").trim();
    match tag {
        "lowmemorykiller" => {
            let name = between(first, "Kill '", "'")?;
            let pid = between(first, "' (", ")").and_then(|p| p.trim().parse().ok());
            Some(Derived {
                kind: EventKind::LmkKill,
                detail: first.to_owned(),
                pid,
                process: Some(name.to_owned()),
            })
        }
        "AndroidRuntime" if first.starts_with("FATAL EXCEPTION") => {
            let process = msg
                .lines()
                .find_map(|l| between(l.trim(), "Process: ", ","))
                .map(str::to_owned);
            let pid = msg
                .lines()
                .find_map(|l| l.trim().split("PID: ").nth(1))
                .and_then(|p| p.trim().parse().ok())
                .or(entry.pid);
            let cause =
                msg.lines().map(str::trim).skip(1).find(|l| {
                    !l.is_empty() && !l.starts_with("Process:") && !l.starts_with("PID:")
                });
            let mut detail = first.to_owned();
            if let Some(c) = cause {
                detail.push_str(" — ");
                detail.push_str(c);
            }
            Some(Derived {
                kind: EventKind::Crash,
                detail,
                pid,
                process,
            })
        }
        "am_crash" => {
            // EventLogTags: am_crash (PID|1|5),(User|1|5),(Process Name|3),(Flags|1|5),
            // (Exception|3),(Message|3),(File|3),(Line|1|5),(Recoverable|1|5).
            let f = event_fields(first);
            let pid = f.first().and_then(|p| p.parse().ok());
            let process = f.get(2).map(|s| (*s).to_owned());
            let exception = f.get(4).copied().unwrap_or("crash");
            let message = f.get(5).copied().unwrap_or("");
            let detail = if message.is_empty() {
                exception.to_owned()
            } else {
                format!("{exception}: {message}")
            };
            Some(Derived {
                kind: EventKind::Crash,
                detail,
                pid,
                process,
            })
        }
        "am_anr" => {
            let f = event_fields(first);
            let pid = f.get(1).and_then(|p| p.parse().ok());
            let process = f.get(2).map(|s| (*s).to_owned());
            let reason = f.get(4).copied().unwrap_or("ANR");
            Some(Derived {
                kind: EventKind::Anr,
                detail: reason.to_owned(),
                pid,
                process,
            })
        }
        "ActivityManager" if entry.level >= LogLevel::Error && first.starts_with("ANR in ") => {
            let process = first["ANR in ".len()..]
                .split([' ', '('])
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let pid = msg
                .lines()
                .find_map(|l| l.trim().strip_prefix("PID: "))
                .and_then(|p| p.trim().parse().ok());
            let reason = msg
                .lines()
                .find_map(|l| l.trim().strip_prefix("Reason: "))
                .map(str::trim);
            let mut detail = first.to_owned();
            if let Some(r) = reason {
                detail.push_str(" — ");
                detail.push_str(r);
            }
            Some(Derived {
                kind: EventKind::Anr,
                detail,
                pid,
                process,
            })
        }
        "am_low_memory" => {
            let n = event_fields(first).first().copied().unwrap_or("?");
            Some(Derived {
                kind: EventKind::MemoryWarning,
                detail: format!("System low on memory ({n} cached processes)"),
                pid: None,
                process: None,
            })
        }
        _ => None,
    }
}

/// `[a,b,c]` → `["a","b","c"]`; a bare scalar → one field.
fn event_fields(s: &str) -> Vec<&str> {
    let inner = s
        .trim()
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(s.trim());
    inner.split(',').map(str::trim).collect()
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let a = s.find(start)? + start.len();
    let b = s[a..].find(end)? + a;
    Some(&s[a..b])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn entry(tag: &str, level: LogLevel, message: &str) -> LogEntry {
        LogEntry {
            seq: 0,
            ts_ms: 0,
            pid: Some(999),
            process: "x".into(),
            tag: Some(tag.into()),
            subsystem: None,
            category: None,
            buffer: None,
            level,
            message: message.into(),
        }
    }

    #[test]
    fn lmk_kill() {
        let d = derive(&entry(
            "lowmemorykiller",
            LogLevel::Info,
            "Kill 'com.example.app' (12345), uid 10234, oom_score_adj 900 to free 51200kB rss, 0kB swap; reason: low watermark is breached",
        ))
        .unwrap();
        assert_eq!(d.kind, EventKind::LmkKill);
        assert_eq!(d.pid, Some(12345));
        assert_eq!(d.process.as_deref(), Some("com.example.app"));
    }

    #[test]
    fn fatal_exception() {
        let d = derive(&entry(
            "AndroidRuntime",
            LogLevel::Error,
            "FATAL EXCEPTION: main\nProcess: com.example.app, PID: 4242\njava.lang.NullPointerException: boom\n\tat com.example.Foo.bar(Foo.java:1)",
        ))
        .unwrap();
        assert_eq!(d.kind, EventKind::Crash);
        assert_eq!(d.pid, Some(4242));
        assert_eq!(d.process.as_deref(), Some("com.example.app"));
        assert_eq!(
            d.detail,
            "FATAL EXCEPTION: main — java.lang.NullPointerException: boom"
        );
    }

    #[test]
    fn am_crash_and_anr() {
        let d = derive(&entry(
            "am_crash",
            LogLevel::Info,
            "[4242,10234,com.example.app,952745541,java.lang.NullPointerException,boom,Foo.java,1,0]",
        ))
        .unwrap();
        assert_eq!(d.kind, EventKind::Crash);
        assert_eq!(d.pid, Some(4242));
        assert_eq!(d.process.as_deref(), Some("com.example.app"));
        assert_eq!(d.detail, "java.lang.NullPointerException: boom");

        let d = derive(&entry(
            "am_anr",
            LogLevel::Info,
            "[0,4242,com.example.app,952745541,Input dispatching timed out]",
        ))
        .unwrap();
        assert_eq!(d.kind, EventKind::Anr);
        assert_eq!(d.pid, Some(4242));
        assert_eq!(d.detail, "Input dispatching timed out");

        let d = derive(&entry(
            "ActivityManager",
            LogLevel::Error,
            "ANR in com.example.app (com.example.app/.Main)\nPID: 4242\nReason: Input dispatching timed out",
        ))
        .unwrap();
        assert_eq!(d.kind, EventKind::Anr);
        assert_eq!(d.pid, Some(4242));
        assert_eq!(d.process.as_deref(), Some("com.example.app"));
        assert!(d.detail.ends_with("Input dispatching timed out"));
    }

    #[test]
    fn low_memory_and_noise() {
        let d = derive(&entry("am_low_memory", LogLevel::Info, "[17]")).unwrap();
        assert_eq!(d.kind, EventKind::MemoryWarning);
        assert!(
            derive(&entry(
                "HeatmapThread",
                LogLevel::Error,
                "!@ CAM New BSOH: 0"
            ))
            .is_none()
        );
        assert!(
            derive(&entry(
                "AndroidRuntime",
                LogLevel::Info,
                "Using default boot image"
            ))
            .is_none()
        );
        assert_eq!(thermal_change_detail(0, 1), "None → Light");
        assert_eq!(thermal_change_detail(3, 6), "Severe → Shutdown");
    }
}
