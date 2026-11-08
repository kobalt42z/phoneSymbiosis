use thiserror::Error;

/// One error type shared by all backends, so front ends can show friendly messages.
#[derive(Debug, Error)]
pub enum Error {
    /// No Bluetooth adapter was found or it is turned off.
    #[error("Bluetooth adapter unavailable: {0}")]
    AdapterUnavailable(String),

    /// The requested phone is not known (never paired / out of range).
    #[error("device not found: {0}")]
    DeviceNotFound(String),

    /// The phone is known but not connected.
    #[error("device not connected: {0}")]
    NotConnected(String),

    /// A required system service (BlueZ, PipeWire, obexd...) is missing.
    #[error("system service unavailable: {0}")]
    ServiceUnavailable(String),

    /// The action is valid but not possible right now (e.g. answering with no incoming call).
    #[error("invalid state: {0}")]
    InvalidState(String),

    /// Bad input from the user (e.g. malformed phone number).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// The feature is not implemented on this platform (yet).
    #[error("not supported on this platform: {0}")]
    NotSupported(String),

    /// The phone refused the request (e.g. contacts access not allowed on the iPhone).
    #[error("phone refused the request: {0}")]
    Refused(String),

    /// Anything else coming from the backend.
    #[error("backend error: {0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, Error>;
