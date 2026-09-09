#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Store round-trip: replay the recorded fake session, then query history
//! at several steps, logs with every filter field, events, marks,
//! list/open/delete; downsampling on a synthetic series; retention;
//! recovery; the non-blocking writer queue.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use statasmon_protocol::{
    ConnectionState, DeviceSummary, LogFilter, LogLevel, Mark, Message, MetricId, MetricValue,
    Platform, Transport,
};
use statasmon_store::{Store, StoreConfig, StoreError, parse_jsonl, replay_jsonl};

fn fixture() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/fake/session-3dev-30s.jsonl"
    ))
    .unwrap()
}

fn open(dir: &Path) -> Arc<Store> {
    Store::open(StoreConfig::new(dir)).unwrap()
}

fn device(id: &str) -> DeviceSummary {
    DeviceSummary {
        id: id.into(),
        platform: Platform::Fake,
        name: "Test".into(),
        model_name: "Model".into(),
        transport: Transport::Fake,
        state: ConnectionState::Tier2,
    }
}

fn samples(device_id: &str, ts_ms: i64, values: &[(MetricId, f64)]) -> Arc<Message> {
    Arc::new(Message::Samples {
        device_id: device_id.into(),
        ts_ms,
        values: values
            .iter()
            .map(|(id, value)| MetricValue {
                id: *id,
                value: *value,
            })
            .collect(),
    })
}

fn filter() -> LogFilter {
    LogFilter::default()
}

#[test]
fn replays_the_fixture_and_answers_every_query() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let text = fixture();
    let msgs = parse_jsonl(&text).unwrap();
    let ids = replay_jsonl(&store, &text).unwrap();
    assert_eq!(ids.len(), 3, "{ids:?}");
    assert!(ids.iter().all(|id| id.contains('-') && !id.contains(':')));

    // Index counters match the fixture exactly.
    let list = store.list().unwrap();
    assert_eq!(list.len(), 3);
    for s in &list {
        assert!(!s.live);
        assert!(s.ended_at_ms.is_some());
        assert!(s.size_bytes > 0);
        let want_samples = msgs
            .iter()
            .filter(
                |m| matches!(m, Message::Samples { device_id, .. } if *device_id == s.device_id),
            )
            .count() as u64;
        let want_logs: u64 = msgs
            .iter()
            .filter_map(|m| match m {
                Message::Log { device_id, entries } if *device_id == s.device_id => {
                    Some(entries.len() as u64)
                }
                _ => None,
            })
            .sum();
        let want_events = msgs
            .iter()
            .filter(|m| matches!(m, Message::Event { device_id, .. } if *device_id == s.device_id))
            .count() as u64;
        assert_eq!(s.sample_count, want_samples, "{} samples", s.id);
        assert_eq!(s.log_count, want_logs, "{} logs", s.id);
        assert_eq!(s.event_count, want_events, "{} events", s.id);
        let stats = store.stats(&s.id).unwrap();
        assert_eq!(stats.log_count, s.log_count);
    }

    // Open: info, metrics, events.
    let iphone = ids.iter().find(|i| i.starts_with("fake_iphone")).unwrap();
    let detail = store.open_session(iphone).unwrap();
    assert_eq!(detail.summary.device_id, "fake:iphone");
    assert_eq!(detail.summary.platform, Platform::Ios);
    assert!(detail.info.is_some(), "info document stored");
    assert!(detail.metrics.contains(&MetricId::CpuTotal));
    assert!(!detail.events.is_empty());
    assert!(detail.marks.is_empty());

    // History: raw, then downsampled at several steps, each bucket the
    // average of its raw points.
    let from = detail.summary.started_at_ms - 1000;
    let to = detail.summary.ended_at_ms.unwrap() + 1000;
    let raw = store
        .history(iphone, MetricId::CpuTotal, from, to, 0)
        .unwrap();
    assert_eq!(raw.len() as u64, detail.summary.sample_count);
    assert!(raw.windows(2).all(|w| w[0].ts_ms < w[1].ts_ms));
    for step in [2_000u32, 5_000, 10_000] {
        let ds = store
            .history(iphone, MetricId::CpuTotal, from, to, step)
            .unwrap();
        assert!(
            !ds.is_empty() && ds.len() < raw.len(),
            "step {step}: {}",
            ds.len()
        );
        for p in &ds {
            let bucket: Vec<f64> = raw
                .iter()
                .filter(|r| {
                    from + (r.ts_ms - from).div_euclid(i64::from(step)) * i64::from(step) == p.ts_ms
                })
                .map(|r| r.value)
                .collect();
            assert!(!bucket.is_empty());
            let mean = bucket.iter().sum::<f64>() / bucket.len() as f64;
            assert!(
                (mean - p.value).abs() < 1e-9,
                "step {step} bucket {}",
                p.ts_ms
            );
        }
    }
    let window = store
        .history(iphone, MetricId::CpuTotal, raw[5].ts_ms, raw[9].ts_ms, 0)
        .unwrap();
    assert_eq!(window.len(), 5);
    assert!(
        store
            .history(iphone, MetricId::BatCycles, from, to, 0)
            .unwrap()
            .len()
            <= raw.len()
    );

    // Cores.
    let cores = store.cores_history(iphone, from, to).unwrap();
    assert!(!cores.is_empty());
    assert!(cores.iter().all(|(_, c)| !c.is_empty()));

    // Logs: every filter field, checked against the raw entries.
    let entries: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Log { device_id, entries } if device_id == "fake:iphone" => Some(entries),
            _ => None,
        })
        .flatten()
        .cloned()
        .collect();
    assert!(entries.len() > 20);
    let all = store.logs(iphone, &filter(), 0, 10_000).unwrap();
    assert_eq!(all.total as usize, entries.len());
    assert_eq!(all.entries.len(), entries.len());
    assert_eq!(all.entries[0], entries[0], "entries round-trip exactly");
    assert_eq!(all.first_seq, entries[0].seq);

    let page = store.logs(iphone, &filter(), 3, 4).unwrap();
    assert_eq!(page.entries.len(), 4);
    assert_eq!(page.first_seq, entries[3].seq);
    assert_eq!(page.total as usize, entries.len());

    let process = entries[0].process.clone();
    let by_process = store
        .logs(
            iphone,
            &LogFilter {
                processes: vec![process.to_uppercase()],
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    assert_eq!(
        by_process.total as usize,
        entries
            .iter()
            .filter(|e| e.process.eq_ignore_ascii_case(&process))
            .count()
    );
    assert!(by_process.total > 0);

    let levels = vec![LogLevel::Error, LogLevel::Fault];
    let by_level = store
        .logs(
            iphone,
            &LogFilter {
                levels: levels.clone(),
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    assert_eq!(
        by_level.total as usize,
        entries.iter().filter(|e| levels.contains(&e.level)).count()
    );
    assert!(by_level.entries.iter().all(|e| levels.contains(&e.level)));

    let mid = entries[entries.len() / 3].ts_ms;
    let end = entries[entries.len() * 2 / 3].ts_ms;
    let by_time = store
        .logs(
            iphone,
            &LogFilter {
                from_ms: Some(mid),
                to_ms: Some(end),
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    assert_eq!(
        by_time.total as usize,
        entries
            .iter()
            .filter(|e| e.ts_ms >= mid && e.ts_ms <= end)
            .count()
    );
    assert!(by_time.total > 0);

    // Substring: a word from a real message, upper-cased to prove case-insensitivity.
    let word = entries
        .iter()
        .flat_map(|e| e.message.split_whitespace())
        .find(|w| w.len() >= 4 && w.chars().all(|c| c.is_ascii_alphabetic()))
        .unwrap()
        .to_owned();
    let by_query = store
        .logs(
            iphone,
            &LogFilter {
                query: Some(word.to_uppercase()),
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    let lower = word.to_lowercase();
    let want: BTreeSet<u64> = entries
        .iter()
        .filter(|e| {
            e.message.to_lowercase().contains(&lower)
                || e.tag
                    .as_deref()
                    .is_some_and(|t| t.to_lowercase().contains(&lower))
                || e.subsystem
                    .as_deref()
                    .is_some_and(|s| s.to_lowercase().contains(&lower))
        })
        .map(|e| e.seq)
        .collect();
    let got: BTreeSet<u64> = by_query.entries.iter().map(|e| e.seq).collect();
    assert_eq!(got, want, "substring {word:?}");
    assert!(!got.is_empty());

    // Short queries (< 3 chars) use LIKE; wildcards are literal.
    let pct = store
        .logs(
            iphone,
            &LogFilter {
                query: Some("%".into()),
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    assert_eq!(
        pct.total as usize,
        entries.iter().filter(|e| e.message.contains('%')).count()
    );

    // Regex.
    let re = regex::RegexBuilder::new(&format!("^{word}|\\d{{3,}}"))
        .case_insensitive(true)
        .build()
        .unwrap();
    let by_regex = store
        .logs(
            iphone,
            &LogFilter {
                query: Some(format!("^{word}|\\d{{3,}}")),
                regex: true,
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    let want: BTreeSet<u64> = entries
        .iter()
        .filter(|e| {
            re.is_match(&e.message)
                || e.tag.as_deref().is_some_and(|t| re.is_match(t))
                || e.subsystem.as_deref().is_some_and(|s| re.is_match(s))
        })
        .map(|e| e.seq)
        .collect();
    let got: BTreeSet<u64> = by_regex.entries.iter().map(|e| e.seq).collect();
    assert_eq!(got, want);
    assert!(matches!(
        store.logs(
            iphone,
            &LogFilter {
                query: Some("(".into()),
                regex: true,
                ..filter()
            },
            0,
            10
        ),
        Err(StoreError::InvalidFilter(_))
    ));

    // Buffers (Android).
    let android = ids.iter().find(|i| i.starts_with("fake_android")).unwrap();
    let android_entries: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Log { device_id, entries } if device_id == "fake:android" => Some(entries),
            _ => None,
        })
        .flatten()
        .collect();
    let buffer = android_entries
        .iter()
        .find_map(|e| e.buffer.clone())
        .expect("android entries carry a buffer");
    let by_buffer = store
        .logs(
            android,
            &LogFilter {
                buffers: vec![buffer.to_uppercase()],
                ..filter()
            },
            0,
            10_000,
        )
        .unwrap();
    assert_eq!(
        by_buffer.total as usize,
        android_entries
            .iter()
            .filter(|e| e
                .buffer
                .as_deref()
                .is_some_and(|b| b.eq_ignore_ascii_case(&buffer)))
            .count()
    );
    assert!(by_buffer.total > 0);
    let reader = store.reader(android).unwrap();
    eprintln!("fts5 trigram available: {}", reader.fts_enabled());
    let counts = reader.counts().unwrap();
    assert_eq!(counts.logs as usize, android_entries.len());

    // Delete.
    let path = dir.path().join("sessions").join(format!("{iphone}.sqlite"));
    assert!(path.is_file());
    store.delete(iphone).unwrap();
    assert!(!path.exists());
    assert_eq!(store.list().unwrap().len(), 2);
    assert!(matches!(
        store.open_session(iphone),
        Err(StoreError::UnknownSession(_))
    ));
    assert!(matches!(
        store.delete("nope"),
        Err(StoreError::UnknownSession(_))
    ));
    let totals = store.store_stats().unwrap();
    assert_eq!(totals.session_count, 2);
    assert!(totals.total_bytes > 0);
}

#[test]
fn downsamples_by_bucket_average_and_caps_points() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let id = store
        .begin("fake:x-synthetic", &device("fake:x"), None, 0)
        .unwrap();
    assert_eq!(id, "fake_x-synthetic");
    for i in 0..10 {
        assert!(store.write(&samples(
            "fake:x",
            i * 1000,
            &[(MetricId::CpuTotal, i as f64)]
        )));
    }
    store.flush().unwrap();
    let pts = store
        .history(&id, MetricId::CpuTotal, 0, 9000, 4000)
        .unwrap();
    let got: Vec<(i64, f64)> = pts.iter().map(|p| (p.ts_ms, p.value)).collect();
    assert_eq!(got, vec![(0, 1.5), (4000, 5.5), (8000, 8.5)]);
    let raw = store
        .history(&id, MetricId::CpuTotal, 2000, 5000, 0)
        .unwrap();
    assert_eq!(raw.len(), 4);
    assert!(
        store
            .history(&id, MetricId::MemUsed, 0, 9000, 1000)
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .history(&id, MetricId::CpuTotal, 9000, 0, 0)
            .unwrap()
            .is_empty()
    );
    assert!(store.is_live(&id));
    let live = store.list().unwrap();
    assert!(live[0].live);
    assert_eq!(live[0].sample_count, 10);
    assert_eq!(live[0].elapsed_ms, 9000);

    // 12 000 raw points at 1 Hz: a raw query is capped to ≤ 5 000 buckets.
    for i in 10..12_000 {
        while !store.write(&samples("fake:x", i * 1000, &[(MetricId::CpuTotal, 1.0)])) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    store.flush().unwrap();
    let capped = store
        .history(&id, MetricId::CpuTotal, 0, 12_000_000, 0)
        .unwrap();
    assert!(
        capped.len() <= 5000 && capped.len() >= 4000,
        "{}",
        capped.len()
    );
    let capped = store
        .history(&id, MetricId::CpuTotal, 0, 12_000_000, 1)
        .unwrap();
    assert!(capped.len() <= 5000, "{}", capped.len());
    store.end(&id, 12_000_000, 12_000_000).unwrap();
    store.flush().unwrap();
    assert!(!store.is_live(&id));
    assert_eq!(store.get(&id).unwrap().elapsed_ms, 12_000_000);
}

#[test]
fn marks_ids_and_info_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(dir.path());
    let a = store
        .begin("fake:y-20260909-120000", &device("fake:y"), None, 10)
        .unwrap();
    let b = store
        .begin("fake:y-20260909-120000", &device("fake:y2"), None, 20)
        .unwrap();
    assert_eq!(a, "fake_y-20260909-120000");
    assert_eq!(b, "fake_y-20260909-120000-2");
    assert_eq!(store.active_session("fake:y").as_deref(), Some(a.as_str()));
    assert_eq!(store.active_session("fake:y2").as_deref(), Some(b.as_str()));
    store
        .mark(
            &b,
            &Mark {
                ts_ms: 25,
                label: "Login".into(),
            },
        )
        .unwrap();
    let info = statasmon_protocol::DeviceInfo {
        identity: statasmon_protocol::DeviceIdentity {
            name: Some("Named".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    store.set_info(&b, &info).unwrap();
    store.end(&b, 30, 10).unwrap();
    store.flush().unwrap();
    let detail = store.open_session(&b).unwrap();
    assert_eq!(
        detail.marks,
        vec![Mark {
            ts_ms: 25,
            label: "Login".into()
        }]
    );
    assert_eq!(detail.summary.mark_count, 1);
    assert_eq!(detail.info.unwrap().identity.name.as_deref(), Some("Named"));
    assert!(store.marks(&b).unwrap().len() == 1);
    // A live session cannot be deleted.
    assert!(matches!(store.delete(&a), Err(StoreError::Live(_))));
    // Beginning another session for the same device ends the live one.
    let c = store
        .begin("fake:y-later", &device("fake:y"), None, 40)
        .unwrap();
    store.write(&samples("fake:y", 41, &[(MetricId::CpuTotal, 1.0)]));
    store.flush().unwrap();
    let a_row = store.get(&a).unwrap();
    assert_eq!(a_row.ended_at_ms, Some(40));
    assert!(!a_row.live);
    assert!(store.get(&c).unwrap().live);
    assert_eq!(store.get(&c).unwrap().sample_count, 1);
}

#[test]
fn retention_deletes_old_sessions_and_enforces_the_size_cap() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path()).with_keep_days(30)).unwrap();
    let now = statasmon_store::now_ms();
    let day = 86_400_000;
    let old = store
        .begin("old", &device("fake:z"), None, now - 40 * day)
        .unwrap();
    store.write(&samples(
        "fake:z",
        now - 40 * day,
        &[(MetricId::CpuTotal, 1.0)],
    ));
    store.end(&old, now - 40 * day + 1000, 1000).unwrap();
    let recent = store
        .begin("recent", &device("fake:z"), None, now - day)
        .unwrap();
    store.write(&samples("fake:z", now - day, &[(MetricId::CpuTotal, 1.0)]));
    store.end(&recent, now - day + 1000, 1000).unwrap();
    let live = store.begin("live", &device("fake:z"), None, now).unwrap();
    store.flush().unwrap();
    assert_eq!(store.list().unwrap().len(), 3);
    store.sweep().unwrap();
    let ids: Vec<String> = store.list().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![live.clone(), recent.clone()]);
    assert!(
        !dir.path()
            .join("sessions")
            .join(format!("{old}.sqlite"))
            .exists()
    );

    // Size cap: the oldest ended session goes first; live ones never.
    drop(store);
    let store = Store::open(
        StoreConfig::new(dir.path())
            .with_keep_days(30)
            .with_max_total_bytes(Some(1)),
    )
    .unwrap();
    // Reopening ends the session left live by the previous instance.
    let list = store.list().unwrap();
    assert!(list.iter().all(|s| s.ended_at_ms.is_some() && !s.live));
    let live2 = store.begin("live2", &device("fake:z"), None, now).unwrap();
    store.sweep().unwrap();
    let ids: Vec<String> = store.list().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![live2]);
}

#[test]
fn full_writer_queue_drops_and_counts_instead_of_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(StoreConfig::new(dir.path()).with_writer_capacity(8)).unwrap();
    let gate = store.writer_gate();
    let id = store.begin("q", &device("fake:q"), None, 0).unwrap();
    let held = gate.lock();
    let started = std::time::Instant::now();
    let total = 10_000;
    let mut accepted = 0u64;
    for i in 0..total {
        if store.write(&samples("fake:q", i, &[(MetricId::CpuTotal, 0.0)])) {
            accepted += 1;
        }
    }
    let took = started.elapsed();
    assert!(
        took < std::time::Duration::from_secs(2),
        "write blocked: {took:?}"
    );
    let dropped = store.dropped();
    assert!(dropped > 0, "nothing dropped with a blocked writer");
    assert_eq!(accepted + dropped, total as u64);
    drop(held);
    store.end(&id, total, total).unwrap();
    store.flush().unwrap();
    assert_eq!(store.stats(&id).unwrap().sample_count, accepted);
}
