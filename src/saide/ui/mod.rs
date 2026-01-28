mod dialog;
mod indicator;
mod mapping;
mod player;
mod saide;
mod state;
mod theme;
mod toolbar;

pub use {saide::SAideApp, theme::AppColors, toolbar::Toolbar};

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
    pub latency_decode_ms: f32,
    pub latency_upload_ms: f32,
    pub latency_p95_ms: f32,
}
