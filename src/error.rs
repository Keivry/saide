//! Unified error handling for Saide
//!
//! Error hierarchy:
//! - Cancelled: Normal shutdown (CancellationToken triggered)
//! - ConnectionLost: Device disconnected (USB/WiFi)
//! - Decode: Video/audio decoding errors
//! - Adb: ADB command failures
//! - Io: Network/file I/O errors (preserves ErrorKind)
//! - Other: Unexpected errors

use {std::io, thiserror::Error};

/// SAide unified error type
#[derive(Error, Debug)]
pub enum SAideError {
    /// Normal shutdown via CancellationToken
    #[error("Operation cancelled")]
    Cancelled,

    /// Device connection lost (USB/WiFi disconnect)
    #[error("Connection lost: {reason} (device: {device})")]
    ConnectionLost { device: String, reason: String },

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
    #[error("ADB command failed: {command} on device {device}: {message}")]
    Adb {
        device: String,
        command: String,
        message: String,
    },

    /// Network I/O error (preserves original ErrorKind for precise detection)
    #[error("I/O error: {source}")]
    Io {
        #[source]
        source: io::Error,
    },

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
    pub fn is_connection_lost(&self) -> bool { matches!(self, SAideError::ConnectionLost { .. }) }

    /// Check if error should show UI overlay
    pub fn should_show_overlay(&self) -> bool { self.is_connection_lost() }

    /// Check if error should be logged
    pub fn should_log(&self) -> bool { !self.is_cancelled() }

    /// Check if error indicates connection shutdown (using ErrorKind for precision)
    pub fn is_io_shutdown(&self) -> bool {
        if let SAideError::Io { source } = self {
            matches!(
                source.kind(),
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
        if let SAideError::Io { source } = self {
            matches!(
                source.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            )
        } else {
            false
        }
    }
}

// Automatic conversions from common error types
impl From<io::Error> for SAideError {
    fn from(source: io::Error) -> Self { SAideError::Io { source } }
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
        let err = SAideError::ConnectionLost {
            device: "emulator-5554".to_string(),
            reason: "USB disconnected".to_string(),
        };
        assert!(!err.is_cancelled());
        assert!(err.is_connection_lost());
        assert!(err.should_show_overlay());
        assert!(err.should_log());
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "pipe broken");
        let err = SAideError::from(io_err);
        assert!(matches!(err, SAideError::Io { .. }));
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
