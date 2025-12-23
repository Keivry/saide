pub mod app;
pub mod config;
pub mod constant;
pub mod controller;
pub mod decoder;
pub mod error;
pub mod scrcpy;
pub mod sync;
pub mod sys;

pub use {
    decoder::{
        AudioDecoder,
        AudioPlayer,
        AutoDecoder,
        DecodedAudio,
        DecodedFrame,
        H264Decoder,
        Nv12RenderResources,
        OpusDecoder,
        VideoDecoder,
        new_nv12_render_callback,
    },
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
    sync::AVSync,
    sys::{GpuType, detect_gpu},
};
