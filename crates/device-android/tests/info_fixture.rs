//! `read_info` against the recorded Galaxy Tab S8 fixture through
//! `FixtureShell` (commands match `manifest.json`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use statasmon_device::DeviceSource;
use statasmon_device_android::info::{ConnectionFacts, read_info};
use statasmon_device_android::{AndroidSource, FixtureShell, Probe, Shell, SourceSeed};
use statasmon_protocol::{ConnectionState, Platform, ToolVersion, Transport};

fn shell() -> FixtureShell {
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/android/sm-x700-android16");
    FixtureShell::load(dir).unwrap()
}

#[tokio::test]
async fn galaxy_tab_s8_info_sheet() {
    let shell = shell();
    let probe = Probe {
        sdk: 36,
        cpu_count: 8,
        notes: vec!["battery sysfs: not readable".to_owned()],
        ..Probe::default()
    };
    let facts = ConnectionFacts {
        serial: "R52W303E34B".to_owned(),
        transport: Some(Transport::Usb),
        tools: vec![ToolVersion {
            name: "adb".to_owned(),
            version: "37.0.1-13800976".to_owned(),
        }],
        pairing: Some("authorized".to_owned()),
    };
    let info = read_info(&shell, &probe, &facts).await.unwrap();

    let id = &info.identity;
    assert_eq!(id.model_identifier.as_deref(), Some("SM-X700"));
    assert_eq!(id.manufacturer.as_deref(), Some("samsung"));
    assert_eq!(id.marketing_name.as_deref(), Some("Samsung SM-X700"));
    assert_eq!(id.model_number.as_deref(), Some("gts8wifixx"));
    assert_eq!(id.unique_id.as_deref(), Some("R52W303E34B"));
    assert_eq!(id.serial_number.as_deref(), Some("R5XXXXXXXXX"));
    assert_eq!(id.region.as_deref(), Some("ISRAEL"));
    assert_eq!(id.name, None, "device_name is not in the fixture");

    let sw = &info.software;
    assert_eq!(sw.os_name.as_deref(), Some("Android"));
    assert_eq!(sw.os_version.as_deref(), Some("16"));
    assert_eq!(sw.api_level, Some(36));
    assert_eq!(
        sw.os_build.as_deref(),
        Some("BP2A.250605.031.A3.X700XXS9EYK1")
    );
    assert_eq!(sw.security_patch.as_deref(), Some("2025-12-01"));
    assert_eq!(
        sw.kernel_version.as_deref(),
        Some("5.10.236-android12-9-32140295-abX700XXS9EYK1")
    );

    let hw = &info.hardware;
    assert_eq!(hw.chip_identifier.as_deref(), Some("SM8450"));
    assert_eq!(hw.chip_name.as_deref(), Some("Snapdragon 8 Gen 1"));
    assert_eq!(hw.chip_manufacturer.as_deref(), Some("Qualcomm"));
    assert_eq!(hw.architecture.as_deref(), Some("arm64-v8a"));
    assert!(hw.abis.contains(&"arm64-v8a".to_owned()));
    assert_eq!(hw.abis.len(), 3);
    assert_eq!(hw.cpu_cores, Some(8));
    assert_eq!(hw.performance_cores, Some(4));
    assert_eq!(hw.efficiency_cores, Some(4));
    assert_eq!(hw.cpu_max_mhz, Some(2995));
    assert_eq!(hw.gpu_name.as_deref(), Some("Adreno"));
    assert_eq!(hw.ram_bytes, Some(7_467_144 * 1024));
    assert_eq!(hw.storage_bytes, Some(110_594_012 * 1024));
    assert_eq!(hw.board.as_deref(), Some("taro"));
    assert_eq!(hw.screen.as_deref(), Some("1600×2560 @ 340 dpi"));
    assert_eq!(hw.battery_design_mah, None);

    let st = &info.state;
    assert_eq!(st.uptime_secs, Some(88_923));
    assert!(st.boot_time_ms.unwrap() > 1_577_836_800_000);
    assert_eq!(st.time_zone.as_deref(), Some("Asia/Jerusalem"));
    assert_eq!(st.locale.as_deref(), Some("en-US"));
    assert_eq!(st.developer_mode, Some(true));
    assert_eq!(st.usb_debugging, Some(true));
    assert_eq!(st.wireless_debugging, Some(false));
    assert_eq!(st.selinux_mode.as_deref(), Some("Enforcing"));

    let cx = &info.connection;
    assert_eq!(cx.transport, Some(Transport::Usb));
    assert_eq!(cx.tier.as_deref(), Some("full"));
    assert_eq!(cx.pairing.as_deref(), Some("authorized"));
    assert_eq!(cx.tools[0].version, "37.0.1-13800976");

    assert_eq!(
        info.raw.get("ro.bootloader").map(String::as_str),
        Some("X700XXS9EYK1")
    );
    assert_eq!(
        info.raw.get("ro.product.device").map(String::as_str),
        Some("gts8wifi")
    );
    assert!(info.raw.contains_key("ro.build.fingerprint"));
    assert!(info.raw.contains_key("persist.sys.timezone"));
    assert!(info.raw["cpufreq.policies"].contains("policy7"));
    assert_eq!(info.raw["probe.note.0"], "battery sysfs: not readable");
    assert!(info.raw.len() <= 256 + 8);
    assert!(!info.raw.keys().any(|k| k.starts_with("ro.boot.")));
}

#[tokio::test]
async fn untrusted_source_exposes_no_capabilities() {
    let shell: Arc<dyn Shell> = Arc::new(shell());
    let src = AndroidSource::untrusted(
        shell,
        SourceSeed::for_serial("ZY22ABCDEF"),
        "Accept the USB debugging prompt on the device",
    );
    assert_eq!(src.id(), "android:ZY22ABCDEF");
    assert_eq!(src.platform(), Platform::Android);
    let s = src.summary();
    assert_eq!(s.state, ConnectionState::NeedsTrust);
    assert_eq!(s.transport, Transport::Usb);
    assert!(
        !src.capabilities()
            .is_available(statasmon_protocol::Capability::DeviceInfo)
    );
    assert_eq!(
        src.capabilities()
            .reason(statasmon_protocol::Capability::CpuTotal),
        Some("Accept the USB debugging prompt on the device")
    );
    assert!(src.info().await.is_err());
}

#[tokio::test]
async fn fixture_source_connects_and_emits_state_capabilities_info() {
    use statasmon_device::EventSink;
    use statasmon_protocol::{DeviceAction, Message, SamplingConfig};

    let shell: Arc<dyn Shell> = Arc::new(shell());
    let src = AndroidSource::connect(shell, SourceSeed::for_serial("192.168.50.20:5555"))
        .await
        .unwrap();
    assert_eq!(src.id(), "android:192.168.50.20:5555");
    assert_eq!(src.summary().transport, Transport::AdbTcp);
    assert_eq!(src.summary().state, ConnectionState::Tier2);
    assert_eq!(src.summary().model_name, "Samsung SM-X700");
    // The fixture cannot answer the batched probe, so everything is
    // unavailable with a reason, but the sheet still comes from getprop.
    let info = src.info().await.unwrap();
    assert_eq!(
        info.hardware.chip_name.as_deref(),
        Some("Snapdragon 8 Gen 1")
    );

    let (sink, mut rx) = EventSink::channel(64);
    src.start(sink, SamplingConfig::default()).await.unwrap();
    assert!(matches!(
        rx.recv().await.unwrap(),
        Message::DeviceState {
            state: ConnectionState::Tier2,
            ..
        }
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        Message::Capabilities { .. }
    ));
    assert!(matches!(rx.recv().await.unwrap(), Message::Info { .. }));

    let err = src
        .run_action(DeviceAction::PcapStart {
            path: "/tmp/x.pcap".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), statasmon_protocol::ErrorCode::Unsupported);
    assert_eq!(err.hint(), Some("Packet capture needs root on Android"));
    let err = src
        .run_action(DeviceAction::FetchCrashReports)
        .await
        .unwrap_err();
    assert!(err.hint().unwrap().contains("dropbox"));
    let err = src
        .run_action(DeviceAction::Bugreport {
            path: "/tmp/b.zip".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), statasmon_protocol::ErrorCode::AdbNotFound);

    assert!(matches!(
        src.run_action(DeviceAction::RefreshInfo).await.unwrap(),
        statasmon_device::ActionResult::Done
    ));
    assert!(matches!(
        rx.recv().await.unwrap(),
        Message::Capabilities { .. }
    ));
    assert!(matches!(rx.recv().await.unwrap(), Message::Info { .. }));
    src.stop().await.unwrap();
    src.stop().await.unwrap();
}
