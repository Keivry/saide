pub mod avsync;
pub mod config;
pub mod constant;
pub mod controller;
pub mod decoder;
pub mod error;
pub mod gpu;
pub mod i18n;
pub mod profiler;
pub mod saide;
pub mod scrcpy;

pub use {
    avsync::AVSync,
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
    gpu::{GpuType, detect_gpu},
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
};
