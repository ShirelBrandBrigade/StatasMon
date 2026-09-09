//! Shared state behind every Tauri command.

use std::sync::Arc;

use parking_lot::Mutex;
use statasmon_core::Engine;
use statasmon_device_fake::FakeHandle;
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
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}
