// SPDX-License-Identifier: MIT OR Apache-2.0

use {ffmpeg::Error as FfmpegError, ffmpeg_next as ffmpeg, thiserror::Error};

pub type Result<T> = std::result::Result<T, VideoError>;

#[derive(Clone, Debug, Error)]
pub enum VideoError {
    /// Initialization error
    #[error("Video initialization error: {0}")]
    InitializationError(String),

    /// From FFmpeg error
    #[error("FFmpeg error: {0}")]
    FfmpegError(#[from] FfmpegError),

    /// Video decoding error
    #[error("Video decoding error: {0}")]
    DecodingError(String),
}
