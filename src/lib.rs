pub mod app;
pub mod config;
pub mod controller;
pub mod decoder;
pub mod scrcpy;
pub mod sync;
pub mod v4l2;

pub use {
    decoder::{
        AudioDecoder,
        AudioPlayer,
        AutoDecoder,
        DecodedAudio,
        DecodedFrame,
        GpuType,
        H264Decoder,
        OpusDecoder,
        OpusFfmpegDecoder,
        VideoDecoder,
        detect_gpu,
    },
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
};
