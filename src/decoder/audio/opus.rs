//! Opus audio decoder using FFmpeg

use {
    super::{AudioDecoder, DecodedAudio},
    anyhow::{Context, Result},
    ffmpeg_next as ffmpeg,
    tracing::{debug, info, trace},
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

        // Set FFmpeg log level to error only (suppress warnings)
        unsafe {
            ffmpeg::sys::av_log_set_level(ffmpeg::sys::AV_LOG_ERROR);
        }

        // Find Opus decoder
        let codec =
            ffmpeg::decoder::find(ffmpeg::codec::Id::OPUS).context("Opus decoder not found")?;

        // Create decoder context with proper initialization
        let mut context = ffmpeg::codec::context::Context::new_with_codec(codec);

        unsafe {
            let ctx_ptr = context.as_mut_ptr();
            (*ctx_ptr).sample_rate = sample_rate as i32;
            (*ctx_ptr).ch_layout.nb_channels = channels as i32;
            (*ctx_ptr).sample_fmt =
                ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Planar).into();
        }

        let decoder = context.decoder().audio()?;

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
        match self.decoder.send_packet(&ffmpeg_packet) {
            Ok(_) => {}
            Err(e) => {
                // First packet might be config/header, skip it
                debug!("Skip packet (might be config): {}", e);
                return Ok(None);
            }
        }

        // Receive decoded frame
        let mut frame = ffmpeg::util::frame::Audio::empty();
        match self.decoder.receive_frame(&mut frame) {
            Ok(_) => {
                let samples = extract_f32_samples(&frame)?;
                trace!(
                    "Decoded audio: {} samples, {} channels, format={:?}",
                    samples.len() / self.channels as usize,
                    self.channels,
                    frame.format()
                );

                Ok(Some(DecodedAudio {
                    samples,
                    sample_rate: self.sample_rate,
                    channels: self.channels,
                    pts,
                }))
            }
            Err(ffmpeg::Error::Other {
                errno: ffmpeg::error::EAGAIN,
            }) => {
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
///
/// Supports both Packed (LRLRLR...) and Planar (LLL...RRR...) formats
fn extract_f32_samples(frame: &ffmpeg::util::frame::Audio) -> Result<Vec<f32>> {
    let format = frame.format();
    let channels = frame.channels() as usize;
    let nb_samples = frame.samples();

    match format {
        // Packed: LRLRLR... (interleaved)
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Packed) => {
            let data = frame.data(0);
            let samples: Vec<f32> = data
                .chunks_exact(4)
                .map(|chunk| f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            Ok(samples)
        }

        // Planar: LLL...RRR... (separate channels)
        ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Planar) => {
            let mut samples = Vec::with_capacity(nb_samples * channels);

            // Interleave channels: L[0]R[0]L[1]R[1]...
            for i in 0..nb_samples {
                for ch in 0..channels {
                    let data = frame.data(ch);
                    let offset = i * 4; // 4 bytes per f32
                    if offset + 4 <= data.len() {
                        let sample = f32::from_ne_bytes([
                            data[offset],
                            data[offset + 1],
                            data[offset + 2],
                            data[offset + 3],
                        ]);
                        samples.push(sample);
                    }
                }
            }

            Ok(samples)
        }

        _ => {
            anyhow::bail!(
                "Unsupported audio format: {:?}. Expected F32(Packed) or F32(Planar)",
                format
            )
        }
    }
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
