//! Tunnel error mapping and the capability sets the source publishes for
//! Developer Mode on / off / tunnel failed (steps 4.1–4.2, 4.6).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use statasmon_device_ios::IosError;
use statasmon_device_ios::source::{capabilities_for, reasons, to_device_error};
use statasmon_device_ios::tunnel::{self, is_transport_error};
use statasmon_device_ios::{tier1_capabilities, tier2_capabilities};
use statasmon_protocol::{Capability, ErrorCode};

#[test]
fn tunnel_errors_map_to_the_bus_code() {
    let e = to_device_error("ios:X", IosError::Tunnel("RSD handshake: boom".into()));
    assert_eq!(e.code(), ErrorCode::TunnelFailed);
    assert!(e.message().contains("RSD handshake: boom"));
    assert!(e.hint().is_some());

    let off = to_device_error("ios:X", IosError::DeveloperModeOff);
    assert_eq!(off.code(), ErrorCode::DeveloperModeOff);
    assert!(off.hint().unwrap().contains("Developer Mode"));
    assert!(tunnel::hints::DEV_MODE.contains("Developer Mode"));
    assert!(tunnel::hints::RETRY.contains("Tier 1"));
}

#[test]
fn transport_errors_kill_the_tunnel_service_errors_do_not() {
    assert!(is_transport_error(&idevice::IdeviceError::Socket(
        std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "remote server connection closed"
        )
    )));
    assert!(is_transport_error(&idevice::IdeviceError::Timeout));
    assert!(!is_transport_error(&idevice::IdeviceError::ServiceNotFound));
    assert!(!is_transport_error(
        &idevice::IdeviceError::UnexpectedResponse("no data in reply".into())
    ));
}

#[test]
fn capability_sets_for_every_tier_state() {
    // Developer Mode off: every DVT capability names the setting.
    let off = tier1_capabilities(Some(false), true);
    for cap in [
        Capability::CpuTotal,
        Capability::CpuPerCore,
        Capability::MemoryBreakdown,
        Capability::Processes,
        Capability::NetworkThroughput,
        Capability::NetworkPerProcess,
        Capability::Energy,
    ] {
        assert_eq!(off.reason(cap), Some(reasons::DEV_MODE_OFF), "{cap:?}");
    }
    // Tier 1 keeps working regardless.
    for cap in [
        Capability::DeviceInfo,
        Capability::BatteryBasic,
        Capability::StorageTotals,
        Capability::Logs,
        Capability::CrashReports,
        Capability::PacketCapture,
    ] {
        assert!(off.is_available(cap), "{cap:?}");
    }

    // Developer Mode on, tunnel not (yet) up.
    let pending = tier1_capabilities(Some(true), true);
    assert_eq!(
        pending.reason(Capability::CpuTotal),
        Some(reasons::NEEDS_TUNNEL)
    );

    // Tunnel failed: the tunnel reason, tier 1 intact.
    let failed = capabilities_for(reasons::TUNNEL_FAILED, false);
    assert_eq!(
        failed.reason(Capability::Processes),
        Some(reasons::TUNNEL_FAILED)
    );
    assert_eq!(
        failed.reason(Capability::BatteryDetail),
        Some(reasons::BATTERY_DETAIL_REFUSED)
    );
    assert!(failed.is_available(Capability::Logs));

    // Tier 2: the developer services, with reasons for what they lack.
    let on = tier2_capabilities(true, false);
    assert!(on.is_available(Capability::CpuPerCore));
    assert!(on.is_available(Capability::NetworkPerProcess));
    assert!(on.is_available(Capability::ProcessThreads));
    assert_eq!(on.reason(Capability::Energy), Some(reasons::ENERGY_OFF));
    assert_eq!(
        on.reason(Capability::WifiDetails),
        Some(reasons::WIFI_DETAILS_TIER2)
    );
    assert_eq!(
        on.reason(Capability::ForegroundApp),
        Some(reasons::NO_FRONT_APP)
    );
    assert_eq!(
        on.reason(Capability::CpuFrequency),
        Some(reasons::NO_FREQUENCY)
    );
    assert!(tier2_capabilities(true, true).is_available(Capability::Energy));
    // Every capability has an entry in every set.
    for set in [&off, &pending, &failed, &on] {
        assert_eq!(set.entries.len(), Capability::ALL.len());
    }
}
