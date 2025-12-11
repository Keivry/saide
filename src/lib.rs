pub mod app;
pub mod config;
pub mod controller;
pub mod decoder;
pub mod scrcpy;
pub mod v4l2;

pub use {
    decoder::{DecodedFrame, H264Decoder, VideoDecoder},
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
};
