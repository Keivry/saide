//! Video decoder module using FFmpeg

mod h264;

pub use h264::H264Decoder;

use anyhow::Result;

/// Decoded video frame
#[derive(Debug)]
pub struct DecodedFrame {
    /// Frame data (YUV420P format)
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Presentation timestamp (microseconds)
    pub pts: i64,
}

/// Video decoder trait
pub trait VideoDecoder {
    /// Decode a packet
    fn decode(&mut self, packet: &[u8], pts: i64) -> Result<Option<DecodedFrame>>;
    
    /// Flush decoder (get remaining frames)
    fn flush(&mut self) -> Result<Vec<DecodedFrame>>;
}
