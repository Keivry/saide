//! Unified error handling for Saide
//!
//! Error hierarchy:
//! - Cancelled: Normal shutdown (CancellationToken triggered)
//! - ConnectionLost: Device disconnected (USB/WiFi)
//! - Decode: Video/audio decoding errors
//! - Adb: ADB command failures
//! - IO: Network/file I/O errors
//! - Other: Unexpected errors

use {std::io, thiserror::Error};

/// SAide unified error type
#[derive(Error, Debug, Clone)]
pub enum SAideError {
    /// Normal shutdown via CancellationToken
    #[error("Operation cancelled")]
    Cancelled,

    /// Device connection lost (USB/WiFi disconnect)
    #[error("Connection lost: {0}")]
    ConnectionLost(String),

    /// Video error
    #[error("Video error: {0}")]
    Video(String),

    /// Audio error
    #[error("Audio error: {0}")]
    Audio(String),

    /// Video/audio decoding error
    #[error("Decode error: {0}")]
    Decode(String),

    /// A/V Format error
    #[error("Format error: {0}")]
    Format(String),

    /// ADB command execution error
    #[error("ADB error: {0}")]
    Adb(String),

    /// Network I/O error
    #[error("Network I/O error: {0}")]
    Io(String),

    /// Scrcpy protocol error
    #[error("Scrcpy protocol error: {0}")]
    Protocol(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Channel send/receive error
    #[error("Channel error: {0}")]
    Channel(String),

    /// Ui error
    #[error("UI error: {0}")]
    Ui(String),

    /// Other unexpected errors
    #[error("Unexpected error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, SAideError>;

impl SAideError {
    /// Check if error is a normal shutdown
    pub fn is_cancelled(&self) -> bool { matches!(self, SAideError::Cancelled) }

    /// Check if error is connection lost
    pub fn is_connection_lost(&self) -> bool { matches!(self, SAideError::ConnectionLost(_)) }

    /// Check if error should show UI overlay
    pub fn should_show_overlay(&self) -> bool { self.is_connection_lost() }

    /// Check if error should be logged
    pub fn should_log(&self) -> bool { !self.is_cancelled() }

    /// Check if error indicates connection shutdown
    pub fn is_io_shutdown(&self) -> bool {
        if let SAideError::Io(msg) = self {
            msg.contains("Connection reset")
                || msg.contains("Broken pipe")
                || msg.contains("Connection aborted")
                || msg.contains("Unexpected end of file")
        } else {
            false
        }
    }

    /// Check if error is a timeout
    pub fn is_timeout(&self) -> bool {
        if let SAideError::Io(msg) = self {
            msg.contains("would block") || msg.contains("timed out")
        } else {
            false
        }
    }
}

// Automatic conversions from common error types
impl From<io::Error> for SAideError {
    fn from(err: io::Error) -> Self { SAideError::Io(err.to_string()) }
}

impl<T> From<crossbeam_channel::SendError<T>> for SAideError {
    fn from(err: crossbeam_channel::SendError<T>) -> Self { SAideError::Channel(err.to_string()) }
}

impl From<crossbeam_channel::RecvError> for SAideError {
    fn from(err: crossbeam_channel::RecvError) -> Self { SAideError::Channel(err.to_string()) }
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
        let err = SAideError::ConnectionLost("USB disconnected".to_string());
        assert!(!err.is_cancelled());
        assert!(err.is_connection_lost());
        assert!(err.should_show_overlay());
        assert!(err.should_log());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
        let err = SAideError::from(io_err);
        assert!(matches!(err, SAideError::Io(_)));
    }
}
