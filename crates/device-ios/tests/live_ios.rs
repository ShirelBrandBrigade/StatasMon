//! Live tier-1 checks against a connected iPad (10th generation, iPadOS
//! 17.4.1). Skipped unless STATASMON_LIVE_IOS=1.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use idevice::IdeviceService;
use statasmon_device::{DeviceSource, EventSink};
use statasmon_device_ios::battery::{BatteryBasic, BatteryDetail, fetch_ioregistry};
use statasmon_device_ios::info::{ConnectionFacts, read_info};
use statasmon_device_ios::lockdown::Lockdown;
use statasmon_device_ios::muxd::{IosProvider, read_pair_record, validate_pair_record};
use statasmon_device_ios::storage::{fetch_app_sizes, fetch_disk_usage};
use statasmon_device_ios::{IosSeed, IosSource, MuxdDiscovery};
use statasmon_protocol::{
    Capability, ConnectionState, Message, MetricId, SamplingConfig, Transport,
};

fn live() -> bool {
    if std::env::var("STATASMON_LIVE_IOS").as_deref() == Ok("1") {
        true
    } else {
        eprintln!("skipped: set STATASMON_LIVE_IOS=1");
        false
    }
}

#[tokio::test]
async fn live_discovery_pairing_info_battery_storage() {
    if !live() {
        return;
    }
    let disc = MuxdDiscovery::new();
    let devices = disc.list().await.expect("usbmuxd reachable");
    assert!(!devices.is_empty(), "no iOS device on usbmuxd");
    let dev = &devices[0];
    eprintln!(
        "device: {} mux {} {:?}",
        dev.udid, dev.device_id, dev.connection_type
    );
    let provider = Arc::new(IosProvider::new(dev, disc.addr().clone()));

    let Some(pair) = read_pair_record(disc.addr(), &dev.udid).await.unwrap() else {
        eprintln!("no pair record yet: NeedsTrust would be surfaced by the discovery");
        return;
    };
    validate_pair_record(&provider, &pair)
        .await
        .expect("pair record accepted");
    provider.set_pair(pair.clone());

    let facts = ConnectionFacts {
        udid: dev.udid.clone(),
        transport: Some(Transport::Usb),
        pairing: Some("paired".into()),
        tier: Some("tier 1 (no tunnel)".into()),
        tunnel: None,
    };
    let started = std::time::Instant::now();
    let info = read_info(&*provider, &pair, &facts).await.unwrap();
    eprintln!(
        "info took {} ms: {} / {} / {} {} / chip {} ({:?} cores) / dev mode {:?} / storage {:?} / notes {:?}",
        started.elapsed().as_millis(),
        info.identity.marketing_name.as_deref().unwrap_or("?"),
        info.identity.model_identifier.as_deref().unwrap_or("?"),
        info.software.os_name.as_deref().unwrap_or("?"),
        info.software.os_version.as_deref().unwrap_or("?"),
        info.hardware.chip_name.as_deref().unwrap_or("?"),
        info.hardware.cpu_cores,
        info.state.developer_mode,
        info.hardware.storage_bytes,
        info.raw.get("statasmon/notes"),
    );
    let product = info.identity.model_identifier.clone().unwrap();
    assert!(
        product == "iPad13,18" || product == "iPad13,19",
        "ProductType {product}"
    );
    assert_eq!(info.software.os_version.as_deref(), Some("17.4.1"));
    assert_eq!(info.hardware.chip_name.as_deref(), Some("A14 Bionic"));
    assert_eq!(
        info.identity.marketing_name.as_deref(),
        Some("iPad (10th generation)")
    );
    assert!(info.hardware.storage_bytes.unwrap() > 32 << 30);
    assert!(info.state.developer_mode.is_some(), "amfi answered");

    let mut ld = Lockdown::connect(&*provider, &pair).await.unwrap();
    let basic = BatteryBasic::parse(&ld.battery_domain().await.unwrap());
    eprintln!("battery basic: {basic:?}");
    assert!(basic.percent.unwrap() <= 100);
    drop(ld);
    let mut diag =
        idevice::services::diagnostics_relay::DiagnosticsRelayClient::connect(&*provider)
            .await
            .unwrap();
    let detail = BatteryDetail::parse(&fetch_ioregistry(&mut diag).await.unwrap());
    eprintln!(
        "battery detail: {detail:?} health {:?}",
        detail.health_pct()
    );
    let v = detail.voltage_v.unwrap();
    assert!((3.0..=5.0).contains(&v), "voltage {v}");

    let usage = fetch_disk_usage(&*provider, &pair).await.unwrap();
    eprintln!("disk: {usage:?}");
    assert!(usage.total.unwrap() > 32 << 30);
    let apps = fetch_app_sizes(&*provider).await.unwrap();
    eprintln!("{} apps; top: {:?}", apps.len(), apps.first());
    assert!(!apps.is_empty());
}

#[tokio::test]
async fn live_source_start_emits_samples() {
    if !live() {
        return;
    }
    let disc = MuxdDiscovery::new();
    let dev = disc
        .list()
        .await
        .unwrap()
        .into_iter()
        .next()
        .expect("device");
    let provider = Arc::new(IosProvider::new(&dev, disc.addr().clone()));
    let Some(pair) = read_pair_record(disc.addr(), &dev.udid).await.unwrap() else {
        eprintln!("not paired; skipped");
        return;
    };
    provider.set_pair(pair.clone());
    let seed = IosSeed::new(&dev.udid, dev.device_id, Transport::Usb);
    let source = IosSource::connect(provider, pair, seed).await.unwrap();
    assert_eq!(source.summary().state, ConnectionState::Tier1);
    assert!(source.capabilities().is_available(Capability::BatteryBasic));
    let (sink, mut rx) = EventSink::channel(256);
    source
        .start(
            sink,
            SamplingConfig {
                medium_ms: 1000,
                slow_ms: 2000,
                ..SamplingConfig::default()
            },
        )
        .await
        .unwrap();
    let mut got_battery = false;
    let mut got_disk = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !(got_battery && got_disk) {
        let msg = tokio::time::timeout_at(deadline, rx.recv())
            .await
            .expect("samples within 20 s")
            .unwrap();
        if let Message::Samples { values, .. } = msg {
            for v in &values {
                eprintln!("  {} = {}", v.id.as_str(), v.value);
            }
            got_battery |= values.iter().any(|v| v.id == MetricId::BatPercent);
            got_disk |= values.iter().any(|v| v.id == MetricId::DiskTotal);
        }
    }
    source.stop().await.unwrap();
}
