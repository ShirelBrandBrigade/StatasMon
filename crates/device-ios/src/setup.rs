//! Installing what iPhones and iPads need on this computer (ADR-022).
//!
//! macOS ships Apple's USB service. Windows gets it from Apple's own
//! installer, which cannot be automated. On Linux it is the `usbmuxd`
//! package, and this module can install it when the user asks: the install
//! runs through `pkexec`, so the system's own dialog asks for the password
//! and StatasMon never sees it. Nothing here runs unless the user pressed
//! the button.
//!
//! The OS and package-manager knowledge lives here and nowhere else.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

/// How long an install may take before it is abandoned.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Lines of installer output kept for the result message.
const OUTPUT_TAIL_LINES: usize = 12;

/// Package managers this app knows how to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Apt,
    Dnf,
    Pacman,
    Zypper,
}

impl PackageManager {
    /// In detection order.
    pub const ALL: [PackageManager; 4] = [
        PackageManager::Apt,
        PackageManager::Dnf,
        PackageManager::Pacman,
        PackageManager::Zypper,
    ];

    /// The executable that identifies it.
    pub fn binary(self) -> &'static str {
        match self {
            PackageManager::Apt => "apt-get",
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
            PackageManager::Zypper => "zypper",
        }
    }

    /// Name shown to the user.
    pub fn label(self) -> &'static str {
        match self {
            PackageManager::Apt => "apt",
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
            PackageManager::Zypper => "zypper",
        }
    }

    /// The exact command line run as root, shown to the user before it runs.
    pub fn install_script(self) -> &'static str {
        match self {
            PackageManager::Apt => {
                "apt-get update && apt-get install -y usbmuxd && (systemctl start usbmuxd || true)"
            }
            PackageManager::Dnf => "dnf install -y usbmuxd && (systemctl start usbmuxd || true)",
            PackageManager::Pacman => {
                "pacman -S --noconfirm --needed usbmuxd && (systemctl start usbmuxd || true)"
            }
            PackageManager::Zypper => {
                "zypper --non-interactive install usbmuxd && (systemctl start usbmuxd || true)"
            }
        }
    }
}

/// Directories searched for executables (a desktop launcher's `PATH` can be short).
const BIN_DIRS: [&str; 5] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin", "/usr/local/bin"];

fn on_disk(binary: &str) -> bool {
    BIN_DIRS.iter().any(|d| Path::new(d).join(binary).is_file())
}

/// First known package manager for which `exists` answers true.
pub fn detect_package_manager(exists: impl Fn(&str) -> bool) -> Option<PackageManager> {
    PackageManager::ALL
        .into_iter()
        .find(|pm| exists(pm.binary()))
}

/// What the UI needs to offer (or not offer) the install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppleSupportStatus {
    /// "macos", "windows" or "linux".
    pub os: String,
    /// True when pressing the button can do the install on this computer.
    pub installable: bool,
    /// "apt", "dnf", "pacman", "zypper".
    pub package_manager: Option<String>,
    /// The command that would run as root, for display.
    pub command: Option<String>,
    /// Why it cannot be installed from the app, when it cannot.
    pub reason: Option<String>,
}

/// Decide from facts, so every branch is testable on any host.
pub fn status_from(
    os: &str,
    package_manager: Option<PackageManager>,
    has_pkexec: bool,
) -> AppleSupportStatus {
    let base = |installable, pm: Option<PackageManager>, reason: Option<&str>| AppleSupportStatus {
        os: os.to_owned(),
        installable,
        package_manager: pm.map(|p| p.label().to_owned()),
        command: pm.map(|p| p.install_script().to_owned()),
        reason: reason.map(str::to_owned),
    };
    match os {
        "linux" => match (package_manager, has_pkexec) {
            (Some(pm), true) => base(true, Some(pm), None),
            (Some(pm), false) => base(
                false,
                Some(pm),
                Some(
                    "pkexec (polkit) is not installed, so the system cannot ask for your password; run the command in a terminal with sudo.",
                ),
            ),
            (None, _) => base(
                false,
                None,
                Some(
                    "No supported package manager was found (apt, dnf, pacman, zypper); install the usbmuxd package with your distribution's tool.",
                ),
            ),
        },
        "windows" => base(
            false,
            None,
            Some(
                "Install the free Apple Devices app from the Microsoft Store (or iTunes from apple.com).",
            ),
        ),
        _ => base(
            false,
            None,
            Some("macOS provides the Apple USB service itself."),
        ),
    }
}

fn host_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "macos"
    }
}

/// The status for this computer.
pub fn apple_support_status() -> AppleSupportStatus {
    let os = host_os();
    if os != "linux" {
        return status_from(os, None, false);
    }
    status_from(os, detect_package_manager(on_disk), on_disk("pkexec"))
}

/// Why an install did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    #[error("{0}")]
    NotInstallable(String),
    #[error("The password prompt was dismissed; nothing was installed.")]
    Dismissed,
    #[error("The system did not authorise the install.")]
    NotAuthorised,
    #[error("The install did not finish within {0} s.")]
    TimedOut(u64),
    #[error("The installer failed (exit {code}): {tail}")]
    Failed { code: i32, tail: String },
    #[error("Could not start the installer: {0}")]
    Spawn(String),
}

/// Last lines of what the installer printed.
pub fn output_tail(stdout: &str, stderr: &str) -> String {
    let all: Vec<&str> = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect();
    let from = all.len().saturating_sub(OUTPUT_TAIL_LINES);
    all[from..].join("\n")
}

/// pkexec reserves 126 (dialog dismissed) and 127 (not authorised).
pub fn classify_exit(code: i32, tail: String) -> Result<String, SetupError> {
    match code {
        0 => Ok(tail),
        126 => Err(SetupError::Dismissed),
        127 => Err(SetupError::NotAuthorised),
        code => Err(SetupError::Failed { code, tail }),
    }
}

/// Install usbmuxd through the system's password dialog. Only ever called
/// from the user's click; returns the installer's last lines on success.
pub async fn install_apple_support() -> Result<String, SetupError> {
    let status = apple_support_status();
    let (true, Some(script)) = (status.installable, status.command.clone()) else {
        return Err(SetupError::NotInstallable(status.reason.unwrap_or_else(
            || "Nothing to install on this computer.".to_owned(),
        )));
    };
    let mut cmd = tokio::process::Command::new("pkexec");
    cmd.arg("/bin/sh")
        .arg("-c")
        .arg(&script)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    tracing::info!(script = %script, "ios setup: installing usbmuxd through pkexec");
    let out = tokio::time::timeout(INSTALL_TIMEOUT, cmd.output())
        .await
        .map_err(|_| SetupError::TimedOut(INSTALL_TIMEOUT.as_secs()))?
        .map_err(|e| SetupError::Spawn(e.to_string()))?;
    let tail = output_tail(
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
    );
    classify_exit(out.status.code().unwrap_or(-1), tail)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_first_known_package_manager() {
        assert_eq!(
            detect_package_manager(|b| b == "dnf" || b == "zypper"),
            Some(PackageManager::Dnf)
        );
        assert_eq!(
            detect_package_manager(|b| b == "apt-get"),
            Some(PackageManager::Apt)
        );
        assert_eq!(detect_package_manager(|_| false), None);
    }

    #[test]
    fn every_script_installs_usbmuxd_without_prompts() {
        for pm in PackageManager::ALL {
            let s = pm.install_script();
            assert!(s.contains("usbmuxd"), "{s}");
            assert!(s.starts_with(pm.binary()), "{s}");
            assert!(
                s.contains("-y") || s.contains("--noconfirm") || s.contains("--non-interactive"),
                "{s}"
            );
        }
    }

    #[test]
    fn status_explains_every_case() {
        let ok = status_from("linux", Some(PackageManager::Apt), true);
        assert!(ok.installable);
        assert_eq!(ok.package_manager.as_deref(), Some("apt"));
        assert!(ok.command.unwrap().contains("apt-get install -y usbmuxd"));
        assert_eq!(ok.reason, None);

        let no_polkit = status_from("linux", Some(PackageManager::Dnf), false);
        assert!(!no_polkit.installable);
        assert!(no_polkit.command.is_some());
        assert!(no_polkit.reason.unwrap().contains("pkexec"));

        let unknown = status_from("linux", None, true);
        assert!(!unknown.installable);
        assert!(unknown.reason.unwrap().contains("usbmuxd"));

        let win = status_from("windows", None, false);
        assert!(!win.installable);
        assert!(win.reason.unwrap().contains("Apple Devices"));

        let mac = status_from("macos", None, false);
        assert!(!mac.installable && mac.command.is_none());
    }

    #[test]
    fn exit_codes_and_output_tail() {
        assert_eq!(classify_exit(0, "done".into()), Ok("done".to_owned()));
        assert_eq!(
            classify_exit(126, String::new()),
            Err(SetupError::Dismissed)
        );
        assert_eq!(
            classify_exit(127, String::new()),
            Err(SetupError::NotAuthorised)
        );
        assert!(matches!(
            classify_exit(100, "E: broken".into()),
            Err(SetupError::Failed { code: 100, .. })
        ));
        let many: String = (0..30).map(|i| format!("line {i}\n")).collect();
        let tail = output_tail(&many, "warn: x\n\n");
        assert_eq!(tail.lines().count(), OUTPUT_TAIL_LINES);
        assert!(tail.ends_with("warn: x"));
    }
}
