//! Opus audio decoder using FFmpeg

use {
    super::{AudioDecoder, DecodedAudio},
    anyhow::{Context, Result},
    ffmpeg_next as ffmpeg,
    tracing::{debug, info},
};

/// Opus decoder
pub struct OpusDecoder {
    decoder: ffmpeg::decoder::Audio,
    sample_rate: u32,
    channels: u16,
}

impl OpusDecoder {
    /// Create new Opus decoder
    ///
    /// # Arguments
    /// * `sample_rate` - Expected sample rate (typically 48000 Hz for Android)
    /// * `channels` - Number of channels (1=mono, 2=stereo)
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        ffmpeg::init().context("Failed to initialize FFmpeg")?;

        // Find Opus decoder
        let codec = ffmpeg::decoder::find(ffmpeg::codec::Id::OPUS)
            .context("Opus decoder not found")?;

        // Create decoder context
        let decoder = ffmpeg::codec::context::Context::new_with_codec(codec)
            .decoder()
            .audio()?;

        info!(
            "Initialized Opus decoder: {}Hz, {} channels",
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
        // Create FFmpeg packet
        let mut ffmpeg_packet = ffmpeg::codec::packet::Packet::copy(packet);
        ffmpeg_packet.set_pts(Some(pts));

        // Send packet to decoder
        self.decoder
            .send_packet(&ffmpeg_packet)
            .context("Failed to send packet to Opus decoder")?;

        // Receive decoded frame
        let mut frame = ffmpeg::util::frame::Audio::empty();
        match self.decoder.receive_frame(&mut frame) {
            Ok(_) => {
                let samples = extract_f32_samples(&frame)?;
                debug!(
                    "Decoded audio: {} samples, {} channels",
                    samples.len() / self.channels as usize,
                    self.channels
                );

                Ok(Some(DecodedAudio {
                    samples,
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    pts,
                }))
            }
            Err(ffmpeg::Error::Other { errno: ffmpeg::error::EAGAIN }) => {
                // Need more data
                Ok(None)
            }
            Err(e) => Err(e).context("Failed to decode Opus frame"),
        }
    }

    fn flush(&mut self) -> Result<Vec<DecodedAudio>> {
        self.decoder.send_eof()?;

        let mut frames = Vec::new();
        loop {
            let mut frame = ffmpeg::util::frame::Audio::empty();
            match self.decoder.receive_frame(&mut frame) {
                Ok(_) => {
                    let samples = extract_f32_samples(&frame)?;
                    frames.push(DecodedAudio {
                        samples,
                        sample_rate: self.sample_rate,
                        channels: self.channels,
                        pts: frame.pts().unwrap_or(0),
                    });
                }
                Err(ffmpeg::Error::Eof) => break,
                Err(e) => return Err(e).context("Failed to flush Opus decoder"),
            }
        }

        Ok(frames)
    }
}

/// Extract f32 samples from FFmpeg audio frame
fn extract_f32_samples(frame: &ffmpeg::util::frame::Audio) -> Result<Vec<f32>> {
    let format = frame.format();

    // Ensure format is F32 packed
    if format != ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed) {
        anyhow::bail!("Unexpected audio format: {:?}", format);
    }

    let data = frame.data(0);
    let sample_count = data.len() / std::mem::size_of::<f32>();

    // Convert &[u8] to &[f32]
    let samples: Vec<f32> = data
        .chunks_exact(4)
        .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    if samples.len() != sample_count {
        anyhow::bail!(
            "Sample count mismatch: expected {}, got {}",
            sample_count,
            samples.len()
        );
    }

    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_decoder_creation() {
        let decoder = OpusDecoder::new(48000, 2);
        assert!(decoder.is_ok());
    }
}
