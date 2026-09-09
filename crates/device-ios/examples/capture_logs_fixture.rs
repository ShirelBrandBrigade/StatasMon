//! Record an os_trace fixture from the first USB iOS device (step 3.5).
//!
//! Streams 20 s of `com.apple.os_trace_relay` and writes, scrubbed:
//! - `fixtures/ios/logs/ipad-17.4.1-ostrace-20s.jsonl` (one `LogEntry` per
//!   line, capped at 4 000 lines)
//! - `fixtures/ios/logs/thermalmonitord-lines.txt` (every thermalmonitord
//!   message seen)
//!
//! Scrubbing replaces the device name (and the owner's name it usually
//! carries), UDID, serial, Wi-Fi/Bluetooth MACs, any other MAC-shaped
//! token, e-mail addresses and SSID values with placeholders.
//!
//! Run: `cargo run -p statasmon-device-ios --example capture_logs_fixture
//! [seconds] [out-dir]`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use idevice::IdeviceService;
use idevice::provider::IdeviceProvider;
use idevice::services::lockdown::LockdownClient;
use idevice::services::os_trace_relay::OsTraceRelayClient;
use idevice::usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection};
use statasmon_device_ios::ostrace::to_entry;
use statasmon_device_ios::thermal::Derive;
use statasmon_protocol::LogEntry;

const MAX_LINES: usize = 4_000;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let seconds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    let out_dir = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ios/logs")
    });
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut mux = UsbmuxdConnection::default().await.expect("usbmuxd");
    let dev = mux
        .get_devices()
        .await
        .expect("device list")
        .into_iter()
        .find(|d| d.connection_type == Connection::Usb)
        .expect("a USB-connected iOS device");
    let provider = dev.to_provider(UsbmuxdAddr::default(), "StatasMon-logs-fixture");

    let mut scrub = Scrubber::new(&dev.udid);
    match lockdown_values(&provider).await {
        Ok(values) => scrub.add_values(values),
        Err(e) => eprintln!("lockdown values unavailable ({e}); scrubbing UDID only"),
    }

    let client = OsTraceRelayClient::connect(&provider)
        .await
        .expect("os_trace_relay");
    let mut rx = client.start_trace(None).await.expect("StartActivity");

    let jsonl_path = out_dir.join("ipad-17.4.1-ostrace-20s.jsonl");
    let thermal_path = out_dir.join("thermalmonitord-lines.txt");
    let mut jsonl = std::io::BufWriter::new(std::fs::File::create(&jsonl_path).unwrap());
    let mut thermal_lines: Vec<String> = Vec::new();
    let mut derive = Derive::new();
    let mut derived = Vec::new();

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut seq = 0u64;
    let mut written = 0usize;
    let mut total = 0usize;
    let mut processes: BTreeSet<String> = BTreeSet::new();
    eprintln!("streaming {seconds} s from mux id {} …", dev.device_id);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let log = match tokio::time::timeout(remaining, rx.next()).await {
            Ok(Ok(l)) => l,
            Ok(Err(e)) => {
                eprintln!("stream error: {e}");
                break;
            }
            Err(_) => break,
        };
        seq += 1;
        total += 1;
        let mut entry: LogEntry = to_entry(seq, &log);
        entry.message = scrub.apply(&entry.message);
        entry.process = scrub.apply(&entry.process);
        processes.insert(entry.process.clone());
        if entry.process.eq_ignore_ascii_case("thermalmonitord") {
            thermal_lines.push(entry.message.clone());
        }
        for d in derive.observe(&entry) {
            derived.push(format!("{d:?}"));
        }
        if written < MAX_LINES {
            serde_json::to_writer(&mut jsonl, &entry).unwrap();
            jsonl.write_all(b"\n").unwrap();
            written += 1;
        }
    }
    jsonl.flush().unwrap();
    std::fs::write(&thermal_path, thermal_lines.join("\n") + "\n").unwrap();

    eprintln!(
        "{total} entries seen, {written} written to {}",
        jsonl_path.display()
    );
    eprintln!(
        "{} thermalmonitord lines → {}",
        thermal_lines.len(),
        thermal_path.display()
    );
    eprintln!(
        "{} distinct processes; sample: {:?}",
        processes.len(),
        processes.iter().take(25).collect::<Vec<_>>()
    );
    eprintln!("derived: {}", derived.len());
    for d in derived.iter().take(20) {
        eprintln!("  {d}");
    }
    eprintln!("last thermal state: {:?}", derive.last_state());
}

/// Lockdown values worth scrubbing from the text.
async fn lockdown_values(
    provider: &dyn IdeviceProvider,
) -> Result<Vec<(&'static str, String)>, idevice::IdeviceError> {
    let mut lockdown = LockdownClient::connect(provider).await?;
    lockdown
        .start_session(&provider.get_pairing_file().await?)
        .await?;
    let mut out = Vec::new();
    for key in [
        "DeviceName",
        "SerialNumber",
        "UniqueDeviceID",
        "WiFiAddress",
        "BluetoothAddress",
        "EthernetAddress",
        "MLBSerialNumber",
        "InternationalMobileEquipmentIdentity",
    ] {
        if let Ok(v) = lockdown.get_value(Some(key), None).await
            && let Some(s) = v.as_string()
            && !s.is_empty()
        {
            out.push((key, s.to_owned()));
        }
    }
    Ok(out)
}

/// Replaces identifying strings with placeholders.
struct Scrubber {
    /// (needle, replacement), longest first.
    exact: Vec<(String, String)>,
}

impl Scrubber {
    fn new(udid: &str) -> Self {
        Self {
            exact: vec![(udid.to_owned(), "<UDID>".to_owned())],
        }
    }

    fn add_values(&mut self, values: Vec<(&'static str, String)>) {
        for (key, value) in values {
            let placeholder = match key {
                "DeviceName" => "<DEVICE-NAME>",
                "SerialNumber" | "MLBSerialNumber" => "<SERIAL>",
                "UniqueDeviceID" => "<UDID>",
                "WiFiAddress" | "BluetoothAddress" | "EthernetAddress" => "<MAC>",
                _ => "<ID>",
            };
            if key == "DeviceName" {
                // "Elia's iPad" → also scrub "Elia" and the URL-ish forms.
                if let Some(owner) = value
                    .split(['\'', '’'])
                    .next()
                    .map(str::trim)
                    .filter(|o| !o.is_empty() && o.len() < value.len())
                {
                    self.exact.push((owner.to_owned(), "<OWNER>".to_owned()));
                }
                self.exact
                    .push((value.replace(' ', "-"), placeholder.to_owned()));
            }
            self.exact.push((value, placeholder.to_owned()));
        }
        self.exact.sort_by_key(|a| std::cmp::Reverse(a.0.len()));
    }

    fn apply(&self, text: &str) -> String {
        let mut s = text.to_owned();
        for (needle, rep) in &self.exact {
            if s.contains(needle.as_str()) {
                s = s.replace(needle.as_str(), rep);
            }
            let lower = needle.to_ascii_lowercase();
            if lower != *needle && s.contains(lower.as_str()) {
                s = s.replace(lower.as_str(), rep);
            }
        }
        s = scrub_macs(&s);
        s = scrub_emails(&s);
        scrub_ssids(&s)
    }
}

/// `aa:bb:cc:dd:ee:ff` (or `-` separated) → `<MAC>`.
fn scrub_macs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 17 <= chars.len() && is_mac(&chars[i..i + 17]) {
            let before_ok = i == 0 || !chars[i - 1].is_ascii_hexdigit();
            let after_ok = i + 17 == chars.len() || !chars[i + 17].is_ascii_hexdigit();
            if before_ok && after_ok {
                out.push_str("<MAC>");
                i += 17;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_mac(c: &[char]) -> bool {
    (0..17).all(|k| {
        if k % 3 == 2 {
            c[k] == ':' || c[k] == '-'
        } else {
            c[k].is_ascii_hexdigit()
        }
    })
}

/// `name@host.tld` → `<EMAIL>`.
fn scrub_emails(s: &str) -> String {
    if !s.contains('@') {
        return s.to_owned();
    }
    s.split(' ')
        .map(|tok| {
            let core = tok.trim_matches(|c: char| !c.is_alphanumeric());
            if let Some((user, host)) = core.split_once('@')
                && !user.is_empty()
                && host.contains('.')
                && host
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
            {
                tok.replace(core, "<EMAIL>")
            } else {
                tok.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `SSID: "Foo"` / `SSID=Foo` / `ssid Foo` → `<SSID>`.
fn scrub_ssids(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let Some(pos) = lower.find("ssid") else {
        return s.to_owned();
    };
    let mut out = String::new();
    let mut rest = s;
    let mut at = pos;
    loop {
        let (head, tail) = rest.split_at(at + 4);
        out.push_str(head);
        let tail_trim = tail.trim_start_matches([' ', ':', '=', '\t']);
        let sep_len = tail.len() - tail_trim.len();
        out.push_str(&tail[..sep_len]);
        let value_len = if let Some(inner) = tail_trim.strip_prefix('"') {
            inner.find('"').map(|n| n + 2).unwrap_or(tail_trim.len())
        } else if let Some(inner) = tail_trim.strip_prefix('\'') {
            inner.find('\'').map(|n| n + 2).unwrap_or(tail_trim.len())
        } else {
            tail_trim
                .find([' ', ',', ';', ')', ']', '}'])
                .unwrap_or(tail_trim.len())
        };
        if value_len > 0 {
            out.push_str("<SSID>");
        }
        rest = &tail_trim[value_len..];
        match rest.to_ascii_lowercase().find("ssid") {
            Some(p) => at = p,
            None => {
                out.push_str(rest);
                return out;
            }
        }
    }
}
