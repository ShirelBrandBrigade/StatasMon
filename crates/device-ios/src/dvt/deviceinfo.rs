//! `com.apple.instruments.server.services.deviceinfo` (step 4.5):
//! `hardwareInformation` (cores, and RAM on the builds that report it),
//! `machTimeInfo` (uptime from `mach_absolute_time` and the timebase) and
//! `runningProcesses` (the fallback process list when sysmontap is down).
//! Read at connect and refreshed at the slow cadence by the source's
//! tier-2 supervisor; [`apply`] overlays the facts on the tier-1 sheet.

use std::collections::BTreeMap;

use idevice::dvt::remote_server::Channel;
use idevice::{IdeviceError, ReadWrite};
use plist::{Dictionary, Value};
use statasmon_device::now_ms;
use statasmon_protocol::{DeviceInfo, ProcessRow, ProcessState};

use super::parse::{get_uint, uint};
use crate::error::IosError;
use crate::lockdown::render;
use crate::tunnel::{DvtHub, Tunnel};

/// The DVT channel identifier.
pub const SERVICE: &str = "com.apple.instruments.server.services.deviceinfo";
/// `DeviceInfo.connection.tier` at tier 2.
pub const TIER2: &str = "tier 2 (developer tunnel)";

/// What `hardwareInformation` told us.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareFacts {
    pub cpu_cores: Option<u32>,
    pub physical_cpus: Option<u32>,
    pub ram_bytes: Option<u64>,
    /// Every key, rendered, for `DeviceInfo.raw` (`dvt.hardwareInformation/<key>`).
    pub raw: BTreeMap<String, String>,
}

/// Parse the `hardwareInformation` dictionary. RAM keys differ by build
/// (`physicalMemory`, `hwMemSize`, `physMemSize`, `memorySize`); a value
/// under 256 MiB is not a byte count and is ignored.
pub fn parse_hardware(d: &Dictionary) -> HardwareFacts {
    let cores = |k: &str| {
        get_uint(d, k)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| *v > 0)
    };
    let ram = [
        "physicalMemory",
        "hwMemSize",
        "physMemSize",
        "memorySize",
        "PhysicalMemory",
    ]
    .iter()
    .find_map(|k| get_uint(d, k))
    .filter(|b| *b >= (1 << 28));
    HardwareFacts {
        cpu_cores: cores("numberOfCpus").or_else(|| cores("hw.ncpu")),
        physical_cpus: cores("numberOfPhysicalCpus"),
        ram_bytes: ram,
        raw: d.iter().map(|(k, v)| (k.clone(), render(v))).collect(),
    }
}

/// `machTimeInfo`: `[mach_absolute_time, numer, denom]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachTime {
    pub abs: u64,
    pub numer: u64,
    pub denom: u64,
}

impl MachTime {
    /// Seconds since boot.
    pub fn uptime_secs(&self) -> Option<u64> {
        if self.denom == 0 || self.numer == 0 {
            return None;
        }
        let ns = (self.abs as u128) * (self.numer as u128) / (self.denom as u128);
        u64::try_from(ns / 1_000_000_000).ok()
    }
}

/// Parse the `machTimeInfo` reply (an array, or a dictionary on some builds).
pub fn parse_mach_time(v: &Value) -> Option<MachTime> {
    match v {
        Value::Array(a) if a.len() >= 3 => Some(MachTime {
            abs: uint(&a[0])?,
            numer: uint(&a[1])?,
            denom: uint(&a[2])?,
        }),
        Value::Dictionary(d) => Some(MachTime {
            abs: get_uint(d, "mach_absolute_time").or_else(|| get_uint(d, "machAbsoluteTime"))?,
            numer: get_uint(d, "numer").unwrap_or(1),
            denom: get_uint(d, "denom").unwrap_or(1),
        }),
        _ => None,
    }
}

/// A `runningProcesses` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    pub pid: u32,
    pub name: String,
    pub is_application: bool,
    pub path: Option<String>,
}

/// Parse the `runningProcesses` reply.
pub fn parse_running_processes(v: &Value) -> Vec<RunningProcess> {
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(Value::as_dictionary)
        .filter_map(|d| {
            let pid = get_uint(d, "pid").and_then(|p| u32::try_from(p).ok())?;
            Some(RunningProcess {
                pid,
                name: d
                    .get("name")
                    .and_then(Value::as_string)
                    .unwrap_or("")
                    .to_owned(),
                is_application: d
                    .get("isApplication")
                    .and_then(Value::as_boolean)
                    .unwrap_or(false),
                path: d
                    .get("realAppName")
                    .and_then(Value::as_string)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// Process rows from the static list (no CPU/memory; the fallback table).
pub fn fallback_rows(list: &[RunningProcess]) -> Vec<ProcessRow> {
    list.iter()
        .map(|p| ProcessRow {
            pid: p.pid,
            name: p.name.clone(),
            app_id: None,
            cpu_pct: None,
            mem_bytes: None,
            threads: None,
            net_bps: None,
            state: ProcessState::Unknown,
            energy: None,
        })
        .collect()
}

/// Everything one deviceinfo pass returns.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeviceFacts {
    pub hardware: HardwareFacts,
    pub mach_time: Option<MachTime>,
    pub processes: Vec<RunningProcess>,
    /// Host time at the read (for `boot_time_ms`).
    pub read_at_ms: i64,
}

impl DeviceFacts {
    pub fn uptime_secs(&self) -> Option<u64> {
        self.mach_time.and_then(|m| m.uptime_secs())
    }

    pub fn boot_time_ms(&self) -> Option<i64> {
        let up = i64::try_from(self.uptime_secs()?).ok()?;
        Some(self.read_at_ms - up * 1000)
    }
}

async fn call<R: ReadWrite>(
    channel: &mut Channel<'_, R>,
    method: &str,
) -> Result<Value, IdeviceError> {
    channel
        .call_method(Some(Value::String(method.to_owned())), None, true)
        .await?;
    let msg = channel.read_message().await?;
    msg.data
        .ok_or_else(|| IdeviceError::UnexpectedResponse(format!("{method}: reply without data")))
}

/// One pass over the hub: hardware, mach time, processes.
pub async fn read_with(hub: &mut DvtHub) -> Result<DeviceFacts, IdeviceError> {
    let mut channel = hub.make_channel(SERVICE).await?;
    let hardware = match call(&mut channel, "hardwareInformation").await {
        Ok(Value::Dictionary(d)) => parse_hardware(&d),
        Ok(other) => {
            tracing::debug!(?other, "hardwareInformation: not a dictionary");
            HardwareFacts::default()
        }
        Err(e) if crate::tunnel::is_transport_error(&e) => return Err(e),
        Err(e) => {
            tracing::debug!(error = %e, "hardwareInformation failed");
            HardwareFacts::default()
        }
    };
    let mach_time = match call(&mut channel, "machTimeInfo").await {
        Ok(v) => parse_mach_time(&v),
        Err(e) if crate::tunnel::is_transport_error(&e) => return Err(e),
        Err(e) => {
            tracing::debug!(error = %e, "machTimeInfo failed");
            None
        }
    };
    let processes = match call(&mut channel, "runningProcesses").await {
        Ok(v) => parse_running_processes(&v),
        Err(e) if crate::tunnel::is_transport_error(&e) => return Err(e),
        Err(e) => {
            tracing::debug!(error = %e, "runningProcesses failed");
            Vec::new()
        }
    };
    Ok(DeviceFacts {
        hardware,
        mach_time,
        processes,
        read_at_ms: now_ms(),
    })
}

/// One pass on a fresh hub connection.
pub async fn read(tunnel: &Tunnel) -> Result<DeviceFacts, IosError> {
    let mut hub = tunnel.dvt().await?;
    read_with(&mut hub).await.map_err(|e| {
        tunnel.note_error("deviceinfo", &e);
        IosError::service("dvt/deviceinfo", e)
    })
}

/// Overlay the tier-2 facts on the tier-1 sheet: RAM and core count when
/// tier 1 had none, uptime and boot time, the tier and tunnel lines, and
/// the raw keys; the tier-1 "needs the tunnel" notes are dropped.
pub fn apply(info: &mut DeviceInfo, facts: &DeviceFacts, ram_fallback: Option<u64>, tunnel: &str) {
    if info.hardware.ram_bytes.is_none() {
        info.hardware.ram_bytes = facts.hardware.ram_bytes.or(ram_fallback);
    }
    if info.hardware.cpu_cores.is_none() {
        info.hardware.cpu_cores = facts.hardware.cpu_cores;
    }
    if let Some(up) = facts.uptime_secs() {
        info.state.uptime_secs = Some(up);
        info.state.boot_time_ms = facts.boot_time_ms();
    }
    info.connection.tier = Some(TIER2.to_owned());
    info.connection.tunnel = Some(tunnel.to_owned());
    info.connection.last_heartbeat_ms = Some(facts.read_at_ms);
    for (k, v) in &facts.hardware.raw {
        info.raw
            .insert(format!("dvt.hardwareInformation/{k}"), v.clone());
    }
    if let Some(m) = facts.mach_time {
        info.raw.insert(
            "dvt.machTimeInfo".to_owned(),
            format!("{} × {}/{}", m.abs, m.numer, m.denom),
        );
    }
    let mut notes: Vec<String> = info
        .raw
        .get("statasmon/notes")
        .map(|n| n.split("; ").map(str::to_owned).collect())
        .unwrap_or_default();
    notes.retain(|n| {
        !(n.starts_with("RAM:") && info.hardware.ram_bytes.is_some())
            && !(n.starts_with("Uptime:") && info.state.uptime_secs.is_some())
    });
    if info.hardware.ram_bytes.is_none() {
        notes.push("RAM: neither hardwareInformation nor sysmontap reported it".to_owned());
    }
    if info.state.uptime_secs.is_none() {
        notes.push("Uptime: machTimeInfo was not answered".to_owned());
    }
    if notes.is_empty() {
        info.raw.remove("statasmon/notes");
    } else {
        info.raw
            .insert("statasmon/notes".to_owned(), notes.join("; "));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hardware_and_mach_time_parse() {
        let mut d = Dictionary::new();
        d.insert("numberOfCpus".into(), Value::Integer(6i64.into()));
        d.insert("numberOfPhysicalCpus".into(), Value::Integer(6i64.into()));
        d.insert("hwCPU64BitCapable".into(), Value::Integer(1i64.into()));
        d.insert(
            "physicalMemory".into(),
            Value::Integer(4_294_967_296i64.into()),
        );
        let hw = parse_hardware(&d);
        assert_eq!(hw.cpu_cores, Some(6));
        assert_eq!(hw.physical_cpus, Some(6));
        assert_eq!(hw.ram_bytes, Some(4_294_967_296));
        assert_eq!(
            hw.raw.get("hwCPU64BitCapable").map(String::as_str),
            Some("1")
        );
        // A tiny "memory" value is not a byte count.
        d.insert("physicalMemory".into(), Value::Integer(4i64.into()));
        assert_eq!(parse_hardware(&d).ram_bytes, None);

        let mt = parse_mach_time(&Value::Array(vec![
            Value::Integer(3_600_000_000_000u64.into()),
            Value::Integer(125i64.into()),
            Value::Integer(3i64.into()),
        ]))
        .unwrap();
        // 3.6e12 ticks × 125/3 ns = 1.5e14 ns = 150 000 s.
        assert_eq!(mt.uptime_secs(), Some(150_000));
        assert_eq!(parse_mach_time(&Value::Array(vec![])), None);
        assert_eq!(
            MachTime {
                abs: 1,
                numer: 1,
                denom: 0
            }
            .uptime_secs(),
            None
        );
    }

    #[test]
    fn apply_overlays_the_tier1_sheet() {
        let mut info = DeviceInfo::default();
        info.raw.insert(
            "statasmon/notes".into(),
            "RAM: not reported at tier 1; Uptime: needs the developer tunnel (Phase 4); Other"
                .into(),
        );
        let facts = DeviceFacts {
            hardware: HardwareFacts {
                cpu_cores: Some(6),
                physical_cpus: Some(6),
                ram_bytes: None,
                raw: BTreeMap::new(),
            },
            mach_time: Some(MachTime {
                abs: 1_000_000_000,
                numer: 1,
                denom: 1,
            }),
            processes: vec![],
            read_at_ms: 10_000,
        };
        apply(&mut info, &facts, Some(4 << 30), "tunnel x");
        assert_eq!(info.hardware.ram_bytes, Some(4 << 30));
        assert_eq!(info.hardware.cpu_cores, Some(6));
        assert_eq!(info.state.uptime_secs, Some(1));
        assert_eq!(info.state.boot_time_ms, Some(9_000));
        assert_eq!(info.connection.tier.as_deref(), Some(TIER2));
        assert_eq!(info.connection.tunnel.as_deref(), Some("tunnel x"));
        assert_eq!(
            info.raw.get("statasmon/notes").map(String::as_str),
            Some("Other")
        );

        let rows = fallback_rows(&[RunningProcess {
            pid: 1,
            name: "launchd".into(),
            is_application: false,
            path: None,
        }]);
        assert_eq!(rows[0].name, "launchd");
        assert_eq!(rows[0].cpu_pct, None);
    }
}
