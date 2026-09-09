//! `com.apple.os_trace_relay`: the structured log stream (step 3.5). The
//! thermal state and memory warnings are derived from it (`thermal.rs`).

use std::sync::Arc;

use idevice::provider::IdeviceProvider;
use statasmon_device::EventSink;
use tokio_util::sync::CancellationToken;

/// Stream logs until `cancel` fires; reconnects with backoff while the
/// device is still attached. Emits `Message::Log` batches (≤ 500 entries or
/// 250 ms), `thermal.state` samples and `ThermalChange` / `MemoryWarning` /
/// `Jetsam` events via `thermal::Derive`.
pub async fn run(
    provider: Arc<dyn IdeviceProvider>,
    sink: EventSink,
    device_id: String,
    cancel: CancellationToken,
) {
    let _ = (provider, sink, device_id);
    // Implemented by the logs agent (3.5).
    cancel.cancelled().await;
}
