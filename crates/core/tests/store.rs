#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Engine + store: the fake devices record into a temp store root, the
//! sessions are listed and queried; the recording cap splits a session;
//! a store that cannot open degrades with a notice.

use std::sync::Arc;
use std::time::Duration;

use statasmon_core::{Engine, EngineConfig, StoreConfig};
use statasmon_device_fake::{FakeConfig, FakeDevice, FakeDiscovery, Scenario};
use statasmon_protocol::{
    ErrorCode, EventKind, HistoryQuery, LogFilter, Message, MetricId, SamplingConfig, SessionStatus,
};
use tokio::sync::broadcast::error::RecvError;

fn fast_config() -> EngineConfig {
    EngineConfig::default().with_sampling(SamplingConfig {
        fast_ms: 50,
        medium_ms: 100,
        slow_ms: 500,
        heavy_ms: 0,
    })
}

/// Store errors are logged, not returned: print them so a failing runner
/// (Windows, for one) says what went wrong.
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("warn,statasmon_store=debug")
            }),
        )
        .with_test_writer()
        .try_init();
}

async fn drain(
    rx: &mut tokio::sync::broadcast::Receiver<Arc<Message>>,
    for_ms: u64,
) -> Vec<Arc<Message>> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(for_ms);
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(m) => out.push(m),
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            },
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }
    out
}

#[tokio::test]
async fn records_three_seconds_and_lists_one_session_per_device() {
    init_tracing();
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(fast_config().with_store(StoreConfig::new(dir.path())));
    let store = engine.store().expect("store opened");
    let mut rx = engine.subscribe();
    let discovery = FakeDiscovery::new(FakeConfig::default().with_scenario(Scenario::Quiet));
    engine.attach_discovery(Box::new(discovery));
    let msgs = drain(&mut rx, 3000).await;

    // Session ids on the bus are the store's ids.
    let ids: Vec<String> = msgs
        .iter()
        .filter_map(|m| match &**m {
            Message::Session { session, .. } => session.id.clone(),
            _ => None,
        })
        .collect();
    assert!(!ids.is_empty());
    assert!(
        ids.iter()
            .all(|id| id.starts_with("fake_") && !id.contains(':')),
        "{ids:?}"
    );

    // While live: listed as live with the in-memory counters. The index is
    // written by the store's thread, so let it catch up first (Windows
    // runners are slow enough to list nothing otherwise).
    let store = engine.store().unwrap();
    tokio::task::spawn_blocking({
        let store = Arc::clone(&store);
        move || store.flush()
    })
    .await
    .unwrap()
    .unwrap();
    let live = engine.session_summaries().unwrap();
    assert_eq!(live.len(), 3, "{live:?}");
    assert!(live.iter().all(|s| s.live && s.ended_at_ms.is_none()));
    let snap = engine.snapshot("fake:iphone").unwrap();
    assert_eq!(snap.session.status, SessionStatus::Recording);
    let iphone_id = snap.session.id.clone().unwrap();
    assert!(live.iter().any(|s| s.id == iphone_id));

    // Stored history and logs are reachable through the engine while live.
    tokio::task::spawn_blocking({
        let store = Arc::clone(&store);
        move || store.flush()
    })
    .await
    .unwrap()
    .unwrap();
    let now = statasmon_device::now_ms();
    let stored = engine
        .history(&HistoryQuery {
            device_id: "fake:iphone".into(),
            session_id: Some(iphone_id.clone()),
            metric: MetricId::CpuTotal,
            from_ms: now - 10_000,
            to_ms: now + 1000,
            step_ms: 0,
        })
        .unwrap();
    assert!(stored.len() >= 10, "{}", stored.len());
    let live_ring = engine
        .history(&HistoryQuery {
            device_id: "fake:iphone".into(),
            session_id: None,
            metric: MetricId::CpuTotal,
            from_ms: now - 10_000,
            to_ms: now + 1000,
            step_ms: 0,
        })
        .unwrap();
    assert!(live_ring.len() >= stored.len().saturating_sub(5));
    let page = engine
        .query_logs(
            &LogFilter {
                device_id: "fake:iphone".into(),
                session_id: Some(iphone_id.clone()),
                ..LogFilter::default()
            },
            0,
            5,
        )
        .unwrap();
    assert!(page.total >= 5);
    assert_eq!(page.entries.len(), 5);
    assert_eq!(
        engine
            .count_logs(&LogFilter {
                device_id: "fake:iphone".into(),
                session_id: Some(iphone_id.clone()),
                ..LogFilter::default()
            })
            .unwrap(),
        page.total
    );

    // Stop every device: the sessions end and keep their counts.
    for device in engine.devices() {
        engine.stop_session(&device.id).unwrap();
    }
    tokio::task::spawn_blocking({
        let store = Arc::clone(&store);
        move || store.flush()
    })
    .await
    .unwrap()
    .unwrap();
    let stopped = engine.session_summaries().unwrap();
    assert_eq!(stopped.len(), 3);
    for s in &stopped {
        assert!(!s.live, "{}", s.id);
        assert!(s.ended_at_ms.is_some());
        assert!(s.sample_count > 0, "{} samples", s.id);
        assert!(s.log_count > 0, "{} logs", s.id);
        assert!(s.event_count > 0, "{} events", s.id);
        assert!(s.size_bytes > 0);
        assert!(
            s.elapsed_ms >= 2000 && s.elapsed_ms < 10_000,
            "{}",
            s.elapsed_ms
        );
        let detail = engine.session_detail(&s.id).unwrap();
        assert!(detail.info.is_some(), "{} info", s.id);
        assert!(!detail.metrics.is_empty());
    }
    assert_eq!(store.dropped(), 0);

    // Delete one; the rest stay.
    engine.delete_session(&iphone_id).await.unwrap();
    assert_eq!(engine.session_summaries().unwrap().len(), 2);
    assert!(engine.session_detail(&iphone_id).is_err());
    engine.shutdown().await;
}

#[tokio::test]
async fn splits_the_session_at_the_recording_cap() {
    init_tracing();
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::new(
        fast_config().with_store(StoreConfig::new(dir.path()).with_max_session_ms(2000)),
    );
    let mut rx = engine.subscribe();
    let discovery = FakeDiscovery::new(
        FakeConfig::default()
            .with_scenario(Scenario::Quiet)
            .with_devices(vec![FakeDevice::Iphone]),
    );
    engine.attach_discovery(Box::new(discovery));
    let msgs = drain(&mut rx, 5200).await;
    let ids: Vec<String> = msgs
        .iter()
        .filter_map(|m| match &**m {
            Message::Session { session, .. } => session.id.clone(),
            _ => None,
        })
        .fold(Vec::new(), |mut acc, id| {
            if acc.last() != Some(&id) {
                acc.push(id);
            }
            acc
        });
    assert!(ids.len() >= 2, "no split within 5 s: {ids:?}");
    assert!(msgs.iter().any(|m| matches!(&**m, Message::Event { kind: EventKind::Connection, detail, .. } if detail.starts_with("Session split"))));
    let store = engine.store().unwrap();
    tokio::task::spawn_blocking({
        let store = Arc::clone(&store);
        move || store.flush()
    })
    .await
    .unwrap()
    .unwrap();
    // The split-off session is ended by the store thread; give it up to 5 s.
    let mut sessions = engine.session_summaries().unwrap();
    for _ in 0..50 {
        let ended_all = sessions.iter().filter(|s| !s.live).count() >= 1
            && sessions
                .iter()
                .filter(|s| !s.live)
                .all(|s| s.ended_at_ms.is_some());
        if ended_all {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            move || store.flush()
        })
        .await
        .unwrap()
        .unwrap();
        sessions = engine.session_summaries().unwrap();
    }
    assert!(sessions.len() >= 2, "{sessions:?}");
    let ended: Vec<_> = sessions.iter().filter(|s| !s.live).collect();
    assert!(!ended.is_empty());
    for s in &ended {
        assert!(s.ended_at_ms.is_some(), "{s:?}");
        assert!(
            s.elapsed_ms >= 2000 && s.elapsed_ms < 3500,
            "{} elapsed {}",
            s.id,
            s.elapsed_ms
        );
        assert!(s.sample_count > 0);
    }
    assert_eq!(sessions.iter().filter(|s| s.live).count(), 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn unopenable_store_degrades_with_a_notice() {
    init_tracing();
    let file = tempfile::NamedTempFile::new().unwrap();
    // The root is an existing file: the directories cannot be created.
    let engine = Engine::new(fast_config().with_store(StoreConfig::new(file.path())));
    assert!(engine.store().is_none());
    let notices = engine.notices();
    assert!(
        notices.iter().any(|m| matches!(
            m,
            Message::Error {
                code: ErrorCode::Io,
                device_id: None,
                ..
            }
        )),
        "{notices:?}"
    );
    assert!(engine.session_summaries().unwrap().is_empty());
    engine.shutdown().await;
}
