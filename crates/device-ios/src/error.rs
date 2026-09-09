//! Error type shared by every iOS module.

use statasmon_protocol::ErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum IosError {
    #[error("usbmuxd is not reachable: {0}")]
    MuxdUnavailable(String),
    #[error("device {0} is not paired; accept \"Trust This Computer\" on the device")]
    NotPaired(String),
    #[error("device {0} is gone")]
    DeviceGone(String),
    #[error("Developer Mode is off")]
    DeveloperModeOff,
    #[error("tunnel failed: {0}")]
    Tunnel(String),
    #[error("service {service} failed: {source}")]
    Service {
        service: &'static str,
        #[source]
        source: idevice::IdeviceError,
    },
    #[error("unexpected data from {0}")]
    Parse(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Plist(#[from] plist::Error),
}

impl IosError {
    pub fn service(service: &'static str, source: idevice::IdeviceError) -> Self {
        Self::Service { service, source }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            IosError::MuxdUnavailable(_) => ErrorCode::DriverMissing,
            IosError::NotPaired(_) => ErrorCode::NotTrusted,
            IosError::DeviceGone(_) => ErrorCode::DeviceLost,
            IosError::DeveloperModeOff => ErrorCode::DeveloperModeOff,
            IosError::Tunnel(_) => ErrorCode::TunnelFailed,
            IosError::Service { .. } => ErrorCode::Io,
            IosError::Parse(_) => ErrorCode::Parse,
            IosError::Io(_) => ErrorCode::Io,
            IosError::Plist(_) => ErrorCode::Parse,
        }
    }

    /// A user-facing next step, when there is one.
    pub fn hint(&self) -> Option<String> {
        match self {
            IosError::MuxdUnavailable(_) => Some(
                "On Windows install iTunes or the Apple Devices app (Apple Mobile Device Support); on macOS usbmuxd is built in."
                    .to_owned(),
            ),
            IosError::NotPaired(_) => {
                Some("Unlock the device and tap Trust when asked about this computer.".to_owned())
            }
            IosError::DeveloperModeOff => Some(
                "Settings → Privacy & Security → Developer Mode, then restart the device.".to_owned(),
            ),
            _ => None,
        }
    }
}
