//! Native Opus decoder using libopus directly

use {
    super::{
        error::{AudioError, Result},
        AudioDecoder,
        DecodedAudio,
    },
    opus::{Channels, Decoder},
    tracing::{debug, info, trace},
};

/// Native Opus audio decoder
pub struct OpusDecoder {
    decoder: Decoder,
    sample_rate: u32,
    channels: u16,
}

/// Number of samples per scrcpy audio frame
const SCRCPY_FRAME_SAMPLES: usize = 960; // 20ms at 48kHz

impl OpusDecoder {
    /// Create new Opus decoder
    ///
    /// # Arguments
    /// * `sample_rate` - Sample rate (must be 48000 for Opus)
    /// * `channels` - Number of channels (1=mono, 2=stereo)
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        if sample_rate != 48000 {
            return Err(AudioError::UnsupportedFormat(
                "Opus only supports 48000 Hz sample rate".to_string(),
            ));
        }

        let opus_channels = match channels {
            1 => Channels::Mono,
            2 => Channels::Stereo,
            _ => {
                return Err(AudioError::UnsupportedFormat(
                    "Opus only supports mono or stereo channels".to_string(),
                ));
            }
        };

        let decoder = Decoder::new(sample_rate, opus_channels).map_err(|e| {
            AudioError::InitializationError(format!("Failed to create Opus decoder: {:?}", e))
        })?;

        info!(
            "Initialized native Opus decoder: {}Hz, {} channels",
            sample_rate, channels
        );

        Ok(Self {
            decoder,
            sample_rate,
            channels,
        })
    }
}

impl AudioDecoder for OpusDecoder {
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedAudio>> {
        // Skip empty or very small packets (likely config/padding)
        if packet.len() < 2 {
            debug!("Skipping small packet: {} bytes", packet.len());
            return Ok(None);
        }

        // Decode to PCM (f32, interleaved)
        // Opus can decode up to 5760 samples per channel (120ms at 48kHz)
        // But scrcpy-server typically sends 20ms frames (960 samples)
        let mut output = vec![0f32; SCRCPY_FRAME_SAMPLES * self.channels as usize];

        let samples_per_channel = self.decoder.decode_float(packet, &mut output, false)?;
        if samples_per_channel == 0 {
            trace!("No samples decoded from packet");
            return Ok(None);
        }

        output.truncate(samples_per_channel * self.channels as usize);
        Ok(Some(DecodedAudio {
            samples: output,
            sample_rate: self.sample_rate,
            channels: self.channels,
            pts,
        }))
    }

    fn flush(&mut self) -> Result<Vec<DecodedAudio>> {
        // Opus decoder doesn't need flushing (no internal buffering)
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_native_decoder_creation() {
        let decoder = OpusDecoder::new(48000, 2);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_opus_native_invalid_sample_rate() {
        let decoder = OpusDecoder::new(44100, 2);
        assert!(decoder.is_err());
    }
}
