//! Which [`Capability`] each metric in the catalog depends on. Providers
//! and the fake use it to decide what to emit; the core uses it to know
//! which metrics a snapshot should contain.

use statasmon_protocol::{Capability, CapabilitySet, MetricId};

/// The capability a device must have for `id` to be sampled.
pub fn metric_capability(id: MetricId) -> Capability {
    use Capability as C;
    use MetricId as M;
    match id {
        M::CpuTotal | M::CpuUser | M::CpuSystem => C::CpuTotal,
        M::CpuLoad1 | M::CpuLoad5 => C::LoadAverage,
        M::MemTotal
        | M::MemUsed
        | M::MemFree
        | M::MemApp
        | M::MemWired
        | M::MemCompressed
        | M::MemCached
        | M::MemSwap => C::MemoryBreakdown,
        M::MemPressure => C::MemoryPressure,
        M::ThermalState | M::ThermalRaw => C::ThermalState,
        M::TempSoc | M::TempSkin | M::TempPmu | M::TempDisplay => C::TemperatureSensors,
        M::TempBattery => C::BatteryDetail,
        M::BatPercent | M::BatCharging => C::BatteryBasic,
        M::BatVoltage
        | M::BatCurrent
        | M::BatPower
        | M::BatTemp
        | M::BatCycles
        | M::BatHealth
        | M::BatDesignCapacity
        | M::BatFullCapacity => C::BatteryDetail,
        M::DiskTotal | M::DiskUsed | M::DiskFree | M::DiskWriteBps => C::StorageTotals,
        M::DiskApps | M::DiskMedia | M::DiskSystem | M::DiskOther => C::StorageCategories,
        M::NetRxBps | M::NetTxBps | M::NetRxTotal | M::NetTxTotal | M::NetLatencyMs => {
            C::NetworkThroughput
        }
        M::NetRxPps | M::NetTxPps => C::NetworkPackets,
        M::NetRssi | M::NetLinkMbps => C::WifiDetails,
        M::NetConnections | M::NetRetransmits | M::NetDropped => C::NetworkPerProcess,
        M::ProcCount => C::Processes,
    }
}

/// Every catalog metric the capability set allows, in catalog order.
pub fn allowed_metrics(set: &CapabilitySet) -> Vec<MetricId> {
    MetricId::ALL
        .iter()
        .copied()
        .filter(|id| set.is_available(metric_capability(*id)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use statasmon_protocol::Availability;

    #[test]
    fn all_available_allows_the_whole_catalog() {
        assert_eq!(
            allowed_metrics(&CapabilitySet::all_available()).len(),
            MetricId::ALL.len()
        );
    }

    #[test]
    fn unavailable_capability_removes_its_metrics() {
        let mut set = CapabilitySet::all_available();
        set.set(
            Capability::CpuTotal,
            Availability::Unavailable {
                reason: "Developer Mode is off".into(),
            },
        );
        let allowed = allowed_metrics(&set);
        assert!(!allowed.contains(&MetricId::CpuTotal));
        assert!(!allowed.contains(&MetricId::CpuUser));
        assert!(allowed.contains(&MetricId::BatPercent));
    }
}
