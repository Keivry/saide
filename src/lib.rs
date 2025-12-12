pub mod app;
pub mod config;
pub mod controller;
pub mod decoder;
pub mod scrcpy;
pub mod v4l2;

pub use {
    decoder::{
        detect_gpu, AudioDecoder, AudioPlayer, AutoDecoder, DecodedAudio, DecodedFrame,
        GpuType, H264Decoder, OpusDecoder, VideoDecoder,
    },
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
};
