//! `Probe::parse` against sections assembled from the recorded Galaxy Tab
//! S8 fixture (`fixtures/android/sm-x700-android16`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use statasmon_device_android::FixtureShell;
use statasmon_device_android::probe::{Probe, probe_command, split_sections};
use statasmon_protocol::Capability;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/android/sm-x700-android16")
}

/// Assemble the batched output the way the device would print it.
fn assembled() -> String {
    let fx = FixtureShell::load(fixture_dir()).unwrap();
    let head = |name: &str, n: usize| -> String {
        fx.file(name)
            .lines()
            .take(n)
            .map(|l| format!("{l}\n"))
            .collect()
    };
    let sections: Vec<(&str, String)> = vec![
        ("proc_stat", fx.file("proc_stat_1")),
        ("loadavg", fx.file("proc_loadavg")),
        ("meminfo", fx.file("proc_meminfo")),
        ("psi_memory", fx.file("proc_pressure_memory")),
        ("cpu_online", fx.file("cpu_online")),
        ("cpu_possible", "0-7\n".to_owned()),
        ("cpufreq", "614400\n".to_owned()),
        ("battery", fx.file("dumpsys_battery")),
        (
            "power_supply",
            "ls: /sys/class/power_supply/battery/: Permission denied\n".to_owned(),
        ),
        ("thermalservice", fx.file("dumpsys_thermalservice")),
        ("thermal_headroom", fx.file("thermal_headroom")),
        ("wifi", fx.file("cmd_wifi_status")),
        ("proc_net_dev", fx.file("proc_net_dev_1")),
        ("netstats", head("dumpsys_netstats_detail", 5)),
        ("diskstats", head("dumpsys_diskstats", 3)),
        (
            "df",
            "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/block/dm-58 110594012 29311476 81151464 27% /data\n".to_owned(),
        ),
        ("ps", head("ps", 3)),
        ("proc_1_stat", fx.file("proc_pid_stat_sample").lines().next().unwrap().to_owned() + "\n"),
        ("logcat", head("logcat_dump", 2)),
        ("dropbox", head("dumpsys_dropbox_list", 3)),
        ("activity", "2\n".to_owned()),
        ("sdk", "36\n".to_owned()),
        ("nproc", "6\n".to_owned()),
    ];
    let mut out = String::new();
    for (name, text) in sections {
        out.push_str(&format!("== {name}\n{text}"));
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[test]
fn every_section_of_the_command_is_covered_by_the_fixture() {
    let cmd = probe_command();
    let assembled = assembled();
    let sections = split_sections(&assembled);
    for token in cmd.split("echo '== ").skip(1) {
        let name = token.split('\'').next().unwrap();
        assert!(sections.contains_key(name), "fixture lacks section {name}");
    }
}

#[test]
fn galaxy_tab_s8_probe() {
    let p = Probe::parse(&assembled());
    assert_eq!(p.sdk, 36);
    assert_eq!(p.cpu_count, 8, "possible cpus win over nproc's cpuset view");
    assert_eq!(p.cpu_online, (0..=7).collect::<Vec<u16>>());
    assert!(p.proc_stat);
    assert!(p.cpufreq);
    assert!(p.loadavg);
    assert!(p.meminfo);
    assert!(p.psi_memory, "PSI is readable on this Samsung build");
    assert!(p.battery_dumpsys);
    assert!(
        !p.power_supply_sysfs,
        "battery sysfs is EACCES on this Samsung"
    );
    assert!(p.thermalservice);
    assert!(p.thermal_sensors, "AP/BAT/SKIN sensors are listed");
    assert!(p.thermal_headroom);
    assert!(p.wifi_cmd);
    assert!(p.proc_net_dev);
    assert!(p.netstats);
    assert!(p.diskstats);
    assert!(p.df);
    assert!(p.ps);
    assert!(p.proc_pid_stat);
    assert!(p.logcat);
    assert!(p.dropbox);
    assert!(p.activity_resumed);
    assert!(p.notes.iter().any(|n| n.contains("battery sysfs")));

    let caps = p.capabilities();
    assert_eq!(caps.entries.len(), Capability::ALL.len());
    for c in [
        Capability::DeviceInfo,
        Capability::CpuTotal,
        Capability::CpuPerCore,
        Capability::CpuFrequency,
        Capability::LoadAverage,
        Capability::MemoryBreakdown,
        Capability::MemoryPressure,
        Capability::ThermalState,
        Capability::TemperatureSensors,
        Capability::BatteryBasic,
        Capability::BatteryDetail,
        Capability::StorageTotals,
        Capability::StorageCategories,
        Capability::NetworkThroughput,
        Capability::NetworkPackets,
        Capability::NetworkPerProcess,
        Capability::WifiDetails,
        Capability::Processes,
        Capability::ForegroundApp,
        Capability::Logs,
        Capability::CrashReports,
    ] {
        assert!(caps.is_available(c), "{c:?} should be available");
    }
    assert_eq!(
        caps.reason(Capability::PacketCapture),
        Some("Needs root on Android")
    );
    assert_eq!(
        caps.reason(Capability::Energy),
        Some("Not available over ADB")
    );
}

#[test]
fn locked_down_device_has_a_reason_for_every_gap() {
    let mut text = assembled();
    text = text.replace(
        "== psi_memory\nsome",
        "== psi_memory\ncat: /proc/pressure/memory: Permission denied\nsome_removed",
    );
    let p = Probe::parse(&text);
    assert!(!p.psi_memory);
    let caps = p.capabilities();
    assert!(
        caps.reason(Capability::MemoryPressure)
            .unwrap()
            .contains("PSI")
    );
}
