//! sysmontap parser against `fixtures/ios/dvt/**` (step 4.3). The
//! `shape-only` set is hand-built from the documented message shape; a
//! captured set (`capture_dvt_fixture`) is picked up automatically when
//! present and must parse to the same kind of output.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use plist::Value;
use statasmon_device_ios::dvt::sysmontap::{Attrs, SysmonState, TickContext, collect, parse_tick};
use statasmon_idmap::apple::CoreLayout;
use statasmon_protocol::{CoreKind, MetricId};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ios/dvt")
}

fn attrs_from_manifest(dir: &Path) -> Attrs {
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
    let list = |key: &str| -> Vec<String> {
        manifest[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    Attrs {
        system: list("system_attributes"),
        process: list("process_attributes"),
    }
}

fn value(v: &[statasmon_protocol::MetricValue], id: MetricId) -> Option<f64> {
    v.iter().find(|m| m.id == id).map(|m| m.value)
}

#[test]
fn shape_only_message_parses_to_samples_cores_and_processes() {
    let dir = fixtures().join("shape-only");
    let attrs = attrs_from_manifest(&dir);
    let data: Value = plist::from_file(dir.join("sysmontap_message.plist")).unwrap();
    let msg = collect(&data);
    assert!(!msg.is_empty());
    assert_eq!(msg.cpu_count, Some(6));

    let ctx = TickContext {
        ram_total: None,
        layout: Some(CoreLayout {
            total: 6,
            performance: 2,
            efficiency: 4,
        }),
        pid_net_bps: [(4412u32, 1_200_000.0)].into_iter().collect(),
        energy: [(4412u32, "medium".to_owned())].into_iter().collect(),
        process_period: Duration::from_secs(3),
    };
    let mut state = SysmonState::default();
    let tick = parse_tick(&msg, &attrs, &mut state, 1_000, &ctx);

    // CPU: the system figure, not the per-core mean.
    assert_eq!(value(&tick.samples, MetricId::CpuTotal), Some(12.4));
    // −1 means "not split": no user/system.
    assert_eq!(value(&tick.samples, MetricId::CpuUser), None);
    assert_eq!(value(&tick.samples, MetricId::CpuSystem), None);

    // Cores: six, efficiency first, performance last (A14 = 2P + 4E).
    assert_eq!(tick.cores.len(), 6);
    let kinds: Vec<CoreKind> = tick.cores.iter().map(|c| c.kind).collect();
    assert_eq!(&kinds[..4], &[CoreKind::Efficiency; 4]);
    assert_eq!(&kinds[4..], &[CoreKind::Performance; 2]);
    assert_eq!(tick.cores[4].usage_pct, 40.0);
    assert!(tick.cores.iter().all(|c| c.freq_mhz.is_none() && c.online));

    // Memory: pages × 16 KiB, used = app + wired + compressed.
    let page = 16_384.0;
    assert_eq!(tick.ram_total, Some(4_294_967_296));
    assert_eq!(
        value(&tick.samples, MetricId::MemTotal),
        Some(4_294_967_296.0)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemFree),
        Some(12_000.0 * page)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemWired),
        Some(40_000.0 * page)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemApp),
        Some(90_000.0 * page)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemCached),
        Some(30_000.0 * page)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemCompressed),
        Some(20_000.0 * page)
    );
    assert_eq!(
        value(&tick.samples, MetricId::MemUsed),
        Some((90_000.0 + 40_000.0 + 20_000.0) * page)
    );
    assert_eq!(value(&tick.samples, MetricId::MemSwap), None);

    // Counters: first tick has totals (0 since the first tick) but no rates.
    assert_eq!(value(&tick.samples, MetricId::NetRxTotal), Some(0.0));
    assert_eq!(value(&tick.samples, MetricId::NetRxBps), None);
    assert_eq!(value(&tick.samples, MetricId::DiskWriteBps), None);
    assert_eq!(value(&tick.samples, MetricId::ProcCount), Some(3.0));

    // Processes: sorted by CPU (percent of one core as reported), footprint as memory,
    // per-pid network and energy joined from the shared state.
    let rows = tick.processes.expect("table due on the first tick");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].name, "FieldKit");
    assert_eq!(rows[0].pid, 4412);
    assert!((rows[0].cpu_pct.unwrap() - 35.0).abs() < 0.01);
    assert_eq!(rows[0].mem_bytes, Some(157_286_400));
    assert_eq!(rows[0].threads, Some(22));
    assert_eq!(rows[0].net_bps, Some(1_200_000.0));
    assert_eq!(rows[0].energy.as_deref(), Some("medium"));
    assert_eq!(rows[0].app_id, None);
    assert_eq!(rows[1].name, "kernel_task");
    // A zero footprint falls back to the resident size.
    assert_eq!(rows[1].mem_bytes, Some(52_428_800));
    assert_eq!(rows[2].name, "launchd");

    // Second tick one second later with the counters advanced: rates, and
    // the process table is not due yet.
    let mut data2 = data.clone();
    if let Value::Array(rows) = &mut data2
        && let Some(Value::Dictionary(row)) = rows.first_mut()
        && let Some(Value::Array(system)) = row.get_mut("System")
    {
        system[9] = Value::Integer(1_250_000i64.into()); // netBytesIn +250 000
        system[10] = Value::Integer(600_000i64.into()); // netBytesOut +100 000
        system[11] = Value::Integer(1_100i64.into()); // netPacketsIn +100
        system[13] = Value::Integer(5_500_000i64.into()); // diskBytesWritten +500 000
    }
    let tick2 = parse_tick(&collect(&data2), &attrs, &mut state, 2_000, &ctx);
    assert_eq!(
        value(&tick2.samples, MetricId::NetRxBps),
        Some(250_000.0 * 8.0)
    );
    assert_eq!(
        value(&tick2.samples, MetricId::NetTxBps),
        Some(100_000.0 * 8.0)
    );
    assert_eq!(value(&tick2.samples, MetricId::NetRxPps), Some(100.0));
    assert_eq!(value(&tick2.samples, MetricId::NetTxPps), Some(0.0));
    assert_eq!(
        value(&tick2.samples, MetricId::DiskWriteBps),
        Some(500_000.0)
    );
    assert_eq!(value(&tick2.samples, MetricId::NetRxTotal), Some(250_000.0));
    assert_eq!(value(&tick2.samples, MetricId::NetTxTotal), Some(100_000.0));
    assert!(tick2.processes.is_none());

    // Third tick after the process period: the table is due again.
    let tick3 = parse_tick(&collect(&data2), &attrs, &mut state, 4_000, &ctx);
    assert!(tick3.processes.is_some());
}

#[test]
fn captured_messages_parse_when_present() {
    let root = fixtures();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let mut seen = 0;
    for entry in entries.flatten() {
        let dir = entry.path();
        let file = dir.join("sysmontap_messages.plist");
        if !file.is_file() {
            continue;
        }
        let attrs = attrs_from_manifest(&dir);
        let messages: Value = plist::from_file(&file).unwrap();
        let mut state = SysmonState::default();
        let ctx = TickContext {
            process_period: Duration::from_secs(1),
            ..TickContext::default()
        };
        let mut ticks = 0;
        for (i, m) in messages.as_array().unwrap().iter().enumerate() {
            let msg = collect(m);
            if msg.is_empty() {
                continue;
            }
            let tick = parse_tick(&msg, &attrs, &mut state, 1_000 * (i as i64 + 1), &ctx);
            assert!(
                value(&tick.samples, MetricId::CpuTotal).is_some(),
                "{}: message {i} has no cpu.total",
                dir.display()
            );
            ticks += 1;
        }
        assert!(ticks > 0, "{}: no usable message", dir.display());
        seen += 1;
    }
    eprintln!("captured sysmontap fixture sets parsed: {seen}");
}
