//! usbmuxd reachability: an unreachable address is reported as
//! `DriverMissing` with the install hint; the real socket is listed when
//! present (no device required).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use idevice::usbmuxd::UsbmuxdAddr;
use statasmon_device_ios::{IosError, MuxdDiscovery};
use statasmon_protocol::ErrorCode;

#[tokio::test]
async fn unreachable_usbmuxd_is_driver_missing() {
    let disc =
        MuxdDiscovery::new().with_addr(UsbmuxdAddr::TcpSocket("127.0.0.1:9".parse().unwrap()));
    let err = disc.probe().await.unwrap_err();
    assert!(matches!(err, IosError::MuxdUnavailable(_)));
    assert_eq!(err.code(), ErrorCode::DriverMissing);
    assert!(err.hint().unwrap().contains("iTunes"));
}

#[tokio::test]
async fn platform_socket_lists_devices_when_present() {
    let disc = MuxdDiscovery::new();
    match disc.list().await {
        Ok(devices) => eprintln!("usbmuxd: {} device(s): {devices:?}", devices.len()),
        Err(e) => eprintln!("usbmuxd not reachable here ({e}); skipped"),
    }
}
