//! Crash reports via `com.apple.crashreportmover` + `crashreportcopymobile`
//! (step 3.6).
//!
//! The mover relocates pending reports into the AFC-visible folder; the
//! copy service is plain AFC rooted at `/`. Reports are `.ips` files: the
//! first line is a JSON header (`app_name`, `bundleID`, `timestamp`,
//! `incident_id`, `bug_type`, `name`, `os_version`), the rest is the body
//! (JSON on iOS 15+: `procName`, `pid`, `exception`, `termination`). Reports
//! are never removed from the device.

use std::path::{Path, PathBuf};

use chrono::DateTime;
use idevice::IdeviceService;
use idevice::provider::IdeviceProvider;
use idevice::services::crashreportcopymobile::{CrashReportCopyMobileClient, flush_reports};
use serde_json::Value;

use crate::error::IosError;

/// Never pull more than this many reports in one `fetch`.
pub const MAX_REPORTS: usize = 200;
/// Reports whose file-name date is older than `since_ms` by more than this
/// are not even pulled (the header timestamp is authoritative; the name is
/// local time, so allow a day of slack).
const NAME_SLACK_MS: i64 = 86_400_000;

/// What kind of report an `.ips` file is, from its `bug_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReportKind {
    /// A process crash (`bug_type` 309, or legacy 109).
    Crash,
    /// A jetsam (memory kill) event (`bug_type` 298).
    Jetsam,
    /// Anything else: analytics, wake/power logs, excessive CPU, …
    #[default]
    Other,
}

impl ReportKind {
    pub fn from_bug_type(bug_type: Option<&str>, name: Option<&str>) -> Self {
        match bug_type {
            Some("309") | Some("109") => ReportKind::Crash,
            Some("298") => ReportKind::Jetsam,
            _ if name.is_some_and(|n| n.starts_with("JetsamEvent")) => ReportKind::Jetsam,
            _ => ReportKind::Other,
        }
    }
}

/// One `.ips` report copied to the host.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashReport {
    /// Path relative to the crash folder root (`Foo-2026-….ips`,
    /// `Retired/Foo-2026-….ips`).
    pub name: String,
    pub ts_ms: Option<i64>,
    pub process: Option<String>,
    pub bundle_id: Option<String>,
    pub reason: Option<String>,
    pub local_path: PathBuf,
    pub kind: ReportKind,
    pub bug_type: Option<String>,
    pub pid: Option<u32>,
    pub os_version: Option<String>,
    pub incident_id: Option<String>,
}

/// The parsed header (and the few body fields we surface) of an `.ips`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IpsSummary {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub timestamp: Option<String>,
    pub ts_ms: Option<i64>,
    pub incident_id: Option<String>,
    pub bug_type: Option<String>,
    pub name: Option<String>,
    pub os_version: Option<String>,
    pub process: Option<String>,
    pub pid: Option<u32>,
    pub reason: Option<String>,
    pub kind: ReportKind,
}

/// Parse an `.ips` text: JSON header line, then the body.
pub fn parse_ips(text: &str) -> Result<IpsSummary, IosError> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let header: Value = serde_json::from_str(first.trim())
        .map_err(|e| IosError::Parse(format!("ips header: {e}")))?;
    let s = |k: &str| header.get(k).and_then(Value::as_str).map(str::to_owned);
    let bug_type = header.get("bug_type").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });
    let mut out = IpsSummary {
        app_name: s("app_name"),
        bundle_id: s("bundleID"),
        timestamp: s("timestamp"),
        incident_id: s("incident_id"),
        bug_type: bug_type.clone(),
        name: s("name"),
        os_version: s("os_version"),
        ..IpsSummary::default()
    };
    out.ts_ms = out.timestamp.as_deref().and_then(parse_ips_timestamp);
    out.kind = ReportKind::from_bug_type(bug_type.as_deref(), out.name.as_deref());
    out.process = out.name.clone().or_else(|| out.app_name.clone());

    // Body: JSON on iOS 15+, free text on older devices.
    if let Ok(body) = serde_json::from_str::<Value>(rest.trim()) {
        if let Some(p) = body.get("procName").and_then(Value::as_str) {
            out.process = Some(p.to_owned());
        }
        out.pid = body
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok());
        match out.kind {
            ReportKind::Jetsam => {
                // The killed process is the one carrying a `reason`.
                if let Some(procs) = body.get("processes").and_then(Value::as_array) {
                    let killed = procs
                        .iter()
                        .find(|p| p.get("reason").is_some())
                        .or_else(|| procs.first());
                    if let Some(k) = killed {
                        if let Some(n) = k.get("name").and_then(Value::as_str) {
                            out.process = Some(n.to_owned());
                        }
                        out.pid = k
                            .get("pid")
                            .and_then(Value::as_u64)
                            .and_then(|p| u32::try_from(p).ok());
                        out.reason = k
                            .get("reason")
                            .and_then(Value::as_str)
                            .map(|r| format!("jetsam: {r}"));
                    }
                }
                if out.reason.is_none() {
                    out.reason = Some("jetsam".to_owned());
                }
            }
            _ => {
                let exc = body.get("exception");
                let exc_type = exc.and_then(|e| e.get("type")).and_then(Value::as_str);
                let signal = exc.and_then(|e| e.get("signal")).and_then(Value::as_str);
                let term = body.get("termination");
                let indicator = term
                    .and_then(|t| t.get("indicator"))
                    .and_then(Value::as_str)
                    .or_else(|| term.and_then(|t| t.get("reason")).and_then(Value::as_str));
                let mut parts: Vec<String> = Vec::new();
                match (exc_type, signal) {
                    (Some(t), Some(sig)) => parts.push(format!("{t} ({sig})")),
                    (Some(t), None) => parts.push(t.to_owned()),
                    (None, Some(sig)) => parts.push(sig.to_owned()),
                    (None, None) => {}
                }
                if let Some(i) = indicator.map(str::trim).filter(|s| !s.is_empty()) {
                    parts.push(i.to_owned());
                }
                if parts.is_empty()
                    && let Some(r) = body
                        .get("terminationReason")
                        .and_then(Value::as_str)
                        .or_else(|| body.get("reason").and_then(Value::as_str))
                {
                    parts.push(r.to_owned());
                }
                if !parts.is_empty() {
                    out.reason = Some(parts.join(" — "));
                }
            }
        }
    } else {
        // Legacy text body: "Exception Type:  EXC_CRASH (SIGABRT)".
        for line in rest.lines() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("Exception Type:") {
                out.reason = Some(v.trim().to_owned());
            } else if let Some(v) = l.strip_prefix("Process:") {
                let v = v.trim();
                let name = v.split(" [").next().unwrap_or(v).trim();
                if !name.is_empty() {
                    out.process = Some(name.to_owned());
                }
                if let Some(pid) = v
                    .split('[')
                    .nth(1)
                    .and_then(|p| p.trim_end_matches(']').parse().ok())
                {
                    out.pid = Some(pid);
                }
            }
        }
    }
    Ok(out)
}

/// `"2026-09-09 10:10:10.00 +0300"` (or without the fraction) → epoch ms.
pub fn parse_ips_timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f %z")
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z"))
        .ok()
        .map(|t| t.timestamp_millis())
}

/// The date embedded in a report file name (`Foo-2026-09-09-101010.ips`),
/// read as UTC (the device writes local time; callers allow slack).
pub fn name_ts_ms(name: &str) -> Option<i64> {
    let stem = name.rsplit('/').next()?.trim_end_matches(".ips");
    let parts: Vec<&str> = stem.split('-').collect();
    if parts.len() < 4 {
        return None;
    }
    let n = parts.len();
    let (y, m, d, hms) = (parts[n - 4], parts[n - 3], parts[n - 2], parts[n - 1]);
    if y.len() != 4 || m.len() != 2 || d.len() != 2 || hms.len() < 6 {
        return None;
    }
    let text = format!("{y}-{m}-{d} {}:{}:{}", &hms[0..2], &hms[2..4], &hms[4..6]);
    chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|t| t.and_utc().timestamp_millis())
}

/// Move pending reports on the device, copy new `.ips` files into
/// `dest_dir`, parse their header. `since_ms` skips reports older than that.
///
/// Files already present under `dest_dir` are not pulled again. Every new
/// report is written to disk (so it is not re-pulled next time) but only
/// those at or after `since_ms` are returned. Reports stay on the device.
pub async fn fetch(
    provider: &dyn IdeviceProvider,
    dest_dir: &Path,
    since_ms: Option<i64>,
) -> Result<Vec<CrashReport>, IosError> {
    if let Err(e) = flush_reports(provider).await {
        // The mover is best-effort: without it we still see what was
        // already moved.
        tracing::warn!(target: "ios.crash", "crashreportmover: {e}");
    }
    let mut client = CrashReportCopyMobileClient::connect(provider)
        .await
        .map_err(|e| IosError::service("crashreportcopymobile", e))?;
    let names = list_reports(&mut client).await?;
    tokio::fs::create_dir_all(dest_dir).await?;

    let mut out = Vec::new();
    for name in names {
        let local_path = dest_dir.join(&name);
        if local_path.exists() {
            continue;
        }
        if let (Some(since), Some(named)) = (since_ms, name_ts_ms(&name))
            && named + NAME_SLACK_MS < since
        {
            continue;
        }
        let bytes = match client.pull(name.clone()).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(target: "ios.crash", "pull {name}: {e}");
                continue;
            }
        };
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&local_path, &bytes).await?;
        let text = String::from_utf8_lossy(&bytes);
        let summary = match parse_ips(&text) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(target: "ios.crash", "{name}: {e}");
                IpsSummary::default()
            }
        };
        if let (Some(since), Some(ts)) = (since_ms, summary.ts_ms)
            && ts < since
        {
            continue;
        }
        out.push(CrashReport {
            name,
            ts_ms: summary.ts_ms,
            process: summary.process,
            bundle_id: summary.bundle_id,
            reason: summary.reason,
            local_path,
            kind: summary.kind,
            bug_type: summary.bug_type,
            pid: summary.pid,
            os_version: summary.os_version,
            incident_id: summary.incident_id,
        });
    }
    tracing::info!(target: "ios.crash", "{} new report(s) under {}", out.len(), dest_dir.display());
    Ok(out)
}

/// Top-level `.ips` files plus those one level down in any directory the
/// root lists (`Retired`, `…`), bounded to [`MAX_REPORTS`].
async fn list_reports(client: &mut CrashReportCopyMobileClient) -> Result<Vec<String>, IosError> {
    let root = client
        .ls(None)
        .await
        .map_err(|e| IosError::service("crashreportcopymobile", e))?;
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for name in root {
        let name = name.trim_matches('/').to_owned();
        if name.is_empty() || name == "." || name == ".." {
            continue;
        }
        if name.ends_with(".ips") {
            files.push(name);
        } else if !name.contains('.') {
            dirs.push(name);
        }
    }
    for dir in dirs {
        if files.len() >= MAX_REPORTS {
            break;
        }
        let is_dir = client
            .afc_client
            .get_file_info(format!("/{dir}"))
            .await
            .map(|i| i.st_ifmt == "S_IFDIR")
            .unwrap_or(false);
        if !is_dir {
            continue;
        }
        match client.ls(Some(&format!("/{dir}"))).await {
            Ok(children) => {
                for child in children {
                    let child = child.trim_matches('/');
                    if child.ends_with(".ips") {
                        files.push(format!("{dir}/{child}"));
                    }
                }
            }
            Err(e) => tracing::debug!(target: "ios.crash", "ls {dir}: {e}"),
        }
    }
    files.sort();
    files.truncate(MAX_REPORTS);
    Ok(files)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    pub const APP_CRASH: &str = r#"{"app_name":"FieldKit","timestamp":"2026-09-09 10:10:10.00 +0300","app_version":"1.2","slice_uuid":"0000","build_version":"42","platform":2,"bundleID":"com.example.fieldkit","share_with_app_devs":0,"is_first_party":0,"bug_type":"309","os_version":"iPhone OS 17.4.1 (21E237)","roots_installed":0,"name":"FieldKit","incident_id":"ABCDEF00-0000-0000-0000-000000000001"}
{"uptime":1200,"procRole":"Foreground","version":2,"userID":501,"deployVersion":210,"modelCode":"iPad13,18","coalitionID":600,"osVersion":{"isEmbedded":true,"train":"iPhone OS 17.4.1","releaseType":"User","build":"21E237"},"captureTime":"2026-09-09 10:10:09.9000 +0300","incident":"ABCDEF00-0000-0000-0000-000000000001","pid":4242,"cpuType":"ARM-64","procName":"FieldKit","procPath":"/private/var/containers/Bundle/Application/X/FieldKit.app/FieldKit","bundleInfo":{"CFBundleShortVersionString":"1.2","CFBundleVersion":"42","CFBundleIdentifier":"com.example.fieldkit"},"exception":{"codes":"0x0000000000000000, 0x0000000000000000","rawCodes":[0,0],"type":"EXC_CRASH","signal":"SIGABRT"},"termination":{"flags":0,"code":6,"namespace":"SIGNAL","indicator":"Abort trap: 6","byProc":"FieldKit","byPid":4242},"asi":{"libsystem_c.dylib":["abort() called"]},"faultingThread":0,"threads":[]}
"#;

    pub const JETSAM: &str = r#"{"bug_type":"298","timestamp":"2026-09-09 09:00:00.00 +0300","os_version":"iPhone OS 17.4.1 (21E237)","roots_installed":0,"incident_id":"ABCDEF00-0000-0000-0000-000000000002"}
{"crashReporterKey":"0","kernel":"Darwin Kernel","product":"iPad13,18","incident":"ABCDEF00-0000-0000-0000-000000000002","date":"2026-09-09 09:00:00.00 +0300","build":"iPhone OS 17.4.1 (21E237)","timeDelta":30,"memoryStatus":{"compressorSize":50000,"compressions":100,"decompressions":50,"zoneMapCap":1000,"largestZone":"APFS_4K_OBJS","largestZoneSize":100,"pageSize":16384,"uncompressed":10,"zoneMapSize":100,"memoryPages":{"active":1000,"throttled":0,"fileBacked":1000,"wired":1000,"anonymous":1000,"purgeable":0,"inactive":100,"free":10,"speculative":5}},"largestProcess":"Heavy","genCounter":1,"processes":[{"uuid":"0","states":["daemon","idle"],"lifetimeMax":100,"age":1,"purgeable":0,"fds":50,"coalition":1,"rpages":100,"pid":1,"cpuTime":0.1,"name":"launchd"},{"uuid":"1","states":["frontmost","resume"],"killDelta":1000,"lifetimeMax":50000,"age":10,"purgeable":0,"fds":100,"coalition":2,"rpages":49000,"reason":"vm-pageshortage","pid":5151,"cpuTime":10.0,"name":"Heavy"}]}
"#;

    #[test]
    fn app_crash_header_and_body() {
        let s = parse_ips(APP_CRASH).unwrap();
        assert_eq!(s.app_name.as_deref(), Some("FieldKit"));
        assert_eq!(s.bundle_id.as_deref(), Some("com.example.fieldkit"));
        assert_eq!(s.bug_type.as_deref(), Some("309"));
        assert_eq!(s.kind, ReportKind::Crash);
        assert_eq!(s.name.as_deref(), Some("FieldKit"));
        assert_eq!(s.process.as_deref(), Some("FieldKit"));
        assert_eq!(s.pid, Some(4242));
        assert_eq!(s.os_version.as_deref(), Some("iPhone OS 17.4.1 (21E237)"));
        assert_eq!(
            s.incident_id.as_deref(),
            Some("ABCDEF00-0000-0000-0000-000000000001")
        );
        // 2026-09-09 10:10:10 +03:00 = 07:10:10 UTC.
        assert_eq!(s.ts_ms, parse_ips_timestamp("2026-09-09 07:10:10 +0000"));
        assert_eq!(
            s.reason.as_deref(),
            Some("EXC_CRASH (SIGABRT) — Abort trap: 6")
        );
    }

    #[test]
    fn jetsam_report_is_classified() {
        let s = parse_ips(JETSAM).unwrap();
        assert_eq!(s.kind, ReportKind::Jetsam);
        assert_eq!(s.process.as_deref(), Some("Heavy"));
        assert_eq!(s.pid, Some(5151));
        assert_eq!(s.reason.as_deref(), Some("jetsam: vm-pageshortage"));
        assert!(s.bundle_id.is_none());
        assert_eq!(
            ReportKind::from_bug_type(None, Some("JetsamEvent-2026")),
            ReportKind::Jetsam
        );
        assert_eq!(
            ReportKind::from_bug_type(Some("211"), None),
            ReportKind::Other
        );
    }

    #[test]
    fn legacy_text_body_and_bad_header() {
        let text = "{\"app_name\":\"Old\",\"bug_type\":\"109\",\"timestamp\":\"2020-01-01 00:00:00 +0000\"}\nIncident Identifier: X\nProcess:             Old [77]\nException Type:  EXC_BAD_ACCESS (SIGSEGV)\n";
        let s = parse_ips(text).unwrap();
        assert_eq!(s.kind, ReportKind::Crash);
        assert_eq!(s.process.as_deref(), Some("Old"));
        assert_eq!(s.pid, Some(77));
        assert_eq!(s.reason.as_deref(), Some("EXC_BAD_ACCESS (SIGSEGV)"));
        assert_eq!(s.ts_ms, Some(1_577_836_800_000));
        assert!(parse_ips("not json\n{}").is_err());
    }

    #[test]
    fn timestamps() {
        assert_eq!(
            parse_ips_timestamp("2026-09-09 10:10:10.00 +0300"),
            Some(1_788_937_810_000)
        );
        assert_eq!(
            parse_ips_timestamp("2026-09-09 07:10:10 +0000"),
            Some(1_788_937_810_000)
        );
        assert_eq!(parse_ips_timestamp("yesterday"), None);
        assert_eq!(
            name_ts_ms("Retired/FieldKit-2026-09-09-101010.ips"),
            Some(1_788_948_610_000)
        );
        assert_eq!(
            name_ts_ms("JetsamEvent-2026-09-09-090000.ips"),
            Some(1_788_944_400_000)
        );
        assert_eq!(name_ts_ms("Retired"), None);
        assert_eq!(name_ts_ms("weird-name.ips"), None);
    }
}
