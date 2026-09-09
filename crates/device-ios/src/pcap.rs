//! Packet capture via `com.apple.pcapd` to a `.pcap` file (step 3.7).
//!
//! `pcapd` streams one plist `Data` per packet with a device header
//! (interface, pid, process); `PcapFileWriter` prepends a fake Ethernet
//! header to raw IP frames so Wireshark dissects them. The file is
//! LINKTYPE_ETHERNET, big-endian magic `a1 b2 c3 d4`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use idevice::IdeviceService;
use idevice::provider::IdeviceProvider;
use idevice::services::pcapd::{PcapFileWriter, PcapdClient};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::error::IosError;

/// The big-endian pcap magic the writer emits (first 4 bytes of the file).
pub const PCAP_MAGIC_BE: [u8; 4] = [0xa1, 0xb2, 0xc3, 0xd4];
/// Global header length; a header-only file is a valid empty capture.
pub const PCAP_HEADER_LEN: u64 = 24;

/// Capture until `cancel` fires; returns the number of packets written.
///
/// The file (with its global header) is created before the service is
/// opened, so a capture that is cancelled or fails early still leaves a
/// valid, empty `.pcap` at `path`. Packets are written unbuffered as they
/// arrive, so the file is complete the moment this returns.
pub async fn capture(
    provider: Arc<dyn IdeviceProvider>,
    path: PathBuf,
    cancel: CancellationToken,
) -> Result<u64, IosError> {
    let mut file = tokio::fs::File::create(&path).await?;
    let result = stream_into(&*provider, &mut file, &path, &cancel).await;
    // tokio's File may still have a write in flight: wait for it so the
    // file is complete (and closed) when we return.
    file.flush().await?;
    file.sync_all().await?;
    result
}

async fn stream_into(
    provider: &dyn IdeviceProvider,
    file: &mut tokio::fs::File,
    path: &Path,
    cancel: &CancellationToken,
) -> Result<u64, IosError> {
    let mut writer = PcapFileWriter::new(file).await?;
    let connected = tokio::select! {
        _ = cancel.cancelled() => return Ok(0),
        r = PcapdClient::connect(provider) => r,
    };
    let mut client = connected.map_err(|e| IosError::service("pcapd", e))?;
    tracing::debug!(target: "ios.pcap", "pcapd connected via {}; writing {}", provider.label(), path.display());

    let mut count: u64 = 0;
    loop {
        let next = tokio::select! {
            _ = cancel.cancelled() => break,
            r = client.next_packet() => r,
        };
        match next {
            Ok(packet) => {
                writer.write_packet(&packet).await?;
                count += 1;
            }
            Err(e) => {
                tracing::warn!(target: "ios.pcap", "pcapd stream ended after {count} packets: {e}");
                return Err(IosError::service("pcapd", e));
            }
        }
    }
    tracing::info!(target: "ios.pcap", "capture stopped: {count} packets → {}", path.display());
    Ok(count)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writer_header_matches_magic() {
        let mut buf = Vec::new();
        let _w = PcapFileWriter::new(&mut buf).await.unwrap();
        assert_eq!(buf.len() as u64, PCAP_HEADER_LEN);
        assert_eq!(&buf[..4], &PCAP_MAGIC_BE);
        // LINKTYPE_ETHERNET, big-endian.
        assert_eq!(&buf[20..24], &[0, 0, 0, 1]);
    }
}
