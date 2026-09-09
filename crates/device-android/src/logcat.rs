//! Continuous `logcat` stream → `LogEntry` batches and derived events
//! (step 2.6).

use std::sync::Arc;

use statasmon_device::EventSink;
use tokio_util::sync::CancellationToken;

use crate::shell::Shell;

/// How the stream is opened.
#[derive(Debug, Clone)]
pub struct LogcatConfig {
    /// Buffers to read (default main, system, crash, events).
    pub buffers: Vec<String>,
    /// Restrict to one pid (per-app log) when set.
    pub pid: Option<u32>,
    /// Max entries per `Message::Log`.
    pub batch: usize,
}

impl Default for LogcatConfig {
    fn default() -> Self {
        Self {
            buffers: ["main", "system", "crash", "events"]
                .map(String::from)
                .to_vec(),
            pid: None,
            batch: 200,
        }
    }
}

/// Run until `cancel` fires. Reconnects with backoff if the stream ends
/// while the device is still attached.
pub async fn run(
    shell: Arc<dyn Shell>,
    sink: EventSink,
    device_id: String,
    cfg: LogcatConfig,
    cancel: CancellationToken,
) {
    let _ = (shell, sink, device_id, cfg);
    // Filled in by the samplers agent (2.6).
    cancel.cancelled().await;
}
