pub mod app;
pub mod config;
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
    error::{Result as SaideResult, SaideError},
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
    sync::AVSync,
    sys::{GpuType, detect_gpu},
};

pub const SCRCPY_SERVER_VERSION: &str = "v3.3.3";
