//! Device identity, connection state and the full device information sheet
//! (docs/FEATURES.md §1). Every field a platform cannot provide is `None`;
//! the UI renders a dash with the reason from the capability set.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable device id: `ios:<udid>`, `android:<serial>` or `fake:<n>`.
pub type DeviceId = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Platform {
    Ios,
    Android,
    Fake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Transport {
    Usb,
    Wifi,
    AdbTcp,
    Fake,
}

/// Connection state machine (docs/PLAN.md §3.7 and §3.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionState {
    /// Seen on the bus but not yet opened.
    Detected,
    /// Waiting for the user to trust the computer / accept the debugging prompt.
    NeedsTrust,
    /// Pairing or tunnel handshake in progress.
    Connecting,
    /// iOS tier 1 (no tunnel) or Android base: info, battery, storage, logs.
    Tier1,
    /// Full telemetry (iOS tier 2 with Developer Mode + tunnel; Android always).
    Tier2,
    Disconnected,
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceSummary {
    pub id: DeviceId,
    pub platform: Platform,
    pub name: String,
    pub model_name: String,
    pub transport: Transport,
    pub state: ConnectionState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceIdentity {
    /// User-visible device name ("Elia's iPad").
    pub name: Option<String>,
    /// Marketing name ("iPad (10th generation)", "Galaxy Tab S8").
    pub marketing_name: Option<String>,
    /// Model identifier ("iPad13,18", "SM-X700").
    pub model_identifier: Option<String>,
    /// Model number ("MPQ03LL/A") where the platform has one.
    pub model_number: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    /// UDID (iOS) or ADB serial (Android).
    pub unique_id: Option<String>,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceSoftware {
    /// "iOS", "iPadOS", "Android".
    pub os_name: Option<String>,
    /// "17.4.1", "14".
    pub os_version: Option<String>,
    /// Build identifier ("21E237", "UP1A.231005.007").
    pub os_build: Option<String>,
    /// Android security patch level ("2026-08-05").
    pub security_patch: Option<String>,
    pub kernel_version: Option<String>,
    /// Android API level.
    pub api_level: Option<u32>,
    /// iOS baseband / firmware version where readable.
    pub firmware_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceHardware {
    /// Marketing chip name ("A14 Bionic", "Snapdragon 8 Gen 1").
    pub chip_name: Option<String>,
    /// Raw chip identifier ("t8101" / ChipID 0x8101, "SM8450").
    pub chip_identifier: Option<String>,
    pub chip_manufacturer: Option<String>,
    /// "arm64e", "arm64-v8a".
    pub architecture: Option<String>,
    pub abis: Vec<String>,
    pub cpu_cores: Option<u32>,
    pub performance_cores: Option<u32>,
    pub efficiency_cores: Option<u32>,
    pub cpu_max_mhz: Option<u32>,
    pub gpu_name: Option<String>,
    pub ram_bytes: Option<u64>,
    pub storage_bytes: Option<u64>,
    pub battery_design_mah: Option<u32>,
    /// Board / hardware model ("J271AP", "kalama").
    pub board: Option<String>,
    pub screen: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceState {
    pub uptime_secs: Option<u64>,
    pub boot_time_ms: Option<i64>,
    pub time_zone: Option<String>,
    pub locale: Option<String>,
    /// iOS Developer Mode.
    pub developer_mode: Option<bool>,
    /// Android USB debugging / developer options.
    pub usb_debugging: Option<bool>,
    pub wireless_debugging: Option<bool>,
    pub selinux_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolVersion {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConnectionInfo {
    pub transport: Option<Transport>,
    pub tier: Option<String>,
    pub pairing: Option<String>,
    pub tunnel: Option<String>,
    pub last_heartbeat_ms: Option<i64>,
    pub tools: Vec<ToolVersion>,
}

/// The complete device information document (docs/FEATURES.md §1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceInfo {
    pub identity: DeviceIdentity,
    pub software: DeviceSoftware,
    pub hardware: DeviceHardware,
    pub state: DeviceState,
    pub connection: ConnectionInfo,
    /// Raw platform keys kept verbatim (HardwareModel, ProductType,
    /// ro.bootloader, ...). Shown on the Device screen under "Raw".
    pub raw: BTreeMap<String, String>,
}
