//! The one seam between the ADB transport and everything that parses device
//! output. `adb::AdbShell` implements it for real devices; tests use
//! `FixtureShell` (in `shell::fixture`) which answers from recorded files.

use std::pin::Pin;

use async_trait::async_trait;
use tokio::io::AsyncRead;

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("adb not found: {0}")]
    AdbNotFound(String),
    #[error("device {0} is not connected")]
    DeviceGone(String),
    #[error("adb server error: {0}")]
    Server(String),
    #[error("command timed out after {0} ms")]
    Timeout(u64),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A streaming command (logcat, a long-running shell).
pub type ShellStream = Pin<Box<dyn AsyncRead + Send>>;

/// Runs commands on one device. Implementations must be cheap to clone
/// (Arc inside) and safe to call concurrently from several samplers.
#[async_trait]
pub trait Shell: Send + Sync + std::fmt::Debug {
    /// The ADB serial this shell is bound to.
    fn serial(&self) -> &str;
    /// Run `cmd` through `sh -c` and return its stdout (stderr merged), with a timeout.
    async fn exec(&self, cmd: &str, timeout_ms: u64) -> Result<String, ShellError>;
    /// Start a long-running command and stream its output.
    async fn stream(&self, cmd: &str) -> Result<ShellStream, ShellError>;
}

pub mod fixture {
    //! A `Shell` that replays `fixtures/android/<device>/<name>.txt` files.
    //! Commands are matched by the `cmd` recorded in `manifest.json`; unknown
    //! commands return an empty string so samplers degrade like a locked-down
    //! device would.

    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::{Shell, ShellError, ShellStream};

    #[derive(Debug, Clone)]
    pub struct FixtureShell {
        dir: PathBuf,
        by_cmd: Arc<HashMap<String, String>>,
        serial: String,
    }

    #[derive(serde::Deserialize)]
    struct Entry {
        cmd: String,
    }

    impl FixtureShell {
        /// Load a fixture directory (one produced by `scripts/capture-android-fixture.py`).
        pub fn load(dir: impl AsRef<Path>) -> Result<Self, ShellError> {
            let dir = dir.as_ref().to_path_buf();
            let manifest = std::fs::read_to_string(dir.join("manifest.json"))?;
            let entries: HashMap<String, Entry> = serde_json::from_str(&manifest)
                .map_err(|e| ShellError::Server(format!("bad manifest: {e}")))?;
            let mut by_cmd = HashMap::new();
            for (name, e) in entries {
                by_cmd.insert(e.cmd, name);
            }
            Ok(Self {
                dir,
                by_cmd: Arc::new(by_cmd),
                serial: "fixture".to_owned(),
            })
        }

        /// Raw file content by fixture name (e.g. "dumpsys_battery").
        pub fn file(&self, name: &str) -> String {
            let raw = std::fs::read(self.dir.join(format!("{name}.txt"))).unwrap_or_default();
            let text = String::from_utf8_lossy(&raw);
            text.split("\n[stderr]\n")
                .next()
                .unwrap_or_default()
                .to_owned()
        }
    }

    #[async_trait]
    impl Shell for FixtureShell {
        fn serial(&self) -> &str {
            &self.serial
        }

        async fn exec(&self, cmd: &str, _timeout_ms: u64) -> Result<String, ShellError> {
            Ok(self
                .by_cmd
                .get(cmd)
                .map(|n| self.file(n))
                .unwrap_or_default())
        }

        async fn stream(&self, cmd: &str) -> Result<ShellStream, ShellError> {
            let text = self.exec(cmd, 0).await?;
            Ok(Box::pin(std::io::Cursor::new(text.into_bytes())))
        }
    }
}
