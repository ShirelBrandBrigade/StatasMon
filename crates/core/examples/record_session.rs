//! Record every bus message of an engine session with the fake devices to a
//! JSONL file, for the browser replay (apps/desktop/src/bridge/replayStream.ts)
//! and Playwright.
//!
//! ```text
//! cargo run -p statasmon-core --example record_session -- fixtures/fake/session-3dev-30s.jsonl warm 30
//! ```
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write;
use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device_fake::{FakeConfig, FakeDiscovery, Scenario};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .map_or("fixtures/fake/session-3dev-30s.jsonl", String::as_str);
    let scenario: Scenario = args
        .get(1)
        .map_or("warm", String::as_str)
        .parse()
        .expect("scenario");
    let seconds: u64 = args.get(2).map_or(30, |s| s.parse().expect("seconds"));

    let engine = Engine::new(EngineConfig::default());
    let mut rx = engine.subscribe();
    let config = FakeConfig::default()
        .with_scenario(scenario)
        .with_seed(7)
        .with_log_rate(4.0);
    engine.attach_discovery(Box::new(FakeDiscovery::new(config)));

    let mut file = std::fs::File::create(out).expect("create fixture");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut n = 0usize;
    loop {
        let msg = tokio::select! {
            m = rx.recv() => match m { Ok(m) => m, Err(_) => break },
            _ = tokio::time::sleep_until(deadline) => break,
        };
        serde_json::to_writer(&mut file, &*msg).expect("write");
        file.write_all(b"\n").expect("write");
        n += 1;
    }
    engine.shutdown().await;
    println!("wrote {n} messages to {out}");
}
