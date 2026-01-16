//! Video decoder module using FFmpeg

pub mod audio;
mod auto;
mod error;
mod h264;
mod h264_parser;
mod nv12_render;
mod nvdec;
mod rgba_render;
mod vaapi;

pub use {
    audio::{AudioDecoder, AudioPlayer, DecodedAudio, OpusDecoder},
    auto::AutoDecoder,
    error::VideoError,
    h264::H264Decoder,
    h264_parser::extract_resolution_from_stream,
    nv12_render::{Nv12RenderResources, new_nv12_render_callback},
    nvdec::NvdecDecoder,
    rgba_render::{RgbaRenderResources, new_rgba_render_callback},
    vaapi::VaapiDecoder,
};
use {error::Result, ffmpeg_next::format::Pixel};

/// Decoded video frame
#[derive(Debug)]
pub struct DecodedFrame {
    /// Frame data (RGBA format)
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Presentation timestamp (microseconds)
    pub pts: i64,
    /// Pixel format
    pub format: Pixel,
}

/// Video decoder trait for hardware/software codec abstraction
///
/// Implementers provide packet-based decoding with optional frame output.
/// Supports both H.264 and H.265 (HEVC) depending on implementation.
///
/// # Implementations
/// - [`H264SoftwareDecoder`] - FFmpeg software decode
/// - [`NvdecDecoder`] - NVIDIA GPU decode (H.264/H.265)
/// - [`VaapiDecoder`] - VA-API GPU decode (Linux)
/// - [`AutoDecoder`] - Automatic fallback selection
///
/// [`H264SoftwareDecoder`]: crate::decoder::h264::H264SoftwareDecoder
/// [`NvdecDecoder`]: crate::decoder::nvdec::NvdecDecoder
/// [`VaapiDecoder`]: crate::decoder::vaapi::VaapiDecoder
/// [`AutoDecoder`]: crate::decoder::video::auto::AutoDecoder
///
/// # Thread Safety
/// NOT thread-safe - decoder instances are NOT `Send` or `Sync`.
/// Use separate decoder instances per thread.
///
/// # Error Handling
/// Errors during decode should skip the frame and continue (resilience).
/// Persistent errors (e.g., resolution mismatch) should recreate the decoder.
///
/// # Example
/// ```ignore
/// let mut decoder = AutoDecoder::new(1920, 1080)?;
/// let frame = decoder.decode(&packet_data, pts)?;
/// if let Some(frame) = frame {
///     // Render frame
/// }
/// ```
pub trait VideoDecoder {
    /// Decode a packet and return a frame if available
    ///
    /// # Arguments
    /// - `packet`: Encoded H.264/H.265 NAL units (Annex B format)
    /// - `pts`: Presentation timestamp in microseconds
    ///
    /// # Returns
    /// - `Ok(Some(frame))`: Frame decoded successfully
    /// - `Ok(None)`: Packet consumed, no frame yet (e.g., B-frames buffering)
    /// - `Err(_)`: Decode error (caller should skip frame and continue)
    ///
    /// # Thread Safety
    /// This method is NOT thread-safe. Do not call concurrently.
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>>;

    /// Flush decoder to retrieve buffered frames
    ///
    /// Called during shutdown or resolution change to drain the decoder pipeline.
    ///
    /// # Returns
    /// - `Ok(frames)`: All remaining buffered frames
    /// - `Err(_)`: Flush failed (decoder in invalid state)
    ///
    /// # Notes
    /// After flushing, decoder may need to be recreated (implementation-specific).
    fn flush(&mut self) -> Result<Vec<DecodedFrame>>;
}
