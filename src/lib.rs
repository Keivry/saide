pub mod scrcpy;
pub mod decoder;

// Re-export commonly used types
pub use scrcpy::protocol::{ControlMessage, VideoPacket};
pub use decoder::{DecodedFrame, VideoDecoder, H264Decoder};
