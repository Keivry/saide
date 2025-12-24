use thiserror::Error;

pub type Result<T> = std::result::Result<T, AudioError>;

#[derive(Clone, Debug, Error)]
pub enum AudioError {
    /// Failed to initialize audio device
    InitializationError(String),

    /// Audio playback error
    PlaybackError(String),

    /// Unsupported audio format
    UnsupportedFormat(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::InitializationError(msg) => {
                write!(f, "Audio initialization error: {}", msg)
            }
            AudioError::PlaybackError(msg) => write!(f, "Audio playback error: {}", msg),
            AudioError::UnsupportedFormat(msg) => write!(f, "Unsupported audio format: {}", msg),
        }
    }
}
