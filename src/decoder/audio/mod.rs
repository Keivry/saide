// SPDX-License-Identifier: MIT OR Apache-2.0

//! Audio decoding and playback

mod error;
mod opus;
mod player;

// Export both for flexibility
use error::Result;
pub use {error::AudioError, opus::OpusDecoder, player::AudioPlayer};

/// Decoded audio frame
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// PCM samples (f32, interleaved stereo: L R L R ...)
    pub samples: Vec<f32>,

    /// Sample rate (Hz)
    pub sample_rate: u32,

    /// Number of channels
    pub channels: u16,

    /// Presentation timestamp (microseconds)
    pub pts: i64,
}

/// Audio decoder trait
pub trait AudioDecoder {
    /// Decode audio packet to PCM samples
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedAudio>>;

    /// Flush decoder (get remaining frames)
    fn flush(&mut self) -> Result<Vec<DecodedAudio>>;
}
