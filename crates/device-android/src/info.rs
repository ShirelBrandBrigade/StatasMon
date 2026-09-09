//! The full `DeviceInfo` sheet for an Android device (docs/FEATURES.md §1,
//! docs/research/android.md §1). Every parser is a pure function tested
//! against `fixtures/android/sm-x700-android16`; the command strings match
//! that fixture's `manifest.json` so `FixtureShell` answers them.

use std::collections::BTreeMap;

use statasmon_device::now_ms;
use statasmon_protocol::{
    ConnectionInfo, DeviceHardware, DeviceIdentity, DeviceInfo, DeviceSoftware, DeviceState,
    ToolVersion, Transport,
};

use crate::probe::{Probe, parse_cpu_list};
use crate::shell::{Shell, ShellError};

/// Timeout per info command (getprop is ~100 ms on the Tab S8).
const INFO_TIMEOUT_MS: u64 = 8_000;

/// Command strings (kept identical to `fixtures/android/*/manifest.json`).
pub const CMD_GETPROP: &str = "getprop";
pub const CMD_MEMINFO: &str = "cat /proc/meminfo";
pub const CMD_VERSION: &str = "cat /proc/version";
pub const CMD_UPTIME: &str = "cat /proc/uptime";
pub const CMD_SETTINGS: &str = "for k in development_settings_enabled adb_enabled adb_wifi_enabled; do printf '%s=' $k; settings get global $k; done";
pub const CMD_WM: &str = "wm size; wm density";
pub const CMD_GETENFORCE: &str = "getenforce";
pub const CMD_DF: &str = "df -k";
pub const CMD_CPU_ONLINE: &str = "cat /sys/devices/system/cpu/online";
pub const CMD_CPU_POSSIBLE: &str = "cat /sys/devices/system/cpu/possible";
pub const CMD_CPUFREQ_POLICIES: &str = "for p in /sys/devices/system/cpu/cpufreq/policy*; do echo \"== $p\"; cat $p/related_cpus 2>&1; cat $p/scaling_cur_freq 2>&1; cat $p/cpuinfo_max_freq 2>&1; done";
pub const CMD_DEVICE_NAME: &str = "settings get global device_name";

/// Prefixes of the properties copied verbatim into `DeviceInfo::raw`.
const RAW_PREFIXES: &[&str] = &[
    "ro.product.",
    "ro.build.",
    "ro.soc.",
    "ro.hardware",
    "ro.board.",
    "ro.bootloader",
    "ro.kernel.",
    "persist.sys.",
];
/// Upper bound on `raw` entries.
const RAW_LIMIT: usize = 256;

/// Parse `getprop` output (`[key]: [value]`, values may span lines).
pub fn parse_getprop(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut pending: Option<(String, String)> = None;
    for line in text.lines() {
        if let Some((key, value)) = pending.as_mut() {
            value.push('\n');
            value.push_str(line);
            if line.ends_with(']') {
                value.pop();
                out.insert(std::mem::take(key), std::mem::take(value));
                pending = None;
            }
            continue;
        }
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some((key, value)) = rest.split_once("]: [") else {
            continue;
        };
        if let Some(v) = value.strip_suffix(']') {
            out.insert(key.to_owned(), v.to_owned());
        } else {
            pending = Some((key.to_owned(), value.to_owned()));
        }
    }
    out
}

/// `MemTotal` from `/proc/meminfo`, in bytes.
pub fn parse_mem_total(text: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix("MemTotal:")?;
        let kb: u64 = v.split_whitespace().next()?.parse().ok()?;
        Some(kb * 1024)
    })
}

/// Kernel release from `/proc/version` ("5.10.236-android12-9-…").
pub fn parse_kernel_version(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix("Linux version ")?;
    rest.split_whitespace().next().map(str::to_owned)
}

/// Seconds since boot from `/proc/uptime`.
pub fn parse_uptime_secs(text: &str) -> Option<u64> {
    let first: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(first.max(0.0) as u64)
}

/// `key=value` lines of the settings batch → bool per key (`1` = on).
pub fn parse_settings_flags(text: &str) -> BTreeMap<String, Option<bool>> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim();
            let flag = match v {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            };
            out.insert(k.trim().to_owned(), flag);
        }
    }
    out
}

/// `wm size; wm density` → "1600×2560 @ 340 dpi".
pub fn parse_screen(text: &str) -> Option<String> {
    let mut size = None;
    let mut density = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Physical size:") {
            size = Some(v.trim().replace('x', "×"));
        } else if let Some(v) = t.strip_prefix("Override size:") {
            size = Some(v.trim().replace('x', "×"));
        } else if let Some(v) = t.strip_prefix("Physical density:") {
            density = Some(v.trim().to_owned());
        }
    }
    match (size, density) {
        (Some(s), Some(d)) => Some(format!("{s} @ {d} dpi")),
        (Some(s), None) => Some(s),
        _ => None,
    }
}

/// Total size (bytes) of the filesystem mounted at `mount` in `df -k` output.
pub fn parse_df_total(text: &str, mount: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() >= 6 && f[f.len() - 1] == mount {
            f[1].parse::<u64>().ok().map(|kb| kb * 1024)
        } else {
            None
        }
    })
}

/// One cpufreq policy: which cpus it governs and its maximum frequency (kHz).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuPolicy {
    pub name: String,
    pub cpus: Vec<u16>,
    pub cur_khz: Option<u64>,
    pub max_khz: Option<u64>,
}

/// Parse the policies batch (`== /sys/.../policyN`, related_cpus, cur, max).
pub fn parse_cpu_policies(text: &str) -> Vec<CpuPolicy> {
    let mut out = Vec::new();
    let mut current: Option<CpuPolicy> = None;
    let mut line_no = 0usize;
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("== ") {
            if let Some(p) = current.take() {
                out.push(p);
            }
            let name = path.rsplit('/').next().unwrap_or(path).to_owned();
            current = Some(CpuPolicy {
                name,
                cpus: Vec::new(),
                cur_khz: None,
                max_khz: None,
            });
            line_no = 0;
            continue;
        }
        let Some(p) = current.as_mut() else {
            continue;
        };
        match line_no {
            0 => {
                p.cpus = line
                    .split_whitespace()
                    .filter_map(|c| c.parse().ok())
                    .collect();
            }
            1 => p.cur_khz = line.trim().parse().ok(),
            2 => p.max_khz = line.trim().parse().ok(),
            _ => {}
        }
        line_no += 1;
    }
    if let Some(p) = current {
        out.push(p);
    }
    out
}

/// Split cores into performance/efficiency groups from the policies.
///
/// Heuristic (documented in ADR-009): the policy group with the LOWEST
/// `cpuinfo_max_freq` is the efficiency cluster; every other policy is
/// performance (prime + gold on a 1+3+4 Snapdragon → 4 performance,
/// 4 efficiency). With a single policy the split is unknown and both are
/// `None`.
pub fn core_split(policies: &[CpuPolicy]) -> (Option<u32>, Option<u32>) {
    let with_max: Vec<&CpuPolicy> = policies.iter().filter(|p| p.max_khz.is_some()).collect();
    let Some(min) = with_max.iter().filter_map(|p| p.max_khz).min() else {
        return (None, None);
    };
    let distinct: std::collections::BTreeSet<u64> =
        with_max.iter().filter_map(|p| p.max_khz).collect();
    if distinct.len() < 2 {
        return (None, None);
    }
    let eff: usize = with_max
        .iter()
        .filter(|p| p.max_khz == Some(min))
        .map(|p| p.cpus.len())
        .sum();
    let perf: usize = with_max
        .iter()
        .filter(|p| p.max_khz != Some(min))
        .map(|p| p.cpus.len())
        .sum();
    (Some(perf as u32), Some(eff as u32))
}

fn title_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn non_empty(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "unknown")
        .map(str::to_owned)
}

/// Raw command outputs the sheet is built from (so the builder is pure).
#[derive(Debug, Clone, Default)]
pub struct InfoInputs {
    pub getprop: String,
    pub meminfo: String,
    pub version: String,
    pub uptime: String,
    pub settings: String,
    pub wm: String,
    pub getenforce: String,
    pub df: String,
    pub cpu_online: String,
    pub cpu_possible: String,
    pub cpu_policies: String,
    pub device_name: String,
}

/// Connection facts the transport knows and the device does not.
#[derive(Debug, Clone, Default)]
pub struct ConnectionFacts {
    /// The adb serial (`unique_id`).
    pub serial: String,
    pub transport: Option<Transport>,
    pub tools: Vec<ToolVersion>,
    pub pairing: Option<String>,
}

/// Build the sheet from raw outputs (pure).
pub fn build_info(
    inputs: &InfoInputs,
    probe: &Probe,
    facts: &ConnectionFacts,
    now_ms: i64,
) -> DeviceInfo {
    let props = parse_getprop(&inputs.getprop);
    let prop = |k: &str| non_empty(props.get(k));

    let model = prop("ro.product.model");
    let brand = prop("ro.product.brand").or_else(|| prop("ro.product.manufacturer"));
    let marketing_name = match (&brand, &model) {
        (Some(b), Some(m)) => Some(format!("{} {}", title_case(b), m)),
        (None, Some(m)) => Some(m.clone()),
        _ => None,
    };
    let device_name = {
        let n = inputs.device_name.trim();
        (!n.is_empty() && n != "null").then(|| n.to_owned())
    };

    let identity = DeviceIdentity {
        name: device_name,
        marketing_name,
        model_identifier: model,
        model_number: prop("ro.product.name"),
        manufacturer: prop("ro.product.manufacturer"),
        serial_number: prop("ro.serialno")
            .or_else(|| (!facts.serial.is_empty()).then(|| facts.serial.clone())),
        unique_id: (!facts.serial.is_empty()).then(|| facts.serial.clone()),
        region: prop("ro.csc.country_code").or_else(|| prop("ro.product.locale")),
    };

    let sdk = prop("ro.build.version.sdk")
        .and_then(|s| s.parse::<u32>().ok())
        .or(if probe.sdk > 0 { Some(probe.sdk) } else { None });
    let software = DeviceSoftware {
        os_name: Some("Android".to_owned()),
        os_version: prop("ro.build.version.release"),
        os_build: prop("ro.build.display.id").or_else(|| prop("ro.build.id")),
        security_patch: prop("ro.build.version.security_patch"),
        kernel_version: parse_kernel_version(&inputs.version).or_else(|| prop("ro.kernel.version")),
        api_level: sdk,
        firmware_version: prop("gsm.version.baseband"),
    };

    let soc_model = prop("ro.soc.model")
        .or_else(|| prop("ro.hardware.chipname"))
        .or_else(|| prop("ro.board.platform"))
        .or_else(|| prop("ro.hardware"));
    let chip_name = soc_model
        .as_deref()
        .and_then(statasmon_idmap::soc_marketing_name)
        .map(str::to_owned)
        .or_else(|| {
            prop("ro.board.platform")
                .as_deref()
                .and_then(statasmon_idmap::soc_marketing_name)
                .map(str::to_owned)
        })
        .or_else(|| soc_model.clone());
    let chip_manufacturer = prop("ro.soc.manufacturer").map(|m| {
        statasmon_idmap::soc_manufacturer_name(&m)
            .map(str::to_owned)
            .unwrap_or(m)
    });
    let abis: Vec<String> = prop("ro.product.cpu.abilist")
        .map(|l| {
            l.split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let architecture = abis.first().cloned().or_else(|| prop("ro.product.cpu.abi"));
    let policies = parse_cpu_policies(&inputs.cpu_policies);
    let (performance_cores, efficiency_cores) = core_split(&policies);
    let cpu_max_mhz = policies
        .iter()
        .filter_map(|p| p.max_khz)
        .max()
        .map(|khz| (khz / 1000) as u32);
    let possible = parse_cpu_list(&inputs.cpu_possible);
    let online = parse_cpu_list(&inputs.cpu_online);
    let cpu_cores = if !possible.is_empty() {
        Some(possible.len() as u32)
    } else if !online.is_empty() {
        Some(online.len() as u32)
    } else if probe.cpu_count > 0 {
        Some(u32::from(probe.cpu_count))
    } else {
        None
    };
    let gpu_name = prop("ro.hardware.egl")
        .or_else(|| prop("ro.hardware.vulkan"))
        .map(|g| title_case(&g));
    let hardware = DeviceHardware {
        chip_name,
        chip_identifier: soc_model,
        chip_manufacturer,
        architecture,
        abis,
        cpu_cores,
        performance_cores,
        efficiency_cores,
        cpu_max_mhz,
        gpu_name,
        ram_bytes: parse_mem_total(&inputs.meminfo),
        storage_bytes: parse_df_total(&inputs.df, "/data"),
        battery_design_mah: None,
        board: prop("ro.product.board").or_else(|| prop("ro.board.platform")),
        screen: parse_screen(&inputs.wm),
    };

    let flags = parse_settings_flags(&inputs.settings);
    let flag = |k: &str| flags.get(k).copied().flatten();
    let uptime_secs = parse_uptime_secs(&inputs.uptime);
    let selinux = inputs.getenforce.trim();
    let state = DeviceState {
        uptime_secs,
        boot_time_ms: uptime_secs.map(|u| now_ms - (u as i64) * 1000),
        time_zone: prop("persist.sys.timezone"),
        locale: prop("persist.sys.locale").or_else(|| prop("ro.product.locale")),
        developer_mode: flag("development_settings_enabled"),
        usb_debugging: flag("adb_enabled"),
        wireless_debugging: flag("adb_wifi_enabled"),
        selinux_mode: (!selinux.is_empty() && !selinux.contains("not found"))
            .then(|| selinux.to_owned()),
    };

    let connection = ConnectionInfo {
        transport: facts.transport,
        tier: Some("full".to_owned()),
        pairing: facts.pairing.clone(),
        tunnel: None,
        last_heartbeat_ms: Some(now_ms),
        tools: facts.tools.clone(),
    };

    let mut raw: BTreeMap<String, String> = props
        .iter()
        .filter(|(k, _)| RAW_PREFIXES.iter().any(|p| k.starts_with(p)))
        .take(RAW_LIMIT)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if !policies.is_empty() {
        let desc: Vec<String> = policies
            .iter()
            .map(|p| {
                format!(
                    "{}: cpus {:?} max {} MHz",
                    p.name,
                    p.cpus,
                    p.max_khz.map(|k| k / 1000).unwrap_or(0)
                )
            })
            .collect();
        raw.insert("cpufreq.policies".to_owned(), desc.join("; "));
    }
    if !online.is_empty() {
        raw.insert("cpu.online".to_owned(), inputs.cpu_online.trim().to_owned());
    }
    for (i, note) in probe.notes.iter().enumerate() {
        raw.insert(format!("probe.note.{i}"), note.clone());
    }

    DeviceInfo {
        identity,
        software,
        hardware,
        state,
        connection,
        raw,
    }
}

/// Collect the inputs from the device (all commands run concurrently).
pub async fn read_inputs(shell: &dyn Shell) -> Result<InfoInputs, ShellError> {
    let t = INFO_TIMEOUT_MS;
    let (
        getprop,
        meminfo,
        version,
        uptime,
        settings,
        wm,
        getenforce,
        df,
        cpu_online,
        cpu_possible,
        cpu_policies,
        device_name,
    ) = tokio::try_join!(
        shell.exec(CMD_GETPROP, t),
        shell.exec(CMD_MEMINFO, t),
        shell.exec(CMD_VERSION, t),
        shell.exec(CMD_UPTIME, t),
        shell.exec(CMD_SETTINGS, t),
        shell.exec(CMD_WM, t),
        shell.exec(CMD_GETENFORCE, t),
        shell.exec(CMD_DF, t),
        shell.exec(CMD_CPU_ONLINE, t),
        shell.exec(CMD_CPU_POSSIBLE, t),
        shell.exec(CMD_CPUFREQ_POLICIES, t),
        shell.exec(CMD_DEVICE_NAME, t),
    )?;
    Ok(InfoInputs {
        getprop,
        meminfo,
        version,
        uptime,
        settings,
        wm,
        getenforce,
        df,
        cpu_online,
        cpu_possible,
        cpu_policies,
        device_name,
    })
}

/// Read and build the sheet. `facts` come from the transport (serial,
/// USB/TCP, adb version).
pub async fn read_info(
    shell: &dyn Shell,
    probe: &Probe,
    facts: &ConnectionFacts,
) -> Result<DeviceInfo, ShellError> {
    let inputs = read_inputs(shell).await?;
    Ok(build_info(&inputs, probe, facts, now_ms()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn getprop_multiline_values() {
        let p = parse_getprop("[a]: [1]\n[b]: [x\ny]\n[c]: [3]\nnoise\n");
        assert_eq!(p["a"], "1");
        assert_eq!(p["b"], "x\ny");
        assert_eq!(p["c"], "3");
    }

    #[test]
    fn small_parsers() {
        assert_eq!(
            parse_mem_total("MemTotal:  7467144 kB\n"),
            Some(7_467_144 * 1024)
        );
        assert_eq!(
            parse_kernel_version("Linux version 5.10.236-android12-9 (u@h) (clang) #1 SMP"),
            Some("5.10.236-android12-9".to_owned())
        );
        assert_eq!(parse_uptime_secs("88923.37 105917.08\n"), Some(88_923));
        assert_eq!(
            parse_screen("Physical size: 1600x2560\nPhysical density: 340\n"),
            Some("1600×2560 @ 340 dpi".to_owned())
        );
        assert_eq!(
            parse_df_total(
                "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/block/dm-58 110594012 29311476 81151464 27% /data\n",
                "/data"
            ),
            Some(110_594_012 * 1024)
        );
        let f = parse_settings_flags(
            "development_settings_enabled=1\nadb_enabled=1\nadb_wifi_enabled=0\nx=null\n",
        );
        assert_eq!(f["adb_enabled"], Some(true));
        assert_eq!(f["adb_wifi_enabled"], Some(false));
        assert_eq!(f["x"], None);
    }

    #[test]
    fn policies_and_core_split() {
        let text = "== /sys/devices/system/cpu/cpufreq/policy0\n0 1 2 3\n614400\n1785600\n== /sys/devices/system/cpu/cpufreq/policy4\n4 5 6\n2496000\n2496000\n== /sys/devices/system/cpu/cpufreq/policy7\n7\n806400\n2995200\n";
        let p = parse_cpu_policies(text);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].name, "policy0");
        assert_eq!(p[0].cpus, vec![0, 1, 2, 3]);
        assert_eq!(p[2].max_khz, Some(2_995_200));
        assert_eq!(core_split(&p), (Some(4), Some(4)));
        assert_eq!(core_split(&p[..1]), (None, None));
        assert_eq!(core_split(&[]), (None, None));
    }
}
