pub mod app;
pub mod config;
pub mod controller;
pub mod decoder;
pub mod scrcpy;
pub mod sync;

pub use {
    decoder::{
        AudioDecoder,
        AudioPlayer,
        AutoDecoder,
        DecodedAudio,
        DecodedFrame,
        GpuType,
        H264Decoder,
        Nv12RenderResources,
        OpusDecoder,
        OpusFfmpegDecoder,
        VideoDecoder,
        detect_gpu,
        new_nv12_render_callback,
    },
    scrcpy::{
        connection::ScrcpyConnection,
        protocol::{audio::AudioPacket, control::ControlMessage, video::VideoPacket},
        server::ServerParams,
    },
    sync::AVSync,
};
