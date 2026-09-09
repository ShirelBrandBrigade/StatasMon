//! Provider error type. Every error carries a protocol [`ErrorCode`] so the
//! UI can branch on it, a message, and (where we know one) a user-facing
//! hint that says what to do next.

use std::fmt;

use statasmon_protocol::{DeviceId, ErrorCode, Message};

/// Result alias used by every provider.
pub type DeviceResult<T> = Result<T, DeviceError>;

/// An error raised by a device provider or by discovery.
///
/// `code` classifies it for the UI (docs/PLAN.md §3.5), `message` is the
/// technical text, `hint` is the sentence shown to the user ("Enable
/// Developer Mode under Settings › Privacy & Security").
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct DeviceError {
    code: ErrorCode,
    message: String,
    hint: Option<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl DeviceError {
    /// A new error with the given code and technical message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            source: None,
        }
    }

    /// Attach a user-facing hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach the underlying cause.
    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// The action is not supported by this device, platform or tier.
    pub fn unsupported(what: impl fmt::Display) -> Self {
        Self::new(
            ErrorCode::Unsupported,
            format!("{what} is not supported by this device"),
        )
        .with_hint("This platform or connection tier cannot do that.")
    }

    /// The device went away mid-operation.
    pub fn device_lost(device_id: &str) -> Self {
        Self::new(
            ErrorCode::DeviceLost,
            format!("device {device_id} was lost"),
        )
        .with_hint("Check the cable and reconnect the device.")
    }

    /// An I/O failure with a description of what was being done.
    pub fn io(what: impl fmt::Display, err: std::io::Error) -> Self {
        Self::new(ErrorCode::Io, format!("{what}: {err}"))
            .with_hint("The connection failed. Reconnect the device and try again.")
            .with_source(err)
    }

    /// A parser rejected platform output.
    pub fn parse(what: impl fmt::Display, detail: impl fmt::Display) -> Self {
        Self::new(
            ErrorCode::Parse,
            format!("could not parse {what}: {detail}"),
        )
        .with_hint("The device sent output we do not understand yet; please report it.")
    }

    /// Something that should never happen did.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
            .with_hint("Internal error; restarting the app usually clears it.")
    }

    /// The protocol error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The technical message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The user-facing hint, if any.
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    /// Convert into the protocol `Error` message for the bus.
    pub fn to_message(&self, device_id: Option<DeviceId>) -> Message {
        Message::Error {
            device_id,
            code: self.code,
            message: self.message.clone(),
            hint: self.hint.clone(),
        }
    }
}

impl From<std::io::Error> for DeviceError {
    fn from(err: std::io::Error) -> Self {
        Self::io("I/O error", err)
    }
}

impl From<serde_json::Error> for DeviceError {
    fn from(err: serde_json::Error) -> Self {
        Self::parse("JSON", &err).with_source(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carries_code_hint_and_message() {
        let err = DeviceError::new(ErrorCode::NotTrusted, "pairing refused")
            .with_hint("Tap Trust on the device.");
        assert_eq!(err.code(), ErrorCode::NotTrusted);
        assert_eq!(err.hint(), Some("Tap Trust on the device."));
        assert_eq!(err.to_string(), "pairing refused");
        let msg = err.to_message(Some("ios:1".into()));
        match msg {
            Message::Error {
                device_id,
                code,
                hint,
                ..
            } => {
                assert_eq!(device_id.as_deref(), Some("ios:1"));
                assert_eq!(code, ErrorCode::NotTrusted);
                assert!(hint.is_some());
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn io_errors_keep_the_source() {
        let inner = std::io::Error::other("boom");
        let err = DeviceError::io("reading /proc/stat", inner);
        assert_eq!(err.code(), ErrorCode::Io);
        assert!(std::error::Error::source(&err).is_some());
        assert!(err.hint().is_some());
    }
}
