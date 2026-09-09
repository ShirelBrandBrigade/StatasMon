//! Live probe + info against a connected device. Skipped unless
//! STATASMON_LIVE_ANDROID=<serial> and STATASMON_ADB=<path to adb> are set.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use statasmon_device::DeviceSource;
use statasmon_device_android::adb::AdbServer;
use statasmon_device_android::info::{ConnectionFacts, read_info};
use statasmon_device_android::{AndroidSource, Probe, ProcessShell, Shell, SourceSeed};
use statasmon_protocol::{Capability, ConnectionState, Transport};

fn live() -> Option<(String, String)> {
    match (
        std::env::var("STATASMON_LIVE_ANDROID"),
        std::env::var("STATASMON_ADB"),
    ) {
        (Ok(serial), Ok(adb)) => Some((serial, adb)),
        _ => {
            eprintln!("skipped: set STATASMON_LIVE_ANDROID and STATASMON_ADB");
            None
        }
    }
}

#[tokio::test]
async fn live_probe_and_info() {
    let Some((serial, adb)) = live() else { return };
    let shell = ProcessShell::new(&adb, &serial);
    let started = std::time::Instant::now();
    let probe = Probe::run(&shell).await.unwrap();
    let probe_ms = started.elapsed().as_millis();
    eprintln!("probe took {probe_ms} ms: {probe:#?}");
    assert!(probe_ms < 10_000);
    assert!(
        probe.sdk >= 31,
        "Android 12+ expected, got sdk {}",
        probe.sdk
    );
    assert!(probe.proc_stat && probe.meminfo && probe.loadavg && probe.proc_net_dev);
    assert!(probe.battery_dumpsys && probe.thermalservice && probe.logcat);
    assert!(probe.cpu_count >= 4);
    let caps = probe.capabilities();
    assert!(caps.is_available(Capability::CpuTotal));
    assert!(caps.is_available(Capability::Logs));
    assert!(!caps.is_available(Capability::PacketCapture));

    let facts = ConnectionFacts {
        serial: serial.clone(),
        transport: Some(Transport::Usb),
        tools: vec![],
        pairing: Some("authorized".into()),
    };
    let started = std::time::Instant::now();
    let info = read_info(&shell, &probe, &facts).await.unwrap();
    eprintln!(
        "info took {} ms: {} / {} / {} / api {} / {} / {} cores ({:?}P+{:?}E) / {:?}",
        started.elapsed().as_millis(),
        info.identity.marketing_name.as_deref().unwrap_or("?"),
        info.identity.name.as_deref().unwrap_or("?"),
        info.software.os_version.as_deref().unwrap_or("?"),
        info.software.api_level.unwrap_or(0),
        info.hardware.chip_name.as_deref().unwrap_or("?"),
        info.hardware.cpu_cores.unwrap_or(0),
        info.hardware.performance_cores,
        info.hardware.efficiency_cores,
        info.hardware.screen,
    );
    assert_eq!(info.identity.unique_id.as_deref(), Some(serial.as_str()));
    assert!(info.identity.model_identifier.is_some());
    assert_eq!(info.software.api_level, Some(probe.sdk));
    assert!(info.hardware.chip_identifier.is_some());
    assert!(info.hardware.ram_bytes.unwrap_or(0) > 1 << 30);
    assert!(info.hardware.storage_bytes.unwrap_or(0) > 1 << 30);
    assert!(info.state.uptime_secs.unwrap_or(0) > 0);
    assert_eq!(info.state.usb_debugging, Some(true));
    if serial == "R52W303E34B" {
        assert_eq!(info.identity.model_identifier.as_deref(), Some("SM-X700"));
        assert_eq!(info.software.api_level, Some(36));
        assert_eq!(
            info.hardware.chip_name.as_deref(),
            Some("Snapdragon 8 Gen 1")
        );
        assert_eq!(info.hardware.cpu_cores, Some(8));
        assert!(!probe.power_supply_sysfs);
        assert!(probe.psi_memory);
        assert!(probe.thermal_sensors);
    }
}

#[tokio::test]
async fn live_source_connects() {
    let Some((serial, adb)) = live() else { return };
    let server = AdbServer::new(&adb);
    server.start().await.unwrap();
    let version = server.version().await.unwrap();
    assert!(!version.version.is_empty());
    let devices = server.devices().await.unwrap();
    let dev = devices
        .into_iter()
        .find(|d| d.serial == serial)
        .expect("device listed by adb");
    let shell: Arc<dyn Shell> = Arc::new(server.shell(&serial));
    let seed = SourceSeed {
        device: dev,
        adb: Some(server.clone()),
        adb_version: Some(version),
    };
    let src = AndroidSource::connect(shell, seed).await.unwrap();
    assert_eq!(src.id(), format!("android:{serial}"));
    let s = src.summary();
    assert_eq!(s.state, ConnectionState::Tier2);
    eprintln!("summary: {s:?}");
    assert!(src.capabilities().is_available(Capability::DeviceInfo));
    assert_eq!(src.info().await.unwrap().connection.tools[0].name, "adb");
}
