//! Unified error handling for Saide
//!
//! Error hierarchy:
//! - Cancelled: Normal shutdown (CancellationToken triggered)
//! - ConnectionLost: Device disconnected (USB/WiFi)
//! - Decode: Video/audio decoding errors
//! - Adb: ADB command failures
//! - Io: Network/file I/O errors (preserves ErrorKind)
//! - Other: Unexpected errors

use {
    super::decoder::{VideoError, audio::AudioError},
    serde_json::Error as SerdeError,
    std::{env::VarError, fmt, io},
    thiserror::Error,
    toml::{de::Error as TomlDeError, ser::Error as TomlError},
};

#[derive(Clone, Debug)]
pub struct IoError {
    source_kind: io::ErrorKind,
    message: String,
}

impl IoError {
    pub fn new(source: io::Error) -> Self {
        Self {
            source_kind: source.kind(),
            message: source.to_string(),
        }
    }

    pub fn new_with_message(message: impl Into<String>) -> Self {
        Self {
            source_kind: io::ErrorKind::Other,
            message: message.into(),
        }
    }

    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    pub fn message(&self) -> &str { &self.message }

    pub fn kind(&self) -> io::ErrorKind { self.source_kind }
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.kind())?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ConnectionLost {
    device: String,
    reason: String,
}

impl ConnectionLost {
    pub fn new(device: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for ConnectionLost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Connection lost for device {}: {}",
            self.device, self.reason
        )
    }
}

/// SAide unified error type
#[derive(Clone, Debug, Error)]
pub enum SAideError {
    /// Normal shutdown via CancellationToken
    #[error("Application shutdown requested")]
    Cancelled,

    /// Device connection lost (USB/WiFi disconnect)
    #[error("{0}")]
    ConnectionLost(ConnectionLost),

    /// Video error
    #[error("Video error: {0}")]
    VideoError(#[from] VideoError),

    /// Audio error
    #[error("Audio error: {0}")]
    AudioError(#[from] AudioError),

    /// Video/audio decoding error
    #[error("Decode error: {0}")]
    DecodeError(String),

    /// A/V Format error
    #[error("Format error: {0}")]
    FormatError(String),

    /// ADB command execution error
    #[error("ADB command failed: {0}")]
    AdbError(String),

    /// I/O error (preserves original ErrorKind for precise detection)
    #[error("I/O error: {0}")]
    IoError(IoError),

    /// Scrcpy protocol error
    #[error("Scrcpy protocol error: {0}")]
    ProtocolError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Channel send/receive error
    #[error("Channel error: {0}")]
    ChannelError(String),

    /// Ui error
    #[error("UI error: {0}")]
    UiError(String),

    /// System error
    #[error("System error: {0}")]
    SystemError(String),

    /// Other unexpected errors
    #[error("Unexpected error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SAideError>;

impl SAideError {
    /// Check if error is a normal shutdown
    pub fn is_cancelled(&self) -> bool { matches!(self, SAideError::Cancelled) }

    /// Check if error is connection lost
    pub fn is_connection_lost(&self) -> bool { matches!(self, SAideError::ConnectionLost { .. }) }

    /// Check if error should show UI overlay
    pub fn should_show_overlay(&self) -> bool { self.is_connection_lost() }

    /// Check if error should be logged
    pub fn should_log(&self) -> bool { !self.is_cancelled() }

    /// Check if error indicates connection shutdown (using ErrorKind for precision)
    pub fn is_io_shutdown(&self) -> bool {
        if let SAideError::IoError(err) = self {
            matches!(
                err.kind(),
                io::ErrorKind::ConnectionReset
                    | io::ErrorKind::BrokenPipe
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::UnexpectedEof
            )
        } else {
            false
        }
    }

    /// Check if error is a timeout (using ErrorKind)
    pub fn is_timeout(&self) -> bool {
        if let SAideError::IoError(err) = self {
            matches!(
                err.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            )
        } else {
            false
        }
    }
}

// Automatic conversions from common error types
impl From<io::Error> for SAideError {
    fn from(source: io::Error) -> Self { SAideError::IoError(IoError::new(source)) }
}

impl<T> From<crossbeam_channel::SendError<T>> for SAideError {
    fn from(err: crossbeam_channel::SendError<T>) -> Self {
        SAideError::ChannelError(err.to_string())
    }
}

impl From<crossbeam_channel::RecvError> for SAideError {
    fn from(err: crossbeam_channel::RecvError) -> Self { SAideError::ChannelError(err.to_string()) }
}

impl From<SerdeError> for SAideError {
    fn from(err: SerdeError) -> Self { SAideError::ConfigError(err.to_string()) }
}

impl From<VarError> for SAideError {
    fn from(err: VarError) -> Self { SAideError::SystemError(err.to_string()) }
}

impl From<TomlError> for SAideError {
    fn from(err: TomlError) -> Self { SAideError::ConfigError(err.to_string()) }
}

impl From<TomlDeError> for SAideError {
    fn from(err: TomlDeError) -> Self { SAideError::ConfigError(err.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cancelled_detection() {
        let err = SAideError::Cancelled;
        assert!(err.is_cancelled());
        assert!(!err.is_connection_lost());
        assert!(!err.should_log());
    }

    #[test]
    fn test_connection_lost_detection() {
        let err =
            SAideError::ConnectionLost(ConnectionLost::new("emulator-5554", "USB disconnected"));
        assert!(!err.is_cancelled());
        assert!(err.is_connection_lost());
        assert!(err.should_show_overlay());
        assert!(err.should_log());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
        let err = SAideError::from(io_err);
        assert!(matches!(err, SAideError::IoError { .. }));
        assert!(err.is_io_shutdown());
    }

    #[test]
    fn test_io_shutdown_detection() {
        let errors = vec![
            io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"),
            io::Error::new(io::ErrorKind::ConnectionReset, "connection reset"),
            io::Error::new(io::ErrorKind::ConnectionAborted, "connection aborted"),
            io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected eof"),
        ];

        for io_err in errors {
            let err = SAideError::from(io_err);
            assert!(
                err.is_io_shutdown(),
                "Expected is_io_shutdown() to be true for {:?}",
                err
            );
        }
    }

    #[test]
    fn test_timeout_detection() {
        let timeout_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let err = SAideError::from(timeout_err);
        assert!(err.is_timeout());

        let wouldblock_err = io::Error::new(io::ErrorKind::WouldBlock, "would block");
        let err = SAideError::from(wouldblock_err);
        assert!(err.is_timeout());
    }
}
