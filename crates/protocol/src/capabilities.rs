//! What a connected device can provide right now, and why not otherwise.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Capability {
    DeviceInfo,
    CpuTotal,
    CpuPerCore,
    CpuFrequency,
    LoadAverage,
    MemoryBreakdown,
    MemoryPressure,
    MemoryWarnings,
    ThermalState,
    TemperatureSensors,
    BatteryBasic,
    BatteryDetail,
    StorageTotals,
    StorageCategories,
    StoragePerApp,
    NetworkThroughput,
    NetworkPackets,
    NetworkPerProcess,
    WifiDetails,
    PacketCapture,
    Processes,
    ProcessThreads,
    ForegroundApp,
    Logs,
    PerAppLogs,
    CrashReports,
    Energy,
}

impl Capability {
    pub const ALL: [Capability; 27] = [
        Capability::DeviceInfo,
        Capability::CpuTotal,
        Capability::CpuPerCore,
        Capability::CpuFrequency,
        Capability::LoadAverage,
        Capability::MemoryBreakdown,
        Capability::MemoryPressure,
        Capability::MemoryWarnings,
        Capability::ThermalState,
        Capability::TemperatureSensors,
        Capability::BatteryBasic,
        Capability::BatteryDetail,
        Capability::StorageTotals,
        Capability::StorageCategories,
        Capability::StoragePerApp,
        Capability::NetworkThroughput,
        Capability::NetworkPackets,
        Capability::NetworkPerProcess,
        Capability::WifiDetails,
        Capability::PacketCapture,
        Capability::Processes,
        Capability::ProcessThreads,
        Capability::ForegroundApp,
        Capability::Logs,
        Capability::PerAppLogs,
        Capability::CrashReports,
        Capability::Energy,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
#[ts(export)]
pub enum Availability {
    Available,
    /// The reason is user-facing ("Developer Mode is off on this iPad").
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CapabilityEntry {
    pub capability: Capability,
    pub availability: Availability,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CapabilitySet {
    pub entries: Vec<CapabilityEntry>,
}

impl CapabilitySet {
    /// Every capability listed, all available.
    pub fn all_available() -> Self {
        Self {
            entries: Capability::ALL
                .iter()
                .map(|c| CapabilityEntry {
                    capability: *c,
                    availability: Availability::Available,
                })
                .collect(),
        }
    }

    /// Every capability unavailable with the same reason (e.g. while connecting).
    pub fn none(reason: &str) -> Self {
        Self {
            entries: Capability::ALL
                .iter()
                .map(|c| CapabilityEntry {
                    capability: *c,
                    availability: Availability::Unavailable {
                        reason: reason.to_owned(),
                    },
                })
                .collect(),
        }
    }

    pub fn set(&mut self, capability: Capability, availability: Availability) {
        match self.entries.iter_mut().find(|e| e.capability == capability) {
            Some(e) => e.availability = availability,
            None => self.entries.push(CapabilityEntry {
                capability,
                availability,
            }),
        }
    }

    pub fn is_available(&self, capability: Capability) -> bool {
        self.entries
            .iter()
            .any(|e| e.capability == capability && e.availability == Availability::Available)
    }

    pub fn reason(&self, capability: Capability) -> Option<&str> {
        self.entries
            .iter()
            .find_map(|e| match (&e.capability, &e.availability) {
                (c, Availability::Unavailable { reason }) if *c == capability => {
                    Some(reason.as_str())
                }
                _ => None,
            })
    }
}
