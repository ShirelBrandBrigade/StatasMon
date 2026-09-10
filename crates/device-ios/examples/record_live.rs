//! Record every bus message from the REAL devices (iOS over usbmuxd, Android
//! over adb) to a JSONL file for browser replay and visual testing.
//!
//! ```text
//! cargo run -p statasmon-device-ios --example record_live -- fixtures/fake/live-ipad.jsonl 120
//! ```
//! The output may contain the device name and other identifiers: keep it
//! out of git (fixtures/fake/live-*.jsonl is ignored).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::Write;
use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device_android::adb::AdbDiscovery;
use statasmon_device_ios::muxd::MuxdDiscovery;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .map_or("fixtures/fake/live-ipad.jsonl", String::as_str);
    let seconds: u64 = args.get(1).map_or(120, |s| s.parse().expect("seconds"));
    let engine = Engine::new(EngineConfig::default());
    let mut rx = engine.subscribe();
    engine.attach_discovery(Box::new(MuxdDiscovery::new()));
    engine.attach_discovery(Box::new(AdbDiscovery::from_locator()));
    let mut file = std::fs::File::create(out).expect("create");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut n = 0usize;
    loop {
        let msg = tokio::select! {
            m = rx.recv() => match m { Ok(m) => m, Err(_) => break },
            _ = tokio::time::sleep_until(deadline) => break,
        };
        if n.is_multiple_of(200) {
            eprintln!(
                "{n} messages… latest: {}",
                serde_json::to_string(&*msg)
                    .unwrap_or_default()
                    .chars()
                    .take(120)
                    .collect::<String>()
            );
        }
        serde_json::to_writer(&mut file, &*msg).expect("write");
        file.write_all(b"\n").expect("write");
        n += 1;
    }
    engine.shutdown().await;
    println!("wrote {n} messages to {out}");
}
