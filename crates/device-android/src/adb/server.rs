//! The adb server: start or reuse it, list devices, wireless pairing.
//!
//! Every call spawns the adb binary as a subprocess (ADR-009). The server
//! port is whatever adb itself uses: `ANDROID_ADB_SERVER_PORT` is inherited
//! by the child, so a user who runs Android Studio on 5037 and us on another
//! port only sets that variable.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use statasmon_protocol::ToolVersion;
use tokio::process::Command;

use crate::shell::ShellError;
use crate::shell::process::ProcessShell;

/// `adb start-server` can take a few seconds on first launch.
const START_TIMEOUT_MS: u64 = 15_000;
/// `adb devices -l` is a local round-trip.
const DEVICES_TIMEOUT_MS: u64 = 5_000;
/// `adb pair` / `adb connect` wait for the device over the network.
const NETWORK_TIMEOUT_MS: u64 = 30_000;

/// Connection state as printed by `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdbState {
    /// Authorized and ready for `shell`.
    Device,
    /// The device is waiting for the user to accept the USB debugging prompt.
    Unauthorized,
    /// adbd is not responding (booting, or a stale TCP entry).
    Offline,
    /// The host cannot open the USB device (udev/driver on Linux/Windows).
    NoPermissions,
    /// Anything else (`recovery`, `sideload`, `bootloader`, `connecting`...).
    Other(String),
}

impl AdbState {
    fn parse(word: &str) -> Self {
        match word {
            "device" => AdbState::Device,
            "unauthorized" => AdbState::Unauthorized,
            "offline" => AdbState::Offline,
            w if w.starts_with("no permissions") => AdbState::NoPermissions,
            other => AdbState::Other(other.to_owned()),
        }
    }

    /// The user-facing reason a device in this state provides nothing yet.
    pub fn reason(&self) -> String {
        match self {
            AdbState::Device => String::new(),
            AdbState::Unauthorized => {
                "Accept the USB debugging prompt on the device".to_owned()
            }
            AdbState::Offline => {
                "The device is offline for adb: unplug and replug it, or reconnect over Wi-Fi"
                    .to_owned()
            }
            AdbState::NoPermissions => {
                "The computer cannot open this device: install the USB driver (Windows) or a udev rule (Linux)"
                    .to_owned()
            }
            AdbState::Other(s) => format!("The device is in '{s}' mode; boot it to Android"),
        }
    }
}

/// One line of `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    pub serial: String,
    pub state: AdbState,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<u32>,
    /// True when adb printed a `usb:` attribute (as opposed to a TCP serial).
    pub usb: bool,
}

impl AdbDevice {
    /// True for `host:port` serials (wireless debugging / `adb connect`).
    pub fn is_tcp(&self) -> bool {
        self.serial.contains(':')
    }

    /// Model with adb's underscores restored ("SM_X700" → "SM-X700").
    pub fn model_name(&self) -> Option<String> {
        self.model.as_ref().map(|m| m.replace('_', "-"))
    }
}

/// Parse `adb devices -l` output. Lines that are not device lines (the
/// header, daemon notices, blanks) are skipped.
pub fn parse_devices(text: &str) -> Vec<AdbDevice> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb:")
            || line.starts_with("error:")
        {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(serial) = words.next() else {
            continue;
        };
        let rest: Vec<&str> = words.collect();
        let Some(first) = rest.first() else {
            continue;
        };
        // "no permissions (reason); see [url]" spans several words: take
        // everything up to the first key:value attribute as the state.
        let attr_start = rest
            .iter()
            .position(|w| is_attribute(w))
            .unwrap_or(rest.len());
        let state_text = if attr_start > 1 {
            rest[..attr_start].join(" ")
        } else {
            (*first).to_owned()
        };
        let mut dev = AdbDevice {
            serial: serial.to_owned(),
            state: AdbState::parse(&state_text),
            product: None,
            model: None,
            device: None,
            transport_id: None,
            usb: false,
        };
        for attr in &rest[attr_start..] {
            if let Some((k, v)) = attr.split_once(':') {
                match k {
                    "usb" => dev.usb = true,
                    "product" => dev.product = Some(v.to_owned()),
                    "model" => dev.model = Some(v.to_owned()),
                    "device" => dev.device = Some(v.to_owned()),
                    "transport_id" => dev.transport_id = v.parse().ok(),
                    _ => {}
                }
            }
        }
        out.push(dev);
    }
    out
}

fn is_attribute(word: &str) -> bool {
    matches!(
        word.split_once(':').map(|(k, _)| k),
        Some("usb" | "product" | "model" | "device" | "transport_id")
    )
}

/// Parse `adb version` output into a [`ToolVersion`] (the `Version` line;
/// falls back to the "Android Debug Bridge version" line).
pub fn parse_version(text: &str) -> Option<ToolVersion> {
    let mut bridge = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Version ") {
            return Some(ToolVersion {
                name: "adb".to_owned(),
                version: v.trim().to_owned(),
            });
        }
        if let Some(v) = line.strip_prefix("Android Debug Bridge version ") {
            bridge = Some(v.trim().to_owned());
        }
    }
    bridge.map(|version| ToolVersion {
        name: "adb".to_owned(),
        version,
    })
}

/// A handle on the adb binary and the server it talks to.
#[derive(Debug, Clone)]
pub struct AdbServer {
    adb: Arc<PathBuf>,
}

impl AdbServer {
    /// Use the adb at `path` (from the locator).
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            adb: Arc::new(path.as_ref().to_path_buf()),
        }
    }

    /// The binary in use.
    pub fn adb_path(&self) -> &Path {
        &self.adb
    }

    /// Run `adb <args>` and return stdout followed by stderr.
    pub async fn run(&self, args: &[&str], timeout_ms: u64) -> Result<String, ShellError> {
        let mut c = Command::new(&*self.adb);
        c.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        c.kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_millis(timeout_ms), c.output())
            .await
            .map_err(|_| ShellError::Timeout(timeout_ms))?
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => {
                    ShellError::AdbNotFound(self.adb.display().to_string())
                }
                _ => ShellError::Io(e),
            })?;
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.stderr.is_empty() {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&String::from_utf8_lossy(&out.stderr));
        }
        Ok(text)
    }

    /// Start the server, or reuse the one already listening.
    pub async fn start(&self) -> Result<(), ShellError> {
        let out = self.run(&["start-server"], START_TIMEOUT_MS).await?;
        let lower = out.to_ascii_lowercase();
        if lower.contains("error") || lower.contains("failed to start") {
            return Err(ShellError::Server(out.trim().to_owned()));
        }
        Ok(())
    }

    /// `adb version`.
    pub async fn version(&self) -> Result<ToolVersion, ShellError> {
        let out = self.run(&["version"], DEVICES_TIMEOUT_MS).await?;
        parse_version(&out)
            .ok_or_else(|| ShellError::Server(format!("unexpected version output: {out}")))
    }

    /// `adb devices -l`.
    pub async fn devices(&self) -> Result<Vec<AdbDevice>, ShellError> {
        let out = self.run(&["devices", "-l"], DEVICES_TIMEOUT_MS).await?;
        if !out.contains("List of devices") {
            return Err(ShellError::Server(out.trim().to_owned()));
        }
        Ok(parse_devices(&out))
    }

    /// `adb pair host:port code` (Android 11+ wireless debugging).
    pub async fn pair(&self, host: &str, port: u16, code: &str) -> Result<String, ShellError> {
        let target = format!("{host}:{port}");
        self.run(&["pair", &target, code], NETWORK_TIMEOUT_MS).await
    }

    /// `adb connect host:port`.
    pub async fn connect(&self, host: &str, port: u16) -> Result<String, ShellError> {
        let target = format!("{host}:{port}");
        self.run(&["connect", &target], NETWORK_TIMEOUT_MS).await
    }

    /// A shell bound to one device.
    pub fn shell(&self, serial: &str) -> ProcessShell {
        ProcessShell::new(&*self.adb, serial)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const LISTING: &str = "* daemon not running; starting now at tcp:5037\n\
* daemon started successfully\n\
List of devices attached\n\
R5XXXXXXXXX            device usb:17825792X product:gts8wifixx model:SM_X700 device:gts8wifi transport_id:1\n\
192.168.50.20:5555     device product:panther model:Pixel_7 device:panther transport_id:3\n\
ZY22ABCDEF             unauthorized usb:2-1 transport_id:4\n\
0123456789ABCDEF       offline transport_id:5\n\
1234567890             no permissions (user in plugdev group; are your udev rules wrong?); see [http://developer.android.com/tools/device.html] usb:1-2 transport_id:6\n\
\n";

    #[test]
    fn parses_every_state_and_attribute() {
        let d = parse_devices(LISTING);
        assert_eq!(d.len(), 5);
        assert_eq!(d[0].serial, "R5XXXXXXXXX");
        assert_eq!(d[0].state, AdbState::Device);
        assert!(d[0].usb);
        assert_eq!(d[0].product.as_deref(), Some("gts8wifixx"));
        assert_eq!(d[0].model.as_deref(), Some("SM_X700"));
        assert_eq!(d[0].model_name().as_deref(), Some("SM-X700"));
        assert_eq!(d[0].device.as_deref(), Some("gts8wifi"));
        assert_eq!(d[0].transport_id, Some(1));
        assert!(!d[0].is_tcp());

        assert_eq!(d[1].serial, "192.168.50.20:5555");
        assert!(d[1].is_tcp());
        assert!(!d[1].usb);
        assert_eq!(d[1].model_name().as_deref(), Some("Pixel-7"));

        assert_eq!(d[2].state, AdbState::Unauthorized);
        assert!(d[2].usb);
        assert_eq!(d[3].state, AdbState::Offline);
        assert_eq!(d[4].state, AdbState::NoPermissions);
        assert!(d[4].usb);
        assert_eq!(d[4].transport_id, Some(6));
    }

    #[test]
    fn empty_listing_and_errors() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert!(parse_devices("error: cannot connect to daemon\n").is_empty());
        let d = parse_devices("ABC recovery transport_id:2\n");
        assert_eq!(d[0].state, AdbState::Other("recovery".into()));
    }

    #[test]
    fn version_line() {
        let v = parse_version(
            "Android Debug Bridge version 1.0.41\nVersion 37.0.1-13800976\nInstalled as /x/adb\nRunning on Darwin 25.5.0\n",
        )
        .unwrap();
        assert_eq!(v.name, "adb");
        assert_eq!(v.version, "37.0.1-13800976");
        let v = parse_version("Android Debug Bridge version 1.0.32\n").unwrap();
        assert_eq!(v.version, "1.0.32");
        assert!(parse_version("garbage").is_none());
    }

    #[test]
    fn reasons_are_user_facing() {
        assert!(AdbState::Unauthorized.reason().contains("USB debugging"));
        assert!(AdbState::NoPermissions.reason().contains("driver"));
        assert!(
            AdbState::Other("sideload".into())
                .reason()
                .contains("sideload")
        );
    }
}
