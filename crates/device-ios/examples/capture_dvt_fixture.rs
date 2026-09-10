//! Capture the tier-2 (DVT) fixture set from a connected iOS device with
//! Developer Mode on into `fixtures/ios/dvt/<ProductType>-<ProductVersion>/`
//! (or the directory given as the first argument).
//!
//! ```sh
//! cargo run -p statasmon-device-ios --example capture_dvt_fixture [-- <dir> [<udid>]]
//! ```
//! Files written: `hardware_information.plist`, `mach_time_info.plist`,
//! `running_processes.plist`, `sysmon_attributes.plist` (what the device
//! advertises), `sysmontap_messages.plist` (10 s of raw message payloads),
//! `networking_events.plist` (10 s of decoded events, addresses scrubbed),
//! `rsd_services.plist` (names and ports only), `manifest.json`.
//! Nothing personal is in a sysmontap payload beyond process names; the
//! networking file keeps pids, byte counts, RTTs and interface names only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use idevice::dvt::message::AuxValue;
use idevice::dvt::network_monitor::{NetworkEvent, NetworkMonitorClient};
use idevice::usbmuxd::UsbmuxdAddr;
use plist::{Dictionary, Value};
use statasmon_device_ios::Tunnel;
use statasmon_device_ios::dvt::deviceinfo::SERVICE as DEVICEINFO;
use statasmon_device_ios::dvt::sysmontap::{self, Attrs, SERVICE as SYSMONTAP};
use statasmon_device_ios::lockdown::{Lockdown, string};
use statasmon_device_ios::muxd::{IosProvider, list_devices, read_pair_record};

const CAPTURE: Duration = Duration::from_secs(10);

fn write(dir: &std::path::Path, name: &str, v: &Value) {
    let path = dir.join(name);
    let f = std::fs::File::create(&path).unwrap();
    plist::to_writer_binary(f, v).unwrap();
    println!("wrote {}", path.display());
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let addr = UsbmuxdAddr::from_env_var().unwrap_or_default();
    let devices = list_devices(&addr).await.expect("usbmuxd");
    let dev = match args.get(1) {
        Some(udid) => devices.iter().find(|d| &d.udid == udid),
        None => devices.first(),
    }
    .expect("no iOS device on usbmuxd");
    println!("device {} (mux {})", dev.udid, dev.device_id);
    let provider = Arc::new(IosProvider::new(dev, addr.clone()));
    let pair = read_pair_record(&addr, &dev.udid)
        .await
        .unwrap()
        .expect("no pair record: accept Trust This Computer first (capture_fixture pairs)");
    provider.set_pair(pair.clone());

    let mut ld = Lockdown::connect(&*provider, &pair).await.unwrap();
    let root = ld.root().await.unwrap();
    drop(ld);
    let product = string(&root, "ProductType").unwrap_or_else(|| "unknown".into());
    let version = string(&root, "ProductVersion").unwrap_or_else(|| "0".into());
    let dir = match args.first() {
        Some(d) => PathBuf::from(d),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/ios/dvt")
            .join(format!("{product}-{version}")),
    };
    std::fs::create_dir_all(&dir).unwrap();

    let started = std::time::Instant::now();
    let tunnel = match Tunnel::open(Arc::clone(&provider)).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tunnel failed: {e} (hint: {:?})", e.hint());
            std::process::exit(2);
        }
    };
    println!(
        "tunnel in {} ms: {}",
        started.elapsed().as_millis(),
        tunnel.describe()
    );
    let mut services = Dictionary::new();
    for (name, s) in tunnel.services() {
        let mut d = Dictionary::new();
        d.insert("port".into(), Value::Integer(u64::from(s.port).into()));
        d.insert("uses_remote_xpc".into(), Value::Boolean(s.uses_remote_xpc));
        services.insert(name.clone(), Value::Dictionary(d));
    }
    write(&dir, "rsd_services.plist", &Value::Dictionary(services));

    // deviceinfo.
    let mut hub = tunnel.dvt().await.unwrap();
    {
        let mut ch = hub.make_channel(DEVICEINFO).await.unwrap();
        for (method, file) in [
            ("hardwareInformation", "hardware_information.plist"),
            ("machTimeInfo", "mach_time_info.plist"),
            ("runningProcesses", "running_processes.plist"),
        ] {
            ch.call_method(Some(Value::String(method.into())), None, true)
                .await
                .unwrap();
            match ch.read_message().await.unwrap().data {
                Some(v) => write(&dir, file, &v),
                None => println!("{method}: no data"),
            }
        }
    }
    let (sys_avail, proc_avail) = sysmontap::query_attributes(&mut hub).await.unwrap();
    let mut attrs_dict = Dictionary::new();
    attrs_dict.insert(
        "system".into(),
        Value::Array(sys_avail.iter().cloned().map(Value::String).collect()),
    );
    attrs_dict.insert(
        "process".into(),
        Value::Array(proc_avail.iter().cloned().map(Value::String).collect()),
    );
    write(
        &dir,
        "sysmon_attributes.plist",
        &Value::Dictionary(attrs_dict),
    );
    let attrs = Attrs::select(&sys_avail, &proc_avail);
    println!("system attrs: {:?}", attrs.system);
    println!("process attrs: {:?}", attrs.process);

    // sysmontap: raw payloads for CAPTURE.
    let mut messages = Vec::new();
    {
        let mut ch = hub.make_channel(SYSMONTAP).await.unwrap();
        ch.call_method(
            Some(Value::String("setConfig:".into())),
            Some(vec![AuxValue::archived_value(Value::Dictionary(
                attrs.config(Duration::from_secs(1)),
            ))]),
            false,
        )
        .await
        .unwrap();
        ch.call_method(Some(Value::String("start".into())), None, false)
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + CAPTURE;
        while let Ok(Ok(msg)) = tokio::time::timeout_at(deadline, ch.read_message()).await {
            if let Some(data) = msg.data {
                let m = sysmontap::collect(&data);
                if !m.is_empty() {
                    println!(
                        "sysmontap: cpu {:?} cores {:?} processes {}",
                        m.system_cpu.as_ref().and_then(|d| d.get("CPU_TotalLoad")),
                        m.per_cpu.as_ref().map(Vec::len),
                        m.processes.as_ref().map_or(0, Dictionary::len)
                    );
                }
                messages.push(data);
            }
        }
        let _ = ch
            .call_method(Some(Value::String("stop".into())), None, false)
            .await;
    }
    write(&dir, "sysmontap_messages.plist", &Value::Array(messages));

    // networking: decoded events for CAPTURE, addresses scrubbed.
    let mut hub2 = tunnel.dvt().await.unwrap();
    let mut events = Vec::new();
    {
        let mut nm = NetworkMonitorClient::new(&mut hub2).await.unwrap();
        nm.start_monitoring().await.unwrap();
        let deadline = tokio::time::Instant::now() + CAPTURE;
        while let Ok(Ok(ev)) = tokio::time::timeout_at(deadline, nm.next_event()).await {
            let mut d = Dictionary::new();
            let int = |v: u64| Value::Integer(v.into());
            match &ev {
                NetworkEvent::InterfaceDetection(i) => {
                    d.insert("type".into(), Value::String("interface".into()));
                    d.insert("index".into(), int(u64::from(i.interface_index)));
                    d.insert("name".into(), Value::String(i.name.clone()));
                }
                NetworkEvent::ConnectionDetection(c) => {
                    d.insert("type".into(), Value::String("connection".into()));
                    d.insert("serial".into(), int(c.serial_number));
                    d.insert("pid".into(), int(u64::from(c.pid)));
                    d.insert("interface".into(), int(u64::from(c.interface_index)));
                    d.insert("kind".into(), int(u64::from(c.kind)));
                    d.insert(
                        "family".into(),
                        int(u64::from(c.remote_address.as_ref().map_or(0, |a| a.family))),
                    );
                }
                NetworkEvent::ConnectionUpdate(u) => {
                    d.insert("type".into(), Value::String("update".into()));
                    d.insert("serial".into(), int(u.connection_serial));
                    d.insert("rx_bytes".into(), int(u.rx_bytes));
                    d.insert("tx_bytes".into(), int(u.tx_bytes));
                    d.insert("rx_packets".into(), int(u.rx_packets));
                    d.insert("tx_packets".into(), int(u.tx_packets));
                    d.insert("tx_retx".into(), int(u.tx_retx));
                    d.insert("min_rtt".into(), int(u.min_rtt));
                    d.insert("avg_rtt".into(), int(u.avg_rtt));
                    d.insert("time".into(), int(u.time));
                }
                NetworkEvent::Unknown(t) => {
                    d.insert("type".into(), Value::String(format!("unknown-{t}")));
                }
            }
            events.push(Value::Dictionary(d));
        }
        let _ = nm.stop_monitoring().await;
    }
    println!("networking: {} events", events.len());
    write(&dir, "networking_events.plist", &Value::Array(events));

    let manifest = serde_json::json!({
        "shape_only": false,
        "product_type": product,
        "product_version": version,
        "captured_with": "capture_dvt_fixture",
        "capture_seconds": CAPTURE.as_secs(),
        "system_attributes": attrs.system,
        "process_attributes": attrs.process,
        "advertised_system_attributes": sys_avail,
        "advertised_process_attributes": proc_avail,
        "tunnel": tunnel.describe(),
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    println!("manifest written to {}", dir.display());
    tunnel.close().await;
}
