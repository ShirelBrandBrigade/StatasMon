//! Record a fixture from the fake device.
//!
//! ```text
//! cargo run -p statasmon-device-fake --example record_fixture -- \
//!     fixtures/fake/warm-30s.jsonl warm 30 iphone 5
//! ```
//! Arguments: output path, scenario, seconds, device (iphone|ipad|android),
//! log lines per second. The file starts with `device_attached`,
//! `capabilities` and `info` lines followed by every message the source
//! emitted, one JSON object per line.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Write;
use std::time::Duration;

use statasmon_device::{DeviceSource, EventSink};
use statasmon_device_fake::{FakeDevice, FakeSource, Scenario};
use statasmon_protocol::{Message, SamplingConfig};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = args
        .first()
        .map_or("fixtures/fake/warm-30s.jsonl", String::as_str);
    let scenario: Scenario = args
        .get(1)
        .map_or(Ok(Scenario::Warm), |s| s.parse())
        .expect("scenario");
    let seconds: u64 = args.get(2).map_or(30, |s| s.parse().expect("seconds"));
    let kind = match args.get(3).map(String::as_str) {
        None | Some("iphone") => FakeDevice::Iphone,
        Some("ipad") => FakeDevice::Ipad {
            developer_mode: false,
        },
        Some("android") => FakeDevice::Android,
        Some(other) => panic!("unknown device {other}"),
    };
    let log_rate: f64 = args.get(4).map_or(5.0, |s| s.parse().expect("log rate"));

    let source = FakeSource::new(kind, scenario, 42, 1.0, log_rate);
    let (sink, mut rx) = EventSink::channel(1024);
    let mut file = std::io::BufWriter::new(std::fs::File::create(out).expect("create output"));
    let mut write = |m: &Message| {
        serde_json::to_writer(&mut file, m).expect("serialize");
        file.write_all(b"\n").expect("write");
    };
    write(&Message::DeviceAttached {
        device: source.summary(),
    });
    write(&Message::Capabilities {
        device_id: source.id(),
        set: source.capabilities(),
    });
    write(&Message::Info {
        device_id: source.id(),
        info: Box::new(source.info().await.expect("info")),
    });
    source
        .start(sink, SamplingConfig::default())
        .await
        .expect("start");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut count = 0usize;
    loop {
        tokio::select! {
            Some(msg) = rx.recv() => { write(&msg); count += 1; }
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }
    source.stop().await.expect("stop");
    file.flush().expect("flush");
    println!("wrote {count} messages to {out}");
}
