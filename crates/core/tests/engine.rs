#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Engine integration: fake discovery on a fast cadence through the bus,
//! snapshots, sessions, log queries, detach/reattach.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device::allowed_metrics;
use statasmon_device_fake::{FakeConfig, FakeDevice, FakeDiscovery, Scenario};
use statasmon_protocol::{
    Capability, EventKind, LogFilter, LogLevel, Message, MetricId, SamplingConfig, SessionStatus,
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

fn kind(m: &Message) -> &'static str {
    match m {
        Message::DeviceAttached { .. } => "attached",
        Message::DeviceDetached { .. } => "detached",
        Message::DeviceState { .. } => "state",
        Message::Capabilities { .. } => "capabilities",
        Message::Info { .. } => "info",
        Message::Samples { .. } => "samples",
        Message::Cores { .. } => "cores",
        Message::Processes { .. } => "processes",
        Message::Log { .. } => "log",
        Message::Event { .. } => "event",
        Message::Headline { .. } => "headline",
        Message::Session { .. } => "session",
        Message::Progress { .. } => "progress",
        Message::Error { .. } => "error",
    }
}

#[tokio::test]
async fn quiet_scenario_streams_every_kind_and_fills_the_snapshot() {
    let engine = Engine::new(fast_config());
    let mut rx = engine.subscribe();
    let discovery = FakeDiscovery::new(FakeConfig::default().with_scenario(Scenario::Quiet));
    let handle = discovery.handle();
    engine.attach_discovery(Box::new(discovery));

    let msgs = drain(&mut rx, 2000).await;
    let kinds: BTreeSet<&str> = msgs.iter().map(|m| kind(m)).collect();
    for k in [
        "attached",
        "info",
        "capabilities",
        "samples",
        "cores",
        "processes",
        "log",
        "state",
        "headline",
        "session",
    ] {
        assert!(kinds.contains(k), "missing {k} within 2 s; got {kinds:?}");
    }
    assert!(
        !kinds.contains("error"),
        "unexpected error messages: {msgs:?}"
    );

    let devices = engine.devices();
    assert_eq!(devices.len(), 3);
    for device in &devices {
        let snap = engine.snapshot(&device.id).unwrap();
        assert!(snap.info.is_some(), "{} has no info", device.id);
        let allowed: BTreeSet<MetricId> = allowed_metrics(&snap.capabilities).into_iter().collect();
        let have: BTreeSet<MetricId> = snap.latest.iter().map(|m| m.id).collect();
        assert_eq!(have, allowed, "{} snapshot metrics", device.id);
        assert_eq!(snap.session.status, SessionStatus::Recording);
        assert!(
            snap.session.sample_count >= 10,
            "{} samples {}",
            device.id,
            snap.session.sample_count
        );
        assert!(
            snap.session.log_count >= 10,
            "{} logs {}",
            device.id,
            snap.session.log_count
        );
        assert!(snap.latest_ts_ms.is_some());
        assert!(!snap.headline.sentence.is_empty());
        if snap.capabilities.is_available(Capability::CpuPerCore) {
            assert!(!snap.cores.is_empty(), "{} cores", device.id);
            assert!(!snap.processes.is_empty(), "{} processes", device.id);
        } else {
            assert!(snap.cores.is_empty());
            assert!(snap.processes.is_empty());
        }
    }

    // The iPad is tier 1: dashed CPU with the Developer Mode reason, headline says so.
    let ipad = engine.snapshot("fake:ipad").unwrap();
    assert_eq!(
        ipad.capabilities.reason(Capability::CpuTotal),
        Some("Developer Mode is off on this iPad")
    );
    assert_eq!(
        ipad.headline.sentence,
        "iPad (10th generation) is in limited mode"
    );
    let iphone = engine.snapshot("fake:iphone").unwrap();
    assert_eq!(
        iphone.headline.sentence,
        "Everything looks normal on iPhone 16 Pro"
    );

    // History and logs come from the rings.
    let now = statasmon_device::now_ms();
    let hist = engine
        .history(&statasmon_protocol::HistoryQuery {
            session_id: None,
            device_id: "fake:iphone".into(),
            metric: MetricId::CpuTotal,
            from_ms: now - 10_000,
            to_ms: now + 1000,
            step_ms: 0,
        })
        .unwrap();
    assert!(hist.len() >= 10);
    let page = engine
        .query_logs(
            &LogFilter {
                device_id: "fake:android".into(),
                levels: vec![
                    LogLevel::Debug,
                    LogLevel::Info,
                    LogLevel::Warning,
                    LogLevel::Error,
                ],
                ..LogFilter::default()
            },
            0,
            5,
        )
        .unwrap();
    assert!(page.total >= 5);
    assert_eq!(page.entries.len(), 5);
    assert!(page.entries.iter().all(|e| e.tag.is_some()));

    // Detach and reattach through the fake handle.
    handle.detach("fake:ipad").unwrap();
    let msgs = drain(&mut rx, 500).await;
    assert!(msgs.iter().any(
        |m| matches!(&**m, Message::DeviceDetached { device_id } if device_id == "fake:ipad")
    ));
    assert_eq!(engine.devices().len(), 2);
    handle.reattach("fake:ipad").unwrap();
    let msgs = drain(&mut rx, 500).await;
    assert!(
        msgs.iter().any(
            |m| matches!(&**m, Message::DeviceAttached { device } if device.id == "fake:ipad")
        )
    );
    assert_eq!(engine.devices().len(), 3);
    engine.shutdown().await;
}

#[tokio::test]
async fn pause_stops_storing_and_forwarding_but_keeps_live_values() {
    let engine = Engine::new(fast_config());
    let mut rx = engine.subscribe();
    let discovery = FakeDiscovery::new(
        FakeConfig::default()
            .with_scenario(Scenario::Quiet)
            .with_devices(vec![FakeDevice::Iphone]),
    );
    engine.attach_discovery(Box::new(discovery));
    drain(&mut rx, 400).await;
    let before = engine.snapshot("fake:iphone").unwrap();
    assert!(before.session.sample_count > 0);

    let paused = engine.pause("fake:iphone").unwrap();
    assert_eq!(paused.status, SessionStatus::Paused);
    let msgs = drain(&mut rx, 400).await;
    assert!(msgs.iter().any(|m| matches!(&**m, Message::Event { kind: EventKind::Connection, detail, .. } if detail == "Session paused")));
    assert!(msgs.iter().any(|m| matches!(&**m, Message::Session { session, .. } if session.status == SessionStatus::Paused)));
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(&**m, Message::Samples { .. } | Message::Log { .. })),
        "samples forwarded while paused"
    );
    let during = engine.snapshot("fake:iphone").unwrap();
    assert_eq!(
        during.session.sample_count, paused.sample_count,
        "stored while paused"
    );
    assert!(
        during.latest_ts_ms > before.latest_ts_ms,
        "live values must keep updating while paused"
    );
    assert!(
        during.session.elapsed_ms >= paused.elapsed_ms
            && during.session.elapsed_ms - paused.elapsed_ms < 100
    );

    assert!(matches!(engine.resume("fake:iphone"), Ok(s) if s.status == SessionStatus::Recording));
    assert!(matches!(
        engine.resume("fake:iphone"),
        Err(statasmon_core::EngineError::InvalidTransition { .. })
    ));
    let marked = engine.mark("fake:iphone", "Login").unwrap();
    assert_eq!(marked.marks.len(), 1);
    let msgs = drain(&mut rx, 300).await;
    assert!(msgs.iter().any(|m| matches!(&**m, Message::Event { kind: EventKind::Mark, detail, .. } if detail == "Login")));
    assert!(msgs.iter().any(|m| matches!(&**m, Message::Samples { .. })));

    let stopped = engine.stop_session("fake:iphone").unwrap();
    assert_eq!(stopped.status, SessionStatus::Stopped);
    let restarted = engine.start_session("fake:iphone").unwrap();
    assert_eq!(restarted.sample_count, 0);
    assert_ne!(restarted.id, stopped.id);
    assert!(matches!(
        engine.pause("nope"),
        Err(statasmon_core::EngineError::UnknownDevice(_))
    ));
    engine.shutdown().await;
    assert!(engine.devices().is_empty());
}

#[tokio::test]
async fn warm_scenario_drives_the_headline_through_events() {
    let engine = Engine::new(fast_config());
    let mut rx = engine.subscribe();
    let discovery = FakeDiscovery::new(
        FakeConfig::default()
            .with_scenario(Scenario::Warm)
            .with_devices(vec![FakeDevice::Android])
            .with_time_scale(50.0),
    );
    engine.attach_discovery(Box::new(discovery));
    // Thermal at 0.4 s, memory warning at 0.7 s (scaled).
    let msgs = drain(&mut rx, 1200).await;
    let events: Vec<EventKind> = msgs
        .iter()
        .filter_map(|m| match &**m {
            Message::Event { kind, .. } if !matches!(kind, EventKind::Connection) => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        events,
        vec![EventKind::ThermalChange, EventKind::MemoryWarning]
    );
    let snap = engine.snapshot("fake:android").unwrap();
    assert!(
        snap.headline
            .sentence
            .starts_with("com.example.fieldkit got a memory warning"),
        "{}",
        snap.headline.sentence
    );
    assert_eq!(snap.headline.rule, "memory_warning");
    assert!(
        snap.headline
            .pills
            .iter()
            .any(|p| p.label == "Thermal Fair")
    );
    let rules: Vec<String> = msgs
        .iter()
        .filter_map(|m| match &**m {
            Message::Headline { headline, .. } => Some(headline.rule.clone()),
            _ => None,
        })
        .collect();
    assert!(rules.contains(&"thermal_fair".to_owned()), "{rules:?}");
    assert!(rules.contains(&"memory_warning".to_owned()), "{rules:?}");
    engine.shutdown().await;
}

#[tokio::test]
async fn lagging_subscriber_does_not_break_the_engine() {
    let engine = Engine::new(fast_config());
    let mut lazy = engine.subscribe();
    let discovery = FakeDiscovery::new(FakeConfig::default().with_scenario(Scenario::Quiet));
    engine.attach_discovery(Box::new(discovery));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    // Ingest kept going: counts grew well past what one bus buffer holds.
    let snap = engine.snapshot("fake:iphone").unwrap();
    assert!(snap.session.sample_count >= 15);
    // The lazy receiver either lags or reads; it never stalls the engine.
    match lazy.try_recv() {
        Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
        other => panic!("unexpected {other:?}"),
    }
    engine.shutdown().await;
}
