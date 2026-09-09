//! Engine-level live test: `AdbDiscovery` attached to `statasmon_core::Engine`
//! must produce `DeviceAttached`, `Capabilities` and `Info` for the
//! connected device within 15 s. Skipped unless STATASMON_LIVE_ANDROID and
//! STATASMON_ADB are set. Samples are not asserted (they need the samplers
//! from steps 2.3–2.6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device_android::adb::{AdbDiscovery, AdbServer};
use statasmon_protocol::{Message, Platform};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_engine_attaches_android_device() {
    let (Ok(serial), Ok(adb)) = (
        std::env::var("STATASMON_LIVE_ANDROID"),
        std::env::var("STATASMON_ADB"),
    ) else {
        eprintln!("skipped: set STATASMON_LIVE_ANDROID and STATASMON_ADB");
        return;
    };
    let engine = Engine::new(EngineConfig::default());
    let mut bus = engine.subscribe();
    let discovery = AdbDiscovery::new(AdbServer::new(&adb)).with_poll(Duration::from_millis(500));
    let handle = discovery.handle();
    engine.attach_discovery(Box::new(discovery));

    let id = format!("android:{serial}");
    let (mut attached, mut caps, mut info) = (false, false, false);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !(attached && caps && info) {
        let msg = tokio::time::timeout_at(deadline, bus.recv())
            .await
            .expect("attach, capabilities and info within 15 s")
            .unwrap();
        match &*msg {
            Message::DeviceAttached { device } if device.id == id => {
                assert_eq!(device.platform, Platform::Android);
                eprintln!("attached: {device:?}");
                attached = true;
            }
            Message::Capabilities { device_id, set } if *device_id == id => {
                assert!(!set.entries.is_empty());
                caps = true;
            }
            Message::Info {
                device_id,
                info: sheet,
            } if *device_id == id => {
                eprintln!(
                    "info: {} api {} {}",
                    sheet.identity.marketing_name.as_deref().unwrap_or("?"),
                    sheet.software.api_level.unwrap_or(0),
                    sheet.hardware.chip_name.as_deref().unwrap_or("?")
                );
                assert!(sheet.software.api_level.is_some());
                info = true;
            }
            _ => {}
        }
    }
    assert!(engine.devices().iter().any(|d| d.id == id));
    assert!(handle.adb_path().is_some());
    engine.shutdown().await;
}
