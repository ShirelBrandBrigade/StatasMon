#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Replays the committed fixture at high speed and checks timing and content.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use statasmon_device::{DeviceSource, EventSink};
use statasmon_device_fake::{FakeSource, Fixture};
use statasmon_protocol::{ConnectionState, EventKind, Message, SamplingConfig};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/fake/warm-30s.jsonl")
}

#[test]
fn fixture_is_small_and_parses() {
    let bytes = std::fs::metadata(fixture_path()).unwrap().len();
    assert!(bytes <= 200 * 1024, "fixture is {bytes} bytes");
    let f = Fixture::load(fixture_path()).unwrap();
    assert_eq!(f.summary.id, "fake:iphone");
    assert!(f.info.is_some());
    assert!(
        f.duration_ms() >= 25_000 && f.duration_ms() <= 31_000,
        "{}",
        f.duration_ms()
    );
    assert!(f.messages.iter().any(|m| matches!(
        m,
        Message::Event {
            kind: EventKind::ThermalChange,
            ..
        }
    )));
}

#[tokio::test]
async fn replays_with_scaled_timing_and_rebased_timestamps() {
    let source = FakeSource::from_fixture(fixture_path(), 100.0).unwrap();
    assert_eq!(source.id(), "fake:iphone");
    let (sink, mut rx) = EventSink::channel(8192);
    let start = Instant::now();
    let start_ms = statasmon_device::now_ms();
    source.start(sink, SamplingConfig::default()).await.unwrap();
    let mut msgs = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        tokio::select! {
            Some(m) = rx.recv() => {
                let done = matches!(m, Message::DeviceState { state: ConnectionState::Disconnected, .. });
                msgs.push(m);
                if done { break; }
            }
            _ = tokio::time::sleep_until(deadline) => panic!("replay did not finish"),
        }
    }
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(250) && took < Duration::from_secs(3),
        "took {took:?}"
    );
    let samples = msgs
        .iter()
        .filter(|m| matches!(m, Message::Samples { .. }))
        .count();
    assert!(samples >= 25, "{samples} samples");
    assert!(msgs.iter().any(|m| matches!(
        m,
        Message::Event {
            kind: EventKind::ThermalChange,
            ..
        }
    )));
    for m in &msgs {
        if let Some(ts) = statasmon_device_fake::message_ts(m) {
            assert!(
                ts >= start_ms - 5 && ts <= start_ms + 3000,
                "ts {ts} not rebased"
            );
        }
    }
    source.stop().await.unwrap();
}
