#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Every format and scope from the replayed fake session, with line counts
//! and the bundle's entry list checked; temp-then-rename; progress.

use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

use statasmon_export::{ExportError, export};
use statasmon_protocol::{
    ExportFormat, ExportRequest, ExportScope, LogEntry, LogFilter, LogLevel, Message,
};
use statasmon_store::{Store, StoreConfig, parse_jsonl, replay};

struct Fixture {
    _dir: tempfile::TempDir,
    out: tempfile::TempDir,
    store: Arc<Store>,
    session: String,
    messages: Vec<Message>,
}

fn fixture() -> Fixture {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fake/session-3dev-30s.jsonl"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path())).unwrap();
    let messages = parse_jsonl(&text).unwrap();
    let ids = replay(&store, &messages).unwrap();
    let session = ids
        .iter()
        .find(|i| i.starts_with("fake_iphone"))
        .unwrap()
        .clone();
    Fixture {
        _dir: dir,
        out: tempfile::tempdir().unwrap(),
        store,
        session,
        messages,
    }
}

impl Fixture {
    fn entries(&self) -> Vec<LogEntry> {
        self.messages
            .iter()
            .filter_map(|m| match m {
                Message::Log { device_id, entries } if device_id == "fake:iphone" => Some(entries),
                _ => None,
            })
            .flatten()
            .cloned()
            .collect()
    }

    fn request(&self, format: ExportFormat, scope: ExportScope, name: &str) -> ExportRequest {
        ExportRequest {
            device_id: "fake:iphone".into(),
            session_id: Some(self.session.clone()),
            format,
            scope,
            path: self.out.path().join(name).to_string_lossy().into_owned(),
        }
    }

    fn run(&self, request: &ExportRequest) -> (statasmon_protocol::ExportResult, Vec<f32>) {
        let mut pcts = Vec::new();
        let result = export(&self.store, None, request, |pct, _| pcts.push(pct)).unwrap();
        assert!(Path::new(&result.path).is_file());
        assert!(!Path::new(&format!("{}.part", result.path)).exists());
        assert_eq!(result.bytes, std::fs::metadata(&result.path).unwrap().len());
        assert!(pcts.windows(2).all(|w| w[0] <= w[1]), "{pcts:?}");
        assert_eq!(pcts.first().copied(), Some(0.0));
        assert_eq!(pcts.last().copied(), Some(100.0));
        (result, pcts)
    }
}

fn read_lines(path: &str) -> Vec<String> {
    std::io::BufReader::new(std::fs::File::open(path).unwrap())
        .lines()
        .map(Result::unwrap)
        .collect()
}

#[test]
fn log_format_writes_one_line_per_entry() {
    let f = fixture();
    let entries = f.entries();
    let (result, pcts) = f.run(&f.request(ExportFormat::Log, ExportScope::Session, "all.log"));
    assert_eq!(result.entries as usize, entries.len());
    let lines = read_lines(&result.path);
    assert_eq!(lines.len(), entries.len());
    let re = regex_lite(&lines[0]);
    assert!(re, "{}", lines[0]);
    assert!(lines[0].contains(&entries[0].message));
    assert!(lines[0].contains(&format!("{}[", entries[0].process)));
    assert!(pcts.len() >= 3, "{pcts:?}");

    // Selection scope: exactly the chosen sequence numbers, in order.
    let seqs: Vec<u64> = vec![entries[4].seq, entries[1].seq, entries[9].seq];
    let (result, _) = f.run(&f.request(
        ExportFormat::Log,
        ExportScope::Selection { seqs },
        "sel.log",
    ));
    assert_eq!(result.entries, 3);
    let lines = read_lines(&result.path);
    assert!(lines[0].contains(&entries[1].message));
    assert!(lines[2].contains(&entries[9].message));

    // Range scope: entries inside the range only.
    let from = entries[10].ts_ms;
    let to = entries[20].ts_ms;
    let (result, _) = f.run(&f.request(
        ExportFormat::Log,
        ExportScope::Range {
            from_ms: from,
            to_ms: to,
        },
        "range.log",
    ));
    assert_eq!(
        result.entries as usize,
        entries
            .iter()
            .filter(|e| e.ts_ms >= from && e.ts_ms <= to)
            .count()
    );
}

/// `2026-09-09 14:32:07.412  Proc[1]  LEVEL  origin  message`
fn regex_lite(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() > 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b' '
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && &b[23..25] == b"  "
        && line.split("  ").count() >= 5
}

#[test]
fn jsonl_format_has_a_header_and_full_entries() {
    let f = fixture();
    let entries = f.entries();
    let levels = vec![LogLevel::Error, LogLevel::Fault];
    let want = entries.iter().filter(|e| levels.contains(&e.level)).count();
    assert!(want > 0);
    let (result, _) = f.run(&f.request(
        ExportFormat::Jsonl,
        ExportScope::Filter {
            filter: LogFilter {
                device_id: "whatever".into(),
                session_id: Some("ignored".into()),
                levels: levels.clone(),
                ..LogFilter::default()
            },
        },
        "errors.jsonl",
    ));
    assert_eq!(result.entries as usize, want);
    let lines = read_lines(&result.path);
    assert_eq!(lines.len(), want + 1);
    let header: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(header["statasmon"], 1);
    assert_eq!(header["session"]["id"], f.session);
    assert_eq!(header["scope"]["scope"], "filter");
    for line in &lines[1..] {
        let e: LogEntry = serde_json::from_str(line).unwrap();
        assert!(levels.contains(&e.level));
        assert!(entries.contains(&e));
    }

    // App scope: the app's process names (from the process table) or the id.
    let (result, _) = f.run(&f.request(
        ExportFormat::Jsonl,
        ExportScope::App {
            app_id: "com.example.fieldkit".into(),
        },
        "app.jsonl",
    ));
    let names = f
        .store
        .reader(&f.session)
        .unwrap()
        .processes_for_app("com.example.fieldkit")
        .unwrap();
    let want = entries
        .iter()
        .filter(|e| names.iter().any(|n| n.eq_ignore_ascii_case(&e.process)))
        .count();
    assert_eq!(result.entries as usize, want);
    assert!(want > 0, "fieldkit logs exist in the fixture: {names:?}");
}

#[test]
fn csv_is_wide_for_a_session_long_for_a_range_and_processes_for_an_app() {
    let f = fixture();
    let summary = f.store.get(&f.session).unwrap();
    let reader = f.store.reader(&f.session).unwrap();
    let metrics = reader.metrics().unwrap();

    let (result, _) = f.run(&f.request(ExportFormat::Csv, ExportScope::Session, "wide.csv"));
    let lines = read_lines(&result.path);
    assert_eq!(lines[0].split(',').next(), Some("ts_ms"));
    assert_eq!(lines[0].split(',').count(), metrics.len() + 1);
    assert!(lines[0].contains(",cpu.total"));
    assert_eq!(lines.len() as u64, summary.sample_count + 1);
    assert_eq!(result.entries, summary.sample_count);
    assert!(
        lines[1..]
            .iter()
            .all(|l| l.split(',').count() == metrics.len() + 1)
    );

    let from = summary.started_at_ms + 5_000;
    let to = summary.started_at_ms + 10_000;
    let (result, _) = f.run(&f.request(
        ExportFormat::Csv,
        ExportScope::Range {
            from_ms: from,
            to_ms: to,
        },
        "long.csv",
    ));
    let lines = read_lines(&result.path);
    assert_eq!(lines[0], "ts_ms,metric,value");
    let want = reader
        .row_count(statasmon_store::Table::Samples, from, to)
        .unwrap();
    assert!(want > 0);
    assert_eq!(lines.len() as u64, want + 1);
    assert_eq!(result.entries, want);

    let (result, _) = f.run(&f.request(
        ExportFormat::Csv,
        ExportScope::App {
            app_id: "com.example.fieldkit".into(),
        },
        "app.csv",
    ));
    let lines = read_lines(&result.path);
    assert!(lines[0].starts_with("ts_ms,pid,name,app_id,cpu_pct"));
    assert!(result.entries > 0);
    assert!(
        lines[1..]
            .iter()
            .all(|l| l.contains("com.example.fieldkit"))
    );
}

#[test]
fn bundle_contains_every_part_and_the_side_files() {
    let f = fixture();
    let app_data = tempfile::tempdir().unwrap();
    let crashes = app_data.path().join("crashes").join("fake_iphone");
    std::fs::create_dir_all(&crashes).unwrap();
    std::fs::write(crashes.join("FieldKit-2026-09-09.ips"), b"{}").unwrap();
    std::fs::create_dir_all(crashes.join("nested")).unwrap();
    let pcap_dir = app_data.path().join("pcap");
    std::fs::create_dir_all(&pcap_dir).unwrap();
    std::fs::write(pcap_dir.join(format!("{}.pcap", f.session)), b"pcap").unwrap();

    let request = f.request(ExportFormat::Bundle, ExportScope::Session, "session.zip");
    let mut pcts = Vec::new();
    let result = export(&f.store, Some(app_data.path()), &request, |pct, _| {
        pcts.push(pct);
    })
    .unwrap();
    assert_eq!(pcts.last().copied(), Some(100.0));
    let file = std::fs::File::open(&result.path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "capture.pcap",
            "cores.csv",
            "crashes/FieldKit-2026-09-09.ips",
            "device.json",
            "events.json",
            "logs.jsonl",
            "marks.json",
            "metrics.csv",
            "processes.csv",
            "session.json",
        ]
    );
    let summary = f.store.get(&f.session).unwrap();
    let mut metrics = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("metrics.csv").unwrap(), &mut metrics)
        .unwrap();
    assert_eq!(metrics.lines().count() as u64, summary.sample_count + 1);
    let mut logs = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("logs.jsonl").unwrap(), &mut logs).unwrap();
    assert_eq!(logs.lines().count() as u64, summary.log_count + 1);
    let mut device = String::new();
    std::io::Read::read_to_string(&mut archive.by_name("device.json").unwrap(), &mut device)
        .unwrap();
    let device: serde_json::Value = serde_json::from_str(&device).unwrap();
    assert_eq!(device["id"], "fake:iphone");
    assert!(device["info"]["identity"].is_object());
    assert!(result.entries >= summary.sample_count + summary.log_count);

    // Without an app-data dir the side files are simply absent.
    let (result, _) = f.run(&f.request(ExportFormat::Bundle, ExportScope::Session, "plain.zip"));
    let archive = zip::ZipArchive::new(std::fs::File::open(&result.path).unwrap()).unwrap();
    assert_eq!(archive.len(), 8);
}

#[test]
fn errors_are_reported_and_leave_no_partial_file() {
    let f = fixture();
    let mut request = f.request(ExportFormat::Log, ExportScope::Session, "none.log");
    request.session_id = None;
    assert!(matches!(
        export(&f.store, None, &request, |_, _| {}),
        Err(ExportError::NoSession)
    ));
    let mut request = f.request(ExportFormat::Log, ExportScope::Session, "none.log");
    request.session_id = Some("missing".into());
    assert!(matches!(
        export(&f.store, None, &request, |_, _| {}),
        Err(ExportError::Store(
            statasmon_store::StoreError::UnknownSession(_)
        ))
    ));
    let request = f.request(
        ExportFormat::Log,
        ExportScope::Filter {
            filter: LogFilter {
                query: Some("(".into()),
                regex: true,
                ..LogFilter::default()
            },
        },
        "bad.log",
    );
    let err = export(&f.store, None, &request, |_, _| {}).unwrap_err();
    assert!(matches!(
        err,
        ExportError::Store(statasmon_store::StoreError::InvalidFilter(_))
    ));
    assert!(err.hint().is_some());
    assert!(!f.out.path().join("bad.log").exists());
    assert!(!f.out.path().join("bad.log.part").exists());
    let mut request = f.request(ExportFormat::Log, ExportScope::Session, "x.log");
    request.path = String::new();
    assert!(matches!(
        export(&f.store, None, &request, |_, _| {}),
        Err(ExportError::NoPath)
    ));
}
