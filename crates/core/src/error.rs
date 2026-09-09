//! Engine errors. Every variant maps to a protocol [`ErrorCode`] and carries
//! a user-facing hint, like provider errors do.

use statasmon_device::DeviceError;
use statasmon_protocol::ErrorCode;

use statasmon_protocol::SessionStatus;

/// What can go wrong when the UI calls the engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// No device with that id is attached.
    #[error("unknown device {0}")]
    UnknownDevice(String),
    /// A session command that does not apply in the current state.
    #[error("cannot {action} a session that is {status:?}")]
    InvalidTransition {
        /// The requested action ("pause", "resume", ...).
        action: &'static str,
        /// The current status.
        status: SessionStatus,
    },
    /// A log filter with an invalid regular expression.
    #[error("invalid log filter: {0}")]
    InvalidFilter(String),
    /// The provider failed.
    #[error(transparent)]
    Device(#[from] DeviceError),
    /// The engine was shut down.
    #[error("engine is shut down")]
    ShutDown,
}

impl EngineError {
    /// The protocol error code.
    pub fn code(&self) -> ErrorCode {
        match self {
            EngineError::UnknownDevice(_) => ErrorCode::DeviceLost,
            EngineError::InvalidTransition { .. } | EngineError::InvalidFilter(_) => {
                ErrorCode::Unsupported
            }
            EngineError::Device(e) => e.code(),
            EngineError::ShutDown => ErrorCode::Internal,
        }
    }

    /// What the user can do about it.
    pub fn hint(&self) -> Option<String> {
        match self {
            EngineError::UnknownDevice(_) => {
                Some("The device is no longer connected. Reconnect it and try again.".into())
            }
            EngineError::InvalidTransition { .. } => {
                Some("Check the recording state shown in the sidebar.".into())
            }
            EngineError::InvalidFilter(_) => {
                Some("Fix the regular expression or turn regex mode off.".into())
            }
            EngineError::Device(e) => e.hint().map(str::to_owned),
            EngineError::ShutDown => Some("Restart the app.".into()),
        }
    }
}
