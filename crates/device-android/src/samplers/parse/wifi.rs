//! `cmd wifi status`.

use super::ParseError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiStatus {
    pub enabled: bool,
    pub connected: bool,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    pub rssi_dbm: Option<i32>,
    pub link_mbps: Option<u32>,
    pub freq_mhz: Option<u32>,
    pub standard: Option<String>,
    pub successful_tx: Option<u64>,
    pub retried_tx: Option<u64>,
    pub lost_tx: Option<u64>,
}

impl WifiStatus {
    /// One line for the Connection event: `Wi-Fi Studio-5G · 5180 MHz · 11ac`.
    pub fn summary(&self) -> String {
        if !self.connected {
            return if self.enabled {
                "Wi-Fi on, not connected".to_owned()
            } else {
                "Wi-Fi off".to_owned()
            };
        }
        let mut s = format!("Wi-Fi {}", self.ssid.as_deref().unwrap_or("<hidden>"));
        if let Some(f) = self.freq_mhz {
            s.push_str(&format!(" · {f} MHz"));
        }
        if let Some(std) = &self.standard {
            s.push_str(&format!(" · {std}"));
        }
        s
    }
}

/// Value of `key: value` inside the one-line `WifiInfo:` record (fields
/// are comma separated).
fn field<'a>(info: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}: ");
    let start = info.find(&needle)? + needle.len();
    let rest = &info[start..];
    let end = rest.find(',').unwrap_or(rest.len());
    Some(rest[..end].trim())
}

pub fn parse_wifi_status(text: &str) -> Result<WifiStatus, ParseError> {
    if text.trim().is_empty() {
        return Err(ParseError::Empty("cmd wifi status"));
    }
    let mut w = WifiStatus {
        enabled: text.lines().any(|l| l.trim() == "Wifi is enabled"),
        ..Default::default()
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Wifi is connected to ") {
            w.connected = true;
            w.ssid = Some(rest.trim_matches('"').to_owned());
        } else if let Some(info) = line.strip_prefix("WifiInfo:") {
            if w.ssid.is_none()
                && let Some(s) = field(info, "SSID")
                && s != "<unknown ssid>"
            {
                w.ssid = Some(s.trim_matches('"').to_owned());
            }
            w.bssid = field(info, "BSSID").map(str::to_owned);
            w.rssi_dbm = field(info, "RSSI").and_then(|v| v.parse().ok());
            w.link_mbps = field(info, "Link speed")
                .and_then(|v| v.trim_end_matches("Mbps").trim().parse().ok());
            w.freq_mhz = field(info, "Frequency")
                .and_then(|v| v.trim_end_matches("MHz").trim().parse().ok());
            w.standard = field(info, "Wi-Fi standard").map(str::to_owned);
            if field(info, "Supplicant state") == Some("COMPLETED") {
                w.connected = true;
            }
        } else if let Some((k, v)) = line.split_once(": ") {
            let v = v.trim().parse::<u64>().ok();
            match k {
                "successfulTxPackets" => w.successful_tx = v,
                "retriedTxPackets" => w.retried_tx = v,
                "lostTxPackets" => w.lost_tx = v,
                _ => {}
            }
        }
    }
    // -127 / 0 dBm are "unknown" sentinels in WifiInfo.
    if matches!(w.rssi_dbm, Some(r) if r <= -127 || r >= 0) {
        w.rssi_dbm = None;
    }
    if !w.connected {
        w.rssi_dbm = None;
        w.link_mbps = None;
    }
    Ok(w)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::samplers::parse::fixture;

    #[test]
    fn wifi_fixture() {
        let w = parse_wifi_status(&fixture("cmd_wifi_status")).unwrap();
        assert!(w.enabled && w.connected);
        assert_eq!(w.ssid.as_deref(), Some("Studio-5G"));
        assert_eq!(w.rssi_dbm, Some(-62));
        assert_eq!(w.link_mbps, Some(300));
        assert_eq!(w.freq_mhz, Some(5180));
        assert_eq!(w.standard.as_deref(), Some("11ac"));
        assert_eq!(w.retried_tx, Some(6450));
        assert_eq!(w.lost_tx, Some(8));
        assert_eq!(w.successful_tx, Some(51674));
        assert_eq!(w.summary(), "Wi-Fi Studio-5G · 5180 MHz · 11ac");
    }

    #[test]
    fn wifi_disconnected() {
        let w = parse_wifi_status("Wifi is enabled\nWifi is not connected\n").unwrap();
        assert!(w.enabled && !w.connected);
        assert_eq!(w.rssi_dbm, None);
        assert_eq!(w.summary(), "Wi-Fi on, not connected");
        assert!(parse_wifi_status("").is_err());
    }
}
