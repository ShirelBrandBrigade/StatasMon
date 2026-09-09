//! Packet capture via `com.apple.pcapd` to a `.pcap` file (step 3.7).

use std::path::PathBuf;
use std::sync::Arc;

use idevice::provider::IdeviceProvider;
use tokio_util::sync::CancellationToken;

use crate::error::IosError;

/// Capture until `cancel` fires; returns the number of packets written.
pub async fn capture(
    provider: Arc<dyn IdeviceProvider>,
    path: PathBuf,
    cancel: CancellationToken,
) -> Result<u64, IosError> {
    let _ = (provider, path);
    // Implemented by the logs agent (3.7).
    cancel.cancelled().await;
    Ok(0)
}
