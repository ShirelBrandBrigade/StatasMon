#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The fake runs headless and emits every message kind on a fast cadence.

use std::collections::BTreeSet;
use std::time::Duration;

use statasmon_device::{ActionResult, DeviceSource, EventSink, allowed_metrics};
use statasmon_device_fake::{FakeDevice, FakeSource, Scenario};
use statasmon_protocol::{
    Capability, DeviceAction, ErrorCode, EventKind, Message, MetricId, SamplingConfig,
};

fn fast() -> SamplingConfig {
    SamplingConfig {
        fast_ms: 50,
        medium_ms: 100,
        slow_ms: 500,
        heavy_ms: 0,
    }
}

async fn collect(
    source: &FakeSource,
    rx: &mut tokio::sync::mpsc::Receiver<Message>,
    for_ms: u64,
) -> Vec<Message> {
    let mut out = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_millis(for_ms);
    loop {
        tokio::select! {
            Some(m) = rx.recv() => out.push(m),
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }
    source.stop().await.unwrap();
    out
}

#[tokio::test]
async fn warm_scenario_emits_every_kind_with_time_scale() {
    // time_scale 40 → thermal at 0.5 s, memory warning at 0.875 s.
    let source = FakeSource::new(FakeDevice::Iphone, Scenario::Warm, 1, 40.0, 20.0);
    let (sink, mut rx) = EventSink::channel(4096);
    source.start(sink, fast()).await.unwrap();
    let msgs = collect(&source, &mut rx, 1500).await;

    let kinds: BTreeSet<&str> = msgs
        .iter()
        .map(|m| match m {
            Message::DeviceState { .. } => "state",
            Message::Capabilities { .. } => "capabilities",
            Message::Samples { .. } => "samples",
            Message::Cores { .. } => "cores",
            Message::Processes { .. } => "processes",
            Message::Log { .. } => "log",
            Message::Event { .. } => "event",
            _ => "other",
        })
        .collect();
    for k in [
        "state",
        "capabilities",
        "samples",
        "cores",
        "processes",
        "log",
        "event",
    ] {
        assert!(kinds.contains(k), "missing {k}: {kinds:?}");
    }
    let events: Vec<EventKind> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Event { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(
        events,
        vec![EventKind::ThermalChange, EventKind::MemoryWarning]
    );

    let allowed: BTreeSet<MetricId> = allowed_metrics(&source.capabilities())
        .into_iter()
        .collect();
    let sampled: BTreeSet<MetricId> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Samples { values, .. } => Some(values.iter().map(|v| v.id)),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(sampled, allowed);
    let log_lines: usize = msgs
        .iter()
        .map(|m| match m {
            Message::Log { entries, .. } => entries.len(),
            _ => 0,
        })
        .sum();
    assert!(
        log_lines >= 20,
        "expected ~30 log lines in 1.5 s, got {log_lines}"
    );
}

#[tokio::test]
async fn ipad_tier1_emits_no_cpu_cores_or_processes() {
    let source = FakeSource::new(
        FakeDevice::Ipad {
            developer_mode: false,
        },
        Scenario::Quiet,
        1,
        1.0,
        20.0,
    );
    assert!(!source.capabilities().is_available(Capability::CpuTotal));
    let (sink, mut rx) = EventSink::channel(4096);
    source.start(sink, fast()).await.unwrap();
    let msgs = collect(&source, &mut rx, 400).await;
    assert!(msgs.iter().any(|m| matches!(m, Message::Samples { .. })));
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, Message::Cores { .. } | Message::Processes { .. }))
    );
    assert!(msgs.iter().all(|m| match m {
        Message::Samples { values, .. } => values.iter().all(|v| v.id != MetricId::CpuTotal),
        _ => true,
    }));
}

#[tokio::test]
async fn crash_scenario_and_actions() {
    let source = FakeSource::new(FakeDevice::Android, Scenario::Crash, 3, 100.0, 5.0);
    let (sink, mut rx) = EventSink::channel(4096);
    source.start(sink, fast()).await.unwrap();
    assert_eq!(
        source.run_action(DeviceAction::RefreshInfo).await.unwrap(),
        ActionResult::Done
    );
    let err = source.run_action(DeviceAction::PcapStop).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Unsupported);
    assert!(err.hint().is_some());
    let msgs = collect(&source, &mut rx, 500).await;
    assert!(msgs.iter().any(|m| matches!(m, Message::Info { .. })));
    assert!(msgs.iter().any(|m| matches!(m, Message::Event { kind: EventKind::Crash, process: Some(p), .. } if p == "com.example.fieldkit")));
    assert!(msgs.iter().any(|m| matches!(
        m,
        Message::Event {
            kind: EventKind::AppLaunch,
            ..
        }
    )));
    // Stopped: nothing more arrives.
    assert!(
        rx.try_recv().is_err() || {
            while rx.try_recv().is_ok() {}
            tokio::time::sleep(Duration::from_millis(150)).await;
            rx.try_recv().is_err()
        }
    );
}
