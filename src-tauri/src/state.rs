//! Shared state behind every Tauri command.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use statasmon_core::Engine;
use statasmon_device_fake::FakeHandle;
use statasmon_protocol::Settings;
use tauri::async_runtime::JoinHandle;

/// Which device sources the app was started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMode {
    /// Deterministic fake devices (`STATASMON_SOURCE=fake`, and the default
    /// until real providers exist).
    Fake,
    /// Real providers (Phase 2+).
    Real,
}

pub struct AppState {
    pub engine: Engine,
    pub mode: SourceMode,
    /// Detach/reattach controls when running on the fake.
    pub fake: Option<FakeHandle>,
    /// The forwarding task of the current UI subscription, if any.
    pub forwarder: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// The app data folder (store root, `crashes/`, `pcap/`), when resolved.
    pub app_data_dir: Option<PathBuf>,
    /// Folder holding `settings.json` (the app data folder), when resolved.
    pub settings_dir: Option<PathBuf>,
    /// The settings in effect (loaded at startup, replaced by `set_settings`).
    pub settings: Mutex<Settings>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("mode", &self.mode)
            .field("app_data_dir", &self.app_data_dir)
            .field("settings_dir", &self.settings_dir)
            .finish_non_exhaustive()
    }
}
