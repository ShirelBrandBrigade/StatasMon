//! One-time probe at connect: which sources this device lets the `shell`
//! user read (docs/research/android.md). Samplers consult it; the
//! `CapabilitySet` sent to the UI is derived from it with honest reasons.
//!
//! The probe is ONE batched `sh -c` command that prints sentinel-separated
//! sections (`== name`), so a connect costs one adb round-trip (~0.5 s on
//! the Galaxy Tab S8) instead of twenty. It never enumerates
//! `/sys/class/thermal` zones (9 s on Samsung builds) and never runs
//! `dumpsys meminfo`.

use std::collections::BTreeMap;

use statasmon_protocol::{Availability, Capability, CapabilitySet};

use crate::shell::Shell;

/// Upper bound for the whole batch.
pub const PROBE_TIMEOUT_MS: u64 = 10_000;

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
    /// The thermal HAL lists at least one temperature with a name or a
    /// non-zero value (`thermalservice` alone only proves the status line).
    pub thermal_sensors: bool,
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

/// The sections of the batched command, in order: (sentinel, command).
const SECTIONS: &[(&str, &str)] = &[
    ("proc_stat", "cat /proc/stat"),
    ("loadavg", "cat /proc/loadavg"),
    ("meminfo", "cat /proc/meminfo"),
    ("psi_memory", "cat /proc/pressure/memory"),
    ("cpu_online", "cat /sys/devices/system/cpu/online"),
    ("cpu_possible", "cat /sys/devices/system/cpu/possible"),
    (
        "cpufreq",
        "cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
    ),
    ("battery", "dumpsys -t 5 battery"),
    ("power_supply", "ls /sys/class/power_supply/battery/"),
    ("thermalservice", "dumpsys -t 5 thermalservice"),
    ("thermal_headroom", "cmd thermalservice headroom 1"),
    ("wifi", "cmd wifi status"),
    ("proc_net_dev", "cat /proc/net/dev"),
    ("netstats", "dumpsys -t 5 netstats detail | head -5"),
    ("diskstats", "dumpsys -t 5 diskstats | head -3"),
    ("df", "df -k /data"),
    ("ps", "ps -A -o PID,NAME | head -3"),
    ("proc_1_stat", "cat /proc/1/stat"),
    ("logcat", "logcat -d -t 1 -v uid"),
    ("dropbox", "dumpsys -t 5 dropbox | head -3"),
    (
        "activity",
        "dumpsys -t 5 activity activities | grep -c ResumedActivity",
    ),
    ("sdk", "getprop ro.build.version.sdk"),
    ("nproc", "nproc"),
];

/// The batched shell command the probe runs.
pub fn probe_command() -> String {
    let mut cmd = String::new();
    for (name, c) in SECTIONS {
        cmd.push_str(&format!("echo '== {name}'; {c} 2>&1; "));
    }
    cmd
}

/// Split sentinel-separated output into `name → text` (text without the
/// sentinel line, trailing whitespace trimmed).
pub fn split_sections(output: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut buf = String::new();
    for line in output.lines() {
        if let Some(name) = line.strip_prefix("== ") {
            if let Some(name) = current.take() {
                out.insert(name, std::mem::take(&mut buf).trim_end().to_owned());
            }
            current = Some(name.trim().to_owned());
            continue;
        }
        if current.is_some() {
            buf.push_str(line);
            buf.push('\n');
        }
    }
    if let Some(name) = current {
        out.insert(name, buf.trim_end().to_owned());
    }
    out
}

/// Output that means "the shell user cannot read this".
fn denied(text: &str) -> bool {
    let t = text.trim();
    t.is_empty()
        || t.contains("Permission denied")
        || t.contains("No such file")
        || t.contains("Can't find service")
        || t.contains("not found")
        || t.contains("Operation not permitted")
        || t.starts_with("Error")
}

/// Parse a cpu list such as `0-7`, `0-3,5-7` or `0`.
pub fn parse_cpu_list(text: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for part in text.trim().split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('-') {
            Some((a, b)) => {
                if let (Ok(a), Ok(b)) = (a.parse::<u16>(), b.parse::<u16>())
                    && b >= a
                    && b - a < 1024
                {
                    out.extend(a..=b);
                }
            }
            None => {
                if let Ok(n) = part.parse::<u16>() {
                    out.push(n);
                }
            }
        }
    }
    out
}

/// One `Temperature{mValue=30.2, mType=0, mName=AP, mStatus=0}` line.
#[derive(Debug, Clone, PartialEq)]
pub struct HalTemperature {
    pub name: String,
    pub value: f64,
    pub kind: i32,
}

/// The `Current temperatures from HAL:` block of `dumpsys thermalservice`.
pub fn parse_hal_temperatures(text: &str) -> Vec<HalTemperature> {
    let mut out = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Current temperatures from HAL") {
            in_block = true;
            continue;
        }
        if in_block && !t.starts_with("Temperature{") {
            if t.ends_with(':') {
                in_block = false;
            }
            continue;
        }
        if !in_block {
            continue;
        }
        let body = t.trim_start_matches("Temperature{").trim_end_matches('}');
        let mut name = String::new();
        let mut value = f64::NAN;
        let mut kind = -1;
        for field in body.split(',') {
            if let Some((k, v)) = field.trim().split_once('=') {
                match k.trim() {
                    "mValue" => value = v.trim().parse().unwrap_or(f64::NAN),
                    "mType" => kind = v.trim().parse().unwrap_or(-1),
                    "mName" => name = v.trim().to_owned(),
                    _ => {}
                }
            }
        }
        if !value.is_nan() || !name.is_empty() {
            out.push(HalTemperature { name, value, kind });
        }
    }
    out
}

impl Probe {
    /// Run the batched probe once.
    pub async fn run(shell: &dyn Shell) -> Result<Probe, crate::shell::ShellError> {
        let out = shell.exec(&probe_command(), PROBE_TIMEOUT_MS).await?;
        Ok(Probe::parse(&out))
    }

    /// Build a probe from the batched command's output (pure; tested with
    /// sections assembled from the fixture files).
    pub fn parse(output: &str) -> Probe {
        let s = split_sections(output);
        let get = |k: &str| s.get(k).map(String::as_str).unwrap_or("");
        let mut notes = Vec::new();

        let proc_stat = get("proc_stat").lines().any(|l| l.starts_with("cpu "));
        let loadavg = get("loadavg")
            .split_whitespace()
            .next()
            .and_then(|f| f.parse::<f64>().ok())
            .is_some();
        let meminfo = get("meminfo").contains("MemTotal:");
        let psi_memory = get("psi_memory")
            .lines()
            .any(|l| l.starts_with("some ") && l.contains("avg10="));
        if !psi_memory && !get("psi_memory").is_empty() {
            notes.push("memory PSI: not readable by the shell user".to_owned());
        }
        let cpu_online = parse_cpu_list(get("cpu_online"));
        let cpu_possible = parse_cpu_list(get("cpu_possible"));
        let nproc: u16 = get("nproc").trim().parse().unwrap_or(0);
        // `nproc` honours the shell's cpuset (6 of 8 on the Galaxy Tab S8);
        // prefer the kernel's view of the package.
        let cpu_count = if !cpu_possible.is_empty() {
            cpu_possible.len() as u16
        } else if !cpu_online.is_empty() {
            cpu_online.len() as u16
        } else {
            nproc
        };
        let cpufreq = get("cpufreq")
            .trim()
            .parse::<u64>()
            .map(|f| f > 0)
            .unwrap_or(false);
        if !cpufreq {
            notes.push("cpufreq: scaling_cur_freq not readable".to_owned());
        }

        let battery_dumpsys = get("battery").contains("level:");
        let ps_text = get("power_supply");
        let power_supply_sysfs = !denied(ps_text) && ps_text.lines().count() > 1;
        if !power_supply_sysfs {
            notes.push(
                "battery sysfs (/sys/class/power_supply/battery): not readable; cycle count and design capacity unavailable"
                    .to_owned(),
            );
        }

        let thermal_text = get("thermalservice");
        let thermalservice = thermal_text.contains("Thermal Status:");
        let sensors = parse_hal_temperatures(thermal_text);
        let thermal_sensors = sensors
            .iter()
            .any(|t| !t.name.is_empty() || (t.value.is_finite() && t.value != 0.0));
        if thermalservice && !thermal_sensors {
            notes.push("thermal HAL: no named temperature sensors".to_owned());
        }
        notes.push("thermal zones (/sys/class/thermal): not enumerated by design".to_owned());
        let thermal_headroom = get("thermal_headroom").contains("Headroom");

        let wifi_cmd = get("wifi").contains("Wifi is");
        let proc_net_dev = get("proc_net_dev")
            .lines()
            .any(|l| l.contains(':') && l.split_whitespace().count() >= 10);
        let netstats_text = get("netstats");
        let netstats = !denied(netstats_text);
        let diskstats_text = get("diskstats");
        let diskstats =
            diskstats_text.contains("Data-Free:") || diskstats_text.contains("Latency:");
        let df = get("df").lines().any(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.len() >= 6 && f[1].parse::<u64>().is_ok() && f.last() == Some(&"/data")
        });
        let ps = {
            let lines: Vec<&str> = get("ps").lines().collect();
            lines.first().is_some_and(|h| h.contains("PID")) && lines.len() >= 2
        };
        let proc_pid_stat = get("proc_1_stat").trim_start().starts_with("1 (");
        let logcat_text = get("logcat");
        let logcat = !logcat_text.trim().is_empty()
            && !logcat_text.contains("Permission denied")
            && !logcat_text.contains("Unable to open")
            && !logcat_text.starts_with("logcat:");
        let dropbox = get("dropbox").contains("Drop box contents");
        let activity_resumed = get("activity")
            .trim()
            .parse::<u32>()
            .map(|n| n > 0)
            .unwrap_or(false);
        let sdk = get("sdk").trim().parse().unwrap_or(0);

        Probe {
            sdk,
            cpu_count,
            cpu_online,
            proc_stat,
            cpufreq,
            loadavg,
            meminfo,
            psi_memory,
            battery_dumpsys,
            power_supply_sysfs,
            thermalservice,
            thermal_sensors,
            thermal_headroom,
            wifi_cmd,
            proc_net_dev,
            netstats,
            diskstats,
            df,
            ps,
            proc_pid_stat,
            logcat,
            dropbox,
            activity_resumed,
            notes,
        }
    }

    /// The capability set for the UI, with a reason for every gap.
    pub fn capabilities(&self) -> CapabilitySet {
        use Capability as C;
        let mut set = CapabilitySet::default();
        let mut put = |cap: Capability, ok: bool, reason: &str| {
            set.set(
                cap,
                if ok {
                    Availability::Available
                } else {
                    Availability::Unavailable {
                        reason: reason.to_owned(),
                    }
                },
            );
        };
        let proc_stat_reason = "This device does not let the shell user read /proc/stat";
        put(C::DeviceInfo, true, "");
        put(C::CpuTotal, self.proc_stat, proc_stat_reason);
        put(C::CpuPerCore, self.proc_stat, proc_stat_reason);
        put(
            C::CpuFrequency,
            self.cpufreq,
            "cpufreq sysfs (scaling_cur_freq) is not readable on this device",
        );
        put(
            C::LoadAverage,
            self.loadavg,
            "/proc/loadavg is not readable on this device",
        );
        put(
            C::MemoryBreakdown,
            self.meminfo,
            "/proc/meminfo is not readable on this device",
        );
        put(
            C::MemoryPressure,
            self.psi_memory,
            "/proc/pressure/memory (PSI) is restricted to system processes on this device",
        );
        put(
            C::MemoryWarnings,
            self.logcat,
            "Low-memory kills come from logcat, which the shell user cannot read here",
        );
        put(
            C::ThermalState,
            self.thermalservice,
            "dumpsys thermalservice is not available on this device",
        );
        put(
            C::TemperatureSensors,
            self.thermal_sensors,
            "The thermal HAL on this device lists no readable temperature sensors",
        );
        put(
            C::BatteryBasic,
            self.battery_dumpsys,
            "dumpsys battery is not available on this device",
        );
        // Voltage, temperature and (on many OEM builds) current come from
        // dumpsys battery; only cycle count and design capacity need sysfs.
        put(
            C::BatteryDetail,
            self.battery_dumpsys,
            "dumpsys battery is not available on this device",
        );
        put(
            C::StorageTotals,
            self.df,
            "df cannot read /data on this device",
        );
        put(
            C::StorageCategories,
            self.diskstats,
            "dumpsys diskstats is not available on this device",
        );
        put(
            C::StoragePerApp,
            self.diskstats,
            "Per-app sizes come from dumpsys diskstats, which is not available here",
        );
        put(
            C::NetworkThroughput,
            self.proc_net_dev,
            "/proc/net/dev is not readable on this device",
        );
        put(
            C::NetworkPackets,
            self.proc_net_dev,
            "/proc/net/dev is not readable on this device",
        );
        put(
            C::NetworkPerProcess,
            self.netstats,
            "dumpsys netstats is not available on this device",
        );
        put(
            C::WifiDetails,
            self.wifi_cmd,
            "cmd wifi status is not available on this device",
        );
        put(C::PacketCapture, false, "Needs root on Android");
        put(
            C::Processes,
            self.ps && self.proc_pid_stat,
            "ps / /proc/<pid>/stat are not readable on this device",
        );
        put(
            C::ProcessThreads,
            self.proc_pid_stat,
            "/proc/<pid>/stat is not readable on this device",
        );
        put(
            C::ForegroundApp,
            self.activity_resumed,
            "dumpsys activity does not report a resumed activity on this device",
        );
        put(
            C::Logs,
            self.logcat,
            "logcat is not readable by the shell user on this device",
        );
        put(
            C::PerAppLogs,
            self.logcat,
            "logcat is not readable by the shell user on this device",
        );
        put(
            C::CrashReports,
            self.dropbox,
            "dumpsys dropbox is not available on this device",
        );
        put(C::Energy, false, "Not available over ADB");
        set
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn cpu_lists() {
        assert_eq!(parse_cpu_list("0-7"), (0..=7).collect::<Vec<u16>>());
        assert_eq!(parse_cpu_list("0-3,5-7\n"), vec![0, 1, 2, 3, 5, 6, 7]);
        assert_eq!(parse_cpu_list("0"), vec![0]);
        assert!(parse_cpu_list("").is_empty());
        assert!(parse_cpu_list("cat: no").is_empty());
    }

    #[test]
    fn sections_split_on_sentinels() {
        let s = split_sections("== a\nx\ny\n== b\n== c\nz\n");
        assert_eq!(s["a"], "x\ny");
        assert_eq!(s["b"], "");
        assert_eq!(s["c"], "z");
    }

    #[test]
    fn hal_temperatures() {
        let t = "Cached temperatures:\n\tTemperature{mValue=1.0, mType=2, mName=OLD, mStatus=0}\nCurrent temperatures from HAL:\n\tTemperature{mValue=30.2, mType=0, mName=AP, mStatus=0}\n\tTemperature{mValue=0.0, mType=4, mName=USB, mStatus=0}\nCurrent cooling devices from HAL:\n";
        let v = parse_hal_temperatures(t);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "AP");
        assert!((v[0].value - 30.2).abs() < 1e-9);
        assert_eq!(v[0].kind, 0);
    }

    #[test]
    fn command_has_every_sentinel_and_no_thermal_zones() {
        let c = probe_command();
        for (name, _) in SECTIONS {
            assert!(c.contains(&format!("== {name}")));
        }
        assert!(!c.contains("thermal_zone"));
        assert!(!c.contains("dumpsys -t 5 meminfo"));
    }

    #[test]
    fn empty_output_means_nothing_readable() {
        let p = Probe::parse("");
        assert!(!p.proc_stat && !p.meminfo && !p.logcat);
        let caps = p.capabilities();
        assert!(caps.is_available(Capability::DeviceInfo));
        assert!(!caps.is_available(Capability::CpuTotal));
        assert!(
            caps.reason(Capability::PacketCapture)
                .unwrap()
                .contains("root")
        );
        assert_eq!(caps.entries.len(), Capability::ALL.len());
        for c in Capability::ALL {
            assert!(
                caps.is_available(c) || !caps.reason(c).unwrap_or("").is_empty(),
                "{c:?} has no reason"
            );
        }
    }
}
