//! Finding the `adb` binary (docs/PLAN.md step 2.1).
//!
//! Search order:
//! 1. `STATASMON_ADB` (explicit override; the Settings screen writes it)
//! 2. the copy bundled inside the app (`STATASMON_ADB_BUNDLED_DIR`, set by
//!    the Tauri shell from its resource folder; ADR-021)
//! 3. the app's own managed copy under the app-data folder
//! 4. `adb` on `PATH`
//! 5. `~/Downloads/platform-tools/adb` (developer convenience)
//! 6. the Android SDK default location
//!
//! Platform-specific paths live behind [`HostOs`]; nothing else in the
//! crate branches on the OS.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use statasmon_protocol::ToolVersion;

use super::server::parse_version;

/// Google's platform-tools download page, for the hint when no adb is
/// found at all (builds made with `STATASMON_SKIP_ADB_BUNDLE=1`).
pub const PLATFORM_TOOLS_URL: &str = "https://developer.android.com/tools/releases/platform-tools";

/// Timeout for `adb version` while locating.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// The operating system we run on. The single place that knows about OS
/// differences in path conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Windows,
    Linux,
}

impl HostOs {
    /// The host we were compiled for.
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            HostOs::Windows
        } else if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else {
            HostOs::Linux
        }
    }

    /// The adb executable file name.
    pub fn adb_file_name(self) -> &'static str {
        match self {
            HostOs::Windows => "adb.exe",
            HostOs::MacOs | HostOs::Linux => "adb",
        }
    }

    /// `PATH` separator.
    fn path_separator(self) -> char {
        match self {
            HostOs::Windows => ';',
            HostOs::MacOs | HostOs::Linux => ':',
        }
    }
}

/// Everything the locator reads from the environment, so the search order
/// is a pure function that tests can drive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocatorEnv {
    /// `STATASMON_ADB`.
    pub override_path: Option<PathBuf>,
    /// `STATASMON_ADB_BUNDLED_DIR`: the platform-tools folder shipped inside the app.
    pub bundled_dir: Option<PathBuf>,
    /// `HOME` (macOS/Linux) or `USERPROFILE` (Windows).
    pub home: Option<PathBuf>,
    /// `%LOCALAPPDATA%` on Windows.
    pub local_app_data: Option<PathBuf>,
    /// `PATH`.
    pub path: Option<String>,
}

/// The platform-tools folder shipped inside the app, registered once by the
/// shell at startup (`set_bundled_dir`); `STATASMON_ADB_BUNDLED_DIR` wins
/// when set (tests, dev runs).
static BUNDLED_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Register the bundled platform-tools folder for every later search.
pub fn set_bundled_dir(dir: PathBuf) {
    let _ = BUNDLED_DIR.set(dir);
}

impl LocatorEnv {
    /// Read the real environment.
    pub fn from_process() -> Self {
        Self {
            override_path: std::env::var_os("STATASMON_ADB").map(PathBuf::from),
            bundled_dir: std::env::var_os("STATASMON_ADB_BUNDLED_DIR")
                .map(PathBuf::from)
                .or_else(|| BUNDLED_DIR.get().cloned()),
            home: std::env::home_dir(),
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            path: std::env::var("PATH").ok(),
        }
    }
}

/// Where a candidate came from (shown on the Settings screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdbSource {
    /// `STATASMON_ADB`.
    Env,
    /// Shipped inside the app bundle.
    Bundled,
    /// The app-managed copy under the app-data folder.
    Managed,
    /// Found on `PATH`.
    Path,
    /// `~/Downloads/platform-tools`.
    Downloads,
    /// Android SDK default location.
    Sdk,
}

/// Result of a search.
#[derive(Debug, Clone, PartialEq)]
pub enum AdbLocation {
    Found {
        path: PathBuf,
        source: AdbSource,
        /// `adb version` (`Version 37.0.1-…`), `None` when it did not run.
        version: Option<ToolVersion>,
    },
    Missing {
        /// User-facing instructions, including the managed path to unzip into.
        hint: String,
    },
}

impl AdbLocation {
    /// The path when found.
    pub fn path(&self) -> Option<&Path> {
        match self {
            AdbLocation::Found { path, .. } => Some(path),
            AdbLocation::Missing { .. } => None,
        }
    }
}

/// The app-managed platform-tools directory for `os`.
pub fn managed_dir(os: HostOs, env: &LocatorEnv) -> Option<PathBuf> {
    let base = match os {
        HostOs::MacOs => env.home.as_ref()?.join("Library/Application Support"),
        HostOs::Windows => env.local_app_data.clone()?,
        HostOs::Linux => env.home.as_ref()?.join(".local").join("share"),
    };
    Some(base.join("StatasMon").join("platform-tools"))
}

/// The Android SDK's platform-tools directory for `os`.
fn sdk_dir(os: HostOs, env: &LocatorEnv) -> Option<PathBuf> {
    let base = match os {
        HostOs::MacOs => env.home.as_ref()?.join("Library/Android/sdk"),
        HostOs::Windows => env.local_app_data.clone()?.join("Android").join("Sdk"),
        HostOs::Linux => env.home.as_ref()?.join("Android/Sdk"),
    };
    Some(base.join("platform-tools"))
}

/// Every candidate in search order, whether or not it exists.
pub fn candidates(os: HostOs, env: &LocatorEnv) -> Vec<(AdbSource, PathBuf)> {
    let name = os.adb_file_name();
    let mut out = Vec::new();
    if let Some(p) = &env.override_path {
        out.push((AdbSource::Env, p.clone()));
    }
    if let Some(dir) = &env.bundled_dir {
        out.push((AdbSource::Bundled, dir.join(name)));
    }
    if let Some(dir) = managed_dir(os, env) {
        out.push((AdbSource::Managed, dir.join(name)));
    }
    if let Some(path) = &env.path {
        for dir in path.split(os.path_separator()).filter(|d| !d.is_empty()) {
            out.push((AdbSource::Path, Path::new(dir).join(name)));
        }
    }
    if let Some(home) = &env.home {
        out.push((
            AdbSource::Downloads,
            home.join("Downloads").join("platform-tools").join(name),
        ));
    }
    if let Some(dir) = sdk_dir(os, env) {
        out.push((AdbSource::Sdk, dir.join(name)));
    }
    out
}

/// The instructions shown when nothing was found.
pub fn missing_hint(os: HostOs, env: &LocatorEnv) -> String {
    let managed = managed_dir(os, env)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "the StatasMon app-data folder".to_owned());
    format!(
        "adb (Android platform-tools) was not found. Download platform-tools from \
         {PLATFORM_TOOLS_URL}, then either unzip it so that {managed} contains {name}, \
         put adb on your PATH, or set STATASMON_ADB to the full path of the adb binary.",
        name = os.adb_file_name()
    )
}

/// First candidate that exists as a file.
pub fn first_existing(os: HostOs, env: &LocatorEnv) -> Option<(AdbSource, PathBuf)> {
    candidates(os, env).into_iter().find(|(_, p)| p.is_file())
}

/// Run `<adb> version` and parse the `Version` line.
pub async fn tool_version(adb: &Path) -> Option<ToolVersion> {
    let mut cmd = tokio::process::Command::new(adb);
    cmd.arg("version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    cmd.kill_on_drop(true);
    let out = tokio::time::timeout(VERSION_TIMEOUT, cmd.output())
        .await
        .ok()?
        .ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// Search with the real environment and report the version.
pub async fn locate() -> AdbLocation {
    locate_with(HostOs::current(), &LocatorEnv::from_process()).await
}

/// Search with an explicit host and environment (tests).
pub async fn locate_with(os: HostOs, env: &LocatorEnv) -> AdbLocation {
    match first_existing(os, env) {
        Some((source, path)) => {
            let version = tool_version(&path).await;
            AdbLocation::Found {
                path,
                source,
                version,
            }
        }
        None => AdbLocation::Missing {
            hint: missing_hint(os, env),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn env() -> LocatorEnv {
        LocatorEnv {
            override_path: None,
            bundled_dir: None,
            home: Some(PathBuf::from("/Users/dev")),
            local_app_data: Some(PathBuf::from("C:\\Users\\dev\\AppData\\Local")),
            path: Some("/usr/bin:/opt/homebrew/bin".to_owned()),
        }
    }

    #[test]
    fn bundled_copy_comes_right_after_the_explicit_override() {
        let mut e = env();
        e.override_path = Some(PathBuf::from("/x/adb"));
        e.bundled_dir = Some(PathBuf::from(
            "/Applications/StatasMon.app/Contents/Resources/platform-tools",
        ));
        let c = candidates(HostOs::MacOs, &e);
        assert_eq!(c[0].0, AdbSource::Env);
        assert_eq!(c[1].0, AdbSource::Bundled);
        assert_eq!(
            c[1].1.display().to_string(),
            "/Applications/StatasMon.app/Contents/Resources/platform-tools/adb"
        );
        assert_eq!(c[2].0, AdbSource::Managed);
        let mut w = env();
        w.bundled_dir = Some(PathBuf::from(
            "C:\\Program Files\\StatasMon\\platform-tools",
        ));
        let c = candidates(HostOs::Windows, &w);
        assert_eq!(c[0].0, AdbSource::Bundled);
        assert!(c[0].1.display().to_string().ends_with("adb.exe"));
    }

    #[test]
    fn macos_search_order() {
        let mut e = env();
        e.override_path = Some(PathBuf::from("/x/adb"));
        let c = candidates(HostOs::MacOs, &e);
        let paths: Vec<String> = c.iter().map(|(_, p)| p.display().to_string()).collect();
        assert_eq!(
            paths,
            vec![
                "/x/adb",
                "/Users/dev/Library/Application Support/StatasMon/platform-tools/adb",
                "/usr/bin/adb",
                "/opt/homebrew/bin/adb",
                "/Users/dev/Downloads/platform-tools/adb",
                "/Users/dev/Library/Android/sdk/platform-tools/adb",
            ]
        );
        assert_eq!(c[0].0, AdbSource::Env);
        assert_eq!(c[1].0, AdbSource::Managed);
        assert_eq!(c[2].0, AdbSource::Path);
        assert_eq!(c[4].0, AdbSource::Downloads);
        assert_eq!(c[5].0, AdbSource::Sdk);
    }

    #[test]
    fn windows_paths_use_local_app_data_and_exe() {
        let mut e = env();
        e.path = Some("C:\\tools;D:\\bin".to_owned());
        let c = candidates(HostOs::Windows, &e);
        let paths: Vec<String> = c.iter().map(|(_, p)| p.display().to_string()).collect();
        assert!(paths[0].ends_with("StatasMon/platform-tools/adb.exe"));
        assert!(paths[0].starts_with("C:\\Users\\dev\\AppData\\Local"));
        assert!(paths[1].ends_with("adb.exe"));
        assert!(paths[1].starts_with("C:\\tools"));
        assert!(paths[2].starts_with("D:\\bin"));
        assert!(paths.last().unwrap().contains("Android"));
        assert!(paths.last().unwrap().contains("Sdk"));
    }

    #[test]
    fn missing_hint_names_the_managed_folder_and_url() {
        let hint = missing_hint(HostOs::MacOs, &env());
        assert!(hint.contains(PLATFORM_TOOLS_URL));
        assert!(hint.contains("/Users/dev/Library/Application Support/StatasMon/platform-tools"));
        assert!(hint.contains("STATASMON_ADB"));
    }

    #[tokio::test]
    async fn env_override_wins_and_reports_version() {
        let dir = std::env::temp_dir().join(format!("statasmon-locator-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("adb");
        std::fs::write(
            &fake,
            "#!/bin/sh\necho 'Android Debug Bridge version 1.0.41'\necho 'Version 37.0.1-13800976'\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let e = LocatorEnv {
            override_path: Some(fake.clone()),
            bundled_dir: None,
            home: None,
            local_app_data: None,
            path: None,
        };
        match locate_with(HostOs::current(), &e).await {
            AdbLocation::Found {
                path,
                source,
                version,
            } => {
                assert_eq!(path, fake);
                assert_eq!(source, AdbSource::Env);
                if cfg!(unix) {
                    let v = version.expect("version parsed");
                    assert_eq!(v.name, "adb");
                    assert_eq!(v.version, "37.0.1-13800976");
                }
            }
            other => panic!("unexpected {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn nothing_found_gives_hint() {
        let e = LocatorEnv {
            override_path: Some(PathBuf::from("/definitely/not/here/adb")),
            bundled_dir: None,
            home: Some(PathBuf::from("/definitely/not/here")),
            local_app_data: None,
            path: Some("/definitely/not/here/bin".to_owned()),
        };
        match locate_with(HostOs::MacOs, &e).await {
            AdbLocation::Missing { hint } => assert!(hint.contains("platform-tools")),
            other => panic!("unexpected {other:?}"),
        }
    }
}
