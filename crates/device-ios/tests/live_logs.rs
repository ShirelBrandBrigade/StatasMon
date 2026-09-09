//! Live os_trace / crash / pcap against the first USB device usbmuxd
//! reports. Skipped unless `STATASMON_LIVE_IOS=1`. Read-only on the device:
//! the crash mover only relocates pending reports; nothing is deleted.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use idevice::provider::IdeviceProvider;
use idevice::usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection};
use statasmon_device::EventSink;
use statasmon_device_ios::pcap::{PCAP_HEADER_LEN, PCAP_MAGIC_BE};
use statasmon_device_ios::{crash, ostrace, pcap};
use statasmon_protocol::Message;
use tokio_util::sync::CancellationToken;

async fn live_provider() -> Option<Arc<dyn IdeviceProvider>> {
    if std::env::var("STATASMON_LIVE_IOS").is_err() {
        eprintln!("skipped: set STATASMON_LIVE_IOS=1 with an iOS device on USB");
        return None;
    }
    let mut mux = UsbmuxdConnection::default().await.expect("usbmuxd");
    let devices = mux.get_devices().await.expect("usbmuxd device list");
    let dev = devices
        .into_iter()
        .find(|d| d.connection_type == Connection::Usb)
        .expect("a USB-connected iOS device");
    mux.get_pair_record(&dev.udid)
        .await
        .expect("pair record: accept Trust on the device");
    eprintln!(
        "live device: mux id {} ({}…)",
        dev.device_id,
        &dev.udid[..8]
    );
    Some(Arc::new(
        dev.to_provider(UsbmuxdAddr::default(), "StatasMon-logs"),
    ))
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "statasmon-live-{tag}-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn live_ostrace_delivers_a_batch_and_stops_on_cancel() {
    let Some(provider) = live_provider().await else {
        return;
    };
    let (sink, mut rx) = EventSink::channel(64);
    let cancel = CancellationToken::new();
    let task = tokio::spawn(ostrace::run(
        provider,
        sink,
        "ios:live".into(),
        cancel.clone(),
    ));
    let started = std::time::Instant::now();
    let first = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match rx.recv().await {
                Some(Message::Log { entries, .. }) => break entries,
                Some(other) => eprintln!("(before first batch) {other:?}"),
                None => panic!("sink closed"),
            }
        }
    })
    .await
    .expect("a Log batch within 10 s");
    eprintln!(
        "first batch after {} ms: {} entries; first: {} [{}] {:?} {}",
        started.elapsed().as_millis(),
        first.len(),
        first[0].process,
        first[0].pid.unwrap_or(0),
        first[0].level,
        first[0].message.chars().take(80).collect::<String>()
    );
    assert!(!first.is_empty() && first.len() <= ostrace::BATCH_MAX);
    assert!(first.windows(2).all(|w| w[1].seq > w[0].seq));
    assert!(first[0].ts_ms > 1_600_000_000_000);
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("run() returns within 5 s of cancel")
        .unwrap();
}

#[tokio::test]
async fn live_crash_fetch_writes_under_temp_dir() {
    let Some(provider) = live_provider().await else {
        return;
    };
    let dir = temp_dir("crash");
    let started = std::time::Instant::now();
    let reports = crash::fetch(&*provider, &dir, None).await.unwrap();
    eprintln!(
        "crash fetch: {} report(s) in {} ms under {}",
        reports.len(),
        started.elapsed().as_millis(),
        dir.display()
    );
    for r in reports.iter().take(10) {
        eprintln!(
            "  {:?} {} {:?} {:?} {:?}",
            r.kind, r.name, r.process, r.reason, r.ts_ms
        );
        assert!(r.local_path.is_file());
        assert!(r.local_path.starts_with(&dir));
    }
    // A second fetch pulls nothing new.
    let again = crash::fetch(&*provider, &dir, None).await.unwrap();
    assert!(again.is_empty(), "second fetch must skip present files");
}

#[tokio::test]
async fn live_pcap_writes_a_valid_header() {
    let Some(provider) = live_provider().await else {
        return;
    };
    let path = temp_dir("pcap").join("live.pcap");
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        stop.cancel();
    });
    let count = pcap::capture(provider, path.clone(), cancel).await.unwrap();
    let bytes = std::fs::read(&path).unwrap();
    eprintln!(
        "pcap: {count} packets, {} bytes at {}",
        bytes.len(),
        path.display()
    );
    assert_eq!(&bytes[..4], &PCAP_MAGIC_BE);
    assert!(bytes.len() as u64 >= PCAP_HEADER_LEN);
    if count > 0 {
        assert!(bytes.len() as u64 > PCAP_HEADER_LEN + 16);
    }
}
