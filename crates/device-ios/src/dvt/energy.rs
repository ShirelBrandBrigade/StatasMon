//! `com.apple.xcode.debug-gauge-data-providers.Energy` (step 4.6): Xcode's
//! energy gauge, sampled for the top pids by CPU. Off by default
//! (`IosSource::set_energy_enabled`), behind `Capability::Energy`; it is
//! known to break on some iOS 18 builds, so a failure is logged once, the
//! loop ends and nothing else is affected.
//!
//! Labels: the gauge's `energy.cost` is Xcode's dimensionless "energy
//! impact" score per sampling interval. Thresholds ([`LOW_MAX`],
//! [`MEDIUM_MAX`]) are provisional until measured against Xcode's own
//! Low/High bands on a real device (ADR-013).

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use idevice::dvt::energy_monitor::{EnergyMonitorClient, EnergySample};
use tokio_util::sync::CancellationToken;

use super::{LoopEnd, SharedState};
use crate::tunnel::Tunnel;

/// The DVT channel identifier.
pub const SERVICE: &str = "com.apple.xcode.debug-gauge-data-providers.Energy";
/// `energy.cost` below this is "low".
pub const LOW_MAX: f64 = 1.0;
/// `energy.cost` below this is "medium"; at or above, "high".
pub const MEDIUM_MAX: f64 = 10.0;
/// How many pids are sampled.
pub const MAX_PIDS: usize = 5;

/// The label for a total energy score.
pub fn bucket(total_energy: f64) -> &'static str {
    if !total_energy.is_finite() || total_energy < LOW_MAX {
        "low"
    } else if total_energy < MEDIUM_MAX {
        "medium"
    } else {
        "high"
    }
}

/// Labels per pid from one `sampleAttributes:forPIDs:` answer.
pub fn labels(samples: &[EnergySample]) -> HashMap<u32, String> {
    samples
        .iter()
        .filter(|s| s.pid > 0)
        .map(|s| (s.pid, bucket(s.total_energy).to_owned()))
        .collect()
}

/// The energy loop: every `period` sample the shared `top_pids`, keeping
/// the gauge's sampling set in step with them.
pub async fn run(
    tunnel: std::sync::Arc<Tunnel>,
    period: Duration,
    shared: SharedState,
    cancel: CancellationToken,
) -> LoopEnd {
    let mut hub = match tunnel.dvt().await {
        Ok(h) => h,
        Err(e) => return LoopEnd::TunnelDead(e.to_string()),
    };
    let mut client = match EnergyMonitorClient::new(&mut hub).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "energy gauge unavailable; energy labels stay off");
            return LoopEnd::from_error(&tunnel, "energy channel", &e);
        }
    };
    let mut sampling: Vec<u32> = Vec::new();
    let mut interval = tokio::time::interval(period.max(Duration::from_secs(1)));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                if !sampling.is_empty() {
                    let _ = client.stop_sampling(&sampling).await;
                }
                return LoopEnd::Cancelled;
            }
            _ = interval.tick() => {}
        }
        let wanted: Vec<u32> = shared
            .read()
            .top_pids
            .iter()
            .copied()
            .take(MAX_PIDS)
            .collect();
        if wanted.is_empty() {
            continue;
        }
        let current: HashSet<u32> = sampling.iter().copied().collect();
        let next: HashSet<u32> = wanted.iter().copied().collect();
        if current != next {
            let gone: Vec<u32> = sampling
                .iter()
                .copied()
                .filter(|p| !next.contains(p))
                .collect();
            let new: Vec<u32> = wanted
                .iter()
                .copied()
                .filter(|p| !current.contains(p))
                .collect();
            if !gone.is_empty()
                && let Err(e) = client.stop_sampling(&gone).await
            {
                return end(&tunnel, "energy stopSamplingForPIDs", &e);
            }
            if !new.is_empty()
                && let Err(e) = client.start_sampling(&new).await
            {
                return end(&tunnel, "energy startSamplingForPIDs", &e);
            }
            sampling = wanted.clone();
            // The first sample after a change is zero; wait for the next tick.
            continue;
        }
        let bytes = match client.sample_attributes(&sampling).await {
            Ok(b) => b,
            Err(e) => return end(&tunnel, "energy sampleAttributes", &e),
        };
        match EnergySample::from_bytes(&bytes) {
            Ok(samples) => {
                let l = labels(&samples);
                let mut s = shared.write();
                s.energy.retain(|pid, _| next.contains(pid));
                s.energy.extend(l);
            }
            Err(e) => {
                tracing::warn!(error = %e, "energy gauge answered with an unreadable sample; energy labels stay off");
                return LoopEnd::ServiceFailed(format!("energy decode: {e}"));
            }
        }
    }
}

fn end(tunnel: &Tunnel, stage: &str, e: &idevice::IdeviceError) -> LoopEnd {
    let outcome = LoopEnd::from_error(tunnel, stage, e);
    if !outcome.is_tunnel_dead() {
        tracing::warn!(stage, error = %e, "energy gauge failed; energy labels stay off");
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pid: u32, total: f64) -> EnergySample {
        EnergySample {
            pid,
            timestamp: 0,
            total_energy: total,
            cpu_energy: 0.0,
            gpu_energy: 0.0,
            networking_energy: 0.0,
            display_energy: 0.0,
            location_energy: 0.0,
            appstate_energy: 0.0,
        }
    }

    #[test]
    fn buckets_and_labels() {
        assert_eq!(bucket(0.0), "low");
        assert_eq!(bucket(0.99), "low");
        assert_eq!(bucket(1.0), "medium");
        assert_eq!(bucket(9.99), "medium");
        assert_eq!(bucket(10.0), "high");
        assert_eq!(bucket(f64::NAN), "low");
        let l = labels(&[sample(10, 0.2), sample(11, 42.0), sample(0, 99.0)]);
        assert_eq!(l.get(&10).map(String::as_str), Some("low"));
        assert_eq!(l.get(&11).map(String::as_str), Some("high"));
        assert!(!l.contains_key(&0));
    }
}
