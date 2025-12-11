pub mod decoder;
pub mod scrcpy;

// Re-export commonly used types
pub use {
    decoder::{DecodedFrame, H264Decoder, VideoDecoder},
    scrcpy::protocol::{control::ControlMessage, video::VideoPacket},
};
