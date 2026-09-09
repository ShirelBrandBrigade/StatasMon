//! `build_info` and `IosSource::from_inputs` against `fixtures/ios/**`.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::Arc;

use idevice::usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdDevice};
use statasmon_device::DeviceSource;
use statasmon_device_ios::info::{ConnectionFacts, build_info, load_inputs};
use statasmon_device_ios::{IosProvider, IosSeed, IosSource};
use statasmon_protocol::{Capability, ConnectionState, Platform, Transport};

const UDID: &str = "00000000-0000000000000000";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ios/ipad13-18-17.4.1")
}

fn provider() -> Arc<IosProvider> {
    let dev = UsbmuxdDevice {
        connection_type: Connection::Usb,
        udid: UDID.to_owned(),
        device_id: 7,
    };
    Arc::new(IosProvider::new(&dev, UsbmuxdAddr::default()))
}

#[test]
fn ipad_10th_generation_sheet() {
    let inputs = load_inputs(&fixture_dir()).unwrap();
    let facts = ConnectionFacts {
        udid: UDID.to_owned(),
        transport: Some(Transport::Usb),
        pairing: Some("paired".into()),
        tier: Some("tier 1 (no tunnel)".into()),
        tunnel: None,
    };
    let info = build_info(&inputs, &facts);
    let id = &info.identity;
    assert_eq!(id.marketing_name.as_deref(), Some("iPad (10th generation)"));
    assert_eq!(id.model_identifier.as_deref(), Some("iPad13,18"));
    assert_eq!(id.manufacturer.as_deref(), Some("Apple"));
    assert_eq!(id.unique_id.as_deref(), Some(UDID));
    assert!(id.serial_number.is_some());
    assert!(id.model_number.as_deref().unwrap().starts_with("MPQ03"));

    let sw = &info.software;
    assert_eq!(sw.os_name.as_deref(), Some("iPadOS"));
    assert_eq!(sw.os_version.as_deref(), Some("17.4.1"));
    assert_eq!(sw.os_build.as_deref(), Some("21E237"));

    let hw = &info.hardware;
    assert_eq!(hw.chip_name.as_deref(), Some("A14 Bionic"));
    assert_eq!(hw.chip_identifier.as_deref(), Some("t8101"));
    assert_eq!(hw.architecture.as_deref(), Some("arm64e"));
    assert_eq!(
        (hw.cpu_cores, hw.performance_cores, hw.efficiency_cores),
        (Some(6), Some(2), Some(4))
    );
    assert!(hw.storage_bytes.unwrap() > 32 << 30);
    assert_eq!(hw.board.as_deref(), Some("J271AP"));
    assert!(hw.battery_design_mah.is_some());
    assert_eq!(
        hw.ram_bytes, None,
        "RAM is not readable at tier 1; never guessed"
    );

    assert_eq!(info.state.time_zone.as_deref(), Some("Asia/Jerusalem"));
    assert!(info.state.locale.is_some());
    assert_eq!(info.state.developer_mode, Some(false));
    assert_eq!(info.state.uptime_secs, None);
    assert_eq!(info.connection.tier.as_deref(), Some("tier 1 (no tunnel)"));
    assert_eq!(info.connection.tools[0].name, "idevice");
    assert!(info.raw.contains_key("ProductType"));
    assert!(
        info.raw
            .contains_key("com.apple.disk_usage/TotalDiskCapacity")
    );
    assert!(
        info.raw
            .contains_key("com.apple.security.mac.amfi/DeveloperModeStatus")
    );
}

#[tokio::test]
async fn source_from_fixture_is_tier1_with_reasons() {
    let inputs = load_inputs(&fixture_dir()).unwrap();
    let seed = IosSeed::new(UDID, 7, Transport::Usb);
    let source = IosSource::from_inputs(provider(), None, seed, &inputs);
    assert_eq!(source.id(), format!("ios:{UDID}"));
    assert_eq!(source.platform(), Platform::Ios);
    let summary = source.summary();
    assert_eq!(summary.state, ConnectionState::Tier1);
    assert_eq!(summary.model_name, "iPad (10th generation)");
    assert_eq!(summary.transport, Transport::Usb);
    let caps = source.capabilities();
    assert!(caps.is_available(Capability::DeviceInfo));
    assert!(caps.is_available(Capability::BatteryDetail));
    assert!(caps.is_available(Capability::StoragePerApp));
    assert!(
        caps.reason(Capability::CpuTotal)
            .unwrap()
            .contains("Developer Mode is off")
    );
    assert!(caps.reason(Capability::StorageCategories).is_some());
    let info = source.info().await.unwrap();
    assert_eq!(info.identity.model_identifier.as_deref(), Some("iPad13,18"));
}

#[tokio::test]
async fn untrusted_source_reports_needs_trust() {
    let seed = IosSeed::new(UDID, 7, Transport::Wifi);
    let source = IosSource::untrusted(
        provider(),
        seed,
        "Accept \"Trust This Computer\" on the device",
    );
    assert_eq!(source.summary().state, ConnectionState::NeedsTrust);
    assert_eq!(source.summary().transport, Transport::Wifi);
    assert!(!source.capabilities().is_available(Capability::DeviceInfo));
    assert!(
        source
            .capabilities()
            .reason(Capability::Logs)
            .unwrap()
            .contains("Trust")
    );
    let err = source.info().await.err().unwrap();
    assert_eq!(err.code(), statasmon_protocol::ErrorCode::NotTrusted);
}
