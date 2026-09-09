//! Capture the tier-1 fixture set from a connected iOS device into
//! `fixtures/ios/<ProductType>-<ProductVersion>/` (or the directory given
//! as the first argument), scrubbing identifiers first.
//!
//! ```sh
//! cargo run -p statasmon-device-ios --example capture_fixture [-- <dir> [<udid>]]
//! ```
//! Files written: `lockdown_root.plist`, `lockdown_battery.plist`,
//! `lockdown_disk_usage.plist`, `lockdown_international.plist`,
//! `lockdown_wireless_lockdown.plist`, `ioregistry_battery.plist`,
//! `mobilegestalt.plist`, `installation_proxy_sizes.plist`,
//! `developer_mode.plist`, `manifest.json`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use std::path::PathBuf;
use std::sync::Arc;

use idevice::IdeviceService;
use idevice::services::diagnostics_relay::DiagnosticsRelayClient;
use idevice::services::installation_proxy::InstallationProxyClient;
use idevice::usbmuxd::UsbmuxdAddr;
use statasmon_device_ios::battery::IOREG_CLASS;
use statasmon_device_ios::info::MG_PHYSICAL_MEMORY;
use statasmon_device_ios::lockdown::{self, Lockdown, string};
use statasmon_device_ios::muxd::{
    IosProvider, list_devices, new_host_id, pair_once, read_pair_record, save_pair_record,
};
use statasmon_device_ios::storage::browse_options;

/// Keys replaced by placeholders wherever they appear.
const SCRUB: &[(&str, &str)] = &[
    ("UniqueDeviceID", "00000000-0000000000000000"),
    ("UDID", "00000000-0000000000000000"),
    ("SerialNumber", "SERIAL000000"),
    ("MLBSerialNumber", "MLB000000000000"),
    ("BatterySerialNumber", "BATT000000000"),
    ("Serial", "BATT000000000"),
    ("WiFiAddress", "00:00:00:00:00:00"),
    ("BluetoothAddress", "00:00:00:00:00:00"),
    ("EthernetAddress", "00:00:00:00:00:00"),
    ("DeviceName", "Test iPad"),
    ("PhoneNumber", ""),
    ("InternationalMobileEquipmentIdentity", ""),
    ("InternationalMobileEquipmentIdentity2", ""),
    ("MobileEquipmentIdentifier", ""),
    ("IntegratedCircuitCardIdentity", ""),
    ("UniqueChipID", "0"),
    ("DieID", "0"),
    ("MobileSubscriberCountryCode", ""),
    ("MobileSubscriberNetworkCode", ""),
    ("CarrierBundleInfoArray", ""),
    ("NonVolatileRAM", ""),
    ("DeviceCertificate", ""),
    ("DevicePublicKey", ""),
    ("HostAttached", ""),
    ("EscrowBag", ""),
];

fn scrub(v: &mut plist::Value) {
    match v {
        plist::Value::Dictionary(d) => {
            for (k, val) in d.iter_mut() {
                if let Some((_, repl)) = SCRUB.iter().find(|(key, _)| *key == k) {
                    *val = plist::Value::String((*repl).to_owned());
                } else {
                    scrub(val);
                }
            }
        }
        plist::Value::Array(items) => items.iter_mut().for_each(scrub),
        _ => {}
    }
}

fn write(dir: &std::path::Path, name: &str, value: plist::Value) {
    let mut value = value;
    scrub(&mut value);
    let path = dir.join(name);
    let mut buf = Vec::new();
    plist::to_writer_xml(&mut buf, &value).expect("plist");
    std::fs::write(&path, buf).expect("write fixture");
    println!("wrote {}", path.display());
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let addr = UsbmuxdAddr::from_env_var().unwrap_or_default();
    let devices = list_devices(&addr).await.expect("usbmuxd");
    let dev = devices
        .iter()
        .find(|d| args.get(1).is_none_or(|u| &d.udid == u))
        .expect("no iOS device on usbmuxd");
    println!(
        "device {} (mux id {}, {:?})",
        dev.udid, dev.device_id, dev.connection_type
    );
    let provider = Arc::new(IosProvider::new(dev, addr.clone()));

    let pair = match read_pair_record(&addr, &dev.udid).await.expect("usbmuxd") {
        Some(p) => p,
        None => {
            println!("no pair record; accept Trust This Computer on the device…");
            let host_id = new_host_id();
            loop {
                match pair_once(&addr, &provider, &host_id).await {
                    Ok(Some(p)) => {
                        save_pair_record(&addr, &dev.udid, p.clone())
                            .await
                            .unwrap_or_else(|e| println!("save_pair_record: {e}"));
                        break p;
                    }
                    Ok(None) => tokio::time::sleep(std::time::Duration::from_secs(2)).await,
                    Err(e) => panic!("pairing failed: {e}"),
                }
            }
        }
    };
    provider.set_pair(pair.clone());

    let mut ld = Lockdown::connect(&*provider, &pair)
        .await
        .expect("lockdown session");
    let root = ld.root().await.expect("root");
    let product = string(&root, "ProductType").unwrap_or_else(|| "unknown".into());
    let version = string(&root, "ProductVersion").unwrap_or_else(|| "0".into());
    let dir = args.first().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/ios")
            .join(format!(
                "{}-{}",
                product.to_lowercase().replace(',', "-"),
                version
            ))
    });
    std::fs::create_dir_all(&dir).expect("mkdir");
    write(&dir, "lockdown_root.plist", plist::Value::Dictionary(root));
    for (name, domain) in [
        ("lockdown_battery.plist", lockdown::DOMAIN_BATTERY),
        ("lockdown_disk_usage.plist", lockdown::DOMAIN_DISK_USAGE),
        (
            "lockdown_international.plist",
            lockdown::DOMAIN_INTERNATIONAL,
        ),
        (
            "lockdown_wireless_lockdown.plist",
            lockdown::DOMAIN_WIRELESS,
        ),
    ] {
        match ld.get_domain(Some(domain)).await {
            Ok(d) => write(&dir, name, plist::Value::Dictionary(d)),
            Err(e) => println!("{domain}: {e}"),
        }
    }
    drop(ld);

    let mut dev_mode = plist::Dictionary::new();
    match lockdown::developer_mode_status(&*provider).await {
        Ok(on) => {
            dev_mode.insert("status".into(), plist::Value::Boolean(on));
            write(
                &dir,
                "developer_mode.plist",
                plist::Value::Dictionary(dev_mode),
            );
        }
        Err(e) => println!("amfi: {e}"),
    }

    match DiagnosticsRelayClient::connect(&*provider).await {
        Ok(mut diag) => {
            match diag.ioregistry(None, None, Some(IOREG_CLASS)).await {
                Ok(Some(d)) => write(
                    &dir,
                    "ioregistry_battery.plist",
                    plist::Value::Dictionary(d),
                ),
                Ok(None) => println!("IORegistry: no {IOREG_CLASS} entry"),
                Err(e) => println!("IORegistry: {e}"),
            }
            match diag
                .mobilegestalt(Some(vec![MG_PHYSICAL_MEMORY.to_owned()]))
                .await
            {
                Ok(Some(d)) => write(&dir, "mobilegestalt.plist", plist::Value::Dictionary(d)),
                Ok(None) => println!("MobileGestalt: empty"),
                Err(e) => println!("MobileGestalt: {e}"),
            }
            let _ = diag.goodbye().await;
        }
        Err(e) => println!("diagnostics_relay: {e}"),
    }

    match InstallationProxyClient::connect(&*provider).await {
        Ok(mut client) => match client.browse(Some(browse_options())).await {
            Ok(list) => write(
                &dir,
                "installation_proxy_sizes.plist",
                plist::Value::Array(list),
            ),
            Err(e) => println!("installation_proxy browse: {e}"),
        },
        Err(e) => println!("installation_proxy: {e}"),
    }

    let manifest = serde_json::json!({
        "product_type": product,
        "product_version": version,
        "captured": chrono::Utc::now().to_rfc3339(),
        "tool": "crates/device-ios/examples/capture_fixture.rs",
        "scrubbed": SCRUB.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).expect("json"),
    )
    .expect("write manifest");
    println!("done: {}", dir.display());
}
