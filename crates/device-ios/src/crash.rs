//! Crash reports via `com.apple.crashreportmover` + `crashreportcopymobile`
//! (step 3.6).

use std::path::{Path, PathBuf};

use idevice::provider::IdeviceProvider;

use crate::error::IosError;

/// One `.ips` report copied to the host.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashReport {
    pub name: String,
    pub ts_ms: Option<i64>,
    pub process: Option<String>,
    pub bundle_id: Option<String>,
    pub reason: Option<String>,
    pub local_path: PathBuf,
}

/// Move pending reports on the device, copy new `.ips` files into
/// `dest_dir`, parse their header. `since_ms` skips reports older than that.
pub async fn fetch(
    provider: &dyn IdeviceProvider,
    dest_dir: &Path,
    since_ms: Option<i64>,
) -> Result<Vec<CrashReport>, IosError> {
    let _ = (provider, dest_dir, since_ms);
    // Implemented by the logs agent (3.6).
    Ok(Vec::new())
}
