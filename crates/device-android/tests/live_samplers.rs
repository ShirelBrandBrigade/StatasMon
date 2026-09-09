//! Live sampler test against a connected device. Skipped unless
//! STATASMON_LIVE_ANDROID=<serial> and STATASMON_ADB=<path to adb> are set.
//! Prints the per-tick round-trip of every cadence.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use statasmon_device::EventSink;
use statasmon_device_android::logcat::{self, LogcatConfig};
use statasmon_device_android::probe::Probe;
use statasmon_device_android::samplers::{self, Shared};
use statasmon_device_android::{ProcessShell, Shell};
use statasmon_protocol::{Message, MetricId, ProcessState};
use tokio_util::sync::CancellationToken;

fn live() -> Option<ProcessShell> {
    let (Ok(serial), Ok(adb)) = (
        std::env::var("STATASMON_LIVE_ANDROID"),
        std::env::var("STATASMON_ADB"),
    ) else {
        eprintln!("skipped: set STATASMON_LIVE_ANDROID and STATASMON_ADB");
        return None;
    };
    Some(ProcessShell::new(adb, serial))
}

/// What the real probe says about this tablet (probe.rs is another agent's;
/// built by hand from fixtures/android/sm-x700-android16).
fn probe_sm_x700() -> Probe {
    Probe {
        sdk: 36,
        cpu_count: 8,
        cpu_online: (0..8).collect(),
        proc_stat: true,
        cpufreq: true,
        loadavg: true,
        meminfo: true,
        psi_memory: true,
        battery_dumpsys: true,
        power_supply_sysfs: false,
        thermalservice: true,
        thermal_sensors: true,
        thermal_headroom: true,
        wifi_cmd: true,
        proc_net_dev: true,
        netstats: true,
        diskstats: true,
        df: true,
        ps: true,
        proc_pid_stat: true,
        logcat: true,
        dropbox: true,
        activity_resumed: true,
        ..Probe::default()
    }
}

fn metric(msgs: &[Message], id: MetricId) -> Option<f64> {
    msgs.iter().find_map(|m| match m {
        Message::Samples { values, .. } => values.iter().find(|v| v.id == id).map(|v| v.value),
        _ => None,
    })
}

async fn drain(rx: &mut tokio::sync::mpsc::Receiver<Message>) -> Vec<Message> {
    let mut out = Vec::new();
    while let Ok(m) = rx.try_recv() {
        out.push(m);
    }
    out
}

#[tokio::test]
async fn live_samplers_two_ticks_each() {
    let Some(shell) = live() else { return };
    let shared = Shared::default();
    let mut samplers = samplers::build_shared(&probe_sm_x700(), &shared);
    assert_eq!(samplers.len(), 3);
    let (sink, mut rx) = EventSink::channel(256);
    let device = "android:live";

    let mut all: Vec<(String, Vec<Message>, Vec<Message>)> = Vec::new();
    for s in samplers.iter_mut() {
        let t0 = Instant::now();
        s.tick(&shell, &sink, device, 1).await.unwrap();
        let first_ms = t0.elapsed().as_millis();
        let first = drain(&mut rx).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let t1 = Instant::now();
        s.tick(&shell, &sink, device, 2).await.unwrap();
        let second_ms = t1.elapsed().as_millis();
        let second = drain(&mut rx).await;
        println!(
            "{}: tick 1 = {first_ms} ms ({} msgs), tick 2 = {second_ms} ms ({} msgs)",
            s.name(),
            first.len(),
            second.len()
        );
        all.push((s.name().to_owned(), first, second));
    }

    let (_, fast1, fast2) = &all[0];
    assert!(metric(fast1, MetricId::MemTotal).unwrap() > 4.0 * 1024.0 * 1024.0 * 1024.0);
    assert!(
        metric(fast1, MetricId::CpuTotal).is_none(),
        "no rate on tick 1"
    );
    let cpu = metric(fast2, MetricId::CpuTotal).unwrap();
    assert!((0.0..=100.0).contains(&cpu), "cpu.total {cpu}");
    assert!(
        metric(fast2, MetricId::NetRxBps).is_some(),
        "net.rx_bps on tick 2"
    );
    assert!(metric(fast2, MetricId::MemPressure).is_some());
    let cores = fast2
        .iter()
        .find_map(|m| match m {
            Message::Cores { cores, .. } => Some(cores.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(cores.len(), 8);
    assert!(cores.iter().all(|c| c.freq_mhz.is_some()));
    println!(
        "cores: {:?}",
        cores
            .iter()
            .map(|c| (c.index, c.kind, c.usage_pct, c.freq_mhz))
            .collect::<Vec<_>>()
    );

    let (_, med1, med2) = &all[1];
    assert!(metric(med1, MetricId::BatPercent).is_some());
    assert!(metric(med1, MetricId::BatCharging).is_some());
    let rows = med2
        .iter()
        .find_map(|m| match m {
            Message::Processes { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .unwrap();
    assert!(rows.len() > 50, "{} rows", rows.len());
    assert!(
        rows.iter().any(|r| r.state == ProcessState::Foreground),
        "a foreground app (the screen must be on)"
    );
    assert!(
        rows.iter().any(|r| r.cpu_pct.is_some()),
        "per-process CPU on tick 2"
    );
    let mut top: Vec<_> = rows.iter().filter(|r| r.cpu_pct.is_some()).collect();
    top.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap());
    println!(
        "top: {:?}",
        top.iter()
            .take(5)
            .map(|r| (r.pid, r.name.as_str(), r.cpu_pct, r.threads, r.state))
            .collect::<Vec<_>>()
    );
    assert!(shared.pids.read().len() > 50);

    let (_, slow1, _slow2) = &all[2];
    assert!(metric(slow1, MetricId::ThermalRaw).is_some());
    assert!(metric(slow1, MetricId::TempBattery).is_some());
    assert!(metric(slow1, MetricId::DiskTotal).is_some());
    assert!(metric(slow1, MetricId::NetConnections).is_some());
    println!(
        "slow: {:?}",
        slow1
            .iter()
            .find_map(|m| match m {
                Message::Samples { values, .. } => Some(
                    values
                        .iter()
                        .map(|v| (v.id.as_str(), v.value))
                        .collect::<Vec<_>>()
                ),
                _ => None,
            })
            .unwrap()
    );
    assert!(
        !shared.uid_net.read().is_empty(),
        "per-uid rates after two slow ticks"
    );
}

#[tokio::test]
async fn live_logcat_delivers_then_stops_on_cancel() {
    let Some(shell) = live() else { return };
    let shell: Arc<dyn Shell> = Arc::new(shell);
    let (sink, mut rx) = EventSink::channel(256);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(logcat::run(
        shell,
        sink,
        "android:live".into(),
        LogcatConfig::default(),
        cancel.clone(),
    ));
    let first = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("a Log batch within 10 s")
        .expect("sink open");
    let Message::Log { entries, .. } = first else {
        panic!("expected Log, got {first:?}");
    };
    assert!(!entries.is_empty());
    println!(
        "first batch: {} entries; first = {} {} {:?} {}",
        entries.len(),
        entries[0].process,
        entries[0].level as u8,
        entries[0].tag,
        entries[0].message.chars().take(60).collect::<String>()
    );
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("run() returns after cancel")
        .unwrap();
}
