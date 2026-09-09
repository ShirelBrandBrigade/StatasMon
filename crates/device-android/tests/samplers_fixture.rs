//! Sampler and logcat behaviour against the recorded SM-X700 fixtures: a
//! scripted shell answers each batched tick with fixture sections glued
//! together by the same sentinels the samplers emit.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::VecDeque;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use statasmon_device::EventSink;
use statasmon_device_android::logcat::{self, LogcatConfig};
use statasmon_device_android::probe::Probe;
use statasmon_device_android::samplers::{self, Shared};
use statasmon_device_android::{FixtureShell, Shell, ShellError, ShellStream};
use statasmon_protocol::{CoreKind, EventKind, Message, MetricId, ProcessState};
use tokio_util::sync::CancellationToken;

fn fixtures() -> FixtureShell {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/android/sm-x700-android16");
    FixtureShell::load(dir).unwrap()
}

/// Glue `(section, body)` pairs with the sampler sentinels.
fn compose(sections: &[(&str, String)]) -> String {
    let mut out = String::new();
    for (name, body) in sections {
        out.push_str(&format!("==[{name}]==\n"));
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Answers `exec` with queued outputs in order; `stream` with one text.
#[derive(Debug)]
struct SeqShell {
    outputs: Mutex<VecDeque<String>>,
    stream_text: Mutex<Option<String>>,
    exec_after_stream: Mutex<Option<Result<String, ()>>>,
    cmds: Mutex<Vec<String>>,
}

impl SeqShell {
    fn new(outputs: Vec<String>) -> Self {
        Self {
            outputs: Mutex::new(outputs.into()),
            stream_text: Mutex::new(None),
            exec_after_stream: Mutex::new(None),
            cmds: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Shell for SeqShell {
    fn serial(&self) -> &str {
        "seq"
    }
    async fn exec(&self, cmd: &str, _timeout_ms: u64) -> Result<String, ShellError> {
        self.cmds.lock().push(cmd.to_owned());
        if let Some(r) = self.exec_after_stream.lock().clone() {
            return r.map_err(|()| ShellError::DeviceGone("seq".into()));
        }
        Ok(self.outputs.lock().pop_front().unwrap_or_default())
    }
    async fn stream(&self, _cmd: &str) -> Result<ShellStream, ShellError> {
        let text = self.stream_text.lock().take().unwrap_or_default();
        Ok(Box::pin(std::io::Cursor::new(text.into_bytes())))
    }
}

fn probe_all() -> Probe {
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

fn samples(msgs: &[Message]) -> Vec<(MetricId, f64)> {
    msgs.iter()
        .filter_map(|m| match m {
            Message::Samples { values, .. } => {
                Some(values.iter().map(|v| (v.id, v.value)).collect::<Vec<_>>())
            }
            _ => None,
        })
        .flatten()
        .collect()
}

fn get(msgs: &[Message], id: MetricId) -> Option<f64> {
    samples(msgs)
        .into_iter()
        .find(|(i, _)| *i == id)
        .map(|(_, v)| v)
}

async fn drain(rx: &mut tokio::sync::mpsc::Receiver<Message>) -> Vec<Message> {
    let mut out = Vec::new();
    while let Ok(m) = rx.try_recv() {
        out.push(m);
    }
    out
}

fn max_freq_grep() -> String {
    let maxes = [
        1_785_600u64,
        1_785_600,
        1_785_600,
        1_785_600,
        2_496_000,
        2_496_000,
        2_496_000,
        2_995_200,
    ];
    maxes
        .iter()
        .enumerate()
        .map(|(i, m)| format!("/sys/devices/system/cpu/cpu{i}/cpufreq/cpuinfo_max_freq:{m}\n"))
        .collect()
}

#[tokio::test]
async fn fast_sampler_two_ticks() {
    let f = fixtures();
    let common = |stat: &str| {
        vec![
            ("stat", f.file(stat)),
            ("online", f.file("cpu_online")),
            ("freq", f.file("cpufreq")),
            ("maxfreq", max_freq_grep()),
            ("meminfo", f.file("proc_meminfo")),
            ("psi", f.file("proc_pressure_memory")),
            ("loadavg", f.file("proc_loadavg")),
            ("netdev", f.file("proc_net_dev_1")),
        ]
    };
    let shell = SeqShell::new(vec![
        compose(&common("proc_stat_1")),
        compose(&common("proc_stat_2")),
    ]);
    let mut samplers = samplers::build(&probe_all());
    let fast = samplers
        .iter_mut()
        .find(|s| s.name() == "android-fast")
        .unwrap();
    let (sink, mut rx) = EventSink::channel(64);

    fast.tick(&shell, &sink, "android:fixture", 1_000)
        .await
        .unwrap();
    let first = drain(&mut rx).await;
    assert_eq!(get(&first, MetricId::MemTotal), Some(7_467_144.0 * 1024.0));
    assert_eq!(get(&first, MetricId::MemPressure), Some(0.0));
    assert_eq!(get(&first, MetricId::CpuLoad1), Some(2.27));
    assert_eq!(
        get(&first, MetricId::NetRxTotal),
        Some(0.0),
        "relative to first tick"
    );
    assert_eq!(get(&first, MetricId::NetDropped), Some(0.0));
    assert!(
        get(&first, MetricId::CpuTotal).is_none(),
        "no rate on the first tick"
    );
    assert!(get(&first, MetricId::NetRxBps).is_none());
    assert!(!first.iter().any(|m| matches!(m, Message::Cores { .. })));

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    fast.tick(&shell, &sink, "android:fixture", 2_000)
        .await
        .unwrap();
    let second = drain(&mut rx).await;
    let total = get(&second, MetricId::CpuTotal).unwrap();
    assert!(
        (0.0..=100.0).contains(&total) && total > 0.0,
        "cpu.total {total}"
    );
    let user = get(&second, MetricId::CpuUser).unwrap();
    let system = get(&second, MetricId::CpuSystem).unwrap();
    assert!(user + system <= total + 0.01);
    assert_eq!(
        get(&second, MetricId::NetRxBps),
        Some(0.0),
        "idle second → 0 bit/s"
    );
    assert_eq!(get(&second, MetricId::NetTxPps), Some(0.0));
    let cores = second
        .iter()
        .find_map(|m| match m {
            Message::Cores { cores, .. } => Some(cores.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(cores.len(), 8);
    assert!(cores.iter().all(|c| c.online));
    assert_eq!(cores[0].kind, CoreKind::Efficiency);
    assert_eq!(
        cores[4].kind,
        CoreKind::Efficiency,
        "mid cluster is not the top tier"
    );
    assert_eq!(cores[7].kind, CoreKind::Performance);
    assert_eq!(cores[0].freq_mhz, Some(614));
    assert_eq!(cores[7].freq_mhz, Some(806));
    assert_eq!(
        cores[7].usage_pct, 0.0,
        "cpu7 was fully idle in the capture"
    );
    assert!(cores[0].usage_pct > 0.0 && cores[0].usage_pct <= 100.0);
    // The second command must not re-read cpuinfo_max_freq.
    let cmds = shell.cmds.lock();
    assert!(cmds[0].contains("cpuinfo_max_freq"));
    assert!(!cmds[1].contains("cpuinfo_max_freq"));
    assert!(cmds[1].contains("cat /proc/stat"));
}

#[tokio::test]
async fn medium_sampler_processes_battery_wifi() {
    let f = fixtures();
    let stat1 = f.file("proc_pid_stat_sample");
    // Second tick: init (pid 1) burned 50 more ticks.
    let stat2 = stat1.replacen(
        "1 (init) S 0 0 0 0 -1 1077936384 24311 1521042 1790 126676 163 325",
        "1 (init) S 0 0 0 0 -1 1077936384 24311 1521042 1790 126676 193 345",
        1,
    );
    assert_ne!(stat1, stat2);
    let tick = |stat: String| {
        compose(&[
            ("ps", f.file("ps")),
            ("pidstat", stat),
            ("resumed", f.file("dumpsys_activity_resumed")),
            ("battery", f.file("dumpsys_battery")),
            ("wifi", f.file("cmd_wifi_status")),
        ])
    };
    let shell = SeqShell::new(vec![tick(stat1), tick(stat2)]);
    let shared = Shared::default();
    shared.uid_net.write().insert(10055, 123_456.0); // systemui, from a slow tick
    let mut samplers = samplers::build_shared(&probe_all(), &shared);
    let medium = samplers
        .iter_mut()
        .find(|s| s.name() == "android-medium")
        .unwrap();
    let (sink, mut rx) = EventSink::channel(64);

    medium
        .tick(&shell, &sink, "android:fixture", 1_000)
        .await
        .unwrap();
    let first = drain(&mut rx).await;
    assert_eq!(get(&first, MetricId::BatPercent), Some(100.0));
    assert_eq!(get(&first, MetricId::BatVoltage), Some(4.365));
    assert_eq!(get(&first, MetricId::BatTemp), Some(32.8));
    assert_eq!(get(&first, MetricId::BatCharging), Some(1.0));
    assert!(
        get(&first, MetricId::BatCurrent).is_none(),
        "sysfs denied → no current"
    );
    assert!(get(&first, MetricId::BatHealth).is_none());
    assert_eq!(get(&first, MetricId::NetRssi), Some(-62.0));
    assert_eq!(get(&first, MetricId::NetLinkMbps), Some(300.0));
    assert_eq!(get(&first, MetricId::NetRetransmits), Some(6450.0));
    assert!(get(&first, MetricId::ProcCount).unwrap() > 500.0);
    let conn: Vec<&Message> = first
        .iter()
        .filter(|m| {
            matches!(
                m,
                Message::Event {
                    kind: EventKind::Connection,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(conn.len(), 1);
    if let Message::Event { detail, .. } = conn[0] {
        assert_eq!(detail, "Wi-Fi Studio-5G · 5180 MHz · 11ac");
    }
    let rows = first
        .iter()
        .find_map(|m| match m {
            Message::Processes { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .unwrap();
    assert!(rows.len() > 500);
    let settings = rows
        .iter()
        .find(|r| r.name == "com.android.settings")
        .unwrap();
    assert_eq!(settings.state, ProcessState::Foreground);
    assert_eq!(settings.app_id.as_deref(), Some("com.android.settings"));
    assert_eq!(settings.mem_bytes, Some(199_680 * 1024));
    assert!(
        settings.cpu_pct.is_none(),
        "no per-process rate on the first tick"
    );
    let sysui = rows
        .iter()
        .find(|r| r.name == "com.android.systemui")
        .unwrap();
    assert_eq!(sysui.state, ProcessState::Background);
    assert_eq!(sysui.net_bps, Some(123_456.0), "joined by uid 10055");
    let init = rows.iter().find(|r| r.pid == 1).unwrap();
    assert_eq!(init.threads, Some(3));
    assert_eq!(init.app_id, None);
    assert_eq!(
        shared.pids.read().get(&3133).map(String::as_str),
        Some("com.android.systemui")
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    medium
        .tick(&shell, &sink, "android:fixture", 2_000)
        .await
        .unwrap();
    let second = drain(&mut rx).await;
    assert!(
        !second.iter().any(|m| matches!(m, Message::Event { .. })),
        "unchanged Wi-Fi → no second Connection event"
    );
    let rows = second
        .iter()
        .find_map(|m| match m {
            Message::Processes { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .unwrap();
    let init = rows.iter().find(|r| r.pid == 1).unwrap();
    let cpu = init.cpu_pct.expect("delta available on the second tick");
    assert!(cpu > 0.0, "init cpu {cpu}");
    let sysui = rows.iter().find(|r| r.pid == 3133).unwrap();
    assert_eq!(
        sysui.cpu_pct, None,
        "pid 3133 is not in the 40-pid stat sample"
    );
}

#[tokio::test]
async fn slow_sampler_thermal_disk_netstats() {
    let f = fixtures();
    let tcp = "  sl  local_address rem_address   st tx_queue rx_queue\n\
               \x20  0: D532A8C0:B43E 2E4BFA8E:01BB 01 00000000:00000000 00:00000000 00000000 10214\n\
               \x20  1: D532A8C0:CE0E 139B6652:01BB 08 00000000:00000000 00:00000000 00000000 10214\n";
    let thermal2 = f
        .file("dumpsys_thermalservice")
        .replace("Thermal Status: 0", "Thermal Status: 2");
    let netstats2 = f
        .file("dumpsys_netstats_detail")
        .replacen("rb=705971", "rb=805971", 1);
    let tick = |thermal: String, netstats: String| {
        compose(&[
            ("thermal", thermal),
            ("netstats", netstats),
            ("diskstats", f.file("dumpsys_diskstats")),
            ("df", f.file("df")),
            ("tcp", tcp.to_owned()),
        ])
    };
    let shell = SeqShell::new(vec![
        tick(
            f.file("dumpsys_thermalservice"),
            f.file("dumpsys_netstats_detail"),
        ),
        tick(thermal2, netstats2),
    ]);
    let shared = Shared::default();
    let mut samplers = samplers::build_shared(&probe_all(), &shared);
    let slow = samplers
        .iter_mut()
        .find(|s| s.name() == "android-slow")
        .unwrap();
    let (sink, mut rx) = EventSink::channel(64);

    slow.tick(&shell, &sink, "android:fixture", 1_000)
        .await
        .unwrap();
    let first = drain(&mut rx).await;
    assert_eq!(get(&first, MetricId::ThermalRaw), Some(0.0));
    assert_eq!(get(&first, MetricId::ThermalState), Some(0.0));
    assert_eq!(get(&first, MetricId::TempSoc), Some(30.2));
    assert_eq!(get(&first, MetricId::TempBattery), Some(32.8));
    assert_eq!(get(&first, MetricId::TempSkin), Some(33.9));
    assert!(get(&first, MetricId::TempDisplay).is_none());
    assert!(get(&first, MetricId::TempPmu).is_none());
    assert_eq!(
        get(&first, MetricId::DiskTotal),
        Some(110_594_012.0 * 1024.0)
    );
    assert_eq!(get(&first, MetricId::DiskFree), Some(81_151_464.0 * 1024.0));
    assert_eq!(get(&first, MetricId::DiskUsed), Some(29_311_476.0 * 1024.0));
    assert_eq!(
        get(&first, MetricId::DiskApps),
        Some((13_009_826_304u64 + 9_661_345_814 + 4_793_561_600) as f64)
    );
    assert_eq!(get(&first, MetricId::DiskSystem), Some(24_190_685_184.0));
    assert_eq!(get(&first, MetricId::NetConnections), Some(1.0));
    assert!(
        get(&first, MetricId::DiskWriteBps).is_none(),
        "not measured on Android"
    );
    assert!(!first.iter().any(|m| matches!(m, Message::Event { .. })));
    assert!(shared.uid_net.read().is_empty(), "rates need two ticks");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    slow.tick(&shell, &sink, "android:fixture", 2_000)
        .await
        .unwrap();
    let second = drain(&mut rx).await;
    assert_eq!(get(&second, MetricId::ThermalRaw), Some(2.0));
    assert_eq!(get(&second, MetricId::ThermalState), Some(1.0));
    let ev = second
        .iter()
        .find_map(|m| match m {
            Message::Event {
                kind: EventKind::ThermalChange,
                detail,
                ..
            } => Some(detail.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(ev, "None → Moderate");
    let net = shared.uid_net.read();
    let sys = net[&1000];
    assert!(sys > 0.0, "uid 1000 moved 100 000 bytes: {sys} bit/s");
    assert_eq!(
        net.get(&1073).copied(),
        Some(0.0),
        "unchanged uid → 0 bit/s"
    );
}

#[tokio::test]
async fn logcat_stream_batches_resolves_and_stops() {
    let f = fixtures();
    let shell = SeqShell::new(vec![]);
    *shell.stream_text.lock() = Some(f.file("logcat_dump"));
    // After the stream ends the device is gone → run() returns.
    *shell.exec_after_stream.lock() = Some(Err(()));
    let shell: Arc<dyn Shell> = Arc::new(shell);
    let cfg = LogcatConfig::default();
    cfg.pids
        .write()
        .insert(1621, "vendor.thermal-hal".to_owned());
    let (sink, mut rx) = EventSink::channel(1024);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(logcat::run(
        shell,
        sink,
        "android:fixture".into(),
        cfg,
        cancel.clone(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("run() stops on DeviceGone")
        .unwrap();
    let msgs = drain(&mut rx).await;
    let mut entries = Vec::new();
    for m in &msgs {
        if let Message::Log { entries: e, .. } = m {
            assert!(e.len() <= 200, "batch size honoured: {}", e.len());
            entries.extend(e.iter().cloned());
        }
    }
    assert!(entries.len() > 2500, "{} entries", entries.len());
    assert!(
        entries.windows(2).all(|w| w[1].seq == w[0].seq + 1),
        "seq monotonic"
    );
    assert_eq!(entries[0].buffer.as_deref(), Some("main"));
    assert_eq!(entries[0].process, "vendor.thermal-hal");
    assert_eq!(entries[0].tag.as_deref(), Some("HeatmapThread"));
    assert_eq!(entries[0].ts_ms, 1_788_954_114_561);
    assert!(
        entries
            .iter()
            .any(|e| e.buffer.as_deref() == Some("events"))
    );
    let unresolved = entries
        .iter()
        .find(|e| e.process.starts_with("pid "))
        .unwrap();
    assert!(unresolved.pid.is_some(), "unknown pids keep the number");
}

#[tokio::test]
async fn logcat_derives_events_and_dedupes() {
    let text = "--------- beginning of main\n\
        1788954114.000000  1000  100  100 I lowmemorykiller: Kill 'com.example.bg' (777), uid 10333, oom_score_adj 900 to free 51200kB rss\n\
        1788954115.000000  1000  200  200 E AndroidRuntime: FATAL EXCEPTION: main\n\
        Process: com.example.app, PID: 4242\n\
        java.lang.IllegalStateException: boom\n\
        \tat com.example.Foo.bar(Foo.java:12)\n\
        --------- beginning of events\n\
        1788954115.100000  1000  300  300 I am_crash: [4242,10234,com.example.app,952745541,java.lang.IllegalStateException,boom,Foo.java,12,0]\n\
        1788954120.000000  1000  300  300 I am_anr: [0,4243,com.example.other,952745541,Input dispatching timed out]\n";
    let shell = SeqShell::new(vec![]);
    *shell.stream_text.lock() = Some(text.to_owned());
    *shell.exec_after_stream.lock() = Some(Err(()));
    let shell: Arc<dyn Shell> = Arc::new(shell);
    let (sink, mut rx) = EventSink::channel(64);
    let cancel = CancellationToken::new();
    logcat::run(
        shell,
        sink,
        "android:fixture".into(),
        LogcatConfig::default(),
        cancel,
    )
    .await;
    let msgs = drain(&mut rx).await;
    let events: Vec<(EventKind, Option<u32>, Option<String>, String)> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Event {
                kind,
                pid,
                process,
                detail,
                ..
            } => Some((*kind, *pid, process.clone(), detail.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(events[0].0, EventKind::LmkKill);
    assert_eq!(events[0].1, Some(777));
    assert_eq!(events[0].2.as_deref(), Some("com.example.bg"));
    assert_eq!(events[1].0, EventKind::Crash);
    assert_eq!(events[1].1, Some(4242));
    assert_eq!(events[1].2.as_deref(), Some("com.example.app"));
    assert!(events[1].3.contains("IllegalStateException"));
    assert_eq!(events[2].0, EventKind::Anr);
    assert_eq!(events[2].1, Some(4243));
    let entries: Vec<_> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::Log { entries, .. } => Some(entries.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let fatal = entries.iter().find(|e| e.pid == Some(200)).unwrap();
    assert_eq!(fatal.message.lines().count(), 4, "continuations joined");
    assert_eq!(fatal.buffer.as_deref(), Some("main"));
}

#[tokio::test]
async fn logcat_cancel_stops_run() {
    #[derive(Debug)]
    struct Hang;
    #[async_trait]
    impl Shell for Hang {
        fn serial(&self) -> &str {
            "hang"
        }
        async fn exec(&self, _cmd: &str, _t: u64) -> Result<String, ShellError> {
            Ok("ok".into())
        }
        async fn stream(&self, _cmd: &str) -> Result<ShellStream, ShellError> {
            let (_tx, rx) = tokio::io::duplex(64);
            std::mem::forget(_tx); // keep the writer open: the stream never ends
            Ok(Box::pin(rx))
        }
    }
    let (sink, _rx) = EventSink::channel(8);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(logcat::run(
        Arc::new(Hang),
        sink,
        "android:fixture".into(),
        LogcatConfig::default(),
        cancel.clone(),
    ));
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .expect("stops on cancel")
        .unwrap();
}
