//! Engine-level live test: `MuxdDiscovery` attached to
//! `statasmon_core::Engine` must produce `DeviceAttached`, `Capabilities`
//! and `Info` for the connected iPad within 20 s. Skipped unless
//! STATASMON_LIVE_IOS=1.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use statasmon_core::{Engine, EngineConfig};
use statasmon_device_ios::MuxdDiscovery;
use statasmon_protocol::{ConnectionState, Message, Platform};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_engine_attaches_ios_device() {
    if std::env::var("STATASMON_LIVE_IOS").as_deref() != Ok("1") {
        eprintln!("skipped: set STATASMON_LIVE_IOS=1");
        return;
    }
    let engine = Engine::new(EngineConfig::default());
    let mut bus = engine.subscribe();
    engine.attach_discovery(Box::new(MuxdDiscovery::new()));

    let (mut attached, mut caps, mut info) = (false, false, false);
    let mut id = String::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !(attached && caps && info) {
        let msg = tokio::time::timeout_at(deadline, bus.recv())
            .await
            .expect("attach, capabilities and info within 20 s")
            .unwrap();
        match &*msg {
            Message::DeviceAttached { device } if device.platform == Platform::Ios => {
                eprintln!("attached: {device:?}");
                if device.state == ConnectionState::NeedsTrust {
                    eprintln!("device needs trust; accept the dialog and re-run");
                }
                id = device.id.clone();
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
                    "info: {} {} {}",
                    sheet.identity.marketing_name.as_deref().unwrap_or("?"),
                    sheet.software.os_version.as_deref().unwrap_or("?"),
                    sheet.hardware.chip_name.as_deref().unwrap_or("?")
                );
                info = true;
            }
            Message::DeviceState {
                device_id,
                state,
                detail,
            } if *device_id == id => {
                eprintln!("state: {state:?} {detail:?}");
            }
            _ => {}
        }
    }
    assert!(engine.devices().iter().any(|d| d.id == id));
    engine.shutdown().await;
}
