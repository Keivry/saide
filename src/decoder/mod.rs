//! Video decoder module using FFmpeg

mod h264;
mod rgba_render;
mod vaapi;

use {anyhow::Result, ffmpeg_next::format::Pixel};
pub use {
    h264::H264Decoder,
    rgba_render::{RgbaRenderResources, new_rgba_render_callback},
    vaapi::VaapiDecoder,
};

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

/// Video decoder trait
pub trait VideoDecoder {
    /// Decode a packet
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>>;

    /// Flush decoder (get remaining frames)
    fn flush(&mut self) -> Result<Vec<DecodedFrame>>;
}
