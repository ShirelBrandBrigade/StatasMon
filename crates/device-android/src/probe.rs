//! One-time probe at connect: which sources this device lets the `shell`
//! user read (docs/research/android.md). Samplers consult it; the
//! `CapabilitySet` sent to the UI is derived from it with honest reasons.

use statasmon_protocol::CapabilitySet;

use crate::shell::Shell;

/// What the probe found. Every `bool` is "readable and parseable".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    pub sdk: u32,
    pub cpu_count: u16,
    pub cpu_online: Vec<u16>,
    pub proc_stat: bool,
    pub cpufreq: bool,
    pub loadavg: bool,
    pub meminfo: bool,
    pub psi_memory: bool,
    pub battery_dumpsys: bool,
    pub power_supply_sysfs: bool,
    pub thermalservice: bool,
    pub thermal_headroom: bool,
    pub wifi_cmd: bool,
    pub proc_net_dev: bool,
    pub netstats: bool,
    pub diskstats: bool,
    pub df: bool,
    pub ps: bool,
    pub proc_pid_stat: bool,
    pub logcat: bool,
    pub dropbox: bool,
    pub activity_resumed: bool,
    /// Free-form notes for the Device screen ("thermal zones: EACCES").
    pub notes: Vec<String>,
}

impl Probe {
    /// Run every probe command once (bounded: never touch `/sys/class/thermal`
    /// zone by zone, which takes seconds on Samsung builds).
    pub async fn run(shell: &dyn Shell) -> Probe {
        let _ = shell;
        unimplemented!("implemented in step 2.2 (adb agent)")
    }

    /// The capability set for the UI, with a reason for every gap.
    pub fn capabilities(&self) -> CapabilitySet {
        unimplemented!("implemented in step 2.2 (adb agent)")
    }
}
