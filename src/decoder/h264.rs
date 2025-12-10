//! H.264 video decoder using FFmpeg
//!
//! Note: This is a simplified implementation that decodes packets directly.
//! For production use, should handle codec parameters from stream.

use super::{DecodedFrame, VideoDecoder};
use anyhow::{Context, Result};
use ffmpeg_next as ffmpeg;
use tracing::info;

pub struct H264Decoder {
    // We'll store decoder creation parameters and create it lazily on first packet
    initialized: bool,
    decoder: Option<ffmpeg::decoder::Video>,
}

impl H264Decoder {
    /// Create a new H.264 decoder
    pub fn new(_width: u32, _height: u32) -> Result<Self> {
        ffmpeg::init().context("Failed to initialize FFmpeg")?;
        
        info!("H.264 decoder created (will initialize on first packet)");

        Ok(Self {
            initialized: false,
            decoder: None,
        })
    }

    /// Initialize decoder from first packet
    fn ensure_decoder(&mut self) -> Result<&mut ffmpeg::decoder::Video> {
        if !self.initialized {
            // Create a minimal decoder context
            let decoder = ffmpeg::decoder::find(ffmpeg::codec::Id::H264)
                .context("H.264 decoder not found")?
                .video()
                .context("Failed to create video decoder")?;

            info!("H.264 decoder initialized");
            self.decoder = Some(decoder);
            self.initialized = true;
        }

        Ok(self.decoder.as_mut().unwrap())
    }
}

impl VideoDecoder for H264Decoder {
    fn decode(&mut self, packet_data: &[u8], pts: i64) -> Result<Option<DecodedFrame>> {
        let decoder = self.ensure_decoder()?;

        // Create packet
        let mut packet = ffmpeg::packet::Packet::copy(packet_data);
        packet.set_pts(Some(pts));

        // Send packet to decoder
        decoder
            .send_packet(&packet)
            .context("Failed to send packet to decoder")?;

        // Try to receive a frame
        let mut decoded = ffmpeg::util::frame::Video::empty();
        match decoder.receive_frame(&mut decoded) {
            Ok(()) => {
                let width = decoded.width();
                let height = decoded.height();

                // Copy Y plane (grayscale for now)
                let y_plane = decoded.data(0);
                let y_stride = decoded.stride(0);

                let mut data = Vec::new();
                for row in 0..height as usize {
                    let start = row * y_stride;
                    data.extend_from_slice(&y_plane[start..start + width as usize]);
                }

                Ok(Some(DecodedFrame {
                    data,
                    width,
                    height,
                    pts,
                }))
            }
            Err(ffmpeg::Error::Other { errno: -11 }) => Ok(None), // EAGAIN
            Err(e) => Err(e).context("Failed to receive frame"),
        }
    }

    fn flush(&mut self) -> Result<Vec<DecodedFrame>> {
        if let Some(decoder) = &mut self.decoder {
            decoder.send_eof().context("Failed to send EOF")?;
        }
        Ok(Vec::new())
    }
}
