//! The three scripted devices: identity, information sheet, capabilities
//! and hardware numbers the generator needs.

use std::collections::BTreeMap;

use statasmon_protocol::{
    Availability, Capability, CapabilitySet, ConnectionInfo, ConnectionState, DeviceHardware,
    DeviceId, DeviceIdentity, DeviceInfo, DeviceSoftware, DeviceState, DeviceSummary, Platform,
    ToolVersion, Transport,
};

use crate::config::FakeDevice;

/// Reason shown for iPad tier-1 gaps.
pub const IPAD_DEV_MODE_OFF: &str = "Developer Mode is off on this iPad";
/// Reason shown for Android packet capture.
pub const ANDROID_NO_ROOT: &str = "Needs root on Android";

/// A fully described fake device.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    /// Which scripted device this is.
    pub kind: FakeDevice,
    /// Stable id (`fake:iphone`, `fake:ipad`, `fake:android`).
    pub id: DeviceId,
    /// User-visible device name.
    pub name: String,
    /// Marketing name.
    pub model_name: String,
    /// Which real platform this imitates (used for log shapes and tiers).
    pub imitates: Platform,
    /// Installed RAM in bytes.
    pub ram_bytes: u64,
    /// Storage in bytes.
    pub storage_bytes: u64,
    /// Performance cores.
    pub performance_cores: u16,
    /// Efficiency cores.
    pub efficiency_cores: u16,
    /// Battery design capacity (mAh).
    pub battery_design_mah: u32,
    /// Full connection tier.
    pub tier: ConnectionState,
    /// Whether iOS Developer Mode is on (irrelevant on Android).
    pub developer_mode: bool,
}

impl Profile {
    /// Build the profile for a configured device.
    pub fn new(kind: FakeDevice) -> Self {
        match kind {
            FakeDevice::Iphone => Self {
                kind,
                id: "fake:iphone".into(),
                name: "Elia's iPhone".into(),
                model_name: "iPhone 16 Pro".into(),
                imitates: Platform::Ios,
                ram_bytes: 8 * GIB,
                storage_bytes: 256 * GB,
                performance_cores: 2,
                efficiency_cores: 4,
                battery_design_mah: 3582,
                tier: ConnectionState::Tier2,
                developer_mode: true,
            },
            FakeDevice::Ipad { developer_mode } => Self {
                kind,
                id: "fake:ipad".into(),
                name: "Elia's iPad".into(),
                model_name: "iPad (10th generation)".into(),
                imitates: Platform::Ios,
                ram_bytes: 4 * GIB,
                storage_bytes: 64 * GB,
                performance_cores: 2,
                efficiency_cores: 4,
                battery_design_mah: 7606,
                tier: if developer_mode {
                    ConnectionState::Tier2
                } else {
                    ConnectionState::Tier1
                },
                developer_mode,
            },
            FakeDevice::Android => Self {
                kind,
                id: "fake:android".into(),
                name: "Galaxy Tab S8".into(),
                model_name: "Galaxy Tab S8".into(),
                imitates: Platform::Android,
                ram_bytes: 8 * GIB,
                storage_bytes: 128 * GB,
                performance_cores: 4,
                efficiency_cores: 4,
                battery_design_mah: 8000,
                tier: ConnectionState::Tier2,
                developer_mode: true,
            },
        }
    }

    /// Total core count.
    pub fn cores(&self) -> u16 {
        self.performance_cores + self.efficiency_cores
    }

    /// Sidebar summary.
    pub fn summary(&self) -> DeviceSummary {
        DeviceSummary {
            id: self.id.clone(),
            platform: self.imitates,
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            transport: Transport::Fake,
            state: self.tier.clone(),
        }
    }

    /// What this device and tier can provide.
    pub fn capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::all_available();
        let off = |reason: &str| Availability::Unavailable {
            reason: reason.to_owned(),
        };
        match self.imitates {
            Platform::Ios | Platform::Fake => {
                set.set(
                    Capability::CpuFrequency,
                    off("iOS does not report core frequencies to the host"),
                );
                set.set(
                    Capability::TemperatureSensors,
                    off("iOS exposes only the battery temperature to the host"),
                );
                set.set(
                    Capability::WifiDetails,
                    off("iOS does not expose Wi-Fi signal details to the host"),
                );
                if !self.developer_mode {
                    for c in [
                        Capability::CpuTotal,
                        Capability::CpuPerCore,
                        Capability::LoadAverage,
                        Capability::MemoryBreakdown,
                        Capability::MemoryPressure,
                        Capability::NetworkThroughput,
                        Capability::NetworkPackets,
                        Capability::NetworkPerProcess,
                        Capability::Processes,
                        Capability::ProcessThreads,
                        Capability::ForegroundApp,
                        Capability::Energy,
                    ] {
                        set.set(c, off(IPAD_DEV_MODE_OFF));
                    }
                }
            }
            Platform::Android => {
                set.set(Capability::PacketCapture, off(ANDROID_NO_ROOT));
                set.set(
                    Capability::Energy,
                    off("Android has no per-process energy gauge"),
                );
            }
        }
        set
    }

    /// The device information sheet with every field the platform provides.
    pub fn info(&self, uptime_secs: u64, now_ms: i64) -> DeviceInfo {
        let boot_time_ms = now_ms - i64::try_from(uptime_secs).unwrap_or(0) * 1000;
        let mut raw = BTreeMap::new();
        let (identity, software, hardware, state) = match self.kind {
            FakeDevice::Iphone => {
                raw.insert("ProductType".into(), "iPhone17,1".into());
                raw.insert("HardwareModel".into(), "D93AP".into());
                raw.insert("BuildVersion".into(), "25A354".into());
                raw.insert("ChipID".into(), "0x8140".into());
                raw.insert("DeviceClass".into(), "iPhone".into());
                raw.insert("ActivationState".into(), "Activated".into());
                raw.insert("WiFiAddress".into(), "a4:83:e7:12:34:56".into());
                raw.insert("BluetoothAddress".into(), "a4:83:e7:12:34:57".into());
                (
                    DeviceIdentity {
                        name: Some(self.name.clone()),
                        marketing_name: Some(self.model_name.clone()),
                        model_identifier: Some("iPhone17,1".into()),
                        model_number: Some("MYNA3LL/A".into()),
                        manufacturer: Some("Apple".into()),
                        serial_number: Some("F2LXFAKE0001".into()),
                        unique_id: Some("00008140-000A2C3E0E80001E".into()),
                        region: Some("LL/A".into()),
                    },
                    DeviceSoftware {
                        os_name: Some("iOS".into()),
                        os_version: Some("27.0".into()),
                        os_build: Some("25A354".into()),
                        security_patch: None,
                        kernel_version: Some("Darwin 25.0.0".into()),
                        api_level: None,
                        firmware_version: Some("1.00.03".into()),
                    },
                    DeviceHardware {
                        chip_name: Some("A18 Pro".into()),
                        chip_identifier: Some("t8140".into()),
                        chip_manufacturer: Some("Apple".into()),
                        architecture: Some("arm64e".into()),
                        abis: vec!["arm64e".into(), "arm64".into()],
                        cpu_cores: Some(6),
                        performance_cores: Some(2),
                        efficiency_cores: Some(4),
                        cpu_max_mhz: None,
                        gpu_name: Some("Apple GPU (6-core)".into()),
                        ram_bytes: Some(self.ram_bytes),
                        storage_bytes: Some(self.storage_bytes),
                        battery_design_mah: Some(self.battery_design_mah),
                        board: Some("D93AP".into()),
                        screen: Some("2622×1206 @ 460 ppi".into()),
                    },
                    DeviceState {
                        uptime_secs: Some(uptime_secs),
                        boot_time_ms: Some(boot_time_ms),
                        time_zone: Some("Europe/Paris".into()),
                        locale: Some("en_US".into()),
                        developer_mode: Some(true),
                        usb_debugging: None,
                        wireless_debugging: None,
                        selinux_mode: None,
                    },
                )
            }
            FakeDevice::Ipad { developer_mode } => {
                raw.insert("ProductType".into(), "iPad13,18".into());
                raw.insert("HardwareModel".into(), "J271AP".into());
                raw.insert("BuildVersion".into(), "21E237".into());
                raw.insert("ChipID".into(), "0x8101".into());
                raw.insert("DeviceClass".into(), "iPad".into());
                raw.insert("ActivationState".into(), "Activated".into());
                raw.insert("WiFiAddress".into(), "3c:22:fb:ab:cd:ef".into());
                raw.insert("BluetoothAddress".into(), "3c:22:fb:ab:cd:f0".into());
                (
                    DeviceIdentity {
                        name: Some(self.name.clone()),
                        marketing_name: Some(self.model_name.clone()),
                        model_identifier: Some("iPad13,18".into()),
                        model_number: Some("MPQ03LL/A".into()),
                        manufacturer: Some("Apple".into()),
                        serial_number: Some("GX7KFAKE0002".into()),
                        unique_id: Some("00008101-001E3D2A0C08001E".into()),
                        region: Some("LL/A".into()),
                    },
                    DeviceSoftware {
                        os_name: Some("iPadOS".into()),
                        os_version: Some("17.4.1".into()),
                        os_build: Some("21E237".into()),
                        security_patch: None,
                        kernel_version: Some("Darwin 23.4.0".into()),
                        api_level: None,
                        firmware_version: None,
                    },
                    DeviceHardware {
                        chip_name: Some("A14 Bionic".into()),
                        chip_identifier: Some("t8101".into()),
                        chip_manufacturer: Some("Apple".into()),
                        architecture: Some("arm64e".into()),
                        abis: vec!["arm64e".into(), "arm64".into()],
                        cpu_cores: Some(6),
                        performance_cores: Some(2),
                        efficiency_cores: Some(4),
                        cpu_max_mhz: None,
                        gpu_name: Some("Apple GPU (4-core)".into()),
                        ram_bytes: Some(self.ram_bytes),
                        storage_bytes: Some(self.storage_bytes),
                        battery_design_mah: Some(self.battery_design_mah),
                        board: Some("J271AP".into()),
                        screen: Some("2360×1640 @ 264 ppi".into()),
                    },
                    DeviceState {
                        uptime_secs: Some(uptime_secs),
                        boot_time_ms: Some(boot_time_ms),
                        time_zone: Some("Europe/Paris".into()),
                        locale: Some("en_US".into()),
                        developer_mode: Some(developer_mode),
                        usb_debugging: None,
                        wireless_debugging: None,
                        selinux_mode: None,
                    },
                )
            }
            FakeDevice::Android => {
                raw.insert(
                    "ro.build.fingerprint".into(),
                    "samsung/gts8wifixx/gts8wifi:14/UP1A.231005.007/X700XXU4CXA1:user/release-keys"
                        .into(),
                );
                raw.insert("ro.soc.model".into(), "SM8450".into());
                raw.insert("ro.soc.manufacturer".into(), "Qualcomm".into());
                raw.insert("ro.product.board".into(), "taro".into());
                raw.insert("ro.bootloader".into(), "X700XXU4CXA1".into());
                raw.insert("ro.product.device".into(), "gts8wifi".into());
                (
                    DeviceIdentity {
                        name: Some(self.name.clone()),
                        marketing_name: Some(self.model_name.clone()),
                        model_identifier: Some("SM-X700".into()),
                        model_number: Some("SM-X700NZAAEUB".into()),
                        manufacturer: Some("Samsung".into()),
                        serial_number: Some("R52RFAKE0003".into()),
                        unique_id: Some("R52RFAKE0003".into()),
                        region: Some("EUB".into()),
                    },
                    DeviceSoftware {
                        os_name: Some("Android".into()),
                        os_version: Some("14".into()),
                        os_build: Some("UP1A.231005.007".into()),
                        security_patch: Some("2026-08-01".into()),
                        kernel_version: Some("5.10.198-android12-9".into()),
                        api_level: Some(34),
                        firmware_version: None,
                    },
                    DeviceHardware {
                        chip_name: Some("Snapdragon 8 Gen 1".into()),
                        chip_identifier: Some("SM8450".into()),
                        chip_manufacturer: Some("Qualcomm".into()),
                        architecture: Some("arm64-v8a".into()),
                        abis: vec!["arm64-v8a".into(), "armeabi-v7a".into(), "armeabi".into()],
                        cpu_cores: Some(8),
                        performance_cores: Some(4),
                        efficiency_cores: Some(4),
                        cpu_max_mhz: Some(2995),
                        gpu_name: Some("Adreno 730".into()),
                        ram_bytes: Some(self.ram_bytes),
                        storage_bytes: Some(self.storage_bytes),
                        battery_design_mah: Some(self.battery_design_mah),
                        board: Some("taro".into()),
                        screen: Some("2560×1600 @ 274 dpi".into()),
                    },
                    DeviceState {
                        uptime_secs: Some(uptime_secs),
                        boot_time_ms: Some(boot_time_ms),
                        time_zone: Some("Europe/Paris".into()),
                        locale: Some("en-US".into()),
                        developer_mode: None,
                        usb_debugging: Some(true),
                        wireless_debugging: Some(false),
                        selinux_mode: Some("Enforcing".into()),
                    },
                )
            }
        };
        let (tier, pairing, tunnel) = match self.imitates {
            Platform::Android => ("adb", "authorized", "n/a"),
            _ if self.developer_mode => ("tier 2", "trusted", "userspace tunnel up"),
            _ => ("tier 1", "trusted", "not started (Developer Mode off)"),
        };
        DeviceInfo {
            identity,
            software,
            hardware,
            state,
            connection: ConnectionInfo {
                transport: Some(Transport::Fake),
                tier: Some(tier.into()),
                pairing: Some(pairing.into()),
                tunnel: Some(tunnel.into()),
                last_heartbeat_ms: Some(now_ms),
                tools: vec![ToolVersion {
                    name: "statasmon-device-fake".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                }],
            },
            raw,
        }
    }
}

const GIB: u64 = 1024 * 1024 * 1024;
const GB: u64 = 1_000_000_000;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ipad_tier1_dashes_cpu_with_reason() {
        let p = Profile::new(FakeDevice::Ipad {
            developer_mode: false,
        });
        let caps = p.capabilities();
        assert_eq!(caps.reason(Capability::CpuTotal), Some(IPAD_DEV_MODE_OFF));
        assert!(caps.is_available(Capability::BatteryDetail));
        assert!(caps.is_available(Capability::Logs));
        assert_eq!(p.summary().state, ConnectionState::Tier1);
        let p2 = Profile::new(FakeDevice::Ipad {
            developer_mode: true,
        });
        assert!(p2.capabilities().is_available(Capability::CpuTotal));
        assert_eq!(p2.summary().state, ConnectionState::Tier2);
    }

    #[test]
    fn android_has_no_pcap() {
        let p = Profile::new(FakeDevice::Android);
        assert_eq!(
            p.capabilities().reason(Capability::PacketCapture),
            Some(ANDROID_NO_ROOT)
        );
        let info = p.info(3600, 1_000_000);
        assert_eq!(info.hardware.cpu_cores, Some(8));
        assert!(info.raw.contains_key("ro.build.fingerprint"));
        assert_eq!(info.state.boot_time_ms, Some(1_000_000 - 3_600_000));
    }

    #[test]
    fn iphone_info_is_filled() {
        let info = Profile::new(FakeDevice::Iphone).info(10, 0);
        assert_eq!(
            info.identity.marketing_name.as_deref(),
            Some("iPhone 16 Pro")
        );
        assert_eq!(info.hardware.chip_name.as_deref(), Some("A18 Pro"));
        assert_eq!(info.raw.get("ProductType").unwrap(), "iPhone17,1");
    }
}
