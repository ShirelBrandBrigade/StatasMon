//! Smoke test: start the engine with the fake discovery, print one line per
//! device per second for 10 seconds, exit 0.
//!
//! `STATASMON_FAKE_SCENARIO=crash cargo run -p statasmon-core --example run_fake`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device_fake::FakeDiscovery;
use statasmon_protocol::MetricId;

#[tokio::main]
async fn main() {
    let discovery = FakeDiscovery::from_env();
    println!("scenario: {}", discovery.config().scenario);
    let engine = Engine::new(EngineConfig::default());
    let mut bus = engine.subscribe();
    engine.attach_discovery(Box::new(discovery));

    // Count bus traffic in the background so the bus is exercised too.
    let forwarded = Arc::new(AtomicU64::new(0));
    let counter = {
        let forwarded = Arc::clone(&forwarded);
        tokio::spawn(async move {
            loop {
                match bus.recv().await {
                    Ok(_) => forwarded.fetch_add(1, Ordering::Relaxed),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(k)) => {
                        forwarded.fetch_add(k, Ordering::Relaxed)
                    }
                    Err(_) => break,
                };
            }
        })
    };

    for second in 1..=10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        for device in engine.devices() {
            let Some(snap) = engine.snapshot(&device.id) else {
                continue;
            };
            let get = |id: MetricId| {
                snap.latest
                    .iter()
                    .find(|m| m.id == id)
                    .map_or("—".to_owned(), |m| format!("{:.1}", m.value))
            };
            let mem_gb = snap
                .latest
                .iter()
                .find(|m| m.id == MetricId::MemUsed)
                .map_or("—".to_owned(), |m| format!("{:.2} GB", m.value / 1e9));
            println!(
                "t+{second:>2}s {:<13} {:<7} cpu {:>5} %  mem {:>8}  thermal {:>3}  bat {:>5} %  logs {:>4}  samples {:>3}  [{:?}] {}",
                device.id,
                format!("{:?}", snap.session.status).to_lowercase(),
                get(MetricId::CpuTotal),
                mem_gb,
                get(MetricId::ThermalState),
                get(MetricId::BatPercent),
                snap.session.log_count,
                snap.session.sample_count,
                snap.headline.severity,
                snap.headline.sentence,
            );
        }
    }
    engine.shutdown().await;
    counter.abort();
    println!("bus messages: {}", forwarded.load(Ordering::Relaxed));
}
