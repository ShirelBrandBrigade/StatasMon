//! Live tier-2 checks against a connected iPad (10th generation, iPadOS
//! 17.4.1, A14 = 2P + 4E) with Developer Mode on. Skipped unless
//! STATASMON_LIVE_IOS=1; skipped with the reason when the iPad is absent
//! or Developer Mode is off (then the reason text is asserted).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use statasmon_device::{DeviceSource, EventSink};
use statasmon_device_ios::dvt::{self, Tier2Shared};
use statasmon_device_ios::muxd::{IosProvider, read_pair_record, validate_pair_record};
use statasmon_device_ios::source::reasons;
use statasmon_device_ios::{IosError, IosSeed, IosSource, MuxdDiscovery, Tunnel};
use statasmon_protocol::{
    Capability, ConnectionState, CoreKind, Message, MetricId, SamplingConfig, Transport,
};
use tokio_util::sync::CancellationToken;

fn live() -> bool {
    if std::env::var("STATASMON_LIVE_IOS").as_deref() == Ok("1") {
        true
    } else {
        eprintln!("skipped: set STATASMON_LIVE_IOS=1");
        false
    }
}

/// The paired provider for the first device, or the skip reason.
async fn provider() -> Result<(Arc<IosProvider>, MuxdDiscovery, String), String> {
    let disc = MuxdDiscovery::new();
    let devices = disc
        .list()
        .await
        .map_err(|e| format!("usbmuxd unreachable: {e}"))?;
    let Some(dev) = devices.first() else {
        return Err("no iOS device on usbmuxd".to_owned());
    };
    let provider = Arc::new(IosProvider::new(dev, disc.addr().clone()));
    let Some(pair) = read_pair_record(disc.addr(), &dev.udid).await.unwrap() else {
        return Err("no pair record yet (accept Trust This Computer first)".to_owned());
    };
    validate_pair_record(&provider, &pair)
        .await
        .map_err(|e| format!("pair record rejected: {e}"))?;
    provider.set_pair(pair);
    let udid = dev.udid.clone();
    Ok((provider, disc, udid))
}

#[tokio::test]
async fn live_tunnel_sysmontap_five_seconds() {
    if !live() {
        return;
    }
    let (provider, _disc, udid) = match provider().await {
        Ok(p) => p,
        Err(why) => {
            eprintln!("skipped: {why}");
            return;
        }
    };
    let started = std::time::Instant::now();
    let tunnel = match Tunnel::open(Arc::clone(&provider)).await {
        Ok(t) => t,
        Err(IosError::DeveloperModeOff) => {
            let reason = IosError::DeveloperModeOff.hint().unwrap();
            assert!(reason.contains("Developer Mode"));
            eprintln!("skipped: Developer Mode is off on {udid} ({reason})");
            return;
        }
        Err(e) => panic!("tunnel failed: {e} (hint {:?})", e.hint()),
    };
    eprintln!(
        "tunnel in {} ms: {}",
        started.elapsed().as_millis(),
        tunnel.describe()
    );
    let mut names: Vec<&String> = tunnel.services().keys().collect();
    names.sort();
    eprintln!("RSD services ({}): {names:?}", names.len());
    assert!(tunnel.has_service(statasmon_device_ios::tunnel::DVT_HUB));

    // deviceinfo: cores, RAM (when reported), uptime.
    let facts = dvt::deviceinfo::read(&tunnel).await.expect("deviceinfo");
    eprintln!(
        "hardwareInformation: {:?} / mach time {:?} → uptime {:?} s / {} running processes",
        facts.hardware.raw,
        facts.mach_time,
        facts.uptime_secs(),
        facts.processes.len()
    );
    assert_eq!(
        facts.hardware.cpu_cores,
        Some(6),
        "iPad 10th gen has 6 cores"
    );
    assert!(
        facts.uptime_secs().is_some(),
        "machTimeInfo gives the uptime"
    );

    // sysmontap for 5 s.
    let tunnel = Arc::new(tunnel);
    let (sink, mut rx) = EventSink::channel(1024);
    let shared = Arc::new(parking_lot::RwLock::new(Tier2Shared {
        ram_total: facts.hardware.ram_bytes,
        layout: statasmon_idmap::apple::chip_core_layout("A14 Bionic"),
        ..Tier2Shared::default()
    }));
    let cancel = CancellationToken::new();
    let task = tokio::spawn(dvt::sysmontap::run(
        Arc::clone(&tunnel),
        sink,
        format!("ios:{udid}"),
        Duration::from_secs(1),
        Duration::from_secs(2),
        shared,
        cancel.clone(),
    ));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    let (mut samples, mut cores, mut processes) = (0, 0, 0);
    let mut saw_cpu_total = false;
    let mut six_cores = false;
    while let Ok(Some(msg)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        match msg {
            Message::Samples { values, .. } => {
                samples += 1;
                if let Some(v) = values.iter().find(|v| v.id == MetricId::CpuTotal) {
                    saw_cpu_total = true;
                    let mem = values
                        .iter()
                        .find(|v| v.id == MetricId::MemUsed)
                        .map(|v| v.value);
                    let rx_bps = values
                        .iter()
                        .find(|v| v.id == MetricId::NetRxBps)
                        .map(|v| v.value);
                    eprintln!(
                        "cpu.total {:.1} % mem.used {mem:?} net.rx_bps {rx_bps:?}",
                        v.value
                    );
                }
            }
            Message::Cores { cores: c, .. } => {
                cores += 1;
                if c.len() == 6 {
                    six_cores = true;
                }
                let kinds: Vec<(u16, CoreKind, f32)> =
                    c.iter().map(|x| (x.index, x.kind, x.usage_pct)).collect();
                eprintln!("cores {kinds:?}");
            }
            Message::Processes { rows, .. } => {
                processes += 1;
                let top: Vec<String> = rows
                    .iter()
                    .take(5)
                    .map(|r| {
                        format!(
                            "{}:{} cpu {:?} mem {:?} thr {:?}",
                            r.pid, r.name, r.cpu_pct, r.mem_bytes, r.threads
                        )
                    })
                    .collect();
                eprintln!("processes {} rows, top {top:?}", rows.len());
            }
            _ => {}
        }
        if samples >= 5 && cores >= 5 && processes >= 1 {
            break;
        }
    }
    cancel.cancel();
    let end = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    eprintln!("loop ended: {end:?}; samples {samples} cores {cores} processes {processes}");
    assert!(saw_cpu_total, "no Samples with cpu.total in 8 s");
    assert!(six_cores, "no Cores message with 6 entries");
    assert!(processes >= 1, "no process table");
    assert!(tunnel.is_alive());
}

#[tokio::test]
async fn live_source_reaches_tier2_or_says_why() {
    if !live() {
        return;
    }
    let (provider, disc, udid) = match provider().await {
        Ok(p) => p,
        Err(why) => {
            eprintln!("skipped: {why}");
            return;
        }
    };
    let devices = disc.list().await.unwrap();
    let dev = devices.iter().find(|d| d.udid == udid).unwrap();
    let seed = IosSeed::new(&udid, dev.device_id, Transport::Usb);
    let source = IosSource::connect(Arc::clone(&provider), provider.pair().unwrap(), seed)
        .await
        .expect("tier-1 connect");
    let dev_mode = source.developer_mode();
    eprintln!("Developer Mode: {dev_mode:?}");
    let (sink, mut rx) = EventSink::channel(4096);
    source.start(sink, SamplingConfig::default()).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    let mut state = None;
    let mut caps = None;
    let mut cores_seen = false;
    while let Ok(Some(msg)) = tokio::time::timeout_at(deadline, rx.recv()).await {
        match msg {
            Message::DeviceState {
                state: s, detail, ..
            } => {
                eprintln!("state {s:?} {detail:?}");
                state = Some(s.clone());
                if s == ConnectionState::Tier2 && cores_seen {
                    break;
                }
            }
            Message::Capabilities { set, .. } => caps = Some(set),
            Message::Cores { cores, .. } => {
                cores_seen = cores.len() == 6;
                if cores_seen && state == Some(ConnectionState::Tier2) {
                    break;
                }
            }
            Message::Error {
                code,
                message,
                hint,
                ..
            } => {
                eprintln!("error {code:?}: {message} ({hint:?})");
            }
            _ => {}
        }
    }
    source.stop().await.unwrap();
    let caps = caps.expect("capabilities were published");
    match dev_mode {
        Some(true) => {
            assert_eq!(state, Some(ConnectionState::Tier2), "expected tier 2");
            assert!(caps.is_available(Capability::CpuPerCore));
            assert!(cores_seen, "no 6-core sample at tier 2");
            let info = source.info().await.unwrap();
            eprintln!(
                "tier {:?} tunnel {:?} ram {:?} uptime {:?}",
                info.connection.tier,
                info.connection.tunnel,
                info.hardware.ram_bytes,
                info.state.uptime_secs
            );
            assert!(info.state.uptime_secs.is_some());
        }
        _ => {
            assert_eq!(state, Some(ConnectionState::Tier1));
            assert_eq!(
                caps.reason(Capability::CpuPerCore),
                Some(reasons::DEV_MODE_OFF)
            );
            eprintln!("skipped tier 2: {}", reasons::DEV_MODE_OFF);
        }
    }
}
